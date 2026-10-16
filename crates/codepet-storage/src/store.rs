//! Durable CodePet state. Every mutation is one `BEGIN IMMEDIATE` transaction. Every write
//! transaction runs the fixed order (spec §8.1): settle presence → settle sleep → command
//! body → `finalize_if_due` → `state_version++` → commit. An end that the settlement itself
//! makes due is sealed and committed first, on its own, so a refusing body cannot roll it
//! back.
//!
//! The retired v0.2 task pipeline lives in `store_legacy.rs`, compiled only with the
//! non-default `legacy-tasks` feature (spec §6.5). Its tables stay in every build.
use crate::care;
use crate::life;
use crate::lifecycle_store;
use crate::model::{CareOutcome, MetersView, PauseView, Snapshot};
use crate::schema::{Migration, MIGRATIONS};
use crate::sys::sha256_hex;
use crate::{
    Clock, Result, StatePaths, StorageError, DB_FILE, MAX_CARE_RECEIPTS, MAX_REQUEST_ID_BYTES,
    SCHEMA_VERSION,
};
#[cfg(feature = "legacy-tasks")]
use codepet_core::State;
use codepet_core::{Category, PROTOCOL_VERSION};
use rusqlite::types::FromSql;
use rusqlite::{
    params, Connection, OpenFlags, OptionalExtension, Row, Transaction, TransactionBehavior,
};
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "legacy-tasks")]
#[path = "store_legacy.rs"]
mod legacy;
#[cfg(all(test, feature = "legacy-tasks"))]
pub(crate) use legacy::json_prefix;
#[cfg(feature = "legacy-tasks")]
pub use legacy::{AcceptRequest, SealOutcome, Step};

pub struct Store {
    pub(crate) conn: Connection,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) paths: StatePaths,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store").field("paths", &self.paths).finish()
    }
}

impl Store {
    /// Opens (creating if needed) the database under checked paths. Refuses without
    /// modifying the file when `quick_check` fails or `user_version` is newer than this build.
    pub fn open(paths: &StatePaths, clock: Arc<dyn Clock>) -> Result<Self> {
        Self::open_with(paths, clock, MIGRATIONS)
    }

    pub(crate) fn open_with(
        paths: &StatePaths,
        clock: Arc<dyn Clock>,
        migrations: &[Migration],
    ) -> Result<Self> {
        paths.prepare()?;
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW;
        let conn = Connection::open_with_flags(&paths.db_path, flags)?;
        conn.busy_timeout(Duration::from_millis(5000))?;
        conn.execute_batch("PRAGMA foreign_keys = ON")?;
        // Read-only checks first: nothing is written before these pass.
        let check: Vec<String> = conn
            .prepare("PRAGMA quick_check")?
            .query_map([], get0)?
            .collect::<rusqlite::Result<_>>()?;
        if check != ["ok"] {
            return Err(StorageError::Corrupt);
        }
        let version: i64 = conn.query_row("PRAGMA user_version", [], get0)?;
        let target = i64::try_from(migrations.len()).unwrap_or(i64::MAX);
        if !(0..=target).contains(&version) {
            return Err(StorageError::SchemaUnsupported(version));
        }
        if version == 0 {
            let objects: i64 = conn.query_row("SELECT count(*) FROM sqlite_master", [], get0)?;
            if objects > 0 {
                // Unversioned but non-empty: not a CodePet database this build created.
                return Err(StorageError::SchemaUnsupported(0));
            }
        }
        let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], get0)?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(StorageError::Unavailable("WAL journal mode unavailable"));
        }
        conn.execute_batch("PRAGMA synchronous = FULL")?;
        let mut store = Self {
            conn,
            clock,
            paths: paths.clone(),
        };
        if version > 0 && version < target {
            store.backup(version)?;
        }
        if version < target {
            store.migrate(migrations)?;
        }
        Ok(store)
    }

    pub fn paths(&self) -> &StatePaths {
        &self.paths
    }

    #[cfg(test)]
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    /// `VACUUM INTO` a fresh 0600 file named `codepet.sqlite3.bak-v<old>-<millis>`.
    fn backup(&self, old: i64) -> Result<PathBuf> {
        let path = self
            .paths
            .state_dir
            .join(format!("{DB_FILE}.bak-v{old}-{}", self.now()));
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        let text = path
            .to_str()
            .ok_or(StorageError::Unavailable("backup path is not UTF-8"))?;
        self.conn.execute("VACUUM INTO ?1", [text])?;
        Ok(path)
    }

    fn migrate(&mut self, migrations: &[Migration]) -> Result<()> {
        let at = care::TimeMark::new(self.clock.as_ref());
        let target = i64::try_from(migrations.len()).unwrap_or(i64::MAX);
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        // Re-read under the write lock: another process may have migrated meanwhile.
        let version: i64 = tx.query_row("PRAGMA user_version", [], get0)?;
        if !(0..=target).contains(&version) {
            return Err(StorageError::SchemaUnsupported(version));
        }
        for m in &migrations[usize::try_from(version).unwrap_or(usize::MAX)..] {
            m(&tx, &at)?;
        }
        tx.pragma_update(None, "user_version", target)?;
        tx.commit()?;
        Ok(())
    }

    fn now(&self) -> i64 {
        self.clock.now_ms()
    }

    /// `BEGIN IMMEDIATE`, then the time sample, then the settlement prologue. Every
    /// mutating method ends in [`finish`] (or [`seal`] + commit).
    #[cfg(feature = "legacy-tasks")]
    pub(crate) fn write(&mut self) -> Result<(Transaction<'_>, care::TimeMark)> {
        self.write_settled().map(|(tx, at, _)| (tx, at))
    }

    /// [`write`](Self::write), also reporting whether a pending end became final in the
    /// prologue. A grace that runs out while the prologue settles elapsed time is final
    /// whatever the command body then decides, so the end is sealed and committed on its
    /// own first (a refusing body would otherwise roll it back) and the command continues
    /// in a fresh transaction, re-sampled under the writer lock.
    pub(crate) fn write_settled(&mut self) -> Result<(Transaction<'_>, care::TimeMark, bool)> {
        let (clock, paths) = (self.clock.as_ref(), &self.paths);
        let (tx, at) = begin_settled(&self.conn, clock, paths)?;
        if !life::finalize_if_due(&tx, &at)? {
            return Ok((tx, at, false));
        }
        finish(tx, &at)?;
        let (tx, at) = begin_settled(&self.conn, clock, paths)?;
        Ok((tx, at, true))
    }

    fn read(&self) -> Result<Transaction<'_>> {
        Ok(self.conn.unchecked_transaction()?)
    }

    // ---- reads -------------------------------------------------------------------------

    /// The exact `state.get` result. When something needs settling (a nap, a live
    /// execution segment, a presence sidecar, or a pending end whose slot is free) it takes
    /// the writer lock and runs the full write prologue and epilogue, so it may commit a
    /// settlement or an end (`state.get` is classified as mutating). Otherwise it is a
    /// plain WAL read that never waits for the writer lock and changes nothing. A
    /// checkpoint never invalidates preflight approvals.
    pub fn snapshot(&self) -> Result<Snapshot> {
        let read = self.read()?;
        let needs_settlement = care::snapshot_needs_settlement(&read)?
            || crate::presence_io::present(&self.paths.presence_path())
            || life::death_due(&read)?;
        let tx = if needs_settlement {
            read.commit()?;
            // Take the writer lock before settling so concurrent commands cannot spend
            // the same elapsed credit twice. Settlement re-reads under the lock.
            let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
            let at = care::TimeMark::new(self.clock.as_ref());
            settle_all(&tx, &self.paths, &at)?;
            #[cfg(feature = "legacy-tasks")]
            care::checkpoint_running(&tx, &at)?;
            seal(&tx, &at)?;
            tx
        } else {
            read
        };
        let (meters, _paused) = load_pet(&tx)?;
        let block = life::LifeBlock::load(&tx)?;
        #[cfg(feature = "legacy-tasks")]
        let legacy = legacy::snapshot_block(&tx, _paused)?;
        let snapshot = Snapshot {
            schema_version: SCHEMA_VERSION,
            state_version: life::state_version(&tx)?,
            db_id: life::db_id(&tx)?,
            protocol_version: PROTOCOL_VERSION,
            last_seq: last_seq(&tx)?,
            meters,
            care: care::view(&tx, meters)?,
            progression: care::progression(&tx)?,
            #[cfg(feature = "legacy-tasks")]
            decay: legacy.decay,
            #[cfg(feature = "legacy-tasks")]
            settings: legacy.settings,
            #[cfg(feature = "legacy-tasks")]
            active_task: legacy.active_task,
            #[cfg(feature = "legacy-tasks")]
            recent: legacy.recent,
            #[cfg(feature = "legacy-tasks")]
            providers: legacy.providers,
            life: block.life,
            lifecycle: block.lifecycle,
            unseen_events: block.unseen_events,
            memorial: block.memorial,
        };
        tx.commit()?;
        Ok(snapshot)
    }

    pub fn last_seq(&self) -> Result<i64> {
        last_seq(&self.conn)
    }

    // ---- user actions ------------------------------------------------------------------

    /// Explicit care is idempotent by request ID, including no-op full-meter actions.
    pub fn care(&mut self, request_id: &str, action: &str) -> Result<CareOutcome> {
        if !matches!(
            action,
            "food" | "water" | "play" | "wash" | "bathroom" | "routine"
        ) {
            return Err(StorageError::InvalidParams("unknown care action"));
        }
        self.care_command(request_id, "pet.care", action)
    }

    pub fn sleep(&mut self, request_id: &str) -> Result<CareOutcome> {
        self.care_command(request_id, "pet.sleep", "sleep")
    }

    pub fn wake(&mut self, request_id: &str) -> Result<CareOutcome> {
        self.care_command(request_id, "pet.wake", "wake")
    }

    fn care_command(
        &mut self,
        request_id: &str,
        method: &str,
        action: &str,
    ) -> Result<CareOutcome> {
        validate_request_id(request_id)?;
        let params_sha256 = sha256_hex(action.as_bytes());
        let (tx, at, ended) = self.write_settled()?;
        let (result, replayed) =
            replay_or_run(&tx, request_id, method, &params_sha256, at.wall, || {
                care_body(&tx, &at, request_id, action)
            })
            .map_err(|e| passed_away_if(ended, e))?;
        if !replayed {
            tx.commit()?;
        }
        Ok(result)
    }

    /// Pause/Travel for the living companion: freezes decay, neglect and grace; grants
    /// and refills nothing; never blocks care or Start. Journaled when it changes.
    pub fn set_paused(&mut self, paused: bool) -> Result<PauseView> {
        let (tx, at, ended) = self.write_settled()?;
        let now = at.wall;
        let life = life::active_life(&tx)?
            .ok_or_else(|| passed_away_if(ended, StorageError::NoActivePet))?;
        if lifecycle_store::load(&tx)?.death_pending {
            return Err(StorageError::InvalidState(
                "the companion cannot be paused right now",
            ));
        }
        let changed = tx.execute(
            "UPDATE pet_state SET decay_paused = ?1, updated_at_ms = ?2
             WHERE id = 1 AND decay_paused != ?1",
            params![paused, now],
        )? > 0;
        if changed {
            life::journal(
                &tx,
                &life.life_id,
                if paused { "paused" } else { "resumed" },
                None,
                &serde_json::json!({}),
                None,
                None,
                now,
            )?;
        }
        finish(tx, &at)?;
        Ok(PauseView {
            paused,
            life_id: Some(life.life_id),
        })
    }
}

/// `BEGIN IMMEDIATE`, the time sample, then [`settle_all`]. Sample only after acquiring
/// the writer lock: a contending command must not overwrite a newer sleep/execution
/// checkpoint with an earlier timestamp.
fn begin_settled<'c>(
    conn: &'c Connection,
    clock: &dyn Clock,
    paths: &StatePaths,
) -> Result<(Transaction<'c>, care::TimeMark)> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let at = care::TimeMark::new(clock);
    settle_all(&tx, paths, &at)?;
    Ok((tx, at))
}

/// The settlement prologue of every write (and of a settling snapshot): attended presence
/// first, then rest. Presence reads the nap state before `settle_sleep` changes it.
pub(crate) fn settle_all(tx: &Connection, paths: &StatePaths, at: &care::TimeMark) -> Result<()> {
    lifecycle_store::settle_presence(tx, paths, at)?;
    care::settle_sleep(tx, at)?;
    Ok(())
}

/// The epilogue without the commit: finalize a due death (after any XP this transaction
/// granted), then `meta.state_version += 1`.
pub(crate) fn seal(tx: &Connection, at: &care::TimeMark) -> Result<()> {
    life::finalize_if_due(tx, at)?;
    tx.execute(
        "UPDATE meta SET state_version = state_version + 1 WHERE id = 1",
        [],
    )?;
    Ok(())
}

/// A command whose prologue just made a pending end final refuses with
/// [`StorageError::PassedAway`] instead of the plain "no companion" answer.
pub(crate) fn passed_away_if(ended: bool, e: StorageError) -> StorageError {
    match e {
        StorageError::NoActivePet if ended => StorageError::PassedAway,
        e => e,
    }
}

/// The single ordering helper every mutating method ends in (spec §8.1).
pub(crate) fn finish(tx: Transaction<'_>, at: &care::TimeMark) -> Result<()> {
    seal(&tx, at)?;
    tx.commit()?;
    Ok(())
}

const CARE_METHODS: &str = "('pet.care','pet.sleep','pet.wake')";
const LIFE_RECEIPT_METHODS: &str =
    "('pet.select','pet.events_seen','pet.rules','pet.configure_life')";

/// Idempotency by envelope request id. The same id with the same method and params
/// returns the stored response (`true` = replayed; the caller then commits nothing); the
/// same id with anything else is `IDEMPOTENCY_CONFLICT`. Otherwise `run` executes and its
/// result is stored in the same transaction. Frequent care and lifecycle receipts are
/// pruned to the most recent ones; acceptance, adoption and retirement receipts are kept.
pub(crate) fn replay_or_run<T, F>(
    tx: &Connection,
    request_id: &str,
    method: &str,
    params_sha256: &str,
    now: i64,
    run: F,
) -> Result<(T, bool)>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
    F: FnOnce() -> Result<T>,
{
    validate_request_id(request_id)?;
    let prior: Option<(String, String, String)> = tx
        .query_row(
            "SELECT method, params_sha256, response FROM request_results WHERE request_id = ?1",
            [request_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    if let Some((old_method, sha, response)) = prior {
        if old_method != method || sha != params_sha256 {
            return Err(StorageError::IdempotencyConflict);
        }
        let value = serde_json::from_str(&response).map_err(|_| StorageError::Corrupt)?;
        return Ok((value, true));
    }
    let value = run()?;
    let response = serde_json::to_string(&value)
        .map_err(|_| StorageError::Unavailable("cannot encode receipt"))?;
    tx.execute(
        "INSERT INTO request_results (request_id, method, params_sha256, response,
            created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![request_id, method, params_sha256, response, now],
    )?;
    // Ordinary clicks must not grow the database without bound: keep only the most
    // recent receipts of each frequent group (insertion order).
    for (group, keep) in [
        (CARE_METHODS, MAX_CARE_RECEIPTS),
        (LIFE_RECEIPT_METHODS, crate::MAX_LIFE_RECEIPTS),
    ] {
        if group.contains(&format!("'{method}'")) {
            tx.execute(
                &format!(
                    "DELETE FROM request_results WHERE method IN {group}
                     AND rowid NOT IN (SELECT rowid FROM request_results
                        WHERE method IN {group} ORDER BY rowid DESC LIMIT ?1)"
                ),
                [keep],
            )?;
        }
    }
    Ok((value, false))
}

/// The body of `pet.care` / `pet.sleep` / `pet.wake` after the idempotency check.
fn care_body(
    tx: &Transaction<'_>,
    at: &care::TimeMark,
    request_id: &str,
    action: &str,
) -> Result<CareOutcome> {
    life::active_life(tx)?.ok_or(StorageError::NoActivePet)?;
    if lifecycle_store::load(tx)?.death_pending {
        return Err(StorageError::InvalidState(
            "care is not possible while the current task finishes",
        ));
    }
    let before = load_pet(tx)?.0;
    let state = care::view(tx, before)?;
    let mut changed = false;
    // The v0.2 nap requirement is a retired readiness gate (spec §6.5): only the legacy
    // build records that a nap was started or cut short.
    let rest_required = cfg!(feature = "legacy-tasks");
    if action == "sleep" {
        #[cfg(feature = "legacy-tasks")]
        legacy::ensure_not_executing(tx)?;
        if !state.sleeping {
            tx.execute(
                "UPDATE pet_care SET sleep_started_at_ms = ?1, sleep_wall_ms = ?1,
                sleep_boot = ?2, sleep_steady_ms = ?3,
                rest_required = MAX(rest_required, ?4) WHERE id = 1",
                params![at.wall, at.boot, at.steady, rest_required],
            )?;
            changed = true;
        }
    } else if action == "wake" {
        if state.sleeping {
            tx.execute(
                "UPDATE pet_care SET sleep_started_at_ms = NULL, sleep_wall_ms = NULL,
                sleep_boot = NULL, sleep_steady_ms = NULL,
                rest_required = MAX(rest_required, ?1) WHERE id = 1",
                [rest_required],
            )?;
            changed = true;
        }
    } else {
        if state.sleeping {
            return Err(StorageError::InvalidState(
                "pet is sleeping; finish the nap or wake it before care",
            ));
        }
        let filled: &[Category] = match action {
            "food" => &[Category::Hunger],
            "water" => &[Category::Thirst],
            "play" => &[Category::MentalHealth],
            "wash" | "bathroom" => &[Category::Hygiene],
            "routine" => &Category::ALL,
            _ => return Err(StorageError::InvalidParams("unknown care action")),
        };
        for category in filled {
            let col = category.as_str();
            changed |= tx.execute(
                &format!(
                    "UPDATE pet_state SET {col} = 10000,
                updated_at_ms = ?1 WHERE id = 1 AND {col} != 10000"
                ),
                [at.wall],
            )? > 0;
        }
        if action == "bathroom" || (action == "routine" && state.bathroom_due) {
            changed |= tx.execute(
                "UPDATE pet_care SET starts_since_bathroom = 0
                WHERE id = 1 AND starts_since_bathroom != 0",
                [],
            )? > 0;
        }
        changed |= tx.execute(
            "UPDATE pet_care SET welcome_care_available = 0
            WHERE id = 1 AND welcome_care_available != 0 AND starts_since_bathroom < 2
            AND EXISTS(SELECT 1 FROM pet_state WHERE id = 1 AND hunger = 10000
                AND thirst = 10000 AND mental_health = 10000 AND hygiene = 10000)",
            [],
        )? > 0;
        // Matching care restarts satiety holds, resets lethal neglect and may recover
        // the companion. It is allowed while critical; no fresh grace is granted.
        lifecycle_store::on_care(tx, filled, at.wall)?;
    }
    if changed {
        care::bump(tx)?;
    }
    seal(tx, at)?;
    let meters = load_pet(tx)?.0;
    Ok(CareOutcome {
        request_id: request_id.to_owned(),
        action: action.to_owned(),
        changed,
        meters,
        care: care::view(tx, meters)?,
        lifecycle: Some(life::LifeBlock::load(tx)?.lifecycle),
    })
}

fn get0<T: FromSql>(r: &Row<'_>) -> rusqlite::Result<T> {
    r.get(0)
}

fn validate_request_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > MAX_REQUEST_ID_BYTES
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
    {
        return Err(StorageError::InvalidParams(
            "request id must be 1-128 of [A-Za-z0-9._:-]",
        ));
    }
    Ok(())
}

#[cfg(feature = "legacy-tasks")]
fn append_event(
    c: &Connection,
    task_id: &str,
    from: Option<State>,
    to: State,
    revision: i64,
    now: i64,
) -> Result<i64> {
    c.execute(
        "INSERT INTO task_events (task_id, from_state, to_state, revision, at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![task_id, from.map(State::as_str), to.as_str(), revision, now],
    )?;
    Ok(c.last_insert_rowid())
}

#[cfg(feature = "legacy-tasks")]
fn invalidate_tokens(c: &Connection, task_id: Option<&str>) -> Result<()> {
    c.execute(
        "UPDATE preflight_tokens SET status = 'invalidated'
         WHERE status = 'pending' AND (?1 IS NULL OR task_id = ?1)",
        [task_id],
    )?;
    Ok(())
}

fn last_seq(c: &Connection) -> Result<i64> {
    Ok(c.query_row("SELECT COALESCE(MAX(seq), 0) FROM task_events", [], get0)?)
}

fn load_pet(c: &Connection) -> Result<(MetersView, bool)> {
    Ok(c.query_row(
        "SELECT hunger, thirst, mental_health, hygiene, decay_paused FROM pet_state
         WHERE id = 1",
        [],
        |r| {
            Ok((
                MetersView {
                    hunger: r.get(0)?,
                    thirst: r.get(1)?,
                    mental_health: r.get(2)?,
                    hygiene: r.get(3)?,
                },
                r.get(4)?,
            ))
        },
    )?)
}
