//! `codepet hook` and the frozen shim (SPEC-v0.3 §5.1, §5.8, §9.1, K10): adapter fixtures
//! per client, hostile inputs, and every shim early exit. Each run asserts exit 0 with
//! empty stdout and stderr.
mod hookkit;

use hookkit::{assert_silent_ok, expand, feed, fixtures, Hk, BIN, CLIENTS, TOMBSTONE};
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn env_of(fx: &Value) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = fx["env"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
        .collect();
    env.push((
        "CODEPET_TEST_NOW_MS".into(),
        fx["at_ms"].as_i64().unwrap().to_string(),
    ));
    env
}

fn write_nonce(hk: &Hk, nonce: &str) {
    let p = hk.state.join("test.nonce");
    fs::write(&p, format!("{nonce}\n")).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
}

/// Every fixture through `codepet hook` directly: the record (or the drop) the oracle
/// (the contract's reference model) computed, under the expected event id.
#[test]
fn adapter_fixtures_normalize_like_the_reference_model() {
    let mut seen = 0;
    for client in CLIENTS {
        let fxs = fixtures(client);
        assert!(fxs.len() >= 15, "{client}: {} fixtures", fxs.len());
        for (name, fx) in fxs {
            let ctx = format!("{client}/{name}");
            assert_eq!(fx["client"], client, "{ctx}");
            let hk = Hk::new();
            hk.block_folds();
            if let Some(n) = fx["nonce"].as_str() {
                write_nonce(&hk, n);
            }
            let env = env_of(&fx);
            let env: Vec<(&str, &str)> =
                env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            let out = hk.hook(client, &expand(fx["stdin"].as_str().unwrap()), &env);
            assert_silent_ok(&out, &ctx);
            let records = hk.records();
            let expect = &fx["expect"];
            if let Some(reason) = expect["drop"].as_str() {
                assert!(
                    records.is_empty(),
                    "{ctx}: dropped ({reason}) but wrote {records:?}"
                );
                assert_eq!(hk.inbox_names().len(), 0, "{ctx}: nothing left behind");
            } else {
                let id = expect["event_id"].as_str().unwrap();
                assert_eq!(hk.inbox_names(), [format!("{id}.ev")], "{ctx}: event id");
                assert_eq!(records[0], expect["record"], "{ctx}");
            }
            seen += 1;
        }
    }
    assert!(seen >= 75);
}

/// K10: every fixture through each client's stored command string (or argv) and the real
/// binary prints nothing and exits 0; a duplicate delivery of one firing writes one file.
#[test]
fn stored_commands_are_silent_and_dedupe_duplicate_deliveries() {
    for client in CLIENTS {
        let hk = Hk::new();
        hk.block_folds();
        let mut expected = std::collections::BTreeSet::new();
        for (name, fx) in fixtures(client) {
            match fx["nonce"].as_str() {
                Some(n) => write_nonce(&hk, n),
                None => {
                    let _ = fs::remove_file(hk.state.join("test.nonce"));
                }
            }
            let env = env_of(&fx);
            let env: Vec<(&str, &str)> =
                env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            let stdin = expand(fx["stdin"].as_str().unwrap());
            for _ in 0..2 {
                let out = hk.stored(client, &stdin, &env);
                assert_silent_ok(&out, &format!("{client}/{name}"));
            }
            // Headless Claude stops in the shim unless "count helpers" is on.
            let headless = fx["env"]["CLAUDE_CODE_ENTRYPOINT"]
                .as_str()
                .is_some_and(|e| e.starts_with("sdk"));
            if let Some(id) = fx["expect"]["event_id"].as_str().filter(|_| !headless) {
                expected.insert(format!("{id}.ev"));
            }
        }
        let names: std::collections::BTreeSet<String> = hk.inbox_names().into_iter().collect();
        assert_eq!(names, expected, "{client}: one file per firing");
    }
}

fn claude_stop() -> &'static [u8] {
    br#"{"hook_event_name":"Stop","session_id":"s-1","prompt_id":"p-1"}"#
}

#[test]
fn hostile_inputs_never_hurt_the_client() {
    let hk = Hk::new();
    hk.block_folds();
    let env = [("CLAUDE_CODE_ENTRYPOINT", "cli")];
    // 10 MiB: the first 64 KiB carry the ids, the rest is drained.
    let mut big = br#"{"hook_event_name":"PreToolUse","session_id":"s-big","tool_use_id":"tu","tool_input":{"content":""#.to_vec();
    big.extend(std::iter::repeat_n(b'x', 10 << 20));
    big.extend_from_slice(b"\"}}");
    let t = Instant::now();
    assert_silent_ok(&hk.hook("claude-code", &big, &env), "10 MiB");
    assert!(t.elapsed() < Duration::from_secs(2));
    let r = hk.records();
    assert_eq!(r.len(), 1);
    assert_eq!(
        (r[0]["kind"].as_str(), r[0]["tool"].as_str()),
        (Some("activity"), Some("tu"))
    );
    for name in hk.inbox_names() {
        fs::remove_file(hk.state.join("inbox").join(name)).unwrap();
    }
    let hostile: Vec<(&str, Vec<u8>)> = vec![
        (
            "non-utf8",
            b"{\"hook_event_name\":\"Stop\",\"session_id\":\"\xff\xfe\"}".to_vec(),
        ),
        ("garbage", b"\x00\x01\x02 not json".to_vec()),
        ("empty", Vec::new()),
        ("array", b"[1,2,3]".to_vec()),
        ("deep", {
            let mut v = b"{\"x\":".to_vec();
            v.extend(std::iter::repeat_n(b'[', 200_000));
            v
        }),
        ("missing ids", br#"{"hook_event_name":"Stop"}"#.to_vec()),
        (
            "slash",
            br#"{"hook_event_name":"Stop","session_id":"a/b"}"#.to_vec(),
        ),
        (
            "trailing",
            br#"{"hook_event_name":"Stop","session_id":"s"} trailing"#.to_vec(),
        ),
        (
            "spoof",
            br#"{"hook_event_name":"Stop","prompt":"\"session_id\":\"x\""}"#.to_vec(),
        ),
    ];
    for (name, bytes) in hostile {
        assert_silent_ok(&hk.hook("claude-code", &bytes, &env), name);
        assert!(hk.records().is_empty(), "{name}: dropped");
    }
    // Bad arguments are ignored silently too.
    for args in [
        vec!["hook"],
        vec!["hook", "--state-dir", "rel", "codex"],
        vec!["hook", "--state-dir", "/x", "crush"],
    ] {
        let mut c = Command::new(BIN);
        c.args(&args)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        assert_silent_ok(
            &feed(c.spawn().unwrap(), claude_stop()),
            &format!("{args:?}"),
        );
    }
    assert_eq!(hk.inbox_names().len(), 0);
    // ".." matches the id alphabet ([A-Za-z0-9._:-]) and is kept: ids never name a path
    // (the file name is the hex event id) and the database keeps only their HMACs.
    let dots = br#"{"hook_event_name":"Stop","session_id":"..","prompt_id":"."}"#;
    assert_silent_ok(&hk.hook("claude-code", dots, &env), "dots");
    let names = hk.inbox_names();
    assert_eq!(names.len(), 1);
    assert!(names[0].len() == 35 && names[0].ends_with(".ev"));
}

/// stdin that never closes: the hook (and the shim's Off path) still exit within 0.4 s.
#[test]
fn stdin_never_closed_is_bounded() {
    let hk = Hk::new();
    hk.block_folds();
    let run = |cmd: &mut Command| {
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().unwrap();
        let mut pipe = child.stdin.take().unwrap();
        pipe.write_all(claude_stop()).unwrap();
        let t = Instant::now();
        let out = child.wait_with_output().unwrap();
        let took = t.elapsed();
        drop(pipe);
        (out, took)
    };
    let mut hook = Command::new(BIN);
    hook.args(["hook", "--state-dir"])
        .arg(&hk.state)
        .arg("claude-code")
        .env_clear();
    let (out, took) = run(&mut hook);
    assert_silent_ok(&out, "hook");
    assert!(took < Duration::from_millis(400), "hook took {took:?}");
    assert_eq!(
        hk.records().len(),
        1,
        "the record is written even when stdin stays open"
    );
    fs::write(hk.state.join("off"), b"").unwrap();
    // Through the stored command a shell client runs (see shim_direct_run_job_message for
    // the direct form).
    let mut shim = Command::new("/bin/sh");
    shim.arg("-c")
        .arg(hk.stored_command("codex"))
        .env_clear()
        .env("PATH", "/usr/bin:/bin");
    let (out, took) = run(&mut shim);
    assert_silent_ok(&out, "off shim");
    assert!(took < Duration::from_millis(400), "off shim took {took:?}");
    fs::remove_file(hk.state.join("off")).unwrap();
    fs::remove_file(&hk.shim).unwrap();
    // Missing binary with stdin never closed: the stored command still ends quickly.
    let hk2 = Hk::new();
    hk2.install_shim(std::path::Path::new("/nonexistent/codepet"));
    let mut sh = Command::new("/bin/sh");
    sh.arg("-c")
        .arg(hk2.stored_command("codex"))
        .env_clear()
        .env("PATH", "/usr/bin:/bin");
    let (out, took) = run(&mut sh);
    assert_silent_ok(&out, "missing binary");
    assert!(
        took < Duration::from_millis(400),
        "missing binary took {took:?}"
    );
}

#[cfg(debug_assertions)]
#[test]
fn panics_and_stuck_hooks_end_in_silent_exit_zero() {
    let hk = Hk::new();
    hk.block_folds();
    let out = hk.hook(
        "claude-code",
        claude_stop(),
        &[("CODEPET_FAULT", "panic_in_hook")],
    );
    assert_silent_ok(&out, "panic");
    let t = Instant::now();
    let out = hk.hook(
        "claude-code",
        claude_stop(),
        &[("CODEPET_FAULT", "stuck_in_hook")],
    );
    assert_silent_ok(&out, "stuck");
    let took = t.elapsed();
    assert!(
        took >= Duration::from_millis(900) && took < Duration::from_millis(1800),
        "{took:?}"
    );
    assert!(hk.records().is_empty());
}

#[test]
fn unsafe_or_missing_state_never_creates_anything() {
    // A missing state dir: nothing is created, through the hook or the shim.
    let hk = Hk::new();
    fs::remove_dir_all(&hk.state).unwrap();
    assert_silent_ok(&hk.hook("codex", claude_stop(), &[]), "hook");
    assert_silent_ok(&hk.stored("codex", claude_stop(), &[]), "stored");
    assert!(!hk.state.exists());
    // A symlinked inbox or a group-readable one is refused.
    let hk = Hk::new();
    hk.block_folds();
    let elsewhere = hk.dir.path().join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    fs::set_permissions(&elsewhere, fs::Permissions::from_mode(0o700)).unwrap();
    fs::remove_dir(hk.state.join("inbox")).unwrap();
    symlink(&elsewhere, hk.state.join("inbox")).unwrap();
    assert_silent_ok(
        &hk.hook("claude-code", claude_stop(), &[]),
        "symlinked inbox",
    );
    assert_eq!(fs::read_dir(&elsewhere).unwrap().count(), 0);
    fs::remove_file(hk.state.join("inbox")).unwrap();
    fs::create_dir(hk.state.join("inbox")).unwrap();
    fs::set_permissions(hk.state.join("inbox"), fs::Permissions::from_mode(0o750)).unwrap();
    assert_silent_ok(&hk.hook("claude-code", claude_stop(), &[]), "0750 inbox");
    assert_eq!(hk.inbox_names().len(), 0);
}

#[test]
fn more_than_2048_entries_overflow() {
    let hk = Hk::new();
    hk.block_folds();
    for i in 0..2049 {
        fs::write(hk.state.join("inbox").join(format!("{i:032x}.ev")), b"x").unwrap();
    }
    assert_silent_ok(&hk.hook("claude-code", claude_stop(), &[]), "overflow");
    assert!(hk.state.join("inbox/.overflow").exists());
    assert_eq!(hk.inbox_names().len(), 2050);
}

/// The shim's early exits (§5.1): each stops before the binary. A logging fake binary
/// proves which runs reached it.
#[test]
fn shim_early_exits_stop_before_the_binary() {
    let hk = Hk::new();
    let log = hk.dir.path().join("calls");
    let fake = hk.dir.path().join("fake-codepet");
    fs::write(
        &fake,
        format!(
            "#!/bin/sh\necho \"$@\" >> '{}'\ncat >/dev/null\necho noise\necho noise >&2\nexit 3\n",
            log.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
    hk.install_shim(&fake);
    let calls = || {
        let n = fs::read_to_string(&log).map_or(0, |s| s.lines().count());
        let _ = fs::remove_file(&log);
        n
    };
    let run = |client: &str, env: &[(&str, &str)]| {
        assert_silent_ok(
            &hk.stored(client, b"{}\n", env),
            &format!("{client} {env:?}"),
        );
        calls()
    };
    assert_eq!(run("codex", &[]), 1, "a normal event reaches the binary");
    fs::write(hk.state.join("off"), b"").unwrap();
    assert_eq!(run("codex", &[]), 0, "off");
    fs::remove_file(hk.state.join("off")).unwrap();
    fs::write(hk.state.join("clients/hermes.disconnected"), b"").unwrap();
    assert_eq!(run("hermes", &[]), 0, "disconnected");
    assert_eq!(run("opencode", &[]), 1, "only that client stops");
    assert_eq!(
        run("claude-code", &[("GROK_HOOK_EVENT", "stop")]),
        0,
        "Claude hook inside Grok"
    );
    assert_eq!(
        run("grok-cli", &[("GROK_HOOK_EVENT", "stop")]),
        1,
        "Grok's own hook"
    );
    assert_eq!(
        run("claude-code", &[("CLAUDE_CODE_ENTRYPOINT", "sdk-cli")]),
        0,
        "headless"
    );
    fs::write(hk.state.join("helpers"), b"").unwrap();
    assert_eq!(
        run("claude-code", &[("CLAUDE_CODE_ENTRYPOINT", "sdk-cli")]),
        1,
        "helpers on"
    );
    fs::remove_file(hk.state.join("helpers")).unwrap();
    assert_eq!(
        run("claude-code", &[("CLAUDE_CODE_ENTRYPOINT", "cli")]),
        1,
        "interactive"
    );
    fs::remove_dir(hk.state.join("inbox")).unwrap();
    assert_eq!(run("codex", &[]), 0, "no inbox");
    fs::create_dir(hk.state.join("inbox")).unwrap();
    fs::remove_file(&fake).unwrap();
    assert_eq!(run("codex", &[]), 0, "no binary");
    // The shim deleted (an artifact outlived it) or tombstoned: still silent, exit 0.
    fs::remove_file(&hk.shim).unwrap();
    for c in ["claude-code", "codex", "grok-cli"] {
        assert_silent_ok(&hk.stored(c, b"{}\n", &[]), "deleted shim via sh -c");
    }
    fs::write(&hk.shim, TOMBSTONE).unwrap();
    fs::set_permissions(&hk.shim, fs::Permissions::from_mode(0o755)).unwrap();
    for c in hookkit::CLIENTS {
        assert_silent_ok(&hk.stored(c, &vec![b'x'; 100_000], &[]), "tombstone");
    }
}

/// A client that keeps writing after the hook exited sees EPIPE itself; the hook exits 0.
#[test]
fn a_client_writing_after_exit_only_sees_its_own_epipe() {
    let hk = Hk::new();
    fs::write(hk.state.join("off"), b"").unwrap();
    let mut c = Command::new("/bin/sh");
    c.arg("-c")
        .arg(hk.stored_command("codex"))
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let (out, error) = write_until_closed(c.spawn().unwrap());
    assert_silent_ok(&out, "draining shim");
    assert_eq!(error, Some(std::io::ErrorKind::BrokenPipe));
}

/// Writes to the child's stdin for up to 3 s, until a write fails; returns the output and
/// the writer's error.
fn write_until_closed(
    mut child: std::process::Child,
) -> (std::process::Output, Option<std::io::ErrorKind>) {
    let mut pipe = child.stdin.take().expect("piped stdin");
    let writer = std::thread::spawn(move || {
        let chunk = vec![b'x'; 64 * 1024];
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(3) {
            if let Err(e) = pipe.write_all(&chunk) {
                return Some(e.kind());
            }
        }
        None
    });
    let out = child.wait_with_output().unwrap();
    (out, writer.join().unwrap())
}

/// The hook never opens SQLite: a database held under an exclusive lock changes nothing.
#[test]
fn a_locked_database_does_not_slow_the_hook() {
    let hk = Hk::with_db();
    hk.block_folds();
    let c = rusqlite::Connection::open(hk.state.join("codepet.sqlite3")).unwrap();
    c.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let t = Instant::now();
    assert_silent_ok(&hk.hook("claude-code", claude_stop(), &[]), "locked db");
    assert!(t.elapsed() < Duration::from_millis(400));
    assert_eq!(hk.records().len(), 1);
    c.execute_batch("ROLLBACK").unwrap();
}

/// Contract defect found in WP2 (reported, not fixed here: the shim bytes are frozen).
/// When the 0.3 s drain expires, `timeout -s KILL` is killed and bash (`/bin/sh` on this
/// host) prints its job message "Killed ..." on the **shim's own** stderr. The stored
/// commands of shell clients redirect it (`>/dev/null 2>&1 || true`) and the OpenCode and
/// Hermes plugins spawn the shim with stderr ignored, so no client sees it; a direct run
/// does. Proposed fix: `exec >/dev/null 2>&1` as the shim's (and tombstone's) first
/// command. This test pins the argv form's real behaviour: exit 0, empty stdout, bounded.
#[test]
fn shim_direct_run_job_message() {
    let hk = Hk::new();
    fs::write(hk.state.join("off"), b"").unwrap();
    let mut c = Command::new(&hk.shim);
    c.arg("opencode")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = c.spawn().unwrap();
    let pipe = child.stdin.take().unwrap();
    let t = Instant::now();
    let out = child.wait_with_output().unwrap();
    drop(pipe);
    assert!(out.status.success() && out.stdout.is_empty());
    assert!(t.elapsed() < Duration::from_millis(400));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.is_empty() || err.contains("Killed"),
        "only the shell's job message may appear: {err:?}"
    );
}
