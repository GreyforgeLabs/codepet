//! Test support for the workspace's integration tests (feature `test-support`, enabled
//! only as a dev-dependency feature; never compiled into the `codepet` binary). Builds
//! databases shaped like an upgraded v0.2 install. Nothing here touches a real state
//! directory unless the caller passes one.
use crate::schema::MIGRATIONS;
use crate::{Clock, Result, StatePaths, Store};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use std::sync::Arc;

/// Creates a schema v4 database (the v0.2.0 / v0.2.1 schema, through the unchanged v1-v4
/// migrations) at `paths`, with the unarmed first-run life #1 "Ginger", and closes it.
pub fn create_v4_database(paths: &StatePaths, clock: Arc<dyn Clock>) -> Result<()> {
    drop(Store::open_with(paths, clock, &MIGRATIONS[..4])?);
    Ok(())
}

/// The `user_version` of the existing SQLite file at `path` (never created). The connection
/// only reads; it is opened read-write so that, as the last connection of a WAL database,
/// it removes the `-wal`/`-shm` files on close instead of leaving them behind.
pub fn user_version(path: &Path) -> Result<i64> {
    let c = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    Ok(c.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}
