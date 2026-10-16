//! The v0.3 app switches (spec §7.3, §6.2, §5.10): On/Off (`app.set_enabled`), "Count
//! background helpers" (`app.set_helpers`), "Forget coding history"
//! (`agent.forget_history`) and the two state-dir flags the hook shim reads.
//!
//! The database is authoritative. `$STATE/off` mirrors `meta.enabled = 0` and
//! `$STATE/helpers` mirrors `meta.count_helpers = 1`; every export rewrites or removes them
//! to match ([`sync_flags`]), so a stray or missing flag heals at the next command.
use crate::care::TimeMark;
use crate::model::Snapshot;
use crate::store::V03Policy;
use crate::{Result, StatePaths, StorageError, Store};
use rusqlite::{params, Connection};
use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// `meta.enabled`: CodePet is On.
pub(crate) fn enabled(c: &Connection) -> Result<bool> {
    Ok(c.query_row("SELECT enabled FROM meta WHERE id = 1", [], |r| r.get(0))?)
}

/// `meta.count_helpers`: "Count background helpers" is on.
pub(crate) fn count_helpers(c: &Connection) -> Result<bool> {
    Ok(
        c.query_row("SELECT count_helpers FROM meta WHERE id = 1", [], |r| {
            r.get(0)
        })?,
    )
}

/// Refuses with [`StorageError::Off`] while CodePet is Off. Called inside a method's
/// transaction, after receipt replay (`protocol.json` `off_behaviour`).
pub(crate) fn ensure_on(c: &Connection) -> Result<()> {
    if enabled(c)? {
        Ok(())
    } else {
        Err(StorageError::Off)
    }
}

/// Makes `path` exist as an empty 0600 regular file (`want`) or not exist (`!want`). A
/// symlink, FIFO or other non-regular entry in its place is unlinked first (never followed:
/// the shim's `test -e` would follow a symlink). A directory there is refused.
fn set_flag(path: &Path, want: bool) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => {
            return Err(StorageError::UnsafeStateDir(format!(
                "{}: is a directory",
                path.display()
            )))
        }
        Ok(m) if want && m.is_file() => return Ok(()),
        Ok(_) => {
            // An unwanted flag, or something other than a plain file in its place.
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    if want {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(path)
        {
            Ok(_) => {}
            // Another command created it meanwhile: the same flag.
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

/// Rewrites or removes `$STATE/off` and `$STATE/helpers` to match `meta` (read from `c`).
pub(crate) fn sync_flags(c: &Connection, paths: &StatePaths) -> Result<()> {
    set_flag(&paths.off_flag_path(), !enabled(c)?)?;
    set_flag(&paths.helpers_flag_path(), count_helpers(c)?)
}

/// `$STATE/fold.lock`, taken with `LOCK_NB` (lock order: `fold.lock` before the database).
/// The file is created 0600 if missing and never follows a symlink. `Ok(None)` when the
/// fold holds it.
fn try_fold_lock(paths: &StatePaths) -> Result<Option<fs::File>> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC).bits() as i32)
        .open(paths.fold_lock_path())?;
    if !file.metadata()?.is_file() {
        return Err(StorageError::UnsafeStateDir(
            "fold.lock is not a regular file".into(),
        ));
    }
    match rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => Ok(Some(file)),
        Err(e) if e == rustix::io::Errno::WOULDBLOCK => Ok(None),
        Err(e) => Err(std::io::Error::from(e).into()),
    }
}

impl Store {
    /// `app.set_enabled` (spec §7.3), one write transaction. Turning Off settles presence and
    /// sleep up to the click (the write prologue, while still On), then sets
    /// `meta.enabled = 0`: settlement discards everything after it (blocker `!enabled`,
    /// frozen reason `off`), so there is no decay, grace, XP or death while Off and the nap
    /// is frozen. Turning On sets `meta.enabled = 1` and moves a frozen nap's cursor to now,
    /// so it resumes exactly where it left off with no catch-up. Idempotent; works with no
    /// living companion. The `off` flag follows after the commit (and at every export).
    pub fn set_enabled(&mut self, request_id: &str, on: bool) -> Result<Snapshot> {
        let params = format!("{{\"enabled\":{on}}}");
        let snapshot = self.v03_command(
            request_id,
            "app.set_enabled",
            &params,
            V03Policy::AllowedWhileOff,
            Some("enable_before_commit"),
            |c, at, _| set_enabled_body(c, at, on),
        )?;
        // The DB is authoritative; a flag that cannot be written now heals at the export.
        let _ = sync_flags(&self.conn, &self.paths);
        Ok(snapshot)
    }

    /// `app.set_helpers`: sets `meta.count_helpers`; the `helpers` flag follows.
    pub fn set_helpers(&mut self, request_id: &str, on: bool) -> Result<Snapshot> {
        let params = format!("{{\"enabled\":{on}}}");
        let snapshot = self.v03_command(
            request_id,
            "app.set_helpers",
            &params,
            V03Policy::AllowedWhileOff,
            None,
            |c, _, _| {
                Ok(c.execute(
                    "UPDATE meta SET count_helpers = ?1 WHERE id = 1 AND count_helpers != ?1",
                    [on],
                )? > 0)
            },
        )?;
        let _ = sync_flags(&self.conn, &self.paths);
        Ok(snapshot)
    }

    /// `agent.forget_history` (spec §5.10): deletes `agent_events` and `agent_sessions`, and
    /// clears the timestamps of `activity_turns`: a given row's `given_at_ms = 0` (never back
    /// to NULL, so it stays out of the jar) and `earned_at_ms = 0`, except that a treat earned
    /// in the last 24 h keeps a marked, hour-coarsened earn time so Forget never resets the
    /// earning caps (owner decision 2026-10-16, [`crate::treats::forget_earn_times`]).
    /// Unspent treats and earned XP stay. Takes `fold.lock` (`LOCK_NB`, `BUSY` if the fold
    /// holds it) before the database.
    pub fn forget_history(&mut self, request_id: &str) -> Result<Snapshot> {
        let Some(_lock) = try_fold_lock(&self.paths)? else {
            return Err(StorageError::FoldRunning);
        };
        self.v03_command(
            request_id,
            "agent.forget_history",
            "{}",
            V03Policy::AllowedWhileOff,
            None,
            |c, at, _| {
                let mut n = c.execute("DELETE FROM agent_events", [])?;
                n += c.execute("DELETE FROM agent_sessions", [])?;
                n += crate::treats::forget_earn_times(c, at.wall)?;
                n += c.execute(
                    "UPDATE activity_turns SET given_at_ms = 0
                     WHERE given_at_ms IS NOT NULL AND given_at_ms != 0",
                    [],
                )?;
                Ok(n > 0)
            },
        )
        // `_lock` is released here, after the commit.
    }

    /// Rewrites or removes `$STATE/off` and `$STATE/helpers` from committed state.
    pub fn sync_flags(&self) -> Result<()> {
        sync_flags(&self.conn, &self.paths)
    }
}

fn set_enabled_body(c: &Connection, at: &TimeMark, on: bool) -> Result<bool> {
    let was = enabled(c)?;
    if was == on {
        return Ok(false);
    }
    c.execute("UPDATE meta SET enabled = ?1 WHERE id = 1", [on])?;
    if on {
        // No catch-up: a nap frozen while Off resumes from now. Its remaining time was
        // settled up to the Off click and nothing settled it while Off.
        c.execute(
            "UPDATE pet_care SET sleep_wall_ms = ?1, sleep_boot = ?2, sleep_steady_ms = ?3
             WHERE id = 1 AND sleep_started_at_ms IS NOT NULL",
            params![at.wall, at.boot, at.steady],
        )?;
    }
    Ok(true)
}
