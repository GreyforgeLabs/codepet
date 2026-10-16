//! The coding-client records of v0.3 (spec §5.2, §6.2; `client_hooks` in
//! `docs/v0.3/contracts/schema-v5.sql`): what `clients.status` detected, what the
//! Connect/Disconnect/test jobs did, and the running job of each client.
//!
//! Writers: `clients.status` (file-only detection results), the RPC that starts a job
//! (marks the job running, with a receipt), the detached `codepet clients-job` (per-client
//! results, the ready check's `meta.test_*`) and the fold (`last_seen_ms`, `first_seen_ms`,
//! `events_total`; see `agent_store`). Every visible change bumps `state_version`, so the
//! monotonic export picks it up. Nothing here settles care or touches a life (E1).
use crate::agent_export::{ClientExport, ClientsExport, JobExport};
use crate::store::{replay_or_run, seal};
use crate::{Result, StorageError, Store};
use codepet_core::agent::{ClientId, AGENT_EVENTS_KEEP_MS};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

/// The kinds of a clients job (`client_hooks.job_kind`, the export's `job.kind`).
pub const JOB_KINDS: [&str; 3] = ["connect", "disconnect", "test"];
/// `needs_step` values.
pub const STEP_CODEX_TRUST: &str = "codex_trust";
pub const STEP_CODEX_HOOKS_OFF: &str = "codex_hooks_off";
/// `last_reason` values (`protocol.json` `job_results.reason`).
pub const REASONS: [&str; 5] = [
    "not_installed",
    "too_old",
    "blocked",
    "settings_changed",
    "ready_check",
];

/// One `client_hooks` row, plus `events_7d` (computed, never stored; notes.md §2 item 7).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClientRow {
    pub client: String,
    pub installed: bool,
    pub detected_version: Option<String>,
    pub checked_at_ms: Option<i64>,
    pub connected: bool,
    pub needs_step: Option<String>,
    pub installed_version: Option<String>,
    pub installed_at_ms: Option<i64>,
    pub last_seen_ms: Option<i64>,
    pub first_seen_ms: Option<i64>,
    pub events_total: i64,
    pub last_outcome: Option<String>,
    pub last_reason: Option<String>,
    pub job_id: Option<String>,
    pub job_kind: Option<String>,
    pub events_7d: i64,
}
impl ClientRow {
    /// The reason of the last failed job, else `None` (the export's `failed_reason`).
    pub fn failed_reason(&self) -> Option<&str> {
        match self.last_outcome.as_deref() {
            Some("failed") => self.last_reason.as_deref(),
            _ => None,
        }
    }
    fn export(&self) -> ClientExport {
        ClientExport {
            installed: self.installed,
            version: self.detected_version.clone(),
            connected: self.connected,
            needs_step: self.needs_step.clone(),
            last_seen_ms: self.last_seen_ms,
            first_seen_ms: self.first_seen_ms,
            failed_reason: self.failed_reason().map(str::to_owned),
            job: match (&self.job_id, &self.job_kind) {
                (Some(id), Some(kind)) => Some(JobExport {
                    id: id.clone(),
                    kind: kind.clone(),
                    state: "running".to_owned(),
                }),
                _ => None,
            },
        }
    }
}

/// What one `clients.status` run found for a client (file-only, spec §5.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusUpdate {
    pub client: ClientId,
    /// The binary and the config dir exist.
    pub installed: bool,
    /// The version read from install paths or package metadata.
    pub version: Option<String>,
    /// `Some(connected)` when the artifact files decide it (present and current, missing,
    /// or the client's `disconnected` flag); `None` keeps the stored value.
    pub connected: Option<bool>,
    /// `Some(step)` replaces `needs_step` (`Some(None)` clears it); `None` keeps it.
    pub needs_step: Option<Option<&'static str>>,
}

/// The result of one client in a Connect or Disconnect job (spec §5.2 step 5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobClientResult {
    pub client: ClientId,
    /// `connected`, `needs_step`, `failed` or `disconnected`.
    pub outcome: &'static str,
    /// The artifact is installed (a failure after the artifact step leaves it connected).
    pub connected: bool,
    pub needs_step: Option<&'static str>,
    pub reason: Option<&'static str>,
    /// The client version the artifact was installed for (Connect only).
    pub installed_version: Option<String>,
    /// A successful Connect clears `first_seen_ms` ("Waiting for {app}" until a real event).
    pub reset_first_seen: bool,
}

const ROW_COLUMNS: &str = "client, installed, detected_version, checked_at_ms, connected,
    needs_step, installed_version, installed_at_ms, last_seen_ms, first_seen_ms, events_total,
    last_outcome, last_reason, job_id, job_kind";

fn row_from(r: &rusqlite::Row<'_>) -> rusqlite::Result<ClientRow> {
    Ok(ClientRow {
        client: r.get(0)?,
        installed: r.get(1)?,
        detected_version: r.get(2)?,
        checked_at_ms: r.get(3)?,
        connected: r.get(4)?,
        needs_step: r.get(5)?,
        installed_version: r.get(6)?,
        installed_at_ms: r.get(7)?,
        last_seen_ms: r.get(8)?,
        first_seen_ms: r.get(9)?,
        events_total: r.get(10)?,
        last_outcome: r.get(11)?,
        last_reason: r.get(12)?,
        job_id: r.get(13)?,
        job_kind: r.get(14)?,
        events_7d: 0,
    })
}

/// Every `client_hooks` row in client order, with `events_7d` at `now`.
pub(crate) fn rows(c: &Connection, now: i64) -> Result<Vec<ClientRow>> {
    let mut stmt = c.prepare(&format!("SELECT {ROW_COLUMNS} FROM client_hooks"))?;
    let mut rows: Vec<ClientRow> = stmt
        .query_map([], row_from)?
        .collect::<rusqlite::Result<_>>()?;
    for row in &mut rows {
        row.events_7d = events_7d(c, &row.client, now)?;
    }
    rows.sort_by_key(|r| {
        ClientId::parse(&r.client)
            .map(|c| c as u8)
            .unwrap_or(u8::MAX)
    });
    Ok(rows)
}

/// `agent_vectors.json` `fold.events_7d`: real folded events of the last 7 days.
fn events_7d(c: &Connection, client: &str, now: i64) -> Result<i64> {
    Ok(c.query_row(
        "SELECT COUNT(*) FROM agent_events WHERE client = ?1 AND test = 0
           AND outcome IN ('folded','no_life') AND at_ms > ?2",
        params![client, now - AGENT_EVENTS_KEEP_MS],
        |r| r.get(0),
    )?)
}

/// The export's `clients` map: one entry per row (empty before the first status).
pub(crate) fn export(c: &Connection) -> Result<ClientsExport> {
    let mut stmt = c.prepare(&format!("SELECT {ROW_COLUMNS} FROM client_hooks"))?;
    let rows = stmt
        .query_map([], row_from)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .map(|r| (r.client.clone(), r.export()))
        .collect())
}

fn ensure_row(c: &Connection, client: &str) -> Result<()> {
    c.execute(
        "INSERT INTO client_hooks (client) VALUES (?1) ON CONFLICT(client) DO NOTHING",
        [client],
    )?;
    Ok(())
}

fn bump(c: &Connection) -> Result<()> {
    c.execute(
        "UPDATE meta SET state_version = state_version + 1 WHERE id = 1",
        [],
    )?;
    Ok(())
}

/// The visible part of a row, to tell whether a write changed what the export shows.
fn visible(c: &Connection, client: &str) -> Result<Option<ClientExport>> {
    Ok(c.query_row(
        &format!("SELECT {ROW_COLUMNS} FROM client_hooks WHERE client = ?1"),
        [client],
        row_from,
    )
    .optional()?
    .map(|r| r.export()))
}

/// The receipt of a job-starting method: the job id it returned.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct JobReceipt {
    job_id: String,
}

/// How a job-starting RPC picks its clients.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobTarget {
    /// Exactly these clients (`clients.connect`, `clients.disconnect`).
    Named(Vec<ClientId>),
    /// The connected clients, or only this one if it is connected (`clients.test`).
    Connected(Option<ClientId>),
}

/// What `codepet doctor` shows from `meta` (read only).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DoctorMeta {
    pub enabled: bool,
    pub count_helpers: bool,
    pub events_dropped: i64,
    pub test_state: Option<String>,
}

impl Store {
    /// `meta` fields for `codepet doctor`; a plain read that changes nothing.
    pub fn doctor_meta(&self) -> Result<DoctorMeta> {
        Ok(self.conn.query_row(
            "SELECT enabled, count_helpers, events_dropped, test_state FROM meta WHERE id = 1",
            [],
            |r| {
                Ok(DoctorMeta {
                    enabled: r.get(0)?,
                    count_helpers: r.get(1)?,
                    events_dropped: r.get(2)?,
                    test_state: r.get(3)?,
                })
            },
        )?)
    }

    /// Every `client_hooks` row in client order, with `events_7d`.
    pub fn client_rows(&self) -> Result<Vec<ClientRow>> {
        rows(&self.conn, self.clock.now_ms())
    }

    /// `clients.status` (spec §5.2): records what the file-only detection found for every
    /// client, in one write transaction, and returns the rows. `checked_at_ms` is always
    /// written; `state_version` moves only when something the export shows changed.
    pub fn record_clients_status(&mut self, updates: &[StatusUpdate]) -> Result<Vec<ClientRow>> {
        let now = self.clock.now_ms();
        let tx = rusqlite::Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let mut changed = false;
        for u in updates {
            let id = u.client.as_str();
            ensure_row(&tx, id)?;
            let before = visible(&tx, id)?;
            let version = u.version.as_deref().filter(|v| v.len() <= 64);
            tx.execute(
                "UPDATE client_hooks SET installed = ?2, detected_version = ?3, checked_at_ms = ?4
                 WHERE client = ?1",
                params![id, u.installed, version, now],
            )?;
            if let Some(connected) = u.connected {
                tx.execute(
                    "UPDATE client_hooks SET connected = ?2 WHERE client = ?1",
                    params![id, connected],
                )?;
            }
            if let Some(step) = u.needs_step {
                tx.execute(
                    "UPDATE client_hooks SET needs_step = ?2 WHERE client = ?1",
                    params![id, step],
                )?;
            }
            changed |= visible(&tx, id)? != before;
        }
        if changed {
            bump(&tx)?;
        }
        tx.commit()?;
        self.client_rows()
    }

    /// Starts a clients job from an RPC (`clients.connect`, `clients.disconnect`,
    /// `clients.test`): the receipt (a replay returns the same job id and starts nothing),
    /// the Off policy, `BUSY` when a named client already has a running job, then marks
    /// the job running on each of its clients. Returns `(job_id, clients, replayed)`.
    /// `INVALID_STATE` when a test finds no connected client.
    #[allow(clippy::too_many_arguments)]
    pub fn start_clients_job(
        &mut self,
        request_id: &str,
        method: &str,
        canonical_params: &str,
        policy_refuses_off: bool,
        kind: &str,
        target: &JobTarget,
        job_id: &str,
    ) -> Result<(String, Vec<ClientId>, bool)> {
        if !JOB_KINDS.contains(&kind) {
            return Err(StorageError::InvalidParams("unknown job kind"));
        }
        let sha = crate::sys::sha256_hex(canonical_params.as_bytes());
        let (tx, at, _ended) = self.write_settled()?;
        let mut picked: Vec<ClientId> = Vec::new();
        let (receipt, replayed): (JobReceipt, bool) =
            replay_or_run(&tx, request_id, method, &sha, at.wall, || {
                if policy_refuses_off {
                    crate::app::ensure_on(&tx)?;
                }
                picked = match target {
                    JobTarget::Named(list) => list.clone(),
                    JobTarget::Connected(only) => {
                        let rows = rows(&tx, at.wall)?;
                        ClientId::ALL
                            .into_iter()
                            .filter(|c| only.is_none_or(|o| o == *c))
                            .filter(|c| rows.iter().any(|r| r.client == c.as_str() && r.connected))
                            .collect()
                    }
                };
                if picked.is_empty() {
                    return Err(StorageError::InvalidState("no connected app to check"));
                }
                for c in &picked {
                    ensure_row(&tx, c.as_str())?;
                    let busy: Option<String> = tx.query_row(
                        "SELECT job_id FROM client_hooks WHERE client = ?1",
                        [c.as_str()],
                        |r| r.get(0),
                    )?;
                    if busy.is_some() {
                        return Err(StorageError::JobRunning);
                    }
                }
                for c in &picked {
                    tx.execute(
                        "UPDATE client_hooks SET job_id = ?2, job_kind = ?3 WHERE client = ?1",
                        params![c.as_str(), job_id, kind],
                    )?;
                }
                seal(&tx, &at)?;
                Ok(JobReceipt {
                    job_id: job_id.to_owned(),
                })
            })?;
        if replayed {
            drop(tx);
        } else {
            tx.commit()?;
        }
        Ok((receipt.job_id, picked, replayed))
    }

    /// The clients a job is still marked running on.
    pub fn job_clients(&self, job_id: &str) -> Result<Vec<ClientId>> {
        let mut stmt = self
            .conn
            .prepare("SELECT client FROM client_hooks WHERE job_id = ?1")?;
        let names = stmt
            .query_map([job_id], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut out: Vec<ClientId> = names.iter().filter_map(|n| ClientId::parse(n)).collect();
        out.sort_by_key(|c| *c as u8);
        Ok(out)
    }

    /// Records one client's Connect/Disconnect result (spec §5.2 step 5); the job stays
    /// marked running until [`finish_clients_job`](Self::finish_clients_job).
    pub fn record_job_result(&mut self, r: &JobClientResult) -> Result<()> {
        let now = self.clock.now_ms();
        let id = r.client.as_str();
        let tx = rusqlite::Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        ensure_row(&tx, id)?;
        tx.execute(
            "UPDATE client_hooks SET connected = ?2, needs_step = ?3, last_outcome = ?4,
               last_reason = ?5 WHERE client = ?1",
            params![id, r.connected, r.needs_step, r.outcome, r.reason],
        )?;
        if let Some(v) = r.installed_version.as_deref().filter(|v| v.len() <= 64) {
            tx.execute(
                "UPDATE client_hooks SET installed_version = ?2, installed_at_ms = ?3
                 WHERE client = ?1",
                params![id, v, now],
            )?;
        }
        if r.reset_first_seen {
            tx.execute(
                "UPDATE client_hooks SET first_seen_ms = NULL WHERE client = ?1",
                [id],
            )?;
        }
        bump(&tx)?;
        tx.commit()?;
        Ok(())
    }

    /// Ends a job: clears its running mark on every client still carrying it.
    pub fn finish_clients_job(&mut self, job_id: &str) -> Result<()> {
        let tx = rusqlite::Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let n = tx.execute(
            "UPDATE client_hooks SET job_id = NULL, job_kind = NULL WHERE job_id = ?1",
            [job_id],
        )?;
        if n > 0 {
            bump(&tx)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Clears the running mark of jobs that no longer exist (their journal is gone or done
    /// and their process is dead). `keep` lists the job ids still alive.
    pub fn clear_dead_jobs(&mut self, keep: &[String]) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT job_id FROM client_hooks WHERE job_id IS NOT NULL")?;
        let ids = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        let dead: Vec<String> = ids.into_iter().filter(|id| !keep.contains(id)).collect();
        for id in &dead {
            self.finish_clients_job(id)?;
        }
        Ok(dead)
    }

    /// The ready check starts (`protocol.json` `clients.test`): `meta.test_state = running`,
    /// `test_at_ms = now`, `test_client` = the first tested client in client order.
    pub fn begin_ready_check(&mut self, clients: &[ClientId]) -> Result<i64> {
        let now = self.clock.now_ms();
        let first = ClientId::ALL
            .into_iter()
            .find(|c| clients.contains(c))
            .map(ClientId::as_str);
        let tx = rusqlite::Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        tx.execute(
            "UPDATE meta SET test_state = 'running', test_at_ms = ?1, test_client = ?2
             WHERE id = 1",
            params![now, first],
        )?;
        bump(&tx)?;
        tx.commit()?;
        Ok(now)
    }

    /// The tested clients whose ready-check `turn_done` record was folded since `since_ms`.
    pub fn ready_check_arrivals(
        &self,
        clients: &[ClientId],
        since_ms: i64,
    ) -> Result<Vec<ClientId>> {
        let mut out = Vec::new();
        for c in clients {
            let n: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM agent_events WHERE client = ?1 AND test = 1
                   AND kind = 'turn_done' AND outcome = 'test' AND at_ms >= ?2",
                params![c.as_str(), since_ms],
                |r| r.get(0),
            )?;
            if n > 0 {
                out.push(*c);
            }
        }
        Ok(out)
    }

    /// The ready check ends: `done` when every tested client arrived, else `failed`, with
    /// `test_at_ms = now` either way (the celebration window starts here). Clients that did
    /// not arrive get `last_outcome = failed`, `last_reason = ready_check`; arrived clients
    /// whose last failure was a ready check are cleared.
    pub fn finish_ready_check(
        &mut self,
        tested: &[ClientId],
        arrived: &[ClientId],
    ) -> Result<bool> {
        let now = self.clock.now_ms();
        let done = tested.iter().all(|c| arrived.contains(c));
        let tx = rusqlite::Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        tx.execute(
            "UPDATE meta SET test_state = ?1, test_at_ms = ?2 WHERE id = 1",
            params![if done { "done" } else { "failed" }, now],
        )?;
        for c in tested {
            ensure_row(&tx, c.as_str())?;
            if arrived.contains(c) {
                tx.execute(
                    "UPDATE client_hooks SET last_outcome = CASE WHEN needs_step IS NULL
                       THEN 'connected' ELSE 'needs_step' END, last_reason = NULL
                     WHERE client = ?1 AND last_outcome = 'failed' AND last_reason = 'ready_check'",
                    [c.as_str()],
                )?;
            } else {
                tx.execute(
                    "UPDATE client_hooks SET last_outcome = 'failed', last_reason = 'ready_check'
                     WHERE client = ?1",
                    [c.as_str()],
                )?;
            }
        }
        bump(&tx)?;
        tx.commit()?;
        Ok(done)
    }
}
