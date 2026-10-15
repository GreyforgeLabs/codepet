//! Durable task engine. Every mutation is one `BEGIN IMMEDIATE` transaction; every task state
//! change appends a `task_events` row in that same transaction.
use crate::model::{
    AcceptOutcome, DecayView, Event, EventData, EventsPage, HistoryCursor, HistoryPage, MetersView,
    MockOptions, MockOutcome, PauseView, PreflightResult, PreflightSummary, ProviderInfo,
    ReceiptView, RecoverResult, RecoveryChange, ReviewBrief, ReviewView, SettingsView, Snapshot,
    StartResult, TaskBrief, TaskInput, TaskSummary,
};
use crate::schema::{Migration, MIGRATIONS, SLOT_STATES};
use crate::sys::{self, sha256_hex, Supervisor};
use crate::{
    fault, Clock, Result, StatePaths, StorageError, DB_FILE, MAX_ARTIFACT_JSON_BYTES,
    MAX_DRAFT_JSON_BYTES, MAX_EVENTS_PAGE, MAX_EXCERPT_JSON_BYTES, MAX_HISTORY_PAGE,
    MAX_HISTORY_PAGE_JSON_BYTES, MAX_MOCK_STEP_MS, MAX_OVERRIDE_REASON_BYTES, MAX_REQUEST_ID_BYTES,
    RECENT_TASKS, SCHEMA_VERSION, TOKEN_TTL_MS,
};
use codepet_core::{
    Category, Draft, Meters, Provider, Settings, State, Verification, MAX_TASKS, PROTOCOL_VERSION,
    REWARD_HUNDREDTHS,
};
use rusqlite::types::FromSql;
use rusqlite::{
    params, Connection, ErrorCode, OpenFlags, OptionalExtension, Row, ToSql, Transaction,
    TransactionBehavior,
};
use std::fs::{self, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const ACCEPT_METHOD: &str = "task.accept_artifact";

/// Result of a supervisor compare-and-set step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Advanced {
        seq: i64,
    },
    /// The user requested cancellation; the supervisor must stop and call `confirm_cancelled`.
    Cancelling,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SealOutcome {
    Sealed { digest: String, seq: i64 },
    Cancelling,
}

/// `task.accept_artifact`; `request_id` is the envelope id and the idempotency key.
#[derive(Clone, Copy, Debug)]
pub struct AcceptRequest<'a> {
    pub request_id: &'a str,
    pub task_id: &'a str,
    pub review_digest: &'a str,
    pub acknowledged: bool,
    pub override_reason: Option<&'a str>,
}

pub struct Store {
    conn: Connection,
    clock: Arc<dyn Clock>,
    paths: StatePaths,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store").field("paths", &self.paths).finish()
    }
}

struct TaskRow {
    id: String,
    revision: i64,
    state: State,
    category: Category,
    provider: Provider,
    folder: String,
    instructions: String,
    mock: MockOptions,
    supervisor: Option<Supervisor>,
    review_digest: Option<String>,
    review_verification: Option<Verification>,
    review_simulated: bool,
    created_at_ms: i64,
    updated_at_ms: i64,
}

const TASK_COLS: &str = "id, revision, state, category, provider, folder, instructions, \
    mock_outcome, mock_step_ms, supervisor_pid, supervisor_start_time, review_digest, \
    review_verification, review_simulated, created_at_ms, updated_at_ms";

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
        let now = self.now();
        let target = i64::try_from(migrations.len()).unwrap_or(i64::MAX);
        let tx = self.write()?;
        // Re-read under the write lock: another process may have migrated meanwhile.
        let version: i64 = tx.query_row("PRAGMA user_version", [], get0)?;
        if !(0..=target).contains(&version) {
            return Err(StorageError::SchemaUnsupported(version));
        }
        for m in &migrations[usize::try_from(version).unwrap_or(usize::MAX)..] {
            m(&tx, now)?;
        }
        tx.pragma_update(None, "user_version", target)?;
        tx.commit()?;
        Ok(())
    }

    fn now(&self) -> i64 {
        self.clock.now_ms()
    }

    fn write(&mut self) -> Result<Transaction<'_>> {
        Ok(self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?)
    }

    fn read(&self) -> Result<Transaction<'_>> {
        Ok(self.conn.unchecked_transaction()?)
    }

    // ---- reads -------------------------------------------------------------------------

    /// The exact `state.get` result.
    pub fn snapshot(&self) -> Result<Snapshot> {
        let tx = self.read()?;
        let (meters, paused) = load_pet(&tx)?;
        let settings = load_settings(&tx)?;
        let active_task = active_task(&tx)?;
        let recent = load_tasks(
            &tx,
            "1 ORDER BY created_at_ms DESC, rowid DESC LIMIT ?1",
            &[&RECENT_TASKS],
        )?
        .iter()
        .map(|t| brief(&tx, t))
        .collect::<Result<_>>()?;
        let snapshot = Snapshot {
            schema_version: SCHEMA_VERSION,
            protocol_version: PROTOCOL_VERSION,
            last_seq: last_seq(&tx)?,
            meters,
            decay: DecayView {
                enabled: settings.decay_enabled,
                paused,
            },
            settings,
            active_task,
            recent,
            providers: ProviderInfo::all(),
        };
        tx.commit()?;
        Ok(snapshot)
    }

    pub fn task(&self, task_id: &str) -> Result<TaskSummary> {
        let tx = self.read()?;
        let s = summary(&tx, &load_task(&tx, task_id)?)?;
        tx.commit()?;
        Ok(s)
    }

    /// `task.review`: the task summary, which must carry a sealed review. A `review_ready`
    /// review is returned only while its digest still recomputes from the stored artifact.
    pub fn review(&self, task_id: &str) -> Result<TaskSummary> {
        let tx = self.read()?;
        let t = load_task(&tx, task_id)?;
        if t.review_digest.is_none() {
            return Err(StorageError::InvalidState("task has no review"));
        }
        if t.state == State::ReviewReady && !review_intact(&tx, &t)? {
            return Err(StorageError::ReviewDamaged);
        }
        let s = summary(&tx, &t)?;
        tx.commit()?;
        Ok(s)
    }

    pub fn last_seq(&self) -> Result<i64> {
        last_seq(&self.conn)
    }

    pub fn settings(&self) -> Result<SettingsView> {
        load_settings(&self.conn)
    }

    /// Persisted events with `seq > after_seq`, oldest first. A cursor ahead of storage
    /// (e.g. from another database) yields `ResyncRequired`.
    pub fn events_after(&self, after_seq: i64, limit: Option<u32>) -> Result<EventsPage> {
        let limit = page_limit(limit, MAX_EVENTS_PAGE)?;
        if after_seq < 0 {
            return Err(StorageError::InvalidParams("after_seq must be >= 0"));
        }
        let tx = self.read()?;
        let last = last_seq(&tx)?;
        if after_seq > last {
            return Err(StorageError::ResyncRequired);
        }
        let mut stmt = tx.prepare(
            "SELECT seq, task_id, from_state, to_state, revision, at_ms FROM task_events
             WHERE seq > ?1 ORDER BY seq LIMIT ?2",
        )?;
        let raw = stmt
            .query_map(params![after_seq, limit], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, i64>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        let events = raw
            .into_iter()
            .map(|(seq, task_id, from, to, revision, at_ms)| {
                Ok(Event {
                    v: PROTOCOL_VERSION,
                    event: "task.state",
                    seq,
                    task_id,
                    data: EventData {
                        state: parse(&to, State::parse)?,
                        from: from.map(|f| parse(&f, State::parse)).transpose()?,
                        revision,
                        at_ms,
                    },
                })
            })
            .collect::<Result<_>>()?;
        tx.commit()?;
        Ok(EventsPage {
            events,
            last_seq: last,
        })
    }

    /// Task briefs newest first: all tasks when `before_ms` is `None`; otherwise those created
    /// before it, or in that millisecond but stored before `before_task_id` (the `next`
    /// cursor of the previous page). A page stops early at `MAX_HISTORY_PAGE_JSON_BYTES`.
    pub fn history(
        &self,
        before_ms: Option<i64>,
        before_task_id: Option<&str>,
        limit: Option<u32>,
    ) -> Result<HistoryPage> {
        let limit = page_limit(limit, MAX_HISTORY_PAGE)?;
        let tx = self.read()?;
        let before = before_ms.unwrap_or(i64::MAX);
        let before_rowid = match before_task_id {
            // No cursor: every task, including one stamped i64::MAX.
            None if before_ms.is_none() => i64::MAX,
            None => i64::MIN,
            Some(_) if before_ms.is_none() => {
                return Err(StorageError::InvalidParams(
                    "before_task_id requires before_ms",
                ))
            }
            Some(id) => tx
                .query_row(
                    "SELECT rowid FROM tasks WHERE id = ?1 AND created_at_ms = ?2",
                    params![id, before],
                    get0,
                )
                .optional()?
                .ok_or(StorageError::InvalidParams(
                    "history cursor does not match a task",
                ))?,
        };
        let rows = load_tasks(
            &tx,
            "created_at_ms < ?1 OR (created_at_ms = ?1 AND rowid < ?2)
             ORDER BY created_at_ms DESC, rowid DESC LIMIT ?3",
            &[&before, &before_rowid, &(i64::from(limit) + 1)],
        )?;
        let mut tasks = Vec::new();
        let mut used = 0;
        let mut more = rows.len() > limit as usize;
        for t in rows.iter().take(limit as usize) {
            let b = brief(&tx, t)?;
            let size = json_len(&b) + 1;
            if !tasks.is_empty() && used + size > MAX_HISTORY_PAGE_JSON_BYTES {
                more = true;
                break;
            }
            used += size;
            tasks.push(b);
        }
        let next = tasks
            .last()
            .filter(|_| more)
            .map(|last: &TaskBrief| HistoryCursor {
                before_ms: last.created_at_ms,
                before_task_id: last.task_id.clone(),
            });
        tx.commit()?;
        Ok(HistoryPage { tasks, next })
    }

    // ---- drafts and preflight ----------------------------------------------------------

    pub fn create_task(&mut self, input: &TaskInput) -> Result<TaskSummary> {
        validate_input(input)?;
        let now = self.now();
        let id = sys::uuid_v4()?;
        let tx = self.write()?;
        let drafts: i64 =
            tx.query_row("SELECT count(*) FROM tasks WHERE state = 'draft'", [], get0)?;
        if drafts >= i64::try_from(MAX_TASKS).unwrap_or(i64::MAX) {
            return Err(StorageError::Capacity);
        }
        let d = &input.draft;
        tx.execute(
            "INSERT INTO tasks (id, revision, state, category, provider, folder, instructions,
                mock_outcome, mock_step_ms, created_at_ms, updated_at_ms)
             VALUES (?1, 1, 'draft', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![
                id,
                d.category.as_str(),
                d.provider.as_str(),
                d.folder,
                d.instructions,
                input.mock.outcome.as_str(),
                input.mock.step_ms,
                now
            ],
        )?;
        insert_scopes(&tx, &id, &d.scope)?;
        append_event(&tx, &id, None, State::Draft, 1, now)?;
        let s = summary(&tx, &load_task(&tx, &id)?)?;
        tx.commit()?;
        Ok(s)
    }

    /// Draft-only edit at the expected revision; invalidates unconsumed preflight tokens.
    pub fn update_task(
        &mut self,
        task_id: &str,
        expected_revision: i64,
        input: &TaskInput,
    ) -> Result<TaskSummary> {
        validate_input(input)?;
        let now = self.now();
        let tx = self.write()?;
        let t = load_task(&tx, task_id)?;
        if t.state != State::Draft {
            return Err(StorageError::InvalidState("only drafts can be edited"));
        }
        if t.revision != expected_revision {
            return Err(StorageError::StaleRevision);
        }
        let revision = t.revision.checked_add(1).ok_or(StorageError::Capacity)?;
        let d = &input.draft;
        let n = tx.execute(
            "UPDATE tasks SET revision = ?1, category = ?2, provider = ?3, folder = ?4,
                instructions = ?5, mock_outcome = ?6, mock_step_ms = ?7, updated_at_ms = ?8
             WHERE id = ?9 AND revision = ?10 AND state = 'draft'",
            params![
                revision,
                d.category.as_str(),
                d.provider.as_str(),
                d.folder,
                d.instructions,
                input.mock.outcome.as_str(),
                input.mock.step_ms,
                now,
                t.id,
                t.revision
            ],
        )?;
        if n != 1 {
            return Err(StorageError::StaleRevision);
        }
        tx.execute("DELETE FROM task_scopes WHERE task_id = ?1", [&t.id])?;
        insert_scopes(&tx, &t.id, &d.scope)?;
        invalidate_tokens(&tx, Some(&t.id))?;
        append_event(&tx, &t.id, Some(State::Draft), State::Draft, revision, now)?;
        let s = summary(&tx, &load_task(&tx, &t.id)?)?;
        tx.commit()?;
        Ok(s)
    }

    /// Mock preflight: validates the stored draft and paths, then issues a one-use token
    /// (only its SHA-256 is stored) that supersedes older tokens for the task.
    pub fn preflight(&mut self, task_id: &str, revision: i64) -> Result<PreflightResult> {
        let now = self.now();
        let paths = self.paths.clone();
        let tx = self.write()?;
        let t = load_task(&tx, task_id)?;
        if t.state != State::Draft {
            return Err(StorageError::InvalidState("only drafts can be preflighted"));
        }
        if t.revision != revision {
            return Err(StorageError::StaleRevision);
        }
        let draft = t.draft(load_scopes(&tx, &t.id)?);
        validate_draft(&draft)?;
        if !draft.provider.supported() {
            return Err(StorageError::UnsupportedProvider);
        }
        check_folder(&draft.folder, &paths)?;
        let settings = load_settings(&tx)?;
        let token = sys::hex(&sys::random_bytes::<32>()?);
        let expires_at_ms = now.saturating_add(TOKEN_TTL_MS);
        tx.execute(
            "UPDATE preflight_tokens SET status = 'superseded'
             WHERE task_id = ?1 AND status = 'pending'",
            [&t.id],
        )?;
        tx.execute(
            "INSERT INTO preflight_tokens (token_sha256, task_id, revision, settings_revision,
                status, created_at_ms, expires_at_ms) VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?6)",
            params![
                sha256_hex(token.as_bytes()),
                t.id,
                t.revision,
                settings.revision,
                now,
                expires_at_ms
            ],
        )?;
        tx.commit()?;
        Ok(PreflightResult {
            task_id: t.id,
            revision: t.revision,
            preflight_token: token,
            expires_at_ms,
            summary: PreflightSummary {
                category: draft.category,
                provider: draft.provider,
                folder: draft.folder,
                scope: draft.scope,
                instructions_sha256: sha256_hex(draft.instructions.as_bytes()),
                network: "none (mock)",
                writable: "none (mock: no project files are read or written)",
                timeout_seconds: settings.timeout_seconds,
            },
        })
    }

    /// Explicit Start: consumes the token for the exact revision and records the supervisor
    /// identity with the `preparing` transition.
    pub fn start(
        &mut self,
        task_id: &str,
        revision: i64,
        token: &str,
        supervisor: Supervisor,
    ) -> Result<StartResult> {
        let now = self.now();
        let paths = self.paths.clone();
        let tx = self.write()?;
        let t = load_task(&tx, task_id)?;
        if t.state != State::Draft {
            return Err(StorageError::InvalidState("only drafts can be started"));
        }
        if t.revision != revision {
            return Err(StorageError::StaleRevision);
        }
        if token.len() != 64
            || !token
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        {
            return Err(StorageError::TokenInvalid);
        }
        let hash = sha256_hex(token.as_bytes());
        type TokenRow = (String, i64, i64, String, i64, i64);
        let (owner, token_rev, settings_rev, status, created, expires): TokenRow = tx
            .query_row(
                "SELECT task_id, revision, settings_revision, status, created_at_ms, expires_at_ms
                 FROM preflight_tokens WHERE token_sha256 = ?1",
                [&hash],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )
            .optional()?
            .ok_or(StorageError::TokenInvalid)?;
        if owner != t.id || status != "pending" {
            return Err(StorageError::TokenInvalid);
        }
        // A clock that stepped back past the preflight cannot vouch for the token's age.
        if now >= expires || now < created {
            return Err(StorageError::TokenExpired);
        }
        if token_rev != t.revision || settings_rev != load_settings(&tx)?.revision {
            return Err(StorageError::TokenInvalid);
        }
        if !t.provider.supported() {
            return Err(StorageError::UnsupportedProvider);
        }
        // The folder may have become a symlink since preflight.
        check_folder(&t.folder, &paths)?;
        if active_task_id(&tx)?.is_some() {
            return Err(StorageError::Busy);
        }
        tx.execute(
            "UPDATE preflight_tokens SET status = 'consumed', consumed_at_ms = ?1
             WHERE token_sha256 = ?2 AND status = 'pending'",
            params![now, hash],
        )?;
        tx.execute(
            "UPDATE tasks SET supervisor_pid = ?1, supervisor_start_time = ?2 WHERE id = ?3",
            params![supervisor.pid, start_time_sql(supervisor)?, t.id],
        )?;
        let seq = transition(&tx, &t, State::Preparing, now)?;
        tx.commit()?;
        fault("start_after_commit");
        Ok(StartResult {
            task_id: t.id,
            state: State::Preparing,
            seq,
        })
    }

    // ---- supervisor ----------------------------------------------------------------------

    /// Compare-and-set `from → to` for `running`, `waiting_for_user` or `verifying`. Returns
    /// `Step::Cancelling` without changes if the user requested cancellation.
    pub fn supervisor_step(
        &mut self,
        task_id: &str,
        supervisor: Supervisor,
        from: State,
        to: State,
    ) -> Result<Step> {
        if !matches!(
            to,
            State::Running | State::WaitingForUser | State::Verifying
        ) {
            return Err(StorageError::InvalidParams("not a supervisor step"));
        }
        let step = self.supervised(task_id, supervisor, |t| (t.state == from).then_some(to))?;
        if to == State::Running && matches!(step, Step::Advanced { .. }) {
            fault("supervisor_running");
        }
        Ok(step)
    }

    /// Execution failure: any running phase → terminal `failed` (never rewarded).
    pub fn fail(&mut self, task_id: &str, supervisor: Supervisor) -> Result<Step> {
        self.supervised(task_id, supervisor, |t| {
            matches!(
                t.state,
                State::Preparing | State::Running | State::WaitingForUser | State::Verifying
            )
            .then_some(State::Failed)
        })
    }

    /// Containment confirmed (mock: no child process exists): `cancelling → cancelled`.
    pub fn confirm_cancelled(&mut self, task_id: &str, supervisor: Supervisor) -> Result<i64> {
        let now = self.now();
        let tx = self.write()?;
        let t = load_task(&tx, task_id)?;
        if t.supervisor != Some(supervisor) {
            return Err(StorageError::InvalidState("not this task's supervisor"));
        }
        if t.state != State::Cancelling {
            return Err(StorageError::InvalidState("task is not cancelling"));
        }
        let seq = transition(&tx, &t, State::Cancelled, now)?;
        tx.commit()?;
        Ok(seq)
    }

    fn supervised(
        &mut self,
        task_id: &str,
        supervisor: Supervisor,
        next: impl FnOnce(&TaskRow) -> Option<State>,
    ) -> Result<Step> {
        let now = self.now();
        let tx = self.write()?;
        let t = load_task(&tx, task_id)?;
        if t.supervisor != Some(supervisor) {
            return Err(StorageError::InvalidState("not this task's supervisor"));
        }
        if t.state == State::Cancelling {
            return Ok(Step::Cancelling);
        }
        let to = next(&t).ok_or(StorageError::InvalidState(
            "task is not in the expected state",
        ))?;
        let seq = transition(&tx, &t, to, now)?;
        tx.commit()?;
        Ok(Step::Advanced { seq })
    }

    /// Seals the immutable review: stores the bounded artifact and check, computes the
    /// manifest digest, and moves `verifying → review_ready`.
    pub fn seal_review(
        &mut self,
        task_id: &str,
        supervisor: Supervisor,
        artifact: &str,
        verification: Verification,
        simulated: bool,
    ) -> Result<SealOutcome> {
        if artifact.trim().is_empty()
            || json_len(artifact) > MAX_ARTIFACT_JSON_BYTES
            || artifact.contains('\0')
        {
            return Err(StorageError::InvalidParams(
                "artifact must be non-blank text without NUL, at most 24 KiB JSON-encoded",
            ));
        }
        if simulated && verification == Verification::Passed {
            return Err(StorageError::InvalidParams("a simulated check cannot pass"));
        }
        let now = self.now();
        let tx = self.write()?;
        let t = load_task(&tx, task_id)?;
        if t.supervisor != Some(supervisor) {
            return Err(StorageError::InvalidState("not this task's supervisor"));
        }
        if t.state == State::Cancelling {
            return Ok(SealOutcome::Cancelling);
        }
        if t.state != State::Verifying {
            return Err(StorageError::InvalidState("task is not verifying"));
        }
        let digest = review_digest(&t, artifact.as_bytes(), verification, simulated);
        tx.execute(
            "INSERT INTO artifacts (task_id, kind, content, sha256, length, created_at_ms)
             VALUES (?1, 'text', ?2, ?3, ?4, ?5)",
            params![
                t.id,
                artifact,
                sha256_hex(artifact.as_bytes()),
                artifact.len() as i64,
                now
            ],
        )?;
        tx.execute(
            "INSERT INTO checks (task_id, name, status, simulated, recorded_at_ms)
             VALUES (?1, 'mock', ?2, ?3, ?4)",
            params![t.id, verification.as_str(), simulated, now],
        )?;
        tx.execute(
            "UPDATE tasks SET review_digest = ?1, review_verification = ?2, review_simulated = ?3
             WHERE id = ?4",
            params![digest, verification.as_str(), simulated, t.id],
        )?;
        let seq = transition(&tx, &t, State::ReviewReady, now)?;
        fault("review_before_commit");
        tx.commit()?;
        Ok(SealOutcome::Sealed { digest, seq })
    }

    // ---- user actions ------------------------------------------------------------------

    /// Idempotent: active phases → `cancelling`; already cancelling/cancelled → unchanged.
    pub fn cancel(&mut self, task_id: &str) -> Result<TaskSummary> {
        let now = self.now();
        let tx = self.write()?;
        let t = load_task(&tx, task_id)?;
        match t.state {
            State::Cancelling | State::Cancelled => {}
            State::Preparing | State::Running | State::WaitingForUser | State::Verifying => {
                transition(&tx, &t, State::Cancelling, now)?;
            }
            _ => return Err(StorageError::InvalidState("task cannot be cancelled now")),
        }
        let s = summary(&tx, &load_task(&tx, task_id)?)?;
        tx.commit()?;
        Ok(s)
    }

    /// Exactly-once artifact acceptance. Reward row, meter update, acceptance, state change
    /// and request result commit in one transaction.
    pub fn accept_artifact(&mut self, req: &AcceptRequest<'_>) -> Result<AcceptOutcome> {
        validate_request_id(req.request_id)?;
        let params_sha256 = sha256_hex(canonical_accept_params(req).as_bytes());
        let now = self.now();
        let tx = self.write()?;
        let prior: Option<(String, String, String)> = tx
            .query_row(
                "SELECT method, params_sha256, response FROM request_results
                 WHERE request_id = ?1",
                [req.request_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some((method, sha, response)) = prior {
            if method != ACCEPT_METHOD || sha != params_sha256 {
                return Err(StorageError::IdempotencyConflict);
            }
            return serde_json::from_str(&response).map_err(|_| StorageError::Corrupt);
        }
        let t = load_task(&tx, req.task_id)?;
        let digest = t
            .review_digest
            .as_deref()
            .ok_or(StorageError::InvalidState("task has no review"))?;
        if req.review_digest != digest {
            return Err(StorageError::ReviewChanged);
        }
        if !req.acknowledged {
            return Err(StorageError::AcknowledgementRequired);
        }
        let outcome = if let Some(receipt) = load_receipt(&tx, &t.id)? {
            AcceptOutcome {
                receipt,
                duplicate: true,
            }
        } else {
            if t.state != State::ReviewReady {
                return Err(StorageError::InvalidState("task is not review_ready"));
            }
            // The digest binds acceptance to the sealed content, not just the stored column.
            if !review_intact(&tx, &t)? {
                return Err(StorageError::ReviewDamaged);
            }
            let verification = t.review_verification.ok_or(StorageError::Corrupt)?;
            let reason = req.override_reason.map(str::trim);
            if reason.is_some_and(|r| r.len() > MAX_OVERRIDE_REASON_BYTES || r.contains('\0')) {
                return Err(StorageError::InvalidParams(
                    "override_reason must be at most 512 bytes after trimming, without NUL",
                ));
            }
            if verification != Verification::Passed && reason.is_none_or(str::is_empty) {
                return Err(StorageError::OverrideRequired);
            }
            let reason = reason.filter(|r| !r.is_empty()).map(str::to_owned);
            let col = t.category.as_str();
            let before: u16 = tx.query_row(
                &format!("SELECT {col} FROM pet_state WHERE id = 1"),
                [],
                get0,
            )?;
            let after = Meters::apply_reward(before);
            tx.execute(
                &format!("UPDATE pet_state SET {col} = ?1, updated_at_ms = ?2 WHERE id = 1"),
                params![after, now],
            )?;
            tx.execute(
                "INSERT INTO rewards (task_id, category, amount_hundredths, applied_hundredths,
                    meter_before, meter_after, status, granted_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'granted', ?7)",
                params![
                    t.id,
                    col,
                    REWARD_HUNDREDTHS,
                    after - before,
                    before,
                    after,
                    now
                ],
            )?;
            tx.execute(
                "INSERT INTO acceptances (task_id, request_id, review_digest, verification,
                    override_reason, accepted_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    t.id,
                    req.request_id,
                    digest,
                    verification.as_str(),
                    reason,
                    now
                ],
            )?;
            transition(&tx, &t, State::AcceptedArtifact, now)?;
            AcceptOutcome {
                receipt: ReceiptView {
                    task_id: t.id.clone(),
                    category: t.category,
                    amount_hundredths: REWARD_HUNDREDTHS,
                    verification,
                    override_reason: reason,
                    request_id: req.request_id.to_owned(),
                    accepted_at_ms: now,
                },
                duplicate: false,
            }
        };
        let response = serde_json::to_string(&outcome)
            .map_err(|_| StorageError::Unavailable("cannot encode receipt"))?;
        tx.execute(
            "INSERT INTO request_results (request_id, method, params_sha256, response,
                created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![req.request_id, ACCEPT_METHOD, params_sha256, response, now],
        )?;
        fault("accept_before_commit");
        tx.commit()?;
        fault("accept_after_commit");
        Ok(outcome)
    }

    /// Discards a sealed review (`review_ready`, or abandons `recovery_required`) for the
    /// exact stored digest. Never rewards, restores or changes files, so unlike acceptance it
    /// does not require the artifact to still match: it is the way out of a damaged review.
    /// Repeating it on a discarded task is a no-op.
    pub fn discard(&mut self, task_id: &str, review_digest: &str) -> Result<TaskSummary> {
        let now = self.now();
        let tx = self.write()?;
        let t = load_task(&tx, task_id)?;
        let digest = t
            .review_digest
            .as_deref()
            .ok_or(StorageError::InvalidState("task has no review"))?;
        if digest != review_digest {
            return Err(StorageError::ReviewChanged);
        }
        match t.state {
            State::Discarded => {}
            State::ReviewReady | State::RecoveryRequired => {
                transition(&tx, &t, State::Discarded, now)?;
            }
            _ => return Err(StorageError::InvalidState("task cannot be discarded now")),
        }
        let s = summary(&tx, &load_task(&tx, task_id)?)?;
        tx.commit()?;
        Ok(s)
    }

    /// Explicit recovery. Never resumes work: active phases whose supervisor is confirmed
    /// dead (PID + start time mismatch) become `interrupted`; a `review_ready` task whose
    /// recomputed digest differs becomes `recovery_required`.
    pub fn recover(&mut self) -> Result<RecoverResult> {
        let now = self.now();
        let tx = self.write()?;
        let tasks = load_tasks(
            &tx,
            "state IN ('preparing','running','waiting_for_user','verifying','cancelling',
                'review_ready') ORDER BY created_at_ms, rowid",
            &[],
        )?;
        let mut changed = Vec::new();
        for t in &tasks {
            let to = if t.state == State::ReviewReady {
                (!review_intact(&tx, t)?).then_some(State::RecoveryRequired)
            } else {
                (!t.supervisor.is_some_and(|s| s.is_alive())).then_some(State::Interrupted)
            };
            if let Some(to) = to {
                transition(&tx, t, to, now)?;
                changed.push(RecoveryChange {
                    task_id: t.id.clone(),
                    from: t.state,
                    to,
                });
            }
        }
        let active_task = active_task(&tx)?;
        tx.commit()?;
        Ok(RecoverResult {
            changed,
            active_task,
        })
    }

    /// Validated settings at the expected revision; invalidates unconsumed tokens.
    pub fn update_settings(
        &mut self,
        expected_revision: i64,
        settings: &Settings,
    ) -> Result<SettingsView> {
        settings
            .validate()
            .map_err(|_| StorageError::InvalidParams("settings out of range"))?;
        let now = self.now();
        let tx = self.write()?;
        let current = load_settings(&tx)?;
        if current.revision != expected_revision {
            return Err(StorageError::StaleRevision);
        }
        let r = settings.decay_per_hour;
        tx.execute(
            "UPDATE settings SET revision = revision + 1, decay_enabled = ?1, decay_hunger = ?2,
                decay_thirst = ?3, decay_mental_health = ?4, decay_hygiene = ?5,
                timeout_seconds = ?6, updated_at_ms = ?7 WHERE id = 1",
            params![
                settings.decay_enabled,
                r[0],
                r[1],
                r[2],
                r[3],
                settings.timeout_seconds,
                now
            ],
        )?;
        invalidate_tokens(&tx, None)?;
        let view = load_settings(&tx)?;
        tx.commit()?;
        Ok(view)
    }

    pub fn set_paused(&mut self, paused: bool) -> Result<PauseView> {
        let now = self.now();
        let tx = self.write()?;
        tx.execute(
            "UPDATE pet_state SET decay_paused = ?1, updated_at_ms = ?2 WHERE id = 1",
            params![paused, now],
        )?;
        tx.commit()?;
        Ok(PauseView { paused })
    }
}

impl TaskRow {
    fn from_row(r: &Row<'_>) -> rusqlite::Result<Result<Self>> {
        let text = |i: usize| r.get::<_, String>(i);
        let (state, category, provider, outcome) = (text(2)?, text(3)?, text(4)?, text(7)?);
        let verification: Option<String> = r.get(12)?;
        let start: Option<i64> = r.get(10)?;
        let pid: Option<u32> = r.get(9)?;
        let supervisor = match (pid, start) {
            (Some(pid), Some(start)) => match u64::try_from(start) {
                Ok(start_time) => Some(Supervisor { pid, start_time }),
                Err(_) => return Ok(Err(StorageError::Corrupt)),
            },
            _ => None,
        };
        let (Some(state), Some(category), Some(provider), Some(outcome)) = (
            State::parse(&state),
            Category::parse(&category),
            Provider::parse(&provider),
            MockOutcome::parse(&outcome),
        ) else {
            return Ok(Err(StorageError::Corrupt));
        };
        let review_verification = match verification.as_deref().map(Verification::parse) {
            None => None,
            Some(Some(v)) => Some(v),
            Some(None) => return Ok(Err(StorageError::Corrupt)),
        };
        Ok(Ok(Self {
            id: r.get(0)?,
            revision: r.get(1)?,
            state,
            category,
            provider,
            folder: r.get(5)?,
            instructions: r.get(6)?,
            mock: MockOptions {
                outcome,
                step_ms: r.get(8)?,
            },
            supervisor,
            review_digest: r.get(11)?,
            review_verification,
            review_simulated: r.get::<_, Option<bool>>(13)?.unwrap_or(false),
            created_at_ms: r.get(14)?,
            updated_at_ms: r.get(15)?,
        }))
    }
    fn draft(&self, scope: Vec<String>) -> Draft {
        Draft {
            category: self.category,
            provider: self.provider,
            folder: self.folder.clone(),
            scope,
            instructions: self.instructions.clone(),
        }
    }
}

fn get0<T: FromSql>(r: &Row<'_>) -> rusqlite::Result<T> {
    r.get(0)
}

fn parse<T>(s: &str, f: fn(&str) -> Option<T>) -> Result<T> {
    f(s).ok_or(StorageError::Corrupt)
}

fn start_time_sql(s: Supervisor) -> Result<i64> {
    i64::try_from(s.start_time).map_err(|_| StorageError::InvalidParams("start time too large"))
}

fn page_limit(limit: Option<u32>, max: u32) -> Result<u32> {
    match limit.unwrap_or(max) {
        0 => Err(StorageError::InvalidParams("limit must be at least 1")),
        n if n > max => Err(StorageError::InvalidParams("limit too large")),
        n => Ok(n),
    }
}

fn core_error(e: codepet_core::Error) -> StorageError {
    match e {
        codepet_core::Error::InvalidPath => StorageError::InvalidPath("folder or scope rejected"),
        codepet_core::Error::UnsupportedProvider => StorageError::UnsupportedProvider,
        _ => StorageError::InvalidParams("draft rejected"),
    }
}

fn validate_input(input: &TaskInput) -> Result<()> {
    validate_draft(&input.draft)?;
    if input.mock.step_ms > MAX_MOCK_STEP_MS {
        return Err(StorageError::InvalidParams("mock.step_ms must be 0..=2000"));
    }
    Ok(())
}

/// Core's draft rules plus the JSON budget that keeps every task summary within one frame.
fn validate_draft(d: &Draft) -> Result<()> {
    d.validate().map_err(core_error)?;
    if json_len(&(&d.folder, &d.scope, &d.instructions)) > MAX_DRAFT_JSON_BYTES {
        return Err(StorageError::InvalidParams(
            "folder, scope and instructions together exceed 32 KiB JSON-encoded",
        ));
    }
    Ok(())
}

/// Bytes `value` takes as compact JSON.
fn json_len<T: serde::Serialize + ?Sized>(value: &T) -> usize {
    serde_json::to_vec(value).map_or(usize::MAX, |v| v.len())
}

/// Longest prefix of `s` whose JSON string encoding (quotes included) is at most `max`
/// bytes. Mirrors serde_json escaping: quote, backslash and short control escapes take 2 bytes,
/// other control characters 6.
pub(crate) fn json_prefix(s: &str, max: usize) -> &str {
    let mut used = 2;
    for (i, c) in s.char_indices() {
        let n = match c {
            '"' | '\\' | '\u{8}' | '\u{c}' | '\n' | '\r' | '\t' => 2,
            c if u32::from(c) < 0x20 => 6,
            c => c.len_utf8(),
        };
        if used + n > max {
            return &s[..i];
        }
        used += n;
    }
    s
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

/// Refuses the home directory itself and any folder that contains or lies inside the state
/// directory, comparing lexical and resolved forms. Fails closed when no home is known.
fn check_folder(folder: &str, paths: &StatePaths) -> Result<()> {
    let forms = |p: &Path| {
        let mut v = vec![p.to_path_buf()];
        v.extend(resolve_existing(p));
        v
    };
    let folders = forms(Path::new(folder));
    let homes: Vec<_> = paths.homes().flat_map(|h| forms(h)).collect();
    if homes.is_empty() {
        return Err(StorageError::Unavailable(
            "cannot determine the home directory to check the folder against",
        ));
    }
    let states = forms(&paths.state_dir);
    for f in &folders {
        if homes.iter().any(|h| h == f) {
            return Err(StorageError::InvalidPath("folder is the home directory"));
        }
        if states.iter().any(|s| s.starts_with(f) || f.starts_with(s)) {
            return Err(StorageError::InvalidPath(
                "folder overlaps the CodePet state directory",
            ));
        }
    }
    Ok(())
}

/// Canonical form of `p`: the deepest existing ancestor is canonicalized (resolving
/// symlinks) and the missing components are appended. `None` if nothing resolves.
fn resolve_existing(p: &Path) -> Option<PathBuf> {
    let mut rest = Vec::new();
    let mut cur = p;
    loop {
        if let Ok(mut real) = fs::canonicalize(cur) {
            real.extend(rest.iter().rev());
            return Some(real);
        }
        rest.push(cur.file_name()?);
        cur = cur.parent()?;
    }
}

/// Canonical params JSON for idempotency: fixed sorted keys, serde_json string escaping.
fn canonical_accept_params(req: &AcceptRequest<'_>) -> String {
    let s = |v: &str| serde_json::Value::from(v).to_string();
    format!(
        r#"{{"acknowledged":{},"override_reason":{},"review_digest":{},"task_id":{}}}"#,
        req.acknowledged,
        req.override_reason.map_or_else(|| "null".to_owned(), s),
        s(req.review_digest),
        s(req.task_id)
    )
}

/// Hex SHA-256 of the canonical review manifest: keys sorted, no whitespace,
/// `{"artifact_length","artifact_sha256","category","provider","revision","simulated",
/// "task_id","verification"}`. Built by hand so feature unification cannot reorder keys.
fn review_digest(
    t: &TaskRow,
    artifact: &[u8],
    verification: Verification,
    simulated: bool,
) -> String {
    let s = |v: &str| serde_json::Value::from(v).to_string();
    let manifest = format!(
        r#"{{"artifact_length":{},"artifact_sha256":{},"category":{},"provider":{},"revision":{},"simulated":{},"task_id":{},"verification":{}}}"#,
        artifact.len(),
        s(&sha256_hex(artifact)),
        s(t.category.as_str()),
        s(t.provider.as_str()),
        t.revision,
        simulated,
        s(&t.id),
        s(verification.as_str())
    );
    sha256_hex(manifest.as_bytes())
}

/// Recomputes the review digest from the stored artifact bytes and task columns.
fn review_intact(c: &Connection, t: &TaskRow) -> Result<bool> {
    let (Some(stored), Some(verification)) = (&t.review_digest, t.review_verification) else {
        return Ok(false);
    };
    let content: Option<String> = c
        .query_row(
            "SELECT content FROM artifacts WHERE task_id = ?1",
            [&t.id],
            get0,
        )
        .optional()?;
    Ok(content.is_some_and(|a| {
        review_digest(t, a.as_bytes(), verification, t.review_simulated) == *stored
    }))
}

fn transition(c: &Connection, t: &TaskRow, to: State, now: i64) -> Result<i64> {
    if !State::allowed_transition(t.state, to) {
        return Err(StorageError::InvalidState("transition not allowed"));
    }
    let n = c
        .execute(
            "UPDATE tasks SET state = ?1, updated_at_ms = ?2
             WHERE id = ?3 AND state = ?4 AND revision = ?5",
            params![to.as_str(), now, t.id, t.state.as_str(), t.revision],
        )
        .map_err(|e| {
            // Only the single-active-slot unique index can reject this update.
            if e.sqlite_error_code() == Some(ErrorCode::ConstraintViolation) {
                StorageError::Busy
            } else {
                e.into()
            }
        })?;
    if n != 1 {
        return Err(StorageError::InvalidState("task changed concurrently"));
    }
    append_event(c, &t.id, Some(t.state), to, t.revision, now)
}

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

fn invalidate_tokens(c: &Connection, task_id: Option<&str>) -> Result<()> {
    c.execute(
        "UPDATE preflight_tokens SET status = 'invalidated'
         WHERE status = 'pending' AND (?1 IS NULL OR task_id = ?1)",
        [task_id],
    )?;
    Ok(())
}

fn insert_scopes(c: &Connection, task_id: &str, scope: &[String]) -> Result<()> {
    let mut stmt =
        c.prepare("INSERT INTO task_scopes (task_id, position, path) VALUES (?1, ?2, ?3)")?;
    for (i, path) in scope.iter().enumerate() {
        stmt.execute(params![task_id, i as i64, path])?;
    }
    Ok(())
}

fn load_scopes(c: &Connection, task_id: &str) -> Result<Vec<String>> {
    Ok(
        c.prepare("SELECT path FROM task_scopes WHERE task_id = ?1 ORDER BY position")?
            .query_map([task_id], get0)?
            .collect::<rusqlite::Result<_>>()?,
    )
}

fn load_task(c: &Connection, task_id: &str) -> Result<TaskRow> {
    c.query_row(
        &format!("SELECT {TASK_COLS} FROM tasks WHERE id = ?1"),
        [task_id],
        TaskRow::from_row,
    )
    .optional()?
    .ok_or(StorageError::NotFound)?
}

fn load_tasks(c: &Connection, filter: &str, args: &[&dyn ToSql]) -> Result<Vec<TaskRow>> {
    c.prepare(&format!("SELECT {TASK_COLS} FROM tasks WHERE {filter}"))?
        .query_map(args, TaskRow::from_row)?
        .map(|r| r?)
        .collect()
}

fn active_task_id(c: &Connection) -> Result<Option<String>> {
    Ok(c.query_row(
        &format!("SELECT id FROM tasks WHERE state IN {SLOT_STATES} LIMIT 1"),
        [],
        get0,
    )
    .optional()?)
}

fn active_task(c: &Connection) -> Result<Option<TaskBrief>> {
    active_task_id(c)?
        .map(|id| brief(c, &load_task(c, &id)?))
        .transpose()
}

fn last_seq(c: &Connection) -> Result<i64> {
    Ok(c.query_row("SELECT COALESCE(MAX(seq), 0) FROM task_events", [], get0)?)
}

fn load_settings(c: &Connection) -> Result<SettingsView> {
    Ok(c.query_row(
        "SELECT revision, decay_enabled, decay_hunger, decay_thirst, decay_mental_health,
            decay_hygiene, timeout_seconds FROM settings WHERE id = 1",
        [],
        |r| {
            Ok(SettingsView {
                revision: r.get(0)?,
                decay_enabled: r.get(1)?,
                decay_per_hour: [r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?],
                timeout_seconds: r.get(6)?,
            })
        },
    )?)
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

fn load_receipt(c: &Connection, task_id: &str) -> Result<Option<ReceiptView>> {
    let raw: Option<(String, i64, String, Option<String>, String, i64)> = c
        .query_row(
            "SELECT r.category, r.amount_hundredths, a.verification, a.override_reason,
                a.request_id, a.accepted_at_ms
             FROM acceptances a JOIN rewards r ON r.task_id = a.task_id
             WHERE a.task_id = ?1",
            [task_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .optional()?;
    raw.map(
        |(category, amount, verification, override_reason, request_id, accepted_at_ms)| {
            Ok(ReceiptView {
                task_id: task_id.to_owned(),
                category: parse(&category, Category::parse)?,
                amount_hundredths: u16::try_from(amount).map_err(|_| StorageError::Corrupt)?,
                verification: parse(&verification, Verification::parse)?,
                override_reason,
                request_id,
                accepted_at_ms,
            })
        },
    )
    .transpose()
}

fn brief(c: &Connection, t: &TaskRow) -> Result<TaskBrief> {
    let review = match (&t.review_digest, t.review_verification) {
        (Some(digest), Some(verification)) => {
            let bytes: Option<i64> = c
                .query_row(
                    "SELECT length(CAST(content AS BLOB)) FROM artifacts WHERE task_id = ?1",
                    [&t.id],
                    get0,
                )
                .optional()?;
            Some(ReviewBrief {
                digest: digest.clone(),
                verification,
                simulated: t.review_simulated,
                artifact_bytes: bytes.and_then(|b| usize::try_from(b).ok()).unwrap_or(0),
            })
        }
        _ => None,
    };
    Ok(TaskBrief {
        task_id: t.id.clone(),
        revision: t.revision,
        state: t.state,
        category: t.category,
        provider: t.provider,
        mock: t.mock,
        instructions_excerpt: json_prefix(&t.instructions, MAX_EXCERPT_JSON_BYTES).to_owned(),
        instructions_bytes: t.instructions.len(),
        instructions_sha256: sha256_hex(t.instructions.as_bytes()),
        created_at_ms: t.created_at_ms,
        updated_at_ms: t.updated_at_ms,
        review,
        receipt: load_receipt(c, &t.id)?,
    })
}

fn summary(c: &Connection, t: &TaskRow) -> Result<TaskSummary> {
    let review = match (&t.review_digest, t.review_verification) {
        (Some(digest), Some(verification)) => {
            let artifact: Option<String> = c
                .query_row(
                    "SELECT content FROM artifacts WHERE task_id = ?1",
                    [&t.id],
                    get0,
                )
                .optional()?;
            Some(ReviewView {
                digest: digest.clone(),
                verification,
                // A missing artifact row is reported as empty; recovery flags the mismatch.
                artifact: artifact.unwrap_or_default(),
                simulated: t.review_simulated,
            })
        }
        _ => None,
    };
    Ok(TaskSummary {
        task_id: t.id.clone(),
        revision: t.revision,
        state: t.state,
        category: t.category,
        provider: t.provider,
        folder: t.folder.clone(),
        scope: load_scopes(c, &t.id)?,
        instructions: t.instructions.clone(),
        mock: t.mock,
        created_at_ms: t.created_at_ms,
        updated_at_ms: t.updated_at_ms,
        review,
        receipt: load_receipt(c, &t.id)?,
    })
}
