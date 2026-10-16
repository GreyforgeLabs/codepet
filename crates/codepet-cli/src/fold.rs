//! `codepet fold --state-dir <abs> [--lock-fd N]` (spec §5.1, §6.4): folds the hook inbox
//! into the agent tables under `fold.lock`. Owned by WP2; the WP0 skeleton refuses with
//! `NOT_IMPLEMENTED` and touches nothing.
use std::ffi::OsString;

pub fn run(_args: &[OsString]) -> i32 {
    crate::not_implemented("fold")
}
