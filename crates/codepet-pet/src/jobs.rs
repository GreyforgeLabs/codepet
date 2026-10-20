//! Short child processes started by explicit actions: one `codepet rpc` / `codepet work-rpc`
//! request per action, and fire-and-forget helpers (toast, placement write, work window).
//! Each runs on a short-lived thread that reports back through a channel; a request that
//! does not answer in time is ended. Nothing here polls or runs while idle.

use calloop::channel::Sender;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

const MAX_FRAME: usize = 1 << 20;

#[derive(Debug)]
pub enum JobResult {
    /// A care request's answer: (action, response envelope).
    Care(String, Value),
    /// A work-rpc request finished (the pet re-reads work-state.json).
    Work,
    /// A detached helper finished.
    Done(&'static str),
}

fn fail(code: &str, message: &str) -> Value {
    serde_json::json!({"ok": false, "error": {"code": code, "message": message}})
}

/// Runs `argv` with one request line on stdin and reads one response line (bounded time).
fn request_line(argv: &[String], frame: &str, timeout: Duration) -> Value {
    let Some((cmd, args)) = argv.split_first() else {
        return fail("CLIENT_ERROR", "No command.");
    };
    let child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(_) => return fail("TRANSPORT", "Could not start CodePet."),
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(frame.as_bytes());
        let _ = stdin.write_all(b"\n");
    }
    let stdout = child.stdout.take();
    let pid = child.id();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        if let Some(out) = stdout {
            let _ = BufReader::new(out.take(MAX_FRAME as u64)).read_line(&mut line);
        }
        let _ = tx.send(line);
    });
    let line = rx.recv_timeout(timeout).ok();
    if line.is_none() {
        let _ = rustix::process::kill_process(
            rustix::process::Pid::from_raw(pid as i32).unwrap_or(rustix::process::Pid::INIT),
            rustix::process::Signal::TERM,
        );
    }
    let _ = child.wait();
    match line {
        None => fail("CLIENT_TIMEOUT", "CodePet did not answer in time."),
        Some(l) if l.trim().is_empty() => fail("TRANSPORT", "CodePet exited without an answer."),
        Some(l) => serde_json::from_str::<Value>(l.trim())
            .ok()
            .filter(|v| v.get("ok").is_some_and(Value::is_boolean))
            .unwrap_or_else(|| {
                fail(
                    "PROTOCOL_MISMATCH",
                    "CodePet returned an unreadable answer.",
                )
            }),
    }
}

/// One `codepet rpc` request (v1 envelope) on a worker thread.
pub fn rpc(
    bin: &str,
    id: &str,
    method: &str,
    params: Value,
    action: String,
    tx: Sender<JobResult>,
) {
    let frame =
        serde_json::json!({"v": 1, "id": id, "method": method, "params": params}).to_string();
    let argv = vec![bin.to_string(), "rpc".to_string()];
    let id = id.to_string();
    std::thread::spawn(move || {
        let mut resp = request_line(&argv, &frame, Duration::from_secs(15));
        if resp.get("ok") == Some(&Value::Bool(true))
            && resp.get("id").and_then(Value::as_str) != Some(id.as_str())
        {
            resp = fail("PROTOCOL_MISMATCH", "Unexpected response envelope.");
        }
        let _ = tx.send(JobResult::Care(action, resp));
    });
}

/// One `codepet work-rpc` request on a worker thread.
pub fn work_rpc(bin: &str, method: &str, params: Value, tx: Sender<JobResult>) {
    let frame = serde_json::json!({"method": method, "params": params}).to_string();
    let argv = vec![bin.to_string(), "work-rpc".to_string()];
    std::thread::spawn(move || {
        let _ = request_line(&argv, &frame, Duration::from_secs(30));
        let _ = tx.send(JobResult::Work);
    });
}

/// Runs a helper to completion on a thread (no shell, output discarded).
pub fn detached(label: &'static str, argv: Vec<String>, tx: Option<Sender<JobResult>>) {
    std::thread::spawn(move || {
        if let Some((cmd, args)) = argv.split_first() {
            let _ = Command::new(cmd)
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        if let Some(tx) = tx {
            let _ = tx.send(JobResult::Done(label));
        }
    });
}

/// Runs a helper and returns its trimmed stdout (bounded), for the work-window check.
pub fn output(argv: &[&str], timeout: Duration) -> Option<String> {
    let (cmd, args) = argv.split_first()?;
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut out = child.stdout.take()?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut s = String::new();
        let _ = (&mut out).take(65536).read_to_string(&mut s);
        let _ = tx.send(s);
    });
    let got = rx.recv_timeout(timeout).ok();
    if got.is_none() {
        let _ = child.kill();
    }
    let _ = child.wait();
    got.map(|s| s.trim().to_string())
}
