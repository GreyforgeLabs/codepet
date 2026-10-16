//! A throwaway HOME with stand-ins for the five coding clients (SPEC-v0.3 §9.2
//! "Installers"). Every command runs the built `codepet` with a cleared environment: only
//! `HOME`, `XDG_STATE_HOME`, `PATH` (the fake bin dir, then `/usr/bin:/bin`) and `LANG`,
//! so no real client, client config dir or owner state can be reached. The stand-ins for
//! `claude` and `hermes` reproduce the config-file behaviour of the real CLIs (see their
//! docstrings); Codex, OpenCode and Grok are never run by a Connect (their versions come
//! from install paths), and their stand-ins log any call and fail.
#![allow(dead_code)]
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tempfile::TempDir;

pub const BIN: &str = env!("CARGO_BIN_EXE_codepet");
const FAKE_CLAUDE: &str = include_str!("fake_claude.py");
const FAKE_HERMES: &str = include_str!("fake_hermes.py");
const FAKE_CLIENT: &str = include_str!("fake_client.sh");
pub const COMMANDS: &str = include_str!("../../../../docs/v0.3/contracts/commands.json");

pub struct Home {
    dir: TempDir,
    next: AtomicU64,
    extra_env: Vec<(String, String)>,
}

fn exe(p: &Path, body: &str) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, body).unwrap();
    fs::set_permissions(p, fs::Permissions::from_mode(0o755)).unwrap();
}

pub fn python_has_yaml() -> bool {
    Command::new("python3")
        .args(["-c", "import yaml"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

impl Home {
    pub fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("codepet-clients-")
            .tempdir()
            .unwrap();
        fs::create_dir_all(dir.path().join("home/bin")).unwrap();
        fs::create_dir_all(dir.path().join("state")).unwrap();
        Self {
            dir,
            next: AtomicU64::new(1),
            extra_env: Vec::new(),
        }
    }
    pub fn home(&self) -> PathBuf {
        self.dir.path().join("home")
    }
    pub fn state_dir(&self) -> PathBuf {
        self.dir.path().join("state/codepet")
    }
    pub fn data_dir(&self) -> PathBuf {
        self.home().join(".local/share/codepet")
    }
    pub fn shim(&self) -> PathBuf {
        self.data_dir().join("bin/codepet-hook")
    }
    pub fn set_env(&mut self, k: &str, v: &str) {
        self.extra_env.push((k.to_owned(), v.to_owned()));
    }
    pub fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(BIN);
        c.args(args)
            .env_clear()
            .env("HOME", self.home())
            .env("XDG_STATE_HOME", self.dir.path().join("state"))
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.home().join("bin").display()),
            )
            .env("LANG", "C.UTF-8")
            .current_dir(self.dir.path())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in &self.extra_env {
            c.env(k, v);
        }
        c
    }
    pub fn run(&self, args: &[&str]) -> Output {
        self.cmd(args).output().unwrap()
    }
    pub fn rpc_with(&self, id: &str, method: &str, params: Value, envs: &[(&str, &str)]) -> Value {
        let frame = json!({"v": 1, "id": id, "method": method, "params": params}).to_string();
        let mut c = self.cmd(&["rpc"]);
        c.stdin(Stdio::piped());
        for (k, v) in envs {
            c.env(k, v);
        }
        let mut child = c.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(format!("{frame}\n").as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        let text = String::from_utf8(out.stdout).unwrap();
        serde_json::from_str(text.trim())
            .unwrap_or_else(|e| panic!("{e}: {text:?} {:?}", String::from_utf8_lossy(&out.stderr)))
    }
    pub fn rpc(&self, method: &str, params: Value) -> Value {
        let id = format!("t{}", self.next.fetch_add(1, Ordering::SeqCst));
        self.rpc_with(&id, method, params, &[])
    }
    /// A successful result.
    pub fn ok(&self, method: &str, params: Value) -> Value {
        let r = self.rpc(method, params);
        assert_eq!(r["ok"], true, "{method}: {r}");
        r["result"].clone()
    }
    pub fn err_code(&self, method: &str, params: Value) -> String {
        let r = self.rpc(method, params);
        assert_eq!(r["ok"], false, "{method}: {r}");
        r["error"]["code"].as_str().unwrap().to_owned()
    }
    pub fn journal(&self, job: &str) -> Option<Value> {
        let t = fs::read_to_string(self.state_dir().join(format!("jobs/{job}.json"))).ok()?;
        serde_json::from_str(&t).ok()
    }
    /// Waits for a job's journal to be done (30 s).
    pub fn wait_job(&self, job: &str) -> Value {
        let started = Instant::now();
        loop {
            if let Some(j) = self.journal(job).filter(|j| j["done"] == true) {
                return j;
            }
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "job {job} never finished: {:?}",
                self.journal(job)
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    /// `clients.connect` and wait.
    pub fn connect(&self, ids: &[&str]) -> Value {
        let r = self.ok("clients.connect", json!({ "clients": ids }));
        self.wait_job(r["job_id"].as_str().unwrap())
    }
    pub fn disconnect(&self, id: &str) -> Value {
        let r = self.ok("clients.disconnect", json!({ "client": id }));
        self.wait_job(r["job_id"].as_str().unwrap())
    }
    pub fn result_of<'a>(&self, j: &'a Value, id: &str) -> &'a Value {
        j["results"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id)
            .unwrap_or_else(|| panic!("{id} in {j}"))
    }
    pub fn bar(&self) -> Value {
        serde_json::from_str(&fs::read_to_string(self.state_dir().join("bar-state.json")).unwrap())
            .unwrap()
    }
    pub fn status_of(&self, id: &str) -> Value {
        let s = self.ok("clients.status", json!({}));
        s["clients"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id)
            .unwrap()
            .clone()
    }
    pub fn calls(&self, name: &str) -> Vec<Value> {
        fs::read_to_string(self.home().join(name))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
    pub fn unexpected_calls(&self) -> String {
        fs::read_to_string(self.home().join("unexpected-calls.log")).unwrap_or_default()
    }
    /// The frozen reference text with this home's shim path.
    pub fn with_shim(&self, reference: &str) -> String {
        reference.replace(
            "/home/u/.local/share/codepet/bin/codepet-hook",
            self.shim().to_str().unwrap(),
        )
    }

    // ---- client stand-ins ----
    pub fn install_claude(&self) {
        exe(&self.home().join("bin/claude"), FAKE_CLAUDE);
        fs::create_dir_all(self.home().join(".claude")).unwrap();
    }
    pub fn install_hermes(&self) {
        let agent = self.home().join(".hermes/hermes-agent");
        fs::create_dir_all(&agent).unwrap();
        fs::write(
            agent.join("pyproject.toml"),
            "[project]\nname = \"hermes-agent\"\nversion = \"0.21.3\"\n",
        )
        .unwrap();
        fs::create_dir_all(self.home().join(".hermes/plugins")).unwrap();
        exe(&self.home().join("bin/hermes"), FAKE_HERMES);
    }
    pub fn install_codex(&self, version: &str) {
        let real = self.home().join(format!(
            ".codex/packages/standalone/releases/{version}-x86_64-unknown-linux-musl/bin/codex"
        ));
        exe(&real, FAKE_CLIENT);
        symlink(&real, self.home().join("bin/codex")).unwrap();
    }
    pub fn install_opencode(&self) {
        let real = self.home().join("opt/opencode/1.19.0/opencode");
        exe(&real, FAKE_CLIENT);
        symlink(&real, self.home().join("bin/opencode")).unwrap();
        fs::create_dir_all(self.home().join(".config/opencode")).unwrap();
    }
    pub fn install_grok(&self) {
        let real = self.home().join(".grok/downloads/grok-1.0.52-linux-x86_64");
        exe(&real, FAKE_CLIENT);
        fs::create_dir_all(self.home().join(".grok/bin")).unwrap();
        symlink(
            "../downloads/grok-1.0.52-linux-x86_64",
            self.home().join(".grok/bin/grok"),
        )
        .unwrap();
    }
}

/// `(relative path, bytes)` of every regular file under `dir`.
pub fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    fn walk(root: &Path, d: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        let Ok(rd) = fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, out);
            } else if let Ok(b) = fs::read(&p) {
                out.push((p.strip_prefix(root).unwrap().display().to_string(), b));
            }
        }
    }
    walk(dir, dir, &mut out);
    out.sort();
    out
}
