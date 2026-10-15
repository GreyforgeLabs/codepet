//! `bar-state.json` (spec §10): the bar plugin's and overlay's only passive state input.
//!
//! A bar-safe subset of committed state: no task instructions, folders, artifacts, logs
//! or provider error text. The file is written only under the database writer lock and
//! only when this state is newer than what the file already holds, so it is monotonic in
//! `state_version`. Readers never launch Rust in response to a change of this file.
use crate::model::MetersView;
use crate::schema::SLOT_STATES;
use crate::{care, life, sys, Result, StorageError, Store};
use codepet_core::lifecycle::{IDLE_TIMEOUT_S, RULESET_V1, TICK_MS};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

pub const BAR_SCHEMA: &str = "codepet.bar.v1";
/// The file never exceeds this.
pub const MAX_BAR_STATE_BYTES: usize = 16 * 1024;
pub const MAX_READINESS_REASONS: usize = 8;
pub const MAX_REASON_BYTES: usize = 160;

#[derive(Debug, Serialize)]
struct BarLife {
    life_id: String,
    ordinal: u32,
    name: String,
    appearance: String,
    stage: u8,
    level: u8,
    xp: u32,
    next_stage_xp: Option<u32>,
    status: String,
    mode: String,
    armed: bool,
    pace: String,
    paused: bool,
    fainted: bool,
    death_pending: bool,
}

#[derive(Debug, Serialize)]
struct BarNeed {
    need: String,
    action: String,
    state: String,
    /// Active time until the need turns low, so the bar leader can predict a warning
    /// (including the non-lethal mood and hygiene needs) between settling commands.
    until_warning_ms: Option<i64>,
    until_critical_ms: Option<i64>,
    until_end_ms: Option<i64>,
}

#[derive(Debug, Serialize)]
struct BarCritical {
    need: String,
    action: String,
    grace_ms: i64,
    elapsed_ms: i64,
    remaining_ms: i64,
    since_ms: i64,
}

#[derive(Debug, Serialize)]
struct BarCare {
    sleeping: bool,
    sleep_remaining_ms: i64,
    sleep_started_at_ms: Option<i64>,
    sleep_due: bool,
    bathroom_due: bool,
    ready_to_work: bool,
    readiness_reasons: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BarTask {
    state: String,
    category: String,
}

#[derive(Debug, Serialize)]
struct BarPresence {
    wanted: bool,
    life_id: Option<String>,
    tick_ms: i64,
    idle_timeout_s: i64,
}

#[derive(Debug, Serialize)]
struct BarUnseen {
    seq: i64,
    stage: u8,
}

#[derive(Debug, Serialize)]
struct BarMemorial {
    count: u32,
    latest_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct BarState {
    schema: &'static str,
    state_version: i64,
    /// The database identity; `state_version` is ordered only within one `db_id`.
    db_id: String,
    generated_at_ms: i64,
    app_version: &'static str,
    ruleset: &'static str,
    life: Option<BarLife>,
    meters: MetersView,
    needs: Vec<BarNeed>,
    critical: Option<BarCritical>,
    care: BarCare,
    task: Option<BarTask>,
    presence: BarPresence,
    unseen_events: Vec<BarUnseen>,
    memorial: BarMemorial,
}

/// Longest prefix of `s` that is at most `max` UTF-8 bytes.
pub(crate) fn byte_prefix(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Builds the bar subset from committed rows in `c` (no settlement).
pub(crate) fn build(c: &Connection, generated_at_ms: i64) -> Result<BarState> {
    let block = life::LifeBlock::load(c)?;
    let meters: MetersView = c.query_row(
        "SELECT hunger, thirst, mental_health, hygiene FROM pet_state WHERE id = 1",
        [],
        |r| {
            Ok(MetersView {
                hunger: r.get(0)?,
                thirst: r.get(1)?,
                mental_health: r.get(2)?,
                hygiene: r.get(3)?,
            })
        },
    )?;
    let care = care::view(c, meters)?;
    let task: Option<(String, String)> = c
        .query_row(
            &format!("SELECT state, category FROM tasks WHERE state IN {SLOT_STATES} LIMIT 1"),
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(BarState {
        schema: BAR_SCHEMA,
        state_version: life::state_version(c)?,
        db_id: life::db_id(c)?,
        generated_at_ms,
        app_version: env!("CARGO_PKG_VERSION"),
        ruleset: RULESET_V1,
        life: block.life.as_ref().map(|l| BarLife {
            life_id: l.life_id.clone(),
            ordinal: l.ordinal,
            name: l.name.clone(),
            appearance: l.appearance.clone(),
            stage: l.stage,
            level: l.level,
            xp: l.xp,
            next_stage_xp: l.next_stage.map(|n| n.xp),
            status: l.status.clone(),
            mode: l.mode.clone(),
            armed: l.armed,
            pace: l.pace.clone(),
            paused: l.paused,
            fainted: l.fainted,
            death_pending: l.death_pending,
        }),
        meters,
        needs: block
            .lifecycle
            .needs
            .iter()
            .map(|n| BarNeed {
                need: n.need.clone(),
                action: n.action.clone(),
                state: n.state.clone(),
                until_warning_ms: n.until_warning_ms,
                until_critical_ms: n.until_critical_ms,
                until_end_ms: n.until_end_ms,
            })
            .collect(),
        critical: block.lifecycle.critical.as_ref().map(|c| BarCritical {
            need: c.need.clone(),
            action: c.action.clone(),
            grace_ms: c.grace_ms,
            elapsed_ms: c.elapsed_ms,
            remaining_ms: c.remaining_ms,
            since_ms: c.since_ms,
        }),
        care: BarCare {
            sleeping: care.sleeping,
            sleep_remaining_ms: care.sleep_remaining_ms,
            sleep_started_at_ms: care.sleep_started_at_ms,
            sleep_due: care.sleep_due,
            bathroom_due: care.bathroom_due,
            ready_to_work: care.ready_to_work,
            readiness_reasons: care
                .readiness_reasons
                .iter()
                .take(MAX_READINESS_REASONS)
                .map(|r| byte_prefix(r, MAX_REASON_BYTES).to_owned())
                .collect(),
        },
        task: task.map(|(state, category)| BarTask { state, category }),
        presence: BarPresence {
            wanted: block.presence_wanted,
            life_id: block.life.as_ref().map(|l| l.life_id.clone()),
            tick_ms: TICK_MS,
            idle_timeout_s: IDLE_TIMEOUT_S,
        },
        unseen_events: block
            .unseen_events
            .iter()
            .map(|e| BarUnseen {
                seq: e.seq,
                stage: e.stage,
            })
            .collect(),
        memorial: BarMemorial {
            count: block.memorial.count,
            latest_name: block.memorial.latest.map(|l| l.name),
        },
    })
}

/// The `state_version` the existing file claims **for database `db_id`**; 0 when it is
/// missing, not a regular file, too large, unparsable, or written for another (or an
/// unnamed) database. A file from another database is therefore always replaced instead
/// of pinning the bar to that database's pet until this counter catches up.
fn existing_version(path: &Path, db_id: &str) -> i64 {
    #[derive(serde::Deserialize)]
    struct Head {
        state_version: i64,
        #[serde(default)]
        db_id: Option<String>,
    }
    let Ok(meta) = fs::symlink_metadata(path) else {
        return 0;
    };
    if !meta.is_file() || meta.len() > MAX_BAR_STATE_BYTES as u64 {
        return 0;
    }
    // Never follows a swapped-in symlink or blocks on a FIFO while the DB lock is held.
    let Ok(file) = sys::open_sidecar(path) else {
        return 0;
    };
    match file.metadata() {
        Ok(m) if m.is_file() && (m.dev(), m.ino()) == (meta.dev(), meta.ino()) => {}
        _ => return 0,
    }
    let mut text = Vec::new();
    if file
        .take(MAX_BAR_STATE_BYTES as u64 + 1)
        .read_to_end(&mut text)
        .is_err()
        || text.len() > MAX_BAR_STATE_BYTES
    {
        return 0;
    }
    match serde_json::from_slice::<Head>(&text) {
        Ok(h) if h.db_id.as_deref() == Some(db_id) => h.state_version,
        _ => 0,
    }
}

impl Store {
    /// Writes `bar-state.json` from committed state (spec §10). Takes the writer lock and
    /// does not settle. Returns `Ok(false)` when the file already holds this or a newer
    /// `state_version` of this same database (`db_id`); a file from another database is
    /// always replaced. The temporary file is created exclusively (never following a
    /// symlink) with mode 0600, synced, then renamed while the lock is held, so the file
    /// only ever moves forward.
    pub fn export_bar_state(&self) -> Result<bool> {
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let state = build(&tx, self.clock.now_ms())?;
        let path = self.paths.bar_state_path();
        if state.state_version <= existing_version(&path, &state.db_id) {
            tx.rollback()?;
            return Ok(false);
        }
        let bytes = serde_json::to_vec(&state)
            .map_err(|_| StorageError::Unavailable("cannot encode bar state"))?;
        if bytes.len() > MAX_BAR_STATE_BYTES {
            return Err(StorageError::Unavailable("bar state exceeds 16 KiB"));
        }
        let tmp = self.paths.state_dir.join(format!(
            "{}.tmp-{}-{}",
            crate::paths::BAR_STATE_FILE,
            std::process::id(),
            sys::hex(&sys::random_bytes::<4>()?)
        ));
        let written = (|| -> std::io::Result<()> {
            let mut f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&tmp)?;
            f.write_all(&bytes)?;
            f.sync_all()?;
            fs::rename(&tmp, &path)
        })();
        if let Err(e) = written {
            // Only our own exclusively created temporary file is ever removed.
            if e.kind() != ErrorKind::AlreadyExists {
                let _ = fs::remove_file(&tmp);
            }
            return Err(e.into());
        }
        tx.rollback()?;
        Ok(true)
    }
}
