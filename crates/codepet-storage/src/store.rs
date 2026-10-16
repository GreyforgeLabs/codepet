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
use crate::schema::{self, Migration, MIGRATIONS};
use crate::sys::sha256_hex;
use crate::{
    Clock, Result, StatePaths, StorageError, DB_FILE, MAX_CARE_RECEIPTS, MAX_REQUEST_ID_BYTES,
    SCHEMA_VERSION,
};
use codepet_core::agent::RETENTION_LIMIT;
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

/// What [`Store::open_migrating`] found and did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationReport {
    /// `user_version` this open found under the writer lock (0 for a fresh database). When
    /// another process migrated first, this is already [`SCHEMA_VERSION`].
    pub from: i64,
    /// `user_version` now ([`SCHEMA_VERSION`]).
    pub to: i64,
    /// The `VACUUM INTO` copy taken before an upgrade (none for a fresh database or one
    /// that was already current).
    pub backup: Option<PathBuf>,
}

/// What a brand-new (empty) state directory gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fresh {
    /// v0.3 (spec §2.1, K7): no companion until **Let's go!** (`pet.start` adopts), so the
    /// popup opens on "Pick your pet!" and the name follows the picked species.
    NoPet,
    /// The v0.2 shape: the unarmed first-run life #1 "Ginger" that schema v4 creates. Only
    /// tests use it, to model an upgraded install.
    FirstRunPlaceholder,
}

/// Whether an open may upgrade an existing older database (spec §6.2 "Migration
/// ownership": only `codepet migrate` and `state.get` may).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Upgrade {
    Allowed,
    Refused,
}

impl Store {
    /// Opens (creating if needed) the database under checked paths and migrates an older
    /// one to [`SCHEMA_VERSION`]. Only `codepet migrate` and `rpc state.get` use this
    /// (migration ownership, spec §6.2). Refuses without modifying the file when
    /// `quick_check` fails or `user_version` is newer than this build.
    pub fn open(paths: &StatePaths, clock: Arc<dyn Clock>) -> Result<Self> {
        Ok(Self::open_with_mode(paths, clock, MIGRATIONS, Upgrade::Allowed, Fresh::NoPet)?.0)
    }

    /// [`open`](Self::open), also reporting the version found and the backup taken.
    pub fn open_migrating(
        paths: &StatePaths,
        clock: Arc<dyn Clock>,
    ) -> Result<(Self, MigrationReport)> {
        Self::open_with_mode(paths, clock, MIGRATIONS, Upgrade::Allowed, Fresh::NoPet)
    }

    /// Opens a current database for every other command. A fresh (empty) state creates the
    /// current schema, which is an install, not a migration; an existing older database is
    /// refused untouched with [`StorageError::MigrationRequired`] (run `codepet migrate`).
    pub fn open_current(paths: &StatePaths, clock: Arc<dyn Clock>) -> Result<Self> {
        Ok(Self::open_with_mode(paths, clock, MIGRATIONS, Upgrade::Refused, Fresh::NoPet)?.0)
    }

    /// Test support: opens like [`open`](Self::open), but a brand-new database gets the v0.2
    /// first-run shape (the unarmed life #1 "Ginger"), i.e. what an upgraded v0.2 install
    /// looks like. Never compiled into the `codepet` binary (feature `test-support`).
    #[cfg(any(test, feature = "test-support"))]
    pub fn open_with_first_run_placeholder(
        paths: &StatePaths,
        clock: Arc<dyn Clock>,
    ) -> Result<Self> {
        Ok(Self::open_with_mode(
            paths,
            clock,
            MIGRATIONS,
            Upgrade::Allowed,
            Fresh::FirstRunPlaceholder,
        )?
        .0)
    }

    /// Test support: [`open`](Self::open) with explicit migrations; a brand-new database
    /// gets the v0.2 first-run shape (a v4 database when `migrations` stops at v4).
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn open_with(
        paths: &StatePaths,
        clock: Arc<dyn Clock>,
        migrations: &[Migration],
    ) -> Result<Self> {
        Ok(Self::open_with_mode(
            paths,
            clock,
            migrations,
            Upgrade::Allowed,
            Fresh::FirstRunPlaceholder,
        )?
        .0)
    }

    fn open_with_mode(
        paths: &StatePaths,
        clock: Arc<dyn Clock>,
        migrations: &[Migration],
        upgrade: Upgrade,
        fresh: Fresh,
    ) -> Result<(Self, MigrationReport)> {
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
        if version > 0 && version < target && upgrade == Upgrade::Refused {
            // Checked before the first write (the WAL pragma): the file stays as it was.
            return Err(StorageError::MigrationRequired(version));
        }
        let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], get0)?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(StorageError::Unavailable("WAL journal mode unavailable"));
        }
        conn.execute_batch("PRAGMA synchronous = FULL")?;
        let store = Self {
            conn,
            clock,
            paths: paths.clone(),
        };
        let (from, backup) = if version < target {
            store.migrate(migrations, fresh)?
        } else {
            (version, None)
        };
        let report = MigrationReport {
            from,
            to: target,
            backup,
        };
        Ok((store, report))
    }

    pub fn paths(&self) -> &StatePaths {
        &self.paths
    }

    #[cfg(test)]
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    /// `VACUUM INTO` a fresh 0600 file named `codepet.sqlite3.bak-v<old>-<millis>` (with a
    /// `-<n>` suffix if that name is taken), read through a second connection. The caller
    /// holds the writer lock on `self.conn`, so the copy is exactly the `old` data that is
    /// about to be migrated: no other process can migrate or write in between (WAL readers
    /// never wait for the writer).
    fn backup(&self, old: i64) -> Result<PathBuf> {
        let base = format!("{DB_FILE}.bak-v{old}-{}", self.now());
        let mut n = 0u32;
        let path = loop {
            let name = if n == 0 {
                base.clone()
            } else {
                format!("{base}-{n}")
            };
            let path = self.paths.state_dir.join(name);
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
            {
                Ok(_) => break path,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && n < 99 => n += 1,
                Err(e) => return Err(e.into()),
            }
        };
        let text = path
            .to_str()
            .ok_or(StorageError::Unavailable("backup path is not UTF-8"))?;
        let reader = Connection::open_with_flags(
            &self.paths.db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        reader.busy_timeout(Duration::from_millis(5000))?;
        if let Err(e) = reader.execute("VACUUM INTO ?1", [text]) {
            // Never leave an empty or partial file that looks like a backup.
            let _ = std::fs::remove_file(&path);
            return Err(e.into());
        }
        Ok(path)
    }

    /// Migrates under the writer lock, taken before anything is read or copied: the version
    /// is re-read under it, so of two processes upgrading at once only the first backs up
    /// and migrates, and the second finds the current schema and changes nothing. Returns
    /// the version found under the lock and the backup taken (an upgrade only).
    fn migrate(&self, migrations: &[Migration], fresh: Fresh) -> Result<(i64, Option<PathBuf>)> {
        let target = i64::try_from(migrations.len()).unwrap_or(i64::MAX);
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        // Sampled under the writer lock, like every write.
        let at = care::TimeMark::new(self.clock.as_ref());
        let version: i64 = tx.query_row("PRAGMA user_version", [], get0)?;
        if !(0..=target).contains(&version) {
            return Err(StorageError::SchemaUnsupported(version));
        }
        if version == target {
            // Another process migrated (or created) it meanwhile: nothing to do.
            return Ok((version, None));
        }
        let backup = if version > 0 {
            Some(self.backup(version)?)
        } else {
            None
        };
        // A brand-new v0.3 database has no companion: schema v4 skips its first-run life
        // #1 when this marker exists (see `schema::v4`). The TEMP table is private to this
        // connection and dropped before the commit.
        let no_pet = version == 0 && fresh == Fresh::NoPet && target >= 5;
        if no_pet {
            tx.execute_batch(&format!("CREATE TEMP TABLE {} (x)", schema::FRESH_NO_PET))?;
        }
        for m in &migrations[usize::try_from(version).unwrap_or(usize::MAX)..] {
            m(&tx, &at)?;
        }
        if no_pet {
            tx.execute_batch(&format!("DROP TABLE temp.{}", schema::FRESH_NO_PET))?;
        }
        tx.pragma_update(None, "user_version", target)?;
        tx.commit()?;
        Ok((version, backup))
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
            || life::death_due(&read)?
            || crate::treats::forgotten_due(&read, self.clock.now_ms())?;
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
        let v03 = crate::agent_export::V03Blocks::load(&tx, self.clock.now_ms())?;
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
            enabled: v03.enabled,
            count_helpers: v03.count_helpers,
            agent: v03.agent,
            clients: v03.clients,
            treats: v03.treats,
        };
        tx.commit()?;
        Ok(snapshot)
    }

    pub fn last_seq(&self) -> Result<i64> {
        last_seq(&self.conn)
    }

    /// Runs one idempotent v0.3 mutation (spec §6.2): the settling prologue, the receipt
    /// check (same id + params → replay; other params → `IDEMPOTENCY_CONFLICT`), the Off
    /// policy, `body`, the epilogue (finalize, `state_version++`), the receipt, then the
    /// commit. Returns the `state.get` snapshot taken after the commit, also on replay.
    pub(crate) fn v03_command(
        &mut self,
        request_id: &str,
        method: &str,
        canonical_params: &str,
        policy: V03Policy,
        fault_point: Option<&str>,
        body: impl FnOnce(&Connection, &care::TimeMark, bool) -> Result<bool>,
    ) -> Result<Snapshot> {
        validate_request_id(request_id)?;
        let sha = sha256_hex(canonical_params.as_bytes());
        let (tx, at, ended) = self.write_settled()?;
        let (_receipt, replayed): (V03Receipt, bool) =
            replay_or_run(&tx, request_id, method, &sha, at.wall, || {
                if policy == V03Policy::RefusedWhileOff {
                    crate::app::ensure_on(&tx)?;
                }
                // `ended`: this command's own prologue made a pending end final.
                let changed = body(&tx, &at, ended)?;
                seal(&tx, &at)?;
                Ok(V03Receipt { changed })
            })
            .map_err(|e| passed_away_if(ended, e))?;
        if !replayed {
            if let Some(point) = fault_point {
                crate::fault(point);
            }
            tx.commit()?;
        } else {
            drop(tx);
        }
        self.snapshot()
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
                // Care, sleep and wake would age the life: refused while Off.
                crate::app::ensure_on(&tx)?;
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
        crate::app::ensure_on(&tx)?;
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
/// first, then rest. Presence reads the nap state before `settle_sleep` changes it. Earn
/// times kept by "Forget coding history" that have left the day are cleared here too, so
/// that promise never waits for a hook to fire (`notes.md` §7 item 1).
pub(crate) fn settle_all(tx: &Connection, paths: &StatePaths, at: &care::TimeMark) -> Result<()> {
    crate::treats::expire_forgotten(tx, at.wall, i64::from(RETENTION_LIMIT))?;
    lifecycle_store::settle_presence(tx, paths, at)?;
    // Off freezes the nap too: `app.set_enabled(true)` moves its cursor to the On click.
    if crate::app::enabled(tx)? {
        care::settle_sleep(tx, at)?;
    }
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
/// Frequent v0.3 clicks, pruned like care receipts. `pet.start` receipts are kept, like
/// adoption and retirement.
const V03_RECEIPT_METHODS: &str =
    "('pet.rename','treats.give','app.set_enabled','app.set_helpers','agent.forget_history')";

/// How a v0.3 method treats `meta.enabled = 0` (`protocol.json` `off_behaviour`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum V03Policy {
    /// Runs normally while Off (cosmetic or bookkeeping).
    AllowedWhileOff,
    /// `INVALID_STATE` "CodePet is off", checked after receipt replay.
    RefusedWhileOff,
}

/// The stored receipt of a v0.3 method: whether it changed anything. Its result is always
/// the fresh `state.get` snapshot (also on replay), never a stale copy.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct V03Receipt {
    changed: bool,
}

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
        (V03_RECEIPT_METHODS, crate::MAX_LIFE_RECEIPTS),
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
        changed |= fill_care(tx, at, action, state.bathroom_due)?;
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

/// A care action's fill (`food`, `water`, `play`, `wash`, `bathroom`, `routine`): full
/// meters, the bathroom counter for `bathroom` (and `routine` when due), the welcome routine
/// spent once every meter is full, then the lifecycle care hook (satiety holds, lethal
/// neglect, recovery; no fresh grace). The caller checked that the pet is awake. Returns
/// whether anything changed; the caller bumps the care revision.
pub(crate) fn fill_care(
    tx: &Connection,
    at: &care::TimeMark,
    action: &str,
    bathroom_due: bool,
) -> Result<bool> {
    let mut changed = false;
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
    if action == "bathroom" || (action == "routine" && bathroom_due) {
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
    Ok(changed)
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
