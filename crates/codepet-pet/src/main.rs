//! codepet-pet: CodePet's native desktop pet (Wayland layer shell).
//!
//!   codepet-pet run           run in the foreground (one instance per user session)
//!   codepet-pet start         start detached and wait until it answers (≤ 3 s)
//!   codepet-pet ctl VERB ...  send one control verb (show, hide, toggle, status, ping,
//!                             refresh, openHome, openCard, setReducedMotion BOOL,
//!                             selectScreen NAME, setSessionState LOCKED SUSPENDED,
//!                             setFullscreen BOOL, quit)
//!
//! Exit status: 0 success, 1 failure (ctl: not running), 2 usage error.

mod app;
mod jobs;
mod model;
mod paths;
mod render;
mod scene;

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::Duration;

/// sh -c script for `start`: closes inherited descriptors above 2 (only numeric names
/// from /proc/self/fd reach `exec N>&-`), then runs `setsid -f <exe> run`.
const DETACH: &str = r#"for f in /proc/self/fd/*; do n=${f##*/}; case "$n" in *[!0-9]*|'') continue ;; esac; [ "$n" -gt 2 ] && eval "exec $n>&-"; done; exec setsid -f "$0" run"#;

const USAGE: &str = "usage: codepet-pet run | start | ctl VERB [ARGS...] | --version";

fn ctl(paths: &paths::Paths, line: &str, timeout: Duration) -> Option<String> {
    let stream = UnixStream::connect(&paths.socket).ok()?;
    stream.set_read_timeout(Some(timeout)).ok()?;
    stream.set_write_timeout(Some(timeout)).ok()?;
    (&stream).write_all(format!("{line}\n").as_bytes()).ok()?;
    let mut reply = String::new();
    BufReader::new(&stream).read_line(&mut reply).ok()?;
    Some(reply.trim().to_string())
}

/// Binds the control socket, replacing a stale one; None when another pet answers.
fn bind(paths: &paths::Paths) -> Result<Option<UnixListener>, String> {
    match UnixListener::bind(&paths.socket) {
        Ok(l) => Ok(Some(l)),
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            if ctl(paths, "ping", Duration::from_millis(500)).is_some() {
                return Ok(None);
            }
            let meta = std::fs::symlink_metadata(&paths.socket).map_err(|e| e.to_string())?;
            if !std::os::unix::fs::FileTypeExt::is_socket(&meta.file_type()) {
                return Err(format!(
                    "{} exists and is not a socket",
                    paths.socket.display()
                ));
            }
            std::fs::remove_file(&paths.socket).map_err(|e| e.to_string())?;
            UnixListener::bind(&paths.socket)
                .map(Some)
                .map_err(|e| e.to_string())
        }
        Err(e) => Err(format!("cannot bind {}: {e}", paths.socket.display())),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(paths) = paths::Paths::from_env() else {
        eprintln!("codepet-pet: HOME is not set");
        std::process::exit(1);
    };
    match args.first().map(String::as_str) {
        Some("--version") => println!("codepet-pet {}", env!("CARGO_PKG_VERSION")),
        Some("run") => {
            let listener = match bind(&paths) {
                Ok(Some(l)) => l,
                Ok(None) => {
                    eprintln!("codepet-pet: already running");
                    return;
                }
                Err(e) => {
                    eprintln!("codepet-pet: {e}");
                    std::process::exit(1);
                }
            };
            let socket = paths.socket.clone();
            let result = app::run(paths, listener);
            let _ = std::fs::remove_file(&socket);
            match result {
                Ok(reason) => eprintln!("codepet-pet: exiting ({reason})"),
                Err(e) => {
                    eprintln!("codepet-pet: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some("start") => {
            if ctl(&paths, "ping", Duration::from_millis(500)).is_some() {
                return;
            }
            let exe = std::env::current_exe().unwrap_or_else(|_| "codepet-pet".into());
            // A long-lived pet must not keep its starter's open files (a lock, a pipe):
            // close every inherited descriptor above stderr, then detach with setsid.
            let spawned = std::process::Command::new("/bin/sh")
                .arg("-c")
                .arg(DETACH)
                .arg(exe)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            if !spawned.is_ok_and(|s| s.success()) {
                eprintln!("codepet-pet: could not start");
                std::process::exit(1);
            }
            for _ in 0..30 {
                if ctl(&paths, "ping", Duration::from_millis(300)).is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            eprintln!("codepet-pet: did not answer within 3 s");
            std::process::exit(1);
        }
        Some("ctl") if args.len() >= 2 => {
            let line = args[1..].join(" ");
            if line.contains('\n') || line.len() > 1024 {
                eprintln!("{USAGE}");
                std::process::exit(2);
            }
            match ctl(&paths, &line, Duration::from_secs(3)) {
                Some(reply) => {
                    println!("{reply}");
                    if reply.starts_with("error") {
                        std::process::exit(1);
                    }
                }
                None => {
                    eprintln!("codepet-pet: not running");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
}
