//! Small Linux helpers: UID, randomness, hashing and supervisor identity. No process spawning.
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

/// Effective UID from `/proc/self/status` (std has no safe accessor).
pub(crate) fn current_uid() -> io::Result<u32> {
    let status = fs::read_to_string("/proc/self/status")?;
    status
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .and_then(|rest| rest.split_whitespace().nth(1))
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| io::Error::other("cannot read effective uid"))
}

/// Home directory of `uid` in passwd(5) text (`name:pw:uid:gid:gecos:home:shell`).
pub(crate) fn passwd_home(passwd: &str, uid: u32) -> Option<PathBuf> {
    passwd.lines().find_map(|line| {
        let f: Vec<&str> = line.split(':').collect();
        (f.len() == 7 && f[2].parse() == Ok(uid) && !f[5].is_empty()).then(|| PathBuf::from(f[5]))
    })
}

pub(crate) fn random_bytes<const N: usize>() -> io::Result<[u8; N]> {
    let mut buf = [0u8; N];
    fs::File::open("/dev/urandom")?.read_exact(&mut buf)?;
    Ok(buf)
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[usize::from(b >> 4)] as char);
        out.push(DIGITS[usize::from(b & 15)] as char);
    }
    out
}

pub(crate) fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

/// Random UUID v4 text.
pub(crate) fn uuid_v4() -> io::Result<String> {
    let mut b = random_bytes::<16>()?;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h = hex(&b);
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    ))
}

/// Identity of a per-job supervisor: PID plus `/proc/<pid>/stat` start time (clock ticks
/// since boot). Both must match for the supervisor to count as alive, so a reused PID never
/// passes as the original process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Supervisor {
    pub pid: u32,
    pub start_time: u64,
}
impl Supervisor {
    pub fn current() -> io::Result<Self> {
        let pid = std::process::id();
        proc_start_time(pid)
            .map(|start_time| Self { pid, start_time })
            .ok_or_else(|| io::Error::other("cannot read own /proc start time"))
    }
    /// Alive only when the PID exists, is not a zombie, and its start time matches.
    pub fn is_alive(&self) -> bool {
        self.pid != 0 && proc_start_time(self.pid) == Some(self.start_time)
    }
}

fn proc_start_time(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // comm (field 2) may contain spaces and parentheses; fields resume after the last ')'.
    let rest = &stat[stat.rfind(')')? + 1..];
    let mut fields = rest.split_whitespace();
    let state = fields.next()?;
    if matches!(state, "Z" | "X" | "x") {
        return None;
    }
    // `state` is field 3; starttime is field 22.
    fields.nth(18)?.parse().ok()
}
