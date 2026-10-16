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
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0);
    for (k, v) in &env.overrides {
        cmd.env(k, v);
    }
    for (k, v) in extra {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn()?;
    let pgid = child.id();
    let mut out = child.stdout.take().expect("piped");
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            match out.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if buf.len() < MAX_OUTPUT {
                        let take = n.min(MAX_OUTPUT - buf.len());
                        buf.extend_from_slice(&chunk[..take]);
                    }
                }
            }
        }
        buf
    });
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if started.elapsed() >= timeout {
            timed_out = true;
            kill_group(pgid);
            let _ = child.kill();
            break child.wait().ok();
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    // Whatever the CLI left in its group (a daemon it forked) goes with it.
    kill_group(pgid);
    let stdout = reader.join().unwrap_or_default();
    Ok(CliRun {
        code: if timed_out {
            None
        } else {
            status.and_then(|s| s.code())
        },
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
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
