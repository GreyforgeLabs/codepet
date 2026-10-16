//! The nearest client ancestor's argv (spec §6.4 interactivity). Bounded reads of
//! `/proc/<pid>/stat` and `/proc/<pid>/cmdline` only, at most `ANCESTOR_DEPTH` ancestors;
//! the argv tokens are compared and dropped, never kept or logged.
use codepet_core::agent::ANCESTOR_DEPTH;
use std::fs::File;
use std::io::Read;

const MAX_STAT_BYTES: u64 = 4096;
const MAX_CMDLINE_BYTES: u64 = 16 * 1024;
const MAX_TOKENS: usize = 256;

fn read_bounded(path: &str, cap: u64) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    File::open(path)
        .ok()?
        .take(cap)
        .read_to_end(&mut out)
        .ok()?;
    Some(out)
}

/// `(comm, ppid)` from `/proc/<pid>/stat`; comm may hold spaces and parentheses.
fn comm_and_parent(pid: u32) -> Option<(String, u32)> {
    let stat = read_bounded(&format!("/proc/{pid}/stat"), MAX_STAT_BYTES)?;
    let open = stat.iter().position(|&b| b == b'(')?;
    let close = stat.iter().rposition(|&b| b == b')')?;
    let comm = String::from_utf8_lossy(stat.get(open + 1..close)?).into_owned();
    let rest = std::str::from_utf8(stat.get(close + 1..)?).ok()?;
    // Fields after comm: state (3), ppid (4).
    let ppid = rest.split_whitespace().nth(1)?.parse().ok()?;
    Some((comm, ppid))
}

/// The argv of the nearest ancestor (excluding this process) whose `comm` is `comm`,
/// looking at most [`ANCESTOR_DEPTH`] levels up; `None` when there is none or `/proc` is
/// unreadable (which counts as interactive).
pub fn ancestor_argv(comm: &str) -> Option<Vec<String>> {
    let mut pid = rustix::process::getppid()?.as_raw_pid().unsigned_abs();
    for _ in 0..ANCESTOR_DEPTH {
        if pid <= 1 {
            return None;
        }
        let (c, ppid) = comm_and_parent(pid)?;
        if c == comm {
            let raw = read_bounded(&format!("/proc/{pid}/cmdline"), MAX_CMDLINE_BYTES)?;
            return Some(
                raw.split(|&b| b == 0)
                    .filter(|t| !t.is_empty())
                    .take(MAX_TOKENS)
                    .map(|t| String::from_utf8_lossy(t).into_owned())
                    .collect(),
            );
        }
        pid = ppid;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_this_process_and_finds_no_missing_ancestor() {
        let (comm, ppid) = comm_and_parent(std::process::id()).unwrap();
        assert!(!comm.is_empty());
        assert!(ppid > 0);
        assert_eq!(ancestor_argv("no-such-comm-x"), None);
    }
}
