//! Shared helpers for the v0.3 hook tests (WP2): a throwaway state dir (0700) with an
//! inbox, an optional v5 database, the rendered frozen shim, and runners for the hook, the
//! shim and the clients' stored command strings. Nothing touches a real state dir or a
//! real client; every process gets an explicit, minimal environment.
#![allow(dead_code)]
use serde_json::Value;
use std::fs::{self, DirBuilder};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime};
use tempfile::TempDir;

pub const BIN: &str = env!("CARGO_BIN_EXE_codepet");
pub const TEMPLATE: &str = include_str!("../../../../docs/v0.3/contracts/shim.sh.template");
pub const TOMBSTONE: &str = include_str!("../../../../docs/v0.3/contracts/shim-tombstone.sh");
pub const CLIENTS: [&str; 5] = ["claude-code", "codex", "opencode", "hermes", "grok-cli"];

pub struct Hk {
    pub dir: TempDir,
    /// `$XDG_STATE_HOME/codepet`.
    pub state: PathBuf,
    pub home: PathBuf,
    /// The rendered shim, `<data>/codepet/bin/codepet-hook`.
    pub shim: PathBuf,
}

fn private_dir(p: &Path) {
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(p)
        .unwrap();
    fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
}

impl Hk {
    /// State dir + inbox + clients dir + shim; no database.
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        private_dir(&home);
        let state = dir.path().join("xdg-state/codepet");
        private_dir(&state);
        private_dir(&state.join("inbox"));
        private_dir(&state.join("clients"));
        let shim = dir.path().join("xdg-data/codepet/bin/codepet-hook");
        let hk = Self {
            dir,
            state,
            home,
            shim,
        };
        hk.install_shim(Path::new(BIN));
        hk
    }

    /// As [`Hk::new`], plus a schema v5 database holding a living pet (the v0.2-shaped
    /// first-run placeholder; a fresh v0.3 database has no pet until `pet.start`).
    pub fn with_db() -> Self {
        let hk = Self::new();
        drop(
            codepet_storage::Store::open_with_first_run_placeholder(
                &hk.paths(),
                std::sync::Arc::new(codepet_storage::SystemClock),
            )
            .unwrap(),
        );
        let out = hk.cmd(["state"]).output().unwrap();
        assert!(out.status.success(), "codepet state: {out:?}");
        hk
    }

    /// A v0.2-shaped schema v4 database (not yet migrated): the fold refuses it.
    pub fn create_v4_database(&self) {
        codepet_storage::testing::create_v4_database(
            &self.paths(),
            std::sync::Arc::new(codepet_storage::SystemClock),
        )
        .unwrap();
    }

    fn paths(&self) -> codepet_storage::StatePaths {
        let xdg = self.xdg_state();
        codepet_storage::StatePaths::resolve(|k| {
            (k == "XDG_STATE_HOME").then(|| xdg.clone().into())
        })
        .unwrap()
    }

    pub fn xdg_state(&self) -> PathBuf {
        self.state.parent().unwrap().to_path_buf()
    }

    /// `codepet <args>` with this test's HOME and XDG dirs only.
    pub fn cmd<const N: usize>(&self, args: [&str; N]) -> Command {
        let mut c = Command::new(BIN);
        c.args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &self.home)
            .env("XDG_STATE_HOME", self.xdg_state())
            .env("XDG_DATA_HOME", self.dir.path().join("xdg-data"));
        c
    }

    pub fn install_shim(&self, bin: &Path) {
        let text = TEMPLATE
            .replacen("@CODEPET_STATE_DIR@", self.state.to_str().unwrap(), 1)
            .replacen("@CODEPET_BIN@", bin.to_str().unwrap(), 1);
        fs::create_dir_all(self.shim.parent().unwrap()).unwrap();
        fs::write(&self.shim, text).unwrap();
        fs::set_permissions(&self.shim, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// The command a shell client (Claude Code, Codex, Grok) stores.
    pub fn stored_command(&self, client: &str) -> String {
        format!("{} {client} >/dev/null 2>&1 || true", self.shim.display())
    }

    /// Hooks never start a fold while this holds (`fold.backoff` in the future).
    pub fn block_folds(&self) {
        let p = self.state.join("fold.backoff");
        let f = fs::File::create(&p).unwrap();
        f.set_modified(SystemTime::now() + Duration::from_secs(3600))
            .unwrap();
    }

    pub fn unblock_folds(&self) {
        let _ = fs::remove_file(self.state.join("fold.backoff"));
    }

    pub fn inbox_names(&self) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(self.state.join("inbox"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        v.sort();
        v
    }

    pub fn records(&self) -> Vec<Value> {
        self.inbox_names()
            .iter()
            .filter(|n| n.ends_with(".ev"))
            .map(|n| {
                serde_json::from_slice(&fs::read(self.state.join("inbox").join(n)).unwrap())
                    .unwrap()
            })
            .collect()
    }

    fn base(cmd: &mut Command, env: &[(&str, &str)]) {
        cmd.env_clear().env("PATH", "/usr/bin:/bin");
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
    }

    /// `codepet hook --state-dir <state> <client>` with `stdin` written then closed.
    pub fn hook(&self, client: &str, stdin: &[u8], env: &[(&str, &str)]) -> Output {
        let mut c = Command::new(BIN);
        c.args(["hook", "--state-dir"]).arg(&self.state).arg(client);
        Self::base(&mut c, env);
        feed(c.spawn().unwrap(), stdin)
    }

    /// The shim run directly with argv `[shim, client]` (OpenCode/Hermes form).
    pub fn shim(&self, client: &str, stdin: &[u8], env: &[(&str, &str)]) -> Output {
        let mut c = Command::new(&self.shim);
        c.arg(client);
        Self::base(&mut c, env);
        feed(c.spawn().unwrap(), stdin)
    }

    /// The client's stored command string through `/bin/sh -c` (shell clients) or the
    /// argv form (OpenCode, Hermes).
    pub fn stored(&self, client: &str, stdin: &[u8], env: &[(&str, &str)]) -> Output {
        if matches!(client, "opencode" | "hermes") {
            return self.shim(client, stdin, env);
        }
        let mut c = Command::new("/bin/sh");
        c.arg("-c").arg(self.stored_command(client));
        Self::base(&mut c, env);
        feed(c.spawn().unwrap(), stdin)
    }

    /// Waits until `f` holds (polling every 5 ms) or `limit` passes.
    pub fn wait_for(&self, limit: Duration, mut f: impl FnMut() -> bool) -> bool {
        let end = Instant::now() + limit;
        while Instant::now() < end {
            if f() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        f()
    }

    pub fn bar_state(&self) -> Option<Value> {
        let bytes = fs::read(self.state.join("bar-state.json")).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub fn db(&self) -> rusqlite_free::Db {
        rusqlite_free::Db(self.state.join("codepet.sqlite3"))
    }

    /// Live `codepet fold --state-dir <this state>` processes (exact argv match in /proc).
    pub fn fold_processes(&self) -> usize {
        let want = format!("fold\0--state-dir\0{}\0", self.state.display());
        fs::read_dir("/proc")
            .map(|d| {
                d.filter_map(Result::ok)
                    .filter(|e| {
                        e.file_name()
                            .to_string_lossy()
                            .bytes()
                            .all(|b| b.is_ascii_digit())
                    })
                    .filter(|e| {
                        fs::read(e.path().join("cmdline"))
                            .is_ok_and(|c| String::from_utf8_lossy(&c).contains(&want))
                    })
                    .count()
            })
            .unwrap_or(0)
    }
}

/// Writes `stdin` from a thread (a closed pipe is fine), then collects the output.
pub fn feed(mut child: Child, stdin: &[u8]) -> Output {
    let mut pipe = child.stdin.take().unwrap();
    let data = stdin.to_vec();
    let w = std::thread::spawn(move || {
        let _ = pipe.write_all(&data);
    });
    let out = child.wait_with_output().unwrap();
    let _ = w.join();
    out
}

pub fn assert_silent_ok(out: &Output, ctx: &str) {
    assert!(out.status.success(), "{ctx}: status {:?}", out.status);
    assert!(out.stdout.is_empty(), "{ctx}: stdout {:?}", out.stdout);
    assert!(
        out.stderr.is_empty(),
        "{ctx}: stderr {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `@PAD:<n>@` in a fixture's stdin stands for n bytes of "x".
pub fn expand(stdin: &str) -> Vec<u8> {
    let mut out = String::new();
    let mut rest = stdin;
    while let Some(i) = rest.find("@PAD:") {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 5..];
        let end = tail.find('@').unwrap();
        out.push_str(&"x".repeat(tail[..end].parse().unwrap()));
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out.into_bytes()
}

/// Every fixture file of one client folder, in name order.
pub fn fixtures(client: &str) -> Vec<(String, Value)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(client);
    let mut v: Vec<(String, Value)> = fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                serde_json::from_slice(&fs::read(&p).unwrap()).unwrap(),
            )
        })
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

/// Read-only database access for assertions.
pub mod rusqlite_free {
    use rusqlite::{Connection, OpenFlags};
    use std::path::PathBuf;

    pub struct Db(pub PathBuf);
    impl Db {
        fn conn(&self) -> Connection {
            let c = Connection::open_with_flags(&self.0, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
            c.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
            c
        }
        pub fn int(&self, sql: &str) -> i64 {
            self.conn().query_row(sql, [], |r| r.get(0)).unwrap()
        }
        pub fn text(&self, sql: &str) -> Option<String> {
            self.conn().query_row(sql, [], |r| r.get(0)).unwrap()
        }
    }
}
