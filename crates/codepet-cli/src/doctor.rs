//! `codepet doctor [--json]` (spec §6.3): plain-word health report that changes nothing.
//! Owned by WP3; the WP0 skeleton refuses with `NOT_IMPLEMENTED`.
use std::ffi::OsString;

pub fn run(_args: &[OsString]) -> i32 {
    crate::not_implemented("doctor")
}
