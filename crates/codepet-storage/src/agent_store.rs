//! Agent tables of schema v5 (spec §6.4, `docs/v0.3/contracts/schema-v5.sql`): the fold's
//! per-event transitions, the treat check, retention and the `agent` export block.
//!
//! Invariant E1: the fold writes only `agent_events`, `agent_sessions`, `activity_turns`,
//! `client_hooks`, the agent fields of `meta` (plus `state_version`, which orders the
//! export) and the export. It never settles, decays, revives or ends a life, and never
//! touches care, arming or life state. It never migrates: a database whose version is not
//! [`AGENT_SCHEMA_VERSION`] is a failure of class `schema`.
use crate::agent_export::{AgentExport, AppExport, TestExport, TreatsExport};
use crate::spool::{
    hmac_key, InboxRecord, DOMAIN_APPROVAL, DOMAIN_DEDUPE, DOMAIN_SESSION, DOMAIN_TOOL, DOMAIN_TURN,
};
use crate::{Clock, Result, StatePaths, StorageError, Store, DB_FILE};
use codepet_core::agent::{
    apply_session_event, approval_transition, caps_verdict, fold_gate, pet_state, turn_qualifies,
    CapVerdict, ClientId, DoneTurn, EventKind, FoldGate, FoldOutcome, OpenTurn, Session,
    SessionEvent, SessionRow, SessionState, TreatCheck, AGENT_EVENTS_KEEP_MS,
    AGENT_EVENTS_MAX_ROWS, AGENT_SESSIONS_KEEP_MS, CAP_SESSION_MS, DAY_MS, ERROR_RECENT_MS,
    EVENT_BUCKET_MS, FUTURE_CLAMP_MS, HOUR_MS, JAR_MAX, RETENTION_INTERVAL_MS, RETENTION_LIMIT,
    TURN_TIMES_EXACT_MS,
};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use std::cell::RefCell;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// The schema the fold, the agent export and the treat jar need. WP0 keeps
/// [`crate::SCHEMA_VERSION`] at 4 until WP1's v5 migration lands; then both are 5.
pub const AGENT_SCHEMA_VERSION: i64 = 5;

/// The living companion's statuses (the `lives_one_living` index).
const LIVING: &str = "('healthy','warning','critical')";

/// What one fold transaction did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FoldBatch {
    /// Records committed with an outcome (folded, dropped or test).
    pub recorded: usize,
    /// Duplicate deliveries (the dedupe key existed): no row changed.
    pub duplicates: usize,
    /// Added to `meta.events_dropped`: poison files, an overflow marker and records whose
    /// savepoint failed.
    pub dropped: usize,
    /// `state_version` moved (the export has something new).
    pub changed: bool,
    /// An export-relevant field changed (§8 rule 6): export at once.
    pub urgent: bool,
}

fn private_entry(path: &Path, uid: u32, dir: bool) -> Result<()> {
    let m = fs::symlink_metadata(path)?;
    let ok = if dir { m.is_dir() } else { m.is_file() };
    if !ok || m.uid() != uid || m.mode() & 0o077 != 0 {
        return Err(StorageError::UnsafeStateDir(format!(
            "{}: not a private entry of this user",
            path.display()
        )));
    }
    Ok(())
}

fn user_version(c: &Connection) -> Result<i64> {
    Ok(c.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

impl Store {
    /// Opens the database for the fold: the state dir and the database must already
    /// exist and be private (nothing is created), and the schema must be exactly
    /// [`AGENT_SCHEMA_VERSION`] (nothing is migrated).
    pub fn open_for_fold(state_dir: &Path, clock: Arc<dyn Clock>) -> Result<Self> {
        let uid = rustix::process::geteuid().as_raw();
        private_entry(state_dir, uid, true)?;
        let db_path = state_dir.join(DB_FILE);
        private_entry(&db_path, uid, false)?;
        for suffix in ["-wal", "-shm"] {
            let mut side = db_path.clone().into_os_string();
            side.push(suffix);
            let side = std::path::PathBuf::from(side);
            if fs::symlink_metadata(&side).is_ok() {
                private_entry(&side, uid, false)?;
            }
        }
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW;
        let conn = Connection::open_with_flags(&db_path, flags)?;
        conn.busy_timeout(Duration::from_millis(2000))?;
        conn.execute_batch("PRAGMA foreign_keys = ON")?;
        let version = user_version(&conn)?;
        if version != AGENT_SCHEMA_VERSION {
            return Err(StorageError::SchemaUnsupported(version));
        }
        let mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(StorageError::Unavailable("WAL journal mode unavailable"));
        }
        conn.execute_batch("PRAGMA synchronous = FULL")?;
        Ok(Self {
            conn,
            clock,
            paths: StatePaths {
                state_dir: state_dir.to_path_buf(),
                db_path,
                home: None,
                account_home: None,
                data_dir: None,
            },
        })
    }

    /// The database can be folded into (schema v5).
    pub fn agent_ready(&self) -> Result<bool> {
        Ok(user_version(&self.conn)? == AGENT_SCHEMA_VERSION)
    }

    /// Folds one batch of inbox records in one `BEGIN IMMEDIATE` transaction (§6.4), each
    /// under its own savepoint. `dropped` (poison files and an overflow marker) is added
    /// to `meta.events_dropped`. Commits; the caller unlinks the files and exports.
    pub fn fold_batch(&mut self, events: &[InboxRecord], dropped: usize) -> Result<FoldBatch> {
        let now = self.clock.now_ms();
        let mut tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if user_version(&tx)? != AGENT_SCHEMA_VERSION {
            return Err(StorageError::SchemaUnsupported(user_version(&tx)?));
        }
        let ctx = FoldCtx::load(&tx, now)?;
        let before = Signature::read(&tx, now)?;
        let mut out = FoldBatch {
            dropped,
            ..FoldBatch::default()
        };
        for rec in events {
            let sp = tx.savepoint()?;
            match fold_one(&sp, &ctx, rec) {
                Ok(true) => {
                    sp.commit()?;
                    out.recorded += 1;
                }
                Ok(false) => {
                    sp.commit()?;
                    out.duplicates += 1;
                }
                // Rolled back alone (Savepoint drops with rollback); the file is unlinked.
                Err(_) => out.dropped += 1,
            }
        }
        if out.dropped > 0 {
            tx.execute(
                "UPDATE meta SET events_dropped = events_dropped + ?1 WHERE id = 1",
                [i64::try_from(out.dropped).unwrap_or(i64::MAX)],
            )?;
        }
        if out.recorded > 0 || out.dropped > 0 {
            tx.execute(
                "UPDATE meta SET state_version = state_version + 1 WHERE id = 1",
                [],
            )?;
            out.changed = true;
        }
        out.urgent = Signature::read(&tx, now)? != before;
        crate::fault("fold_before_commit");
        tx.commit()?;
        Ok(out)
    }

    /// Hourly retention (§6.4) in its own transaction; `Ok(false)` when it ran less than
    /// an hour ago. Deletes are indexed and capped at [`RETENTION_LIMIT`] rows each.
    /// `activity_turns` and `activity_xp` rows are never deleted, only coarsened.
    pub fn agent_retention(&mut self) -> Result<bool> {
        let now = self.clock.now_ms();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if user_version(&tx)? != AGENT_SCHEMA_VERSION {
            return Ok(false);
        }
        let last: Option<i64> =
            tx.query_row("SELECT retention_at_ms FROM meta WHERE id = 1", [], |r| {
                r.get(0)
            })?;
        if last.is_some_and(|l| l <= now && now - l < RETENTION_INTERVAL_MS) {
            return Ok(false);
        }
        let limit = i64::from(RETENTION_LIMIT);
        tx.execute(
            "DELETE FROM agent_events WHERE id IN
               (SELECT id FROM agent_events WHERE at_ms < ?1 ORDER BY at_ms LIMIT ?2)",
            params![now - AGENT_EVENTS_KEEP_MS, limit],
        )?;
        tx.execute(
            "DELETE FROM agent_events WHERE id IN
               (SELECT id FROM agent_events ORDER BY id DESC LIMIT ?2 OFFSET ?1)",
            params![i64::from(AGENT_EVENTS_MAX_ROWS), limit],
        )?;
        // Ended sessions after 30 days; a session that never reported its end stopped
        // counting 30 minutes after its last event, so after 30 days it goes too.
        tx.execute(
            "DELETE FROM agent_sessions WHERE session_key IN
               (SELECT session_key FROM agent_sessions WHERE last_event_ms < ?1 LIMIT ?2)",
            params![now - AGENT_SESSIONS_KEEP_MS, limit],
        )?;
        for col in ["earned_at_ms", "given_at_ms"] {
            tx.execute(
                &format!(
                    "UPDATE activity_turns SET {col} = {col} - ({col} % ?1) WHERE id IN
                       (SELECT id FROM activity_turns WHERE {col} > 0 AND {col} < ?2
                        AND {col} % ?1 != 0 LIMIT ?3)"
                ),
                params![HOUR_MS, now - TURN_TIMES_EXACT_MS, limit],
            )?;
        }
        tx.execute("UPDATE meta SET retention_at_ms = ?1 WHERE id = 1", [now])?;
        tx.commit()?;
        Ok(true)
    }

    /// The export's `agent` block from committed rows (read-only).
    pub fn agent_export(&self) -> Result<AgentExport> {
        agent_export(&self.conn, self.clock.now_ms())
    }
}

/// Per-transaction context of the fold.
struct FoldCtx {
    now: i64,
    enabled: bool,
    test_running: bool,
    salt: Vec<u8>,
    life_id: Option<String>,
}
impl FoldCtx {
    fn load(c: &Connection, now: i64) -> Result<Self> {
        let (enabled, test_state, salt): (bool, Option<String>, Option<Vec<u8>>) = c.query_row(
            "SELECT enabled, test_state, hook_salt FROM meta WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let salt = salt
            .filter(|s| s.len() == 32)
            .ok_or(StorageError::Unavailable("meta.hook_salt is missing"))?;
        let life_id = c
            .query_row(
                &format!("SELECT life_id FROM lives WHERE status IN {LIVING}"),
                [],
                |r| r.get(0),
            )
            .optional()?;
        Ok(Self {
            now,
            enabled,
            test_running: test_state.as_deref() == Some("running"),
            salt,
            life_id,
        })
    }
    fn key(&self, domain: &str, client: &str, parts: &[&str]) -> String {
        hmac_key(&self.salt, domain, client, parts)
    }
}

#[allow(clippy::too_many_arguments)]
fn insert_event(
    c: &Connection,
    dedupe: &str,
    rec: &InboxRecord,
    at_ms: i64,
    outcome: FoldOutcome,
    session: Option<&str>,
    turn: Option<&str>,
    tool: Option<&str>,
) -> Result<()> {
    c.execute(
        "INSERT INTO agent_events (dedupe_key, client, kind, class, at_ms, outcome, session_hmac,
           turn_hmac, tool_hmac, interactive, test)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            dedupe,
            rec.client,
            rec.kind,
            rec.note,
            at_ms,
            outcome.as_str(),
            session,
            turn,
            tool,
            rec.interactive,
            rec.test,
        ],
    )?;
    Ok(())
}

/// Folds one record. `Ok(false)` = a duplicate delivery (nothing changed).
fn fold_one(c: &Connection, ctx: &FoldCtx, rec: &InboxRecord) -> Result<bool> {
    let (Some(client), Some(kind)) = (ClientId::parse(&rec.client), EventKind::parse(&rec.kind))
    else {
        return Err(StorageError::InvalidParams("inbox record"));
    };
    let cid = client.as_str();
    let field = rec.session_field();
    let bucket = rec.at_ms.div_euclid(EVENT_BUCKET_MS).to_string();
    let dedupe = ctx.key(DOMAIN_DEDUPE, cid, &[&rec.kind, &field, &rec.dig, &bucket]);
    let exists: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM agent_events WHERE dedupe_key = ?1)",
        [&dedupe],
        |r| r.get(0),
    )?;
    if exists {
        return Ok(false);
    }
    let clamped = if rec.at_ms > ctx.now + FUTURE_CLAMP_MS {
        ctx.now
    } else {
        rec.at_ms
    };
    let at = match fold_gate(ctx.enabled, rec.at_ms, ctx.now, rec.test, ctx.test_running) {
        FoldGate::Stop(outcome) => {
            insert_event(c, &dedupe, rec, clamped, outcome, None, None, None)?;
            return Ok(true);
        }
        FoldGate::Record { at_ms } => at_ms,
    };
    client_record(c, cid, at)?;
    let Some(life_id) = ctx.life_id.as_deref() else {
        insert_event(c, &dedupe, rec, at, FoldOutcome::NoLife, None, None, None)?;
        return Ok(true);
    };
    if let (None, Some(akey)) = (&rec.session, &rec.akey) {
        let key = ctx.key(DOMAIN_APPROVAL, cid, &[akey]);
        if let Some(state) = approval_transition(kind) {
            c.execute(
                "INSERT INTO agent_sessions (session_key, client, approval, interactive, state,
                   state_since_ms, last_event_ms)
                 VALUES (?1, ?2, 1, 1, ?3, ?4, ?4)
                 ON CONFLICT(session_key) DO UPDATE SET
                   state_since_ms = CASE WHEN state = excluded.state THEN state_since_ms
                                         ELSE excluded.state_since_ms END,
                   state = excluded.state,
                   last_event_ms = MAX(last_event_ms, excluded.last_event_ms)",
                params![key, cid, state.as_str(), at],
            )?;
        }
        insert_event(c, &dedupe, rec, at, FoldOutcome::Folded, None, None, None)?;
        return Ok(true);
    }
    let Some(session) = rec.session.as_deref() else {
        return Err(StorageError::InvalidParams(
            "inbox record without a session",
        ));
    };
    let skey = ctx.key(DOMAIN_SESSION, cid, &[session]);
    let turn_key = match (&rec.turn, kind) {
        (Some(t), _) => Some(ctx.key(DOMAIN_TURN, cid, &[session, t])),
        (None, EventKind::TurnStart) => {
            let ev = format!("~ev:{}", rec.event_id());
            Some(ctx.key(DOMAIN_TURN, cid, &[session, &ev]))
        }
        (None, _) => None,
    };
    let tool_key = rec
        .tool
        .as_deref()
        .map(|t| ctx.key(DOMAIN_TOOL, cid, &[session, t]));
    let mut s = load_session(c, &skey)?.unwrap_or_else(|| Session::new(at));
    let failure: RefCell<Option<rusqlite::Error>> = RefCell::new(None);
    let ev = SessionEvent {
        kind,
        at_ms: at,
        turn: turn_key.as_deref(),
        tool: tool_key.as_deref(),
        payload_tools: rec.tools,
    };
    let t = apply_session_event(&mut s, &ev, |turn, tool, since| {
        c.query_row(
            "SELECT EXISTS(SELECT 1 FROM agent_events WHERE session_hmac = ?1 AND turn_hmac = ?2
               AND tool_hmac = ?3 AND at_ms >= ?4 AND outcome = 'folded')",
            params![skey, turn, tool, since],
            |r| r.get(0),
        )
        .unwrap_or_else(|e| {
            failure.borrow_mut().get_or_insert(e);
            false
        })
    });
    if let Some(e) = failure.into_inner() {
        return Err(e.into());
    }
    let interactive = rec.interactive;
    let mut earned = false;
    let mut verdict = None;
    if let Some(check) = &t.treat {
        (earned, verdict) = treat_check(c, life_id, &skey, cid, check, interactive)?;
        if earned {
            if let Some(d) = s.done.as_mut() {
                d.earned = true;
            }
        }
    }
    if t.finished && interactive {
        c.execute(
            "UPDATE meta SET finished_seq = finished_seq + 1, finished_at_ms = ?1,
               finished_app = ?2, finished_treat = ?3, finished_jar_full = ?4 WHERE id = 1",
            params![at, cid, earned, verdict == Some(CapVerdict::JarFull)],
        )?;
    } else if earned {
        // A late tool earned the finished turn's treat: its celebration shows the treat.
        if let Some(check) = &t.treat {
            c.execute(
                "UPDATE meta SET finished_treat = 1
                 WHERE id = 1 AND finished_app = ?1 AND finished_at_ms = ?2",
                params![cid, check.at_ms],
            )?;
        }
    }
    save_session(c, &skey, cid, interactive, &s)?;
    let counted = t.counted_tool_turn.is_some();
    let turn_col = t.counted_tool_turn.as_deref().or(turn_key.as_deref());
    let tool_col = tool_key.as_deref().filter(|_| counted);
    insert_event(
        c,
        &dedupe,
        rec,
        at,
        FoldOutcome::Folded,
        Some(&skey),
        turn_col,
        tool_col,
    )?;
    Ok(true)
}

/// Step 5: `last_seen_ms`, `events_total`; the first real event since Connect sets
/// `first_seen_ms`, clears `needs_step` and greets (`hello_seq++`), even with no life.
fn client_record(c: &Connection, client: &str, at: i64) -> Result<()> {
    c.execute(
        "INSERT INTO client_hooks (client) VALUES (?1) ON CONFLICT(client) DO NOTHING",
        [client],
    )?;
    let first: Option<i64> = c.query_row(
        "SELECT first_seen_ms FROM client_hooks WHERE client = ?1",
        [client],
        |r| r.get(0),
    )?;
    c.execute(
        "UPDATE client_hooks SET last_seen_ms = MAX(COALESCE(last_seen_ms, ?2), ?2),
           events_total = events_total + 1 WHERE client = ?1",
        params![client, at],
    )?;
    if first.is_none() {
        c.execute(
            "UPDATE client_hooks SET first_seen_ms = ?2, needs_step = NULL WHERE client = ?1",
            params![client, at],
        )?;
        c.execute(
            "UPDATE meta SET hello_seq = hello_seq + 1, hello_at_ms = ?2, hello_app = ?1
             WHERE id = 1",
            params![client, at],
        )?;
    }
    Ok(())
}

/// The treat check (§6.4 step 8): X1, qualification, then the durable caps. Returns
/// whether a treat dropped into the jar and the cap verdict when the caps ran.
fn treat_check(
    c: &Connection,
    life_id: &str,
    session_key: &str,
    client: &str,
    check: &TreatCheck,
    interactive: bool,
) -> Result<(bool, Option<CapVerdict>)> {
    let earned_before: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM activity_turns WHERE session_key = ?1 AND turn_key = ?2)",
        params![session_key, check.turn_key],
        |r| r.get(0),
    )?;
    if earned_before || !turn_qualifies(true, check.tools, check.duration_ms, interactive, false) {
        return Ok((false, None));
    }
    let at = check.at_ms;
    let count = |sql: &str, p: &[&dyn rusqlite::ToSql]| -> Result<u32> {
        let n: i64 = c.query_row(sql, p, |r| r.get(0))?;
        Ok(u32::try_from(n).unwrap_or(u32::MAX))
    };
    let jar = count(
        "SELECT COUNT(*) FROM activity_turns WHERE life_id = ?1 AND given_at_ms IS NULL",
        &[&life_id],
    )?;
    let session_recent = count(
        "SELECT COUNT(*) FROM activity_turns WHERE session_key = ?1 AND earned_at_ms > 0
           AND earned_at_ms > ?2",
        &[&session_key, &(at - CAP_SESSION_MS)],
    )? > 0;
    let window = "SELECT COUNT(*) FROM activity_turns WHERE earned_at_ms > 0 AND earned_at_ms > ?1";
    let hour = count(window, &[&(at - HOUR_MS)])?;
    let day = count(window, &[&(at - DAY_MS)])?;
    let verdict = caps_verdict(jar, session_recent, hour, day);
    if verdict != CapVerdict::Allow {
        return Ok((false, Some(verdict)));
    }
    c.execute(
        "INSERT INTO activity_turns (session_key, turn_key, client, life_id, earned_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![session_key, check.turn_key, client, life_id, at.max(0)],
    )?;
    Ok((true, Some(verdict)))
}

fn load_session(c: &Connection, key: &str) -> Result<Option<Session>> {
    let row = c
        .query_row(
            "SELECT state, state_since_ms, last_event_ms, open_turn_key, open_turn_started_ms,
               open_turn_tools, last_done_turn_key, last_done_started_ms, last_done_ms,
               last_done_tools, last_done_earned, last_done_rechecked, error_at_ms
             FROM agent_sessions WHERE session_key = ?1 AND approval = 0",
            [key],
            |r| {
                let state: String = r.get(0)?;
                let open = match (r.get::<_, Option<String>>(3)?, r.get::<_, Option<i64>>(4)?) {
                    (Some(key), Some(started_ms)) => Some(OpenTurn {
                        key,
                        started_ms,
                        tools: r.get(5)?,
                    }),
                    _ => None,
                };
                let done = match (r.get::<_, Option<String>>(6)?, r.get::<_, Option<i64>>(8)?) {
                    (Some(key), Some(done_ms)) => Some(DoneTurn {
                        key,
                        started_ms: r.get::<_, Option<i64>>(7)?.unwrap_or(done_ms),
                        done_ms,
                        tools: r.get(9)?,
                        earned: r.get(10)?,
                        rechecked: r.get(11)?,
                    }),
                    _ => None,
                };
                Ok((
                    state,
                    Session {
                        state: SessionState::Idle,
                        state_since_ms: r.get(1)?,
                        last_event_ms: r.get(2)?,
                        open,
                        done,
                        error_at_ms: r.get(12)?,
                    },
                ))
            },
        )
        .optional()?;
    row.map(|(state, mut s)| {
        s.state = SessionState::parse(&state).ok_or(StorageError::Corrupt)?;
        Ok(s)
    })
    .transpose()
}

fn save_session(
    c: &Connection,
    key: &str,
    client: &str,
    interactive: bool,
    s: &Session,
) -> Result<()> {
    let open = s.open.as_ref();
    let done = s.done.as_ref();
    c.execute(
        "INSERT INTO agent_sessions (session_key, client, approval, interactive, state,
           state_since_ms, last_event_ms, open_turn_key, open_turn_started_ms, open_turn_tools,
           last_done_turn_key, last_done_started_ms, last_done_ms, last_done_tools,
           last_done_earned, last_done_rechecked, error_at_ms)
         VALUES (?1, ?2, 0, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
         ON CONFLICT(session_key) DO UPDATE SET interactive = excluded.interactive,
           state = excluded.state, state_since_ms = excluded.state_since_ms,
           last_event_ms = excluded.last_event_ms, open_turn_key = excluded.open_turn_key,
           open_turn_started_ms = excluded.open_turn_started_ms,
           open_turn_tools = excluded.open_turn_tools,
           last_done_turn_key = excluded.last_done_turn_key,
           last_done_started_ms = excluded.last_done_started_ms,
           last_done_ms = excluded.last_done_ms, last_done_tools = excluded.last_done_tools,
           last_done_earned = excluded.last_done_earned,
           last_done_rechecked = excluded.last_done_rechecked,
           error_at_ms = excluded.error_at_ms",
        params![
            key,
            client,
            interactive,
            s.state.as_str(),
            s.state_since_ms,
            s.last_event_ms,
            open.map(|o| &o.key),
            open.map(|o| o.started_ms),
            open.map_or(0, |o| o.tools),
            done.map(|d| &d.key),
            done.map(|d| d.started_ms),
            done.map(|d| d.done_ms),
            done.map_or(0, |d| d.tools),
            done.is_some_and(|d| d.earned),
            done.is_some_and(|d| d.rechecked),
            s.error_at_ms,
        ],
    )?;
    Ok(())
}

/// The export-relevant fields of §8 rule 6 (a change exports at once).
#[derive(Debug, PartialEq, Eq)]
struct Signature {
    state: String,
    apps: Vec<(String, String)>,
    jar: u32,
    full: bool,
    finished_seq: i64,
    hello_seq: i64,
    error_app: Option<String>,
}
impl Signature {
    fn read(c: &Connection, now: i64) -> Result<Self> {
        let a = agent_export(c, now)?;
        let t = treats_export(c)?;
        Ok(Self {
            state: a.state,
            apps: a.apps.into_iter().map(|x| (x.id, x.state)).collect(),
            jar: t.jar,
            full: t.full,
            finished_seq: a.finished_seq,
            hello_seq: a.hello_seq,
            error_app: a.error_app,
        })
    }
}

type MetaRow = (
    bool,
    i64,
    Option<i64>,
    Option<String>,
    bool,
    bool,
    i64,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<String>,
);

/// The export's `agent` block (spec §8) from committed rows: `agent::pet_state` over the
/// sessions that may count, plus the celebration and ready-check fields of `meta`.
/// Before schema v5 (a v0.2 database not yet migrated) it is the default block.
pub(crate) fn agent_export(c: &Connection, now: i64) -> Result<AgentExport> {
    if user_version(c)? < AGENT_SCHEMA_VERSION {
        return Ok(AgentExport::default());
    }
    let m: MetaRow = c.query_row(
        "SELECT count_helpers, finished_seq, finished_at_ms, finished_app, finished_treat,
           finished_jar_full, hello_seq, hello_at_ms, hello_app, test_state, test_at_ms,
           test_client FROM meta WHERE id = 1",
        [],
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
                r.get(10)?,
                r.get(11)?,
            ))
        },
    )?;
    let mut stmt = c.prepare_cached(
        "SELECT client, approval, interactive, state, state_since_ms, last_event_ms, error_at_ms
         FROM agent_sessions
         WHERE state IN ('working','waiting','error') OR error_at_ms > ?1",
    )?;
    let rows = stmt
        .query_map([now - ERROR_RECENT_MS], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, bool>(1)?,
                r.get::<_, bool>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, Option<i64>>(6)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter_map(
            |(client, approval, interactive, state, since, last, error)| {
                Some(SessionRow {
                    client: ClientId::parse(&client)?,
                    approval,
                    interactive,
                    state: SessionState::parse(&state)?,
                    state_since_ms: since,
                    last_event_ms: last,
                    error_at_ms: error,
                })
            },
        )
        .collect::<Vec<_>>();
    let agg = pet_state(&rows, m.0, now);
    Ok(AgentExport {
        state: agg.state.as_str().to_owned(),
        apps: agg
            .apps
            .iter()
            .map(|a| AppExport {
                id: a.client.as_str().to_owned(),
                state: a.state.as_str().to_owned(),
                since_ms: a.since_ms,
                stale_at_ms: a.stale_at_ms,
            })
            .collect(),
        stale_at_ms: agg.stale_at_ms,
        finished_seq: m.1,
        finished_at_ms: m.2,
        finished_app: m.3,
        finished_treat: m.4,
        finished_jar_full: m.5,
        hello_seq: m.6,
        hello_at_ms: m.7,
        hello_app: m.8,
        test: m.9.map(|state| TestExport {
            state,
            at_ms: m.10.unwrap_or(0),
            client: m.11,
        }),
        error_at_ms: agg.error_at_ms,
        error_app: agg.error_app.map(|c| c.as_str().to_owned()),
        helpers_busy: agg.helpers_busy,
    })
}

/// The export's `treats` block: the living companion's unspent treats.
pub(crate) fn treats_export(c: &Connection) -> Result<TreatsExport> {
    if user_version(c)? < AGENT_SCHEMA_VERSION {
        return Ok(TreatsExport::default());
    }
    let jar: i64 = c.query_row(
        &format!(
            "SELECT COUNT(*) FROM activity_turns WHERE given_at_ms IS NULL
               AND life_id = (SELECT life_id FROM lives WHERE status IN {LIVING})"
        ),
        [],
        |r| r.get(0),
    )?;
    let jar = u32::try_from(jar).unwrap_or(u32::MAX);
    Ok(TreatsExport {
        jar,
        full: jar >= JAR_MAX,
    })
}

/// Development bridge until WP1's v5 migration lands: applies the frozen contract DDL
/// (`docs/v0.3/contracts/schema-v5.sql`) to an existing v4 database, writes a random
/// `meta.hook_salt` and sets `user_version` 5. Tests and the e2e harness only; never
/// called by a shipped command. Remove when `codepet migrate` exists.
#[doc(hidden)]
pub fn dev_apply_v5_contract_schema(db_path: &Path) -> Result<()> {
    const DDL: &str = include_str!("../../../docs/v0.3/contracts/schema-v5.sql");
    let mut c = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    c.busy_timeout(Duration::from_millis(5000))?;
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let v = user_version(&tx)?;
    if v != 4 {
        return Err(StorageError::SchemaUnsupported(v));
    }
    tx.execute_batch(DDL)?;
    let salt = crate::sys::random_bytes::<32>()?;
    tx.execute("UPDATE meta SET hook_salt = ?1 WHERE id = 1", [&salt[..]])?;
    tx.pragma_update(None, "user_version", AGENT_SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
#[path = "agent_store_tests.rs"]
mod tests;
