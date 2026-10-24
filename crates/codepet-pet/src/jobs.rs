//! Short child processes started by explicit actions: one `codepet rpc` / `codepet work-rpc`
//! request per action, and fire-and-forget helpers (toast, placement write, work window).
//! Each runs on a short-lived thread that reports back through a channel; a request that
//! does not answer in time, or a helper that outlives its limit, is ended (TERM, then KILL
//! after a short grace), so no stuck child can hold the pet's busy flags. Nothing here
//! polls or runs while idle: a helper's exit is checked only while that helper is alive.

use calloop::channel::Sender;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

const MAX_FRAME: usize = 1 << 20;
/// How long a fire-and-forget helper (toast, placement write, Home) may run.
const HELPER_LIMIT: Duration = Duration::from_secs(10);
/// How long a child asked to stop (TERM) has before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(1);

/// Waits for `child` until `deadline`, checking at a backoff from 5 ms up to 100 ms.
fn wait_until(child: &mut Child, deadline: Instant) -> Option<ExitStatus> {
    let mut pause = Duration::from_millis(5);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) => {}
            Err(_) => return None,
        }
        let now = Instant::now();
        if now >= deadline {
            return None;
        }
        std::thread::sleep(pause.min(deadline - now));
        pause = (pause * 2).min(Duration::from_millis(100));
    }
}

/// Asks a running child to stop, kills it after [`STOP_GRACE`], and reaps it.
fn stop(child: &mut Child) {
    // The child is not reaped yet, so its PID cannot have been reused.
    if let Some(pid) = rustix::process::Pid::from_raw(child.id() as i32) {
        let _ = rustix::process::kill_process(pid, rustix::process::Signal::TERM);
    }
    if wait_until(child, Instant::now() + STOP_GRACE).is_none() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Runs a helper (no shell, output discarded) for at most `limit`; `None` when it could
/// not start or had to be ended.
fn run_helper(argv: &[String], limit: Duration) -> Option<ExitStatus> {
    let (cmd, args) = argv.split_first()?;
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let status = wait_until(&mut child, Instant::now() + limit);
    if status.is_none() {
        stop(&mut child);
    }
    status
}

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
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        if let Some(out) = stdout {
            let _ = BufReader::new(out.take(MAX_FRAME as u64)).read_line(&mut line);
        }
        let _ = tx.send(line);
    });
    let line = rx.recv_timeout(timeout).ok();
    if line.is_some() {
        // The answer is in: deliver it now and reap the child without ending it.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    } else {
        stop(&mut child);
    }
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

/// Runs a helper on a thread (no shell, output discarded), ending it after [`HELPER_LIMIT`]
/// so a hung helper (a `qs ipc` call to a stuck shell) cannot hold the placement slot.
pub fn detached(label: &'static str, argv: Vec<String>, tx: Option<Sender<JobResult>>) {
    std::thread::spawn(move || {
        let _ = run_helper(&argv, HELPER_LIMIT);
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
        let _ = child.wait();
    } else if wait_until(&mut child, Instant::now() + timeout).is_none() {
        stop(&mut child);
    }
    got.map(|s| s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn sh(script: &str) -> Vec<String> {
        vec!["/bin/sh".into(), "-c".into(), script.into()]
    }

    fn code(v: &Value) -> &str {
        v.pointer("/error/code")
            .and_then(Value::as_str)
            .unwrap_or("")
    }

    #[test]
    fn request_line_reads_one_answer_and_names_failures() {
        let ok = request_line(
            &sh(r#"read -r l; printf '{"ok":true,"id":"a"}\n'"#),
            "{}",
            Duration::from_secs(5),
        );
        assert_eq!(ok["ok"], Value::Bool(true));
        let silent = request_line(&sh("read -r l"), "{}", Duration::from_secs(5));
        assert_eq!(code(&silent), "TRANSPORT");
        let garbage = request_line(&sh("read -r l; echo nope"), "{}", Duration::from_secs(5));
        assert_eq!(code(&garbage), "PROTOCOL_MISMATCH");
        let missing = request_line(
            &["/nonexistent/codepet".to_string()],
            "{}",
            Duration::from_secs(5),
        );
        assert_eq!(code(&missing), "TRANSPORT");
    }

    #[test]
    fn an_answer_is_returned_without_waiting_for_the_child_to_exit() {
        let started = Instant::now();
        let v = request_line(
            &sh(r#"read -r l; printf '{"ok":true}\n'; exec sleep 8"#),
            "{}",
            Duration::from_secs(5),
        );
        assert_eq!(v["ok"], Value::Bool(true));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_timed_out_request_ends_even_a_child_that_ignores_term() {
        let started = Instant::now();
        let v = request_line(
            &sh("trap '' TERM; exec sleep 30"),
            "{}",
            Duration::from_millis(200),
        );
        assert_eq!(code(&v), "CLIENT_TIMEOUT");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_hung_helper_is_ended_after_its_limit() {
        let started = Instant::now();
        let status = run_helper(
            &sh("trap '' TERM; exec sleep 30"),
            Duration::from_millis(200),
        );
        assert!(
            status.is_none(),
            "a helper over its limit is reported as ended"
        );
        assert!(started.elapsed() < Duration::from_secs(5));
        let done = run_helper(&sh("exit 3"), Duration::from_secs(5));
        assert_eq!(done.and_then(|s| s.code()), Some(3));
    }
}
