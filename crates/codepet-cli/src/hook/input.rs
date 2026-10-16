//! Bounded stdin for the hook (spec §5.1 step 1): read at most 64 KiB, then drain to EOF,
//! all within one deadline, with `poll(2)` so a client that never closes stdin cannot hold
//! the hook. Raw `read(2)` on fd 0 (no std buffering, so `poll` sees every byte).
use rustix::event::{poll, PollFd, PollFlags};
use rustix::io::Errno;
use rustix::time::Timespec;
use std::os::fd::AsFd;
use std::time::Instant;

/// What one bounded read produced.
#[derive(Debug, Default)]
pub struct Read {
    pub bytes: Vec<u8>,
    /// The writer closed stdin (or stdin is unusable).
    pub eof: bool,
    /// The read stopped at the size cap; more input may follow.
    pub capped: bool,
}

/// Waits until stdin is readable or `deadline` passes; `false` on timeout.
fn wait(deadline: Instant) -> bool {
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return false;
        }
        let stdin = std::io::stdin();
        let fd = stdin.as_fd();
        let mut fds = [PollFd::new(&fd, PollFlags::IN)];
        let ts = Timespec {
            tv_sec: i64::try_from(left.as_secs()).unwrap_or(i64::MAX),
            tv_nsec: i64::from(left.subsec_nanos()),
        };
        match poll(&mut fds, Some(&ts)) {
            Ok(0) => return false,
            Ok(_) => return true,
            Err(Errno::INTR) => continue,
            // An unusable stdin reads as EOF below.
            Err(_) => return true,
        }
    }
}

/// One `read(2)` from fd 0 into `buf`: `Some(0)` = EOF, `None` = retry (EINTR/EAGAIN).
fn read_once(buf: &mut [u8]) -> Option<usize> {
    let stdin = std::io::stdin();
    match rustix::io::read(stdin.as_fd(), buf) {
        Ok(n) => Some(n),
        Err(Errno::INTR | Errno::AGAIN) => None,
        Err(_) => Some(0),
    }
}

/// Reads until EOF, `cap` bytes or `deadline`.
pub fn read_bounded(cap: usize, deadline: Instant) -> Read {
    let mut out = Read {
        bytes: Vec::with_capacity(4096),
        ..Read::default()
    };
    let mut chunk = [0u8; 16 * 1024];
    while out.bytes.len() < cap {
        if !wait(deadline) {
            return out;
        }
        let want = (cap - out.bytes.len()).min(chunk.len());
        match read_once(&mut chunk[..want]) {
            Some(0) => {
                out.eof = true;
                return out;
            }
            Some(n) => out.bytes.extend_from_slice(&chunk[..n]),
            None => continue,
        }
    }
    out.capped = true;
    out
}

/// Discards stdin until EOF or `deadline`, so a client still writing never meets a
/// closed pipe while the hook lives.
pub fn drain(deadline: Instant) {
    let mut chunk = [0u8; 64 * 1024];
    while wait(deadline) {
        if read_once(&mut chunk) == Some(0) {
            return;
        }
    }
}
