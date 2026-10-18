//! Schema migrations keyed by SQLite `user_version`. `MIGRATIONS[i]` upgrades `i` → `i + 1`.
use crate::care::TimeMark;
use crate::life;
use codepet_core::evolution::{level_for_xp, stages_crossed};
use codepet_core::lifecycle::{Vitals, RULESET_V1, RULES_V1};
use codepet_core::species::SPECIES_IDS;
use codepet_core::State;
use codepet_core::{Settings, METER_START};
use rusqlite::{params, Connection, OptionalExtension, Transaction};

/// One migration step. `at` is the single time sample `Store::migrate` takes before the
/// transaction; v1–v3 use only `at.wall`, exactly where they used `now` before v0.2.
pub(crate) type Migration = fn(&Transaction<'_>, &TimeMark) -> crate::Result<()>;
pub(crate) const MIGRATIONS: &[Migration] = &[v1, v2, v3, v4, v5, v6];

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

fn v1(tx: &Transaction<'_>, at: &TimeMark) -> crate::Result<()> {
    let now = at.wall;
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

/// Additive migration: old meters, rewards, artifacts, request receipts and tasks are
/// intentionally untouched. Existing tasks have no debit row and are never backcharged.
fn v2(tx: &Transaction<'_>, _at: &TimeMark) -> crate::Result<()> {
    Ok(tx.execute_batch(r"
CREATE TABLE pet_care (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision >= 1),
    starts_since_bathroom INTEGER NOT NULL DEFAULT 0 CHECK (starts_since_bathroom >= 0),
    work_sessions INTEGER NOT NULL DEFAULT 0 CHECK (work_sessions >= 0),
    execution_ms INTEGER NOT NULL DEFAULT 0 CHECK (execution_ms >= 0),
    rest_required INTEGER NOT NULL DEFAULT 0 CHECK (rest_required IN (0,1)),
    sleep_remaining_ms INTEGER NOT NULL DEFAULT 120000 CHECK (sleep_remaining_ms BETWEEN 0 AND 120000),
    sleep_started_at_ms INTEGER,
    sleep_wall_ms INTEGER,
    sleep_boot TEXT,
    sleep_steady_ms INTEGER,
    welcome_care_available INTEGER NOT NULL DEFAULT 1 CHECK (welcome_care_available IN (0,1))
);
INSERT INTO pet_care (id) VALUES (1);
CREATE TABLE task_care_usage (
    task_id TEXT PRIMARY KEY NOT NULL REFERENCES tasks(id),
    executed INTEGER NOT NULL DEFAULT 0 CHECK (executed IN (0,1)),
    refunded INTEGER NOT NULL DEFAULT 0 CHECK (refunded IN (0,1)),
    execution_ms INTEGER NOT NULL DEFAULT 0 CHECK (execution_ms >= 0),
    segment_wall_ms INTEGER,
    segment_boot TEXT,
    segment_steady_ms INTEGER
);
-- A separate ledger is the only source of demo progression. No real-work ledger is
-- populated by this release, and historical acceptances are never backfilled.
CREATE TABLE demo_xp_rewards (
    task_id TEXT PRIMARY KEY NOT NULL REFERENCES tasks(id),
    track TEXT NOT NULL CHECK (track IN ('bugfix','quality_performance','learning_specification','cleanup')),
    amount INTEGER NOT NULL CHECK (amount = 20),
    granted_at_ms INTEGER NOT NULL
);
UPDATE preflight_tokens SET status = 'invalidated' WHERE status = 'pending';
")?)
}

/// Additive migration: an audit row for every review-metadata repair made by explicit
/// recovery. The replaced column values are kept verbatim, so repairing a damaged review
/// never destroys historical data, and the row marks the repaired metadata as abandonment
/// metadata rather than a sealed review.
fn v3(tx: &Transaction<'_>, _at: &TimeMark) -> crate::Result<()> {
    Ok(tx.execute_batch(
        r"
CREATE TABLE review_repairs (
    id INTEGER PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    previous_digest TEXT,
    previous_verification TEXT,
    digest TEXT NOT NULL CHECK (length(digest) = 64),
    verification TEXT NOT NULL CHECK (verification IN ('passed','failed','not_verified')),
    verification_source TEXT NOT NULL CHECK (verification_source IN ('stored','checks','default')),
    repaired_at_ms INTEGER NOT NULL
);
CREATE INDEX review_repairs_task ON review_repairs(task_id);
",
    )?)
}

/// The `species IN (...)` list for CHECK clauses, expanded from core's roster.
fn species_sql() -> String {
    let quoted: Vec<String> = SPECIES_IDS.iter().map(|s| format!("'{s}'")).collect();
    quoted.join(",")
}

/// Lifecycle tables (spec §7). Terminal lives and their journal are immutable history:
/// triggers refuse to delete either and refuse every change to a dead or retired life.
fn v4_sql() -> String {
    let species = species_sql();
    format!(
        r"
CREATE TABLE meta (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  state_version INTEGER NOT NULL CHECK (state_version >= 1),
  presence_rejections INTEGER NOT NULL DEFAULT 0 CHECK (presence_rejections >= 0)
);
CREATE TABLE rulesets (
  id TEXT PRIMARY KEY NOT NULL CHECK (id IN ('lifecycle-v1')),
  rules_json TEXT NOT NULL CHECK (length(CAST(rules_json AS BLOB)) <= 4096),
  rules_sha256 TEXT NOT NULL CHECK (length(rules_sha256) = 64),
  installed_at_ms INTEGER NOT NULL
);
CREATE TABLE lives (
  life_id TEXT PRIMARY KEY NOT NULL CHECK (length(life_id) = 29 AND life_id GLOB 'life-*'),
  ordinal INTEGER NOT NULL UNIQUE CHECK (ordinal >= 1),
  name TEXT NOT NULL CHECK (length(CAST(name AS BLOB)) BETWEEN 1 AND 96),
  species TEXT NOT NULL CHECK (species IN ({species})),
  appearance TEXT NOT NULL CHECK (appearance IN ({species})),
  origin TEXT NOT NULL CHECK (origin IN ('migrated','first_run','adopted')),
  status TEXT NOT NULL CHECK (status IN ('healthy','warning','critical','dead','retired')),
  mode TEXT NOT NULL CHECK (mode IN ('permadeath','gentle')),
  armed INTEGER NOT NULL DEFAULT 0 CHECK (armed IN (0,1)),
  armed_at_ms INTEGER,
  pace TEXT NOT NULL CHECK (pace IN ('steady','playtest')),
  ruleset TEXT NOT NULL REFERENCES rulesets(id),
  decay_hunger INTEGER NOT NULL CHECK (decay_hunger BETWEEN 0 AND 25000),
  decay_thirst INTEGER NOT NULL CHECK (decay_thirst BETWEEN 0 AND 25000),
  decay_mental_health INTEGER NOT NULL CHECK (decay_mental_health BETWEEN 0 AND 25000),
  decay_hygiene INTEGER NOT NULL CHECK (decay_hygiene BETWEEN 0 AND 25000),
  full_hold_ms INTEGER NOT NULL CHECK (full_hold_ms BETWEEN 60000 AND 14400000),
  warning_at INTEGER NOT NULL CHECK (warning_at BETWEEN 1000 AND 5000),
  critical_after_ms INTEGER NOT NULL CHECK (critical_after_ms BETWEEN 300000 AND 86400000),
  grace_ms INTEGER NOT NULL CHECK (grace_ms BETWEEN 900000 AND 172800000 AND grace_ms >= critical_after_ms),
  revision INTEGER NOT NULL DEFAULT 1 CHECK (revision >= 1),
  adopted_at_ms INTEGER NOT NULL,
  adoption_request_id TEXT,
  ended_at_ms INTEGER,
  end_cause TEXT CHECK (end_cause IN ('neglect:hunger','neglect:thirst','retired')),
  final_summary TEXT CHECK (final_summary IS NULL OR (json_valid(final_summary) AND length(CAST(final_summary AS BLOB)) <= 2048)),
  CHECK ((status IN ('dead','retired')) = (ended_at_ms IS NOT NULL)),
  CHECK ((status IN ('dead','retired')) = (end_cause IS NOT NULL)),
  CHECK ((status IN ('dead','retired')) = (final_summary IS NOT NULL)),
  CHECK (status != 'dead' OR end_cause LIKE 'neglect:%'),
  CHECK (status != 'retired' OR end_cause = 'retired'),
  CHECK ((armed = 1) = (armed_at_ms IS NOT NULL))
);
CREATE UNIQUE INDEX lives_one_living ON lives((1)) WHERE status IN ('healthy','warning','critical');
CREATE TRIGGER lives_terminal_immutable BEFORE UPDATE ON lives WHEN OLD.status IN ('dead','retired')
BEGIN SELECT RAISE(ABORT, 'terminal life is immutable'); END;
CREATE TRIGGER lives_never_deleted BEFORE DELETE ON lives
BEGIN SELECT RAISE(ABORT, 'lives are history'); END;
CREATE TRIGGER lives_identity_immutable BEFORE UPDATE ON lives
WHEN NEW.life_id IS NOT OLD.life_id OR NEW.ordinal IS NOT OLD.ordinal
  OR NEW.species IS NOT OLD.species OR NEW.origin IS NOT OLD.origin
  OR NEW.pace IS NOT OLD.pace OR NEW.ruleset IS NOT OLD.ruleset
  OR NEW.adopted_at_ms IS NOT OLD.adopted_at_ms
  OR NEW.adoption_request_id IS NOT OLD.adoption_request_id
  OR (OLD.armed = 1 AND (NEW.armed IS NOT OLD.armed OR NEW.armed_at_ms IS NOT OLD.armed_at_ms))
BEGIN SELECT RAISE(ABORT, 'life identity, pace and ruleset are immutable'); END;

CREATE TABLE life_vitals (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  life_id TEXT REFERENCES lives(life_id),
  hold_hunger INTEGER NOT NULL DEFAULT 0 CHECK (hold_hunger >= 0),
  hold_thirst INTEGER NOT NULL DEFAULT 0 CHECK (hold_thirst >= 0),
  hold_mental_health INTEGER NOT NULL DEFAULT 0 CHECK (hold_mental_health >= 0),
  hold_hygiene INTEGER NOT NULL DEFAULT 0 CHECK (hold_hygiene >= 0),
  carry_hunger INTEGER NOT NULL DEFAULT 0 CHECK (carry_hunger BETWEEN 0 AND 3599999),
  carry_thirst INTEGER NOT NULL DEFAULT 0 CHECK (carry_thirst BETWEEN 0 AND 3599999),
  carry_mental_health INTEGER NOT NULL DEFAULT 0 CHECK (carry_mental_health BETWEEN 0 AND 3599999),
  carry_hygiene INTEGER NOT NULL DEFAULT 0 CHECK (carry_hygiene BETWEEN 0 AND 3599999),
  neglect_hunger_ms INTEGER NOT NULL DEFAULT 0 CHECK (neglect_hunger_ms >= 0),
  neglect_thirst_ms INTEGER NOT NULL DEFAULT 0 CHECK (neglect_thirst_ms >= 0),
  critical_need TEXT CHECK (critical_need IN ('hunger','thirst')),
  critical_grace_ms INTEGER,
  critical_elapsed_ms INTEGER NOT NULL DEFAULT 0 CHECK (critical_elapsed_ms >= 0),
  critical_since_ms INTEGER,
  fainted INTEGER NOT NULL DEFAULT 0 CHECK (fainted IN (0,1)),
  death_pending INTEGER NOT NULL DEFAULT 0 CHECK (death_pending IN (0,1)),
  attended_total_ms INTEGER NOT NULL DEFAULT 0 CHECK (attended_total_ms >= 0),
  CHECK ((critical_need IS NULL) = (critical_grace_ms IS NULL)),
  CHECK ((critical_need IS NULL) = (critical_since_ms IS NULL)),
  CHECK (death_pending = 0 OR critical_need IS NOT NULL)
);
CREATE TABLE presence_clock (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  last_boot TEXT, last_steady_ms INTEGER, last_wall_ms INTEGER
);
CREATE TABLE presence_sessions (
  session_id TEXT PRIMARY KEY NOT NULL CHECK (length(session_id) BETWEEN 10 AND 62),
  consumed_ms INTEGER NOT NULL CHECK (consumed_ms >= 0),
  credited_ms INTEGER NOT NULL DEFAULT 0 CHECK (credited_ms >= 0),
  discarded_ms INTEGER NOT NULL DEFAULT 0 CHECK (discarded_ms >= 0),
  first_seen_at_ms INTEGER NOT NULL,
  last_settled_at_ms INTEGER NOT NULL
);
CREATE TRIGGER presence_sessions_never_deleted BEFORE DELETE ON presence_sessions
BEGIN SELECT RAISE(ABORT, 'the presence ledger is never pruned'); END;
CREATE TABLE life_events (
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  life_id TEXT NOT NULL REFERENCES lives(life_id),
  kind TEXT NOT NULL CHECK (kind IN ('born','armed','mode_changed','configured','health_changed',
    'evolved','appearance_changed','paused','resumed','critical','recovered','fainted',
    'death_pending','died','retired')),
  dedup_key TEXT,
  data TEXT NOT NULL DEFAULT '{{}}' CHECK (json_valid(data) AND length(CAST(data AS BLOB)) <= 1024),
  task_id TEXT REFERENCES tasks(id),
  request_id TEXT,
  at_ms INTEGER NOT NULL,
  seen_at_ms INTEGER,
  UNIQUE (life_id, kind, dedup_key)
);
CREATE UNIQUE INDEX life_events_one_terminal ON life_events(life_id) WHERE kind IN ('died','retired');
CREATE INDEX life_events_unseen ON life_events(life_id, seen_at_ms) WHERE kind = 'evolved';
CREATE TRIGGER life_events_no_delete BEFORE DELETE ON life_events
BEGIN SELECT RAISE(ABORT, 'life events are history'); END;
CREATE TRIGGER life_events_only_seen_mutable BEFORE UPDATE ON life_events
WHEN NEW.seq IS NOT OLD.seq OR NEW.life_id IS NOT OLD.life_id OR NEW.kind IS NOT OLD.kind
  OR NEW.dedup_key IS NOT OLD.dedup_key OR NEW.data IS NOT OLD.data OR NEW.task_id IS NOT OLD.task_id
  OR NEW.request_id IS NOT OLD.request_id
  OR NEW.at_ms IS NOT OLD.at_ms OR (OLD.seen_at_ms IS NOT NULL AND NEW.seen_at_ms IS NOT OLD.seen_at_ms)
BEGIN SELECT RAISE(ABORT, 'only seen_at_ms may be set, once'); END;

ALTER TABLE tasks ADD COLUMN life_id TEXT REFERENCES lives(life_id);
ALTER TABLE demo_xp_rewards ADD COLUMN life_id TEXT REFERENCES lives(life_id);
CREATE INDEX demo_xp_rewards_life ON demo_xp_rewards(life_id);
UPDATE preflight_tokens SET status = 'invalidated' WHERE status = 'pending';
"
    )
}

/// Additive lifecycle migration (spec §7, numbered v4 because v3 is `review_repairs`).
/// No existing row is deleted and no meter changes. Life #1 ("Ginger") is created
/// **unarmed**: no decay, neglect or countdown until the rules are explicitly
/// acknowledged, so an upgrade can never cost a pet. Every historic task and demo-XP row
/// is attached to it, so its XP and level equal the old account totals.
fn v4(tx: &Transaction<'_>, at: &TimeMark) -> crate::Result<()> {
    let now = at.wall;
    tx.execute_batch(&v4_sql())?;
    // 1. Version counter and the canonical ruleset.
    tx.execute("INSERT INTO meta (id, state_version) VALUES (1, 1)", [])?;
    tx.execute(
        "INSERT INTO rulesets (id, rules_json, rules_sha256, installed_at_ms)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            RULESET_V1,
            life::lifecycle_rules_json(),
            life::lifecycle_rules_sha256(),
            now
        ],
    )?;
    // A brand-new v0.3 install (spec §2.1, K7) starts without a companion: no life #1, an
    // empty vitals row and the presence clock; `pet.start` adopts the picked species.
    if fresh_no_pet(tx)? {
        tx.execute("INSERT INTO life_vitals (id, life_id) VALUES (1, NULL)", [])?;
        tx.execute(
            "INSERT INTO presence_clock (id, last_boot, last_steady_ms, last_wall_ms)
             VALUES (1, ?1, ?2, ?3)",
            params![at.boot, at.steady, now],
        )?;
        return Ok(());
    }
    // 2–3. Life #1.
    let life1 = life::new_life_id()?;
    let history: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM tasks) OR EXISTS(SELECT 1 FROM demo_xp_rewards)",
        [],
        |r| r.get(0),
    )?;
    let origin = if history { "migrated" } else { "first_run" };
    let meters: [u16; 4] = tx.query_row(
        "SELECT hunger, thirst, mental_health, hygiene FROM pet_state WHERE id = 1",
        [],
        |r| Ok([r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?]),
    )?;
    let r = &RULES_V1.steady;
    let status = if meters.iter().any(|&m| m <= r.warning_at) {
        "warning"
    } else {
        "healthy"
    };
    tx.execute(
        "INSERT INTO lives (life_id, ordinal, name, species, appearance, origin, status, mode,
            armed, armed_at_ms, pace, ruleset, decay_hunger, decay_thirst, decay_mental_health,
            decay_hygiene, full_hold_ms, warning_at, critical_after_ms, grace_ms, adopted_at_ms)
         VALUES (?1, 1, 'Ginger', 'cat', 'cat', ?2, ?3, 'permadeath', 0, NULL, 'steady', ?4,
            ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            life1,
            origin,
            status,
            RULESET_V1,
            r.decay_per_hour[0],
            r.decay_per_hour[1],
            r.decay_per_hour[2],
            r.decay_per_hour[3],
            r.full_hold_ms,
            r.warning_at,
            r.critical_after_ms,
            r.grace_ms,
            now
        ],
    )?;
    // 4. Vitals: every full meter starts a satiety hold; nothing is backdated.
    let v = Vitals::from_meters(meters, r);
    tx.execute(
        "INSERT INTO life_vitals (id, life_id, hold_hunger, hold_thirst, hold_mental_health,
            hold_hygiene) VALUES (1, ?1, ?2, ?3, ?4, ?5)",
        params![
            life1,
            v.hold_ms[0],
            v.hold_ms[1],
            v.hold_ms[2],
            v.hold_ms[3]
        ],
    )?;
    // 5. All historic work belongs to life #1.
    tx.execute("UPDATE tasks SET life_id = ?1", [&life1])?;
    tx.execute("UPDATE demo_xp_rewards SET life_id = ?1", [&life1])?;
    // 6. Presence recorded before the migration is never charged.
    tx.execute(
        "INSERT INTO presence_clock (id, last_boot, last_steady_ms, last_wall_ms)
         VALUES (1, ?1, ?2, ?3)",
        params![at.boot, at.steady, now],
    )?;
    // 7. Journal: born, plus already reached stages as seen history (never celebrated).
    tx.execute(
        "INSERT INTO life_events (life_id, kind, dedup_key, data, at_ms)
         VALUES (?1, 'born', NULL, ?2, ?3)",
        params![
            life1,
            serde_json::json!({ "origin": origin }).to_string(),
            now
        ],
    )?;
    let xp: Option<i64> = tx
        .query_row("SELECT SUM(amount) FROM demo_xp_rewards", [], |r| r.get(0))
        .optional()?
        .flatten();
    let xp = u32::try_from(xp.unwrap_or(0)).map_err(|_| crate::StorageError::Corrupt)?;
    for stage in stages_crossed(0, xp, r) {
        let data = serde_json::json!({
            "from": stage - 1, "to": stage, "xp": xp, "level": level_for_xp(xp, r)
        });
        tx.execute(
            "INSERT INTO life_events (life_id, kind, dedup_key, data, task_id, at_ms, seen_at_ms)
             VALUES (?1, 'evolved', ?2, ?3, NULL, ?4, ?4)",
            params![life1, format!("stage:{stage}"), data.to_string(), now],
        )?;
    }
    Ok(())
}

/// The TEMP marker `Store::migrate` creates for a brand-new database opened by v0.3 (target
/// schema 5 or later). Schema v4 then creates no first-run life: a fresh install has no
/// companion until **Let's go!**. Without it (an upgrade of a v1-v3 database, or a test
/// building the v0.2 shape) v4 behaves exactly as it did in v0.2.
pub(crate) const FRESH_NO_PET: &str = "codepet_fresh_no_pet";

fn fresh_no_pet(tx: &Transaction<'_>) -> crate::Result<bool> {
    Ok(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM temp.sqlite_master WHERE type = 'table' AND name = ?1)",
        [FRESH_NO_PET],
        |r| r.get(0),
    )?)
}

/// The v5 DDL, verbatim from the frozen contract (`docs/v0.3/contracts/schema-v5.sql`):
/// `meta` additions (the On/Off switch, background helpers, the hook salt and the agent
/// sequence fields) and the agent tables (`agent_events`, `agent_sessions`,
/// `activity_turns`, `activity_xp`, `client_hooks`). One source, so the migration can never
/// drift from the contract.
const V5_DDL: &str = include_str!("../../../docs/v0.3/contracts/schema-v5.sql");

/// Where a leftover slot task goes in the v5 migration (spec §6.6 step 3). `None` for a
/// state that holds no slot.
pub(crate) fn migration_close_state(from: State) -> Option<State> {
    match from {
        State::Preparing
        | State::Running
        | State::Verifying
        | State::Cancelling
        | State::WaitingForUser => Some(State::Interrupted),
        State::ReviewReady | State::RecoveryRequired => Some(State::Discarded),
        _ => None,
    }
}

/// Closes one leftover slot task for the v5 migration (spec §6.5, §6.6): a raw state
/// transition plus its `task_events` row, with no reward, refund or care change and no
/// supervisor. It stays outside `legacy-tasks` because the default build's migration uses
/// it. Refuses (`InvalidState`) a task that does not hold the slot or changed meanwhile.
/// Returns the new state.
pub(crate) fn close_task_for_migration(
    c: &Connection,
    task_id: &str,
    now: i64,
) -> crate::Result<State> {
    let row: Option<(String, i64)> = c
        .query_row(
            "SELECT state, revision FROM tasks WHERE id = ?1",
            [task_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (state, revision) = row.ok_or(crate::StorageError::NotFound)?;
    let from = State::parse(&state).ok_or(crate::StorageError::Corrupt)?;
    // Validated against the same list the single-slot index and the v4 rules use.
    if !SLOT_STATES.contains(&format!("'{}'", from.as_str())) {
        return Err(crate::StorageError::InvalidState(
            "only a slot task is closed by the migration",
        ));
    }
    let to = migration_close_state(from).ok_or(crate::StorageError::Corrupt)?;
    let n = c.execute(
        "UPDATE tasks SET state = ?1, updated_at_ms = ?2 WHERE id = ?3 AND state = ?4
         AND revision = ?5",
        params![to.as_str(), now, task_id, from.as_str(), revision],
    )?;
    if n != 1 {
        return Err(crate::StorageError::InvalidState(
            "task changed concurrently",
        ));
    }
    c.execute(
        "INSERT INTO task_events (task_id, from_state, to_state, revision, at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![task_id, from.as_str(), to.as_str(), revision, now],
    )?;
    Ok(to)
}

/// Schema v5 (spec §6.6, `schema-v5.sql`): keeps every existing pet. Runs inside the
/// migration transaction after the `VACUUM INTO …bak-v4-<ms>` backup; any failure rolls the
/// whole migration back.
///
/// 1. The DDL (new tables, `meta` columns, triggers).
/// 2. `meta.hook_salt` = 32 random bytes.
/// 3. Leftover slot tasks closed with [`close_task_for_migration`] (no reward), pending
///    preflight tokens invalidated.
/// 4. `life_vitals.death_pending` cleared with a `recovered` journal note (`v5_migration`);
///    normal settlement re-evaluates the life afterwards.
/// 5. The rest counters zeroed (`work_sessions`, `execution_ms`, `rest_required`,
///    `starts_since_bathroom`); a nap in progress is left alone.
///
/// Lives (name, look, XP, stage, mode, pace, armed state), the journal, memorials and the
/// historic tables are untouched.
fn v5(tx: &Transaction<'_>, at: &TimeMark) -> crate::Result<()> {
    let now = at.wall;
    tx.execute_batch(V5_DDL)?;
    let salt = crate::sys::random_bytes::<32>()?;
    tx.execute("UPDATE meta SET hook_salt = ?1 WHERE id = 1", [&salt[..]])?;
    let slot_tasks: Vec<String> = tx
        .prepare(&format!(
            "SELECT id FROM tasks WHERE state IN {SLOT_STATES} ORDER BY rowid"
        ))?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for id in &slot_tasks {
        close_task_for_migration(tx, id, now)?;
    }
    tx.execute(
        "UPDATE preflight_tokens SET status = 'invalidated' WHERE status = 'pending'",
        [],
    )?;
    let pending: Option<String> = tx
        .query_row(
            "SELECT v.life_id FROM life_vitals v JOIN lives l ON l.life_id = v.life_id
             WHERE v.id = 1 AND v.death_pending = 1
             AND l.status IN ('healthy','warning','critical')",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(life_id) = pending {
        tx.execute(
            "INSERT INTO life_events (life_id, kind, dedup_key, data, at_ms)
             VALUES (?1, 'recovered', 'v5_migration', ?2, ?3)
             ON CONFLICT (life_id, kind, dedup_key) DO NOTHING",
            params![
                life_id,
                serde_json::json!({ "reason": "v5_migration" }).to_string(),
                now
            ],
        )?;
    }
    tx.execute("UPDATE life_vitals SET death_pending = 0 WHERE id = 1", [])?;
    tx.execute(
        "UPDATE pet_care SET work_sessions = 0, execution_ms = 0, rest_required = 0,
            starts_since_bathroom = 0 WHERE id = 1",
        [],
    )?;
    Ok(())
}

/// Preserve every existing life and frozen base rule. Tempo changes are explicit;
/// only newly started Gentle lives choose Lively during creation.
fn v6(tx: &Transaction<'_>, _at: &TimeMark) -> crate::Result<()> {
    Ok(tx.execute_batch(
        "ALTER TABLE lives ADD COLUMN care_tempo TEXT NOT NULL DEFAULT 'steady'
        CHECK (care_tempo IN ('steady','lively'));",
    )?)
}
