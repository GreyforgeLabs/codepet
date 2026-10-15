//! Schema migrations keyed by SQLite `user_version`. `MIGRATIONS[i]` upgrades `i` → `i + 1`.
use codepet_core::{Settings, METER_START};
use rusqlite::{params, Transaction};

pub(crate) type Migration = fn(&Transaction<'_>, i64) -> rusqlite::Result<()>;
pub(crate) const MIGRATIONS: &[Migration] = &[v1];

/// Slot-occupying states; must equal `State::occupies_slot` (checked by a test).
pub(crate) const SLOT_STATES: &str = "('preparing','running','waiting_for_user','verifying',\
'review_ready','cancelling','recovery_required')";

const V1: &str = r"
CREATE TABLE tasks (
    id TEXT PRIMARY KEY NOT NULL,
    revision INTEGER NOT NULL CHECK (revision >= 1),
    state TEXT NOT NULL CHECK (state IN ('draft','preparing','running','waiting_for_user',
        'verifying','review_ready','cancelling','cancelled','failed','interrupted',
        'accepted_artifact','discarded','recovery_required')),
    category TEXT NOT NULL CHECK (category IN ('hunger','thirst','mental_health','hygiene')),
    provider TEXT NOT NULL CHECK (provider IN ('mock','codex','opencode','hermes','claude_code')),
    folder TEXT NOT NULL,
    instructions TEXT NOT NULL,
    mock_outcome TEXT NOT NULL CHECK (mock_outcome IN ('artifact','provider_failure','check_failed')),
    mock_step_ms INTEGER NOT NULL CHECK (mock_step_ms BETWEEN 0 AND 2000),
    supervisor_pid INTEGER,
    supervisor_start_time INTEGER,
    review_digest TEXT,
    review_verification TEXT CHECK (review_verification IN ('passed','failed','not_verified')),
    review_simulated INTEGER CHECK (review_simulated IN (0,1)),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
CREATE UNIQUE INDEX tasks_single_active_slot ON tasks((1)) WHERE state IN ('preparing',
    'running','waiting_for_user','verifying','review_ready','cancelling','recovery_required');
CREATE INDEX tasks_created ON tasks(created_at_ms);
CREATE TABLE task_scopes (
    task_id TEXT NOT NULL REFERENCES tasks(id),
    position INTEGER NOT NULL CHECK (position >= 0),
    path TEXT NOT NULL,
    PRIMARY KEY (task_id, position)
);
CREATE TABLE task_events (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    from_state TEXT CHECK (from_state IN ('draft','preparing','running','waiting_for_user',
        'verifying','review_ready','cancelling','cancelled','failed','interrupted',
        'accepted_artifact','discarded','recovery_required')),
    to_state TEXT NOT NULL CHECK (to_state IN ('draft','preparing','running','waiting_for_user',
        'verifying','review_ready','cancelling','cancelled','failed','interrupted',
        'accepted_artifact','discarded','recovery_required')),
    revision INTEGER NOT NULL,
    at_ms INTEGER NOT NULL
);
CREATE TABLE preflight_tokens (
    token_sha256 TEXT PRIMARY KEY NOT NULL,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    revision INTEGER NOT NULL,
    settings_revision INTEGER NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending','consumed','superseded','invalidated')),
    created_at_ms INTEGER NOT NULL,
    expires_at_ms INTEGER NOT NULL,
    consumed_at_ms INTEGER
);
CREATE INDEX preflight_tokens_task ON preflight_tokens(task_id, status);
CREATE TABLE checks (
    id INTEGER PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    name TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('passed','failed','not_verified')),
    simulated INTEGER NOT NULL CHECK (simulated IN (0,1)),
    recorded_at_ms INTEGER NOT NULL
);
CREATE TABLE artifacts (
    id INTEGER PRIMARY KEY,
    task_id TEXT NOT NULL UNIQUE REFERENCES tasks(id),
    kind TEXT NOT NULL CHECK (kind IN ('text')),
    content TEXT NOT NULL CHECK (length(CAST(content AS BLOB)) <= 2097152),
    sha256 TEXT NOT NULL,
    length INTEGER NOT NULL CHECK (length BETWEEN 0 AND 2097152),
    created_at_ms INTEGER NOT NULL
);
-- Reserved for the journaled apply protocol; nothing writes it in milestone 1.
CREATE TABLE apply_transactions (
    id INTEGER PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    state TEXT NOT NULL CHECK (state IN ('prepared','committed','failed','rolled_back')),
    created_at_ms INTEGER NOT NULL
);
CREATE TABLE acceptances (
    task_id TEXT PRIMARY KEY NOT NULL REFERENCES tasks(id),
    request_id TEXT NOT NULL,
    review_digest TEXT NOT NULL,
    verification TEXT NOT NULL CHECK (verification IN ('passed','failed','not_verified')),
    override_reason TEXT CHECK (override_reason IS NULL OR length(CAST(override_reason AS BLOB)) <= 512),
    accepted_at_ms INTEGER NOT NULL
);
CREATE TABLE rewards (
    id INTEGER PRIMARY KEY,
    task_id TEXT NOT NULL UNIQUE REFERENCES tasks(id),
    category TEXT NOT NULL CHECK (category IN ('hunger','thirst','mental_health','hygiene')),
    amount_hundredths INTEGER NOT NULL CHECK (amount_hundredths > 0),
    applied_hundredths INTEGER NOT NULL CHECK (applied_hundredths BETWEEN 0 AND amount_hundredths),
    meter_before INTEGER NOT NULL CHECK (meter_before BETWEEN 0 AND 10000),
    meter_after INTEGER NOT NULL CHECK (meter_after BETWEEN 0 AND 10000),
    status TEXT NOT NULL CHECK (status IN ('granted','reversed')),
    granted_at_ms INTEGER NOT NULL,
    reversed_at_ms INTEGER
);
CREATE TABLE pet_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    hunger INTEGER NOT NULL CHECK (hunger BETWEEN 0 AND 10000),
    thirst INTEGER NOT NULL CHECK (thirst BETWEEN 0 AND 10000),
    mental_health INTEGER NOT NULL CHECK (mental_health BETWEEN 0 AND 10000),
    hygiene INTEGER NOT NULL CHECK (hygiene BETWEEN 0 AND 10000),
    decay_paused INTEGER NOT NULL CHECK (decay_paused IN (0,1)),
    updated_at_ms INTEGER NOT NULL
);
CREATE TABLE settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    revision INTEGER NOT NULL CHECK (revision >= 1),
    decay_enabled INTEGER NOT NULL CHECK (decay_enabled IN (0,1)),
    decay_hunger INTEGER NOT NULL CHECK (decay_hunger BETWEEN 0 AND 500),
    decay_thirst INTEGER NOT NULL CHECK (decay_thirst BETWEEN 0 AND 500),
    decay_mental_health INTEGER NOT NULL CHECK (decay_mental_health BETWEEN 0 AND 500),
    decay_hygiene INTEGER NOT NULL CHECK (decay_hygiene BETWEEN 0 AND 500),
    timeout_seconds INTEGER NOT NULL CHECK (timeout_seconds BETWEEN 1 AND 7200),
    updated_at_ms INTEGER NOT NULL
);
CREATE TABLE request_results (
    request_id TEXT PRIMARY KEY NOT NULL,
    method TEXT NOT NULL,
    params_sha256 TEXT NOT NULL,
    response TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
);
";

fn v1(tx: &Transaction<'_>, now: i64) -> rusqlite::Result<()> {
    tx.execute_batch(V1)?;
    let s = Settings::default();
    tx.execute(
        "INSERT INTO settings (id, revision, decay_enabled, decay_hunger, decay_thirst,
            decay_mental_health, decay_hygiene, timeout_seconds, updated_at_ms)
         VALUES (1, 1, ?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            s.decay_enabled,
            s.decay_per_hour[0],
            s.decay_per_hour[1],
            s.decay_per_hour[2],
            s.decay_per_hour[3],
            s.timeout_seconds,
            now
        ],
    )?;
    tx.execute(
        "INSERT INTO pet_state (id, hunger, thirst, mental_health, hygiene, decay_paused,
            updated_at_ms) VALUES (1, ?1, ?1, ?1, ?1, 0, ?2)",
        params![METER_START, now],
    )?;
    Ok(())
}
