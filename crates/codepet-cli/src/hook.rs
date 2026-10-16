//! `codepet hook --state-dir <abs> <client-id>` (spec §5.1, §5.8). Owned by WP2.
//!
//! Invariant O2 holds even in the WP0 skeleton: every path exits 0 with empty stdout and
//! stderr, never opens SQLite, never creates a directory and makes no network call. The
//! skeleton therefore records nothing and returns at once (a client writing more stdin
//! sees only its own EPIPE).
use std::ffi::OsString;

/// Runs the hook and returns its exit code, which is always 0.
pub fn run(_args: &[OsString]) -> i32 {
    0
}
