//! `codepet clients-job <job_id>` (spec §5.2): the detached, journaled Connect,
//! Disconnect and ready-check job. Owned by WP3; the WP0 skeleton refuses with
//! `NOT_IMPLEMENTED` and touches nothing.
use std::ffi::OsString;

pub fn run(_args: &[OsString]) -> i32 {
    crate::not_implemented("clients-job")
}
