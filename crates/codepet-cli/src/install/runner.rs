//! Running a client's own CLI inside a clients job (spec §5.2 "Client CLI rules"): stdin
//! `/dev/null`; a minimal environment (`PATH`, `HOME`, the config-dir overrides in effect,
//! plus `DISABLE_AUTOUPDATER=1` and `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` for
//! Claude Code); its own process group; a 10 s timeout that kills the group. The job runs
//! one CLI at a time. Output is kept bounded and never logged whole.
use super::Env;
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The CLI timeout.
pub const CLI_TIMEOUT: Duration = Duration::from_secs(10);
/// Bytes of stdout kept (enough for a `--json` result).
const MAX_OUTPUT: usize = 64 * 1024;

/// One CLI run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CliRun {
    /// The exit code; `None` when killed (timeout) or ended by a signal.
    pub code: Option<i32>,
    pub stdout: String,
    pub timed_out: bool,
}
impl CliRun {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }
}

/// Runs `program args` under the CLI rules; `extra` adds variables (Claude's two).
pub fn run(
    env: &Env,
    program: &Path,
    args: &[&str],
    extra: &[(&str, &str)],
) -> std::io::Result<CliRun> {
    run_with_timeout(env, program, args, extra, CLI_TIMEOUT)
}

pub fn run_with_timeout(
    env: &Env,
    program: &Path,
    args: &[&str],
    extra: &[(&str, &str)],
    timeout: Duration,
) -> std::io::Result<CliRun> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .env_clear()
        .env("PATH", &env.path_var)
        .env("HOME", &env.home)
        .current_dir(&env.home)
        .stderr(Stdio::null());
    for (k, v) in &env.overrides {
        cmd.env(k, v);
    }
    for (k, v) in extra {
        cmd.env(k, v);
    }
    let c = capture(cmd, timeout, MAX_OUTPUT, false)?;
    Ok(CliRun {
        code: if c.timed_out { None } else { c.code },
        stdout: String::from_utf8_lossy(&c.stdout).into_owned(),
        timed_out: c.timed_out,
    })
}

/// What [`capture`] saw.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Captured {
    /// The exit code; `None` when killed or ended by a signal.
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub timed_out: bool,
}

/// How long the pipes may stay open after the process ended (a helper that left the
/// group with `setsid` and kept stdout): reading then stops and the pipe is dropped.
const DRAIN_GRACE: Duration = Duration::from_millis(500);

/// A pipe being drained by a thread: what arrived so far, and whether it reached EOF.
struct Drain {
    buf: Arc<Mutex<Vec<u8>>>,
    eof: std::sync::mpsc::Receiver<()>,
}

/// Reads `r` until EOF on a thread, keeping at most `cap` bytes as they arrive.
fn drain(mut r: impl Read + Send + 'static, cap: usize) -> Drain {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let (tx, eof) = std::sync::mpsc::channel();
    let shared = Arc::clone(&buf);
    std::thread::spawn(move || {
        let mut chunk = [0u8; 8192];
        loop {
            match r.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let mut b = shared.lock().unwrap_or_else(|e| e.into_inner());
                    if b.len() < cap {
                        let take = n.min(cap - b.len());
                        b.extend_from_slice(&chunk[..take]);
                    }
                }
            }
        }
        let _ = tx.send(());
    });
    Drain { buf, eof }
}

impl Drain {
    /// Waits for EOF until `until`, then takes what arrived (the thread, if still
    /// blocked on a pipe an escaped process holds, is left behind).
    fn take(self, until: Instant) -> Vec<u8> {
        let _ = self
            .eof
            .recv_timeout(until.saturating_duration_since(Instant::now()));
        std::mem::take(&mut *self.buf.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

/// Runs `cmd` in its own process group with stdin `/dev/null` and stdout (and, with
/// `with_stderr`, stderr) piped and drained by threads, each capped at `cap` bytes. After
/// `timeout` the whole group is killed. Never blocks on a pipe: reading stops
/// [`DRAIN_GRACE`] after the process ended (or was killed) and whatever arrived is kept,
/// so a process that escaped the group while holding a pipe cannot hang the caller.
pub fn capture(
    mut cmd: Command,
    timeout: Duration,
    cap: usize,
    with_stderr: bool,
) -> std::io::Result<Captured> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .process_group(0);
    if with_stderr {
        cmd.stderr(Stdio::piped());
    }
    let mut child = cmd.spawn()?;
    let pgid = child.id();
    let out = child.stdout.take().map(|o| drain(o, cap));
    let err = child.stderr.take().map(|e| drain(e, cap));
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(None) | Err(_) => {
                timed_out = true;
                kill_group(pgid);
                let _ = child.kill();
                break child.wait().ok();
            }
        }
    };
    // Whatever the process left in its group (a daemon it forked) goes with it.
    kill_group(pgid);
    let until = Instant::now() + DRAIN_GRACE;
    let stdout = out.map(|d| d.take(until)).unwrap_or_default();
    let stderr = err.map(|d| d.take(until)).unwrap_or_default();
    Ok(Captured {
        code: status.and_then(|s| s.code()),
        stdout,
        stderr,
        timed_out,
    })
}

fn kill_group(pgid: u32) {
    if let Some(pid) = i32::try_from(pgid)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    {
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn env() -> Env {
        Env::resolve(|k| match k {
            "HOME" => Some(OsString::from("/tmp")),
            "PATH" => Some(OsString::from("/usr/bin:/bin")),
            "SECRET" => Some(OsString::from("x")),
            _ => None,
        })
        .unwrap()
    }

    #[test]
    fn minimal_environment_and_stdin_null() {
        let r = run(
            &env(),
            Path::new("/bin/sh"),
            &["-c", "env | sort; cat"],
            &[("DISABLE_AUTOUPDATER", "1")],
        )
        .unwrap();
        assert!(r.ok(), "{r:?}");
        let keys: Vec<&str> = r
            .stdout
            .lines()
            .filter_map(|l| l.split_once('=').map(|(k, _)| k))
            .collect();
        for k in &keys {
            assert!(
                ["PATH", "HOME", "DISABLE_AUTOUPDATER", "PWD", "SHLVL", "_"].contains(k),
                "{k}"
            );
        }
        assert!(keys.contains(&"DISABLE_AUTOUPDATER"));
    }

    #[test]
    fn a_helper_that_escapes_the_group_with_the_pipe_never_hangs_the_job() {
        // `setsid` puts the sleeper in its own session: the group kill misses it and it
        // keeps stdout open. The run still ends (the pipe is dropped after the grace).
        let started = Instant::now();
        let r = run_with_timeout(
            &env(),
            Path::new("/bin/sh"),
            &["-c", "echo '{\"outcome\":\"ok\"}'; setsid sleep 5 & exit 0"],
            &[],
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(r.ok() && !r.timed_out, "{r:?}");
        assert!(r.stdout.contains("outcome"), "{r:?}");
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn capture_caps_both_pipes_and_reads_past_the_pipe_buffer() {
        let mut cmd = Command::new("/bin/sh");
        cmd.args([
            "-c",
            "head -c 200000 /dev/zero; head -c 100000 /dev/zero >&2",
        ]);
        let c = capture(cmd, Duration::from_secs(5), 1000, true).unwrap();
        assert_eq!((c.code, c.timed_out), (Some(0), false));
        assert_eq!((c.stdout.len(), c.stderr.len()), (1000, 1000));
    }

    #[test]
    fn a_stuck_cli_is_killed_with_its_group() {
        let started = Instant::now();
        let r = run_with_timeout(
            &env(),
            Path::new("/bin/sh"),
            &["-c", "sleep 30 & sleep 30"],
            &[],
            Duration::from_millis(300),
        )
        .unwrap();
        assert!(r.timed_out && r.code.is_none());
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the group's pipe holders died too"
        );
    }
}
