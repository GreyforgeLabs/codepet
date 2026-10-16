//! Companion lives (spec §6, §8): selection, adoption, rules, leniency, retirement,
//! celebration dedupe, the memorial, and `finalize_if_due`.
//!
//! A life never touches tasks, artifacts, acceptances, receipts, demo XP rows, settings or
//! project files when it ends: only `lives`, `life_events`, `life_vitals`, `meta` and the
//! care revision change. Terminal lives and every journal row are immutable history,
//! enforced by triggers. Nothing in this module deletes a row.
use crate::care::{self, TimeMark};
use crate::lifecycle_store::{self, VitalsRow};
use crate::model::{
    CriticalView, LifeOutcome, LifeSummary, LifeView, LifecycleView, MemorialBrief, MemorialPage,
    MetersView, NeedView, NextStageView, RulesView, Snapshot, UnseenEvent,
};
#[cfg(feature = "legacy-tasks")]
use crate::schema::SLOT_STATES;
use crate::store::{passed_away_if, replay_or_run, seal, V03Policy};
use crate::{fault, sys, Result, StorageError, Store};
use codepet_core::evolution::{
    level_for_xp, next_level_xp, next_stage, stage_for_xp, stage_key, stages_crossed,
};
use codepet_core::lifecycle::{
    self as lc, LifeStatus, Mode, Pace, Rules, Vitals, HOUR_MS, IDLE_TIMEOUT_S, RULESET_V1,
    RULES_V1, SETTLE_CAP_MS, TICK_MS,
};
use codepet_core::species::{default_name, is_species, normalize_pet_name};
use codepet_core::{Category, METER_START};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::{json, Value};

/// Largest `pet.memorial` page.
pub const MAX_MEMORIAL_PAGE: u32 = 20;

/// `pet.adopt`. `name: None` uses the species' default name.
#[derive(Clone, Copy, Debug)]
pub struct AdoptRequest<'a> {
    pub species: &'a str,
    pub name: Option<&'a str>,
    pub mode: Mode,
    pub pace: Pace,
    pub acknowledge_rules: bool,
}

/// `pet.start`. `name: None` keeps an existing life's name, or uses the species' default
/// name for a new one.
#[derive(Clone, Copy, Debug)]
pub struct StartRequest<'a> {
    pub life_id: Option<&'a str>,
    pub species: &'a str,
    pub name: Option<&'a str>,
    pub mode: Mode,
    pub acknowledge_rules: bool,
}

/// `pet.configure_life` leniency request, in the protocol's units. Each `Some` is a new
/// absolute value that may only be more lenient than the life's current one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Leniency {
    pub grace_hours: Option<u16>,
    pub critical_after_hours: Option<u16>,
    /// 25..=100: scales the life's current decay rates down.
    pub decay_percent: Option<u8>,
}

/// A `lives` row with its effective rules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LifeRow {
    pub life_id: String,
    pub ordinal: u32,
    pub name: String,
    pub species: String,
    pub appearance: String,
    pub origin: String,
    pub status: LifeStatus,
    pub mode: Mode,
    pub armed: bool,
    pub pace: Pace,
    pub rules: Rules,
    pub revision: i64,
    pub adopted_at_ms: i64,
}

const LIFE_COLS: &str = "life_id, ordinal, name, species, appearance, origin, status, mode, \
    armed, pace, ruleset, decay_hunger, decay_thirst, decay_mental_health, decay_hygiene, \
    full_hold_ms, warning_at, critical_after_ms, grace_ms, revision, adopted_at_ms";

impl LifeRow {
    fn from_row(r: &Row<'_>) -> rusqlite::Result<Result<Self>> {
        let text = |i: usize| r.get::<_, String>(i);
        let (status, mode, pace, ruleset) = (text(6)?, text(7)?, text(9)?, text(10)?);
        let (Some(status), Some(mode), Some(pace)) = (
            LifeStatus::parse(&status),
            Mode::parse(&mode),
            Pace::parse(&pace),
        ) else {
            return Ok(Err(StorageError::Corrupt));
        };
        if ruleset != RULESET_V1 {
            // Only lifecycle-v1 exists; an unknown ruleset is never interpreted.
            return Ok(Err(StorageError::Corrupt));
        }
        let base = RULES_V1.for_pace(pace);
        let rules = Rules {
            decay_per_hour: [r.get(11)?, r.get(12)?, r.get(13)?, r.get(14)?],
            full_hold_ms: r.get(15)?,
            warning_at: r.get(16)?,
            critical_after_ms: r.get(17)?,
            grace_ms: r.get(18)?,
            ..base.clone()
        };
        if rules.validate().is_err() {
            return Ok(Err(StorageError::Corrupt));
        }
        Ok(Ok(Self {
            life_id: r.get(0)?,
            ordinal: r.get(1)?,
            name: r.get(2)?,
            species: r.get(3)?,
            appearance: r.get(4)?,
            origin: r.get(5)?,
            status,
            mode,
            armed: r.get(8)?,
            pace,
            rules,
            revision: r.get(19)?,
            adopted_at_ms: r.get(20)?,
        }))
    }
}

/// `life-` + 24 random lowercase hex digits (29 bytes, the `lives.life_id` CHECK).
pub(crate) fn new_life_id() -> Result<String> {
    Ok(format!("life-{}", sys::hex(&sys::random_bytes::<12>()?)))
}

/// Canonical JSON of ruleset `lifecycle-v1` (both paces): fixed key order, no
/// whitespace, built by hand so serde feature unification cannot reorder keys. Stored in
/// `rulesets.rules_json` and returned by `lifecycle.rules`.
pub fn lifecycle_rules_json() -> String {
    fn rules(r: &Rules) -> String {
        let lethal: Vec<String> = Category::ALL
            .into_iter()
            .filter(|c| r.is_lethal(*c))
            .map(|c| format!("\"{}\"", c.as_str()))
            .collect();
        let list = |v: &[u32]| v.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
        let stages: Vec<u32> = r.stage_min_level.iter().map(|&m| u32::from(m)).collect();
        format!(
            r#"{{"critical_after_ms":{},"decay_per_hour":[{}],"full_hold_ms":{},"grace_ms":{},"lethal":[{}],"level_xp":[{}],"stage_min_level":[{}],"warning_at":{}}}"#,
            r.critical_after_ms,
            list(&r.decay_per_hour),
            r.full_hold_ms,
            r.grace_ms,
            lethal.join(","),
            list(&r.level_xp),
            list(&stages),
            r.warning_at
        )
    }
    format!(
        r#"{{"id":"{RULESET_V1}","idle_timeout_s":{IDLE_TIMEOUT_S},"playtest":{},"settle_cap_ms":{SETTLE_CAP_MS},"steady":{},"tick_ms":{TICK_MS}}}"#,
        rules(&RULES_V1.playtest),
        rules(&RULES_V1.steady)
    )
}

/// SHA-256 (hex) of [`lifecycle_rules_json`].
pub fn lifecycle_rules_sha256() -> String {
    sys::sha256_hex(lifecycle_rules_json().as_bytes())
}

/// The living companion, if any (at most one, by the `lives_one_living` index).
pub(crate) fn active_life(c: &Connection) -> Result<Option<LifeRow>> {
    let life = c
        .query_row(
            &format!(
                "SELECT {LIFE_COLS} FROM lives
                 WHERE status IN ('healthy','warning','critical')"
            ),
            [],
            LifeRow::from_row,
        )
        .optional()?
        .transpose()?;
    if let Some(l) = &life {
        // The vitals row always belongs to the living companion.
        let owner: Option<String> =
            c.query_row("SELECT life_id FROM life_vitals WHERE id = 1", [], |r| {
                r.get(0)
            })?;
        if owner.as_deref() != Some(l.life_id.as_str()) {
            return Err(StorageError::Corrupt);
        }
    }
    Ok(life)
}

fn life_by_id(c: &Connection, life_id: &str) -> Result<Option<LifeRow>> {
    c.query_row(
        &format!("SELECT {LIFE_COLS} FROM lives WHERE life_id = ?1"),
        [life_id],
        LifeRow::from_row,
    )
    .optional()?
    .transpose()
}

#[cfg(feature = "legacy-tasks")]
pub(crate) fn is_living(c: &Connection, life_id: &str) -> Result<bool> {
    Ok(life_by_id(c, life_id)?.is_some_and(|l| l.status.is_living()))
}

/// The living companion named by a request, or the matching refusal: none living →
/// `NO_ACTIVE_PET`; another life → `STALE_REVISION`; a pending end → `INVALID_STATE`.
fn living_target(c: &Connection, life_id: &str) -> Result<(LifeRow, VitalsRow)> {
    let life = active_life(c)?.ok_or(StorageError::NoActivePet)?;
    if life.life_id != life_id {
        return Err(StorageError::StaleRevision);
    }
    let vitals = lifecycle_store::load(c)?;
    if vitals.death_pending {
        return Err(StorageError::InvalidState(
            "the companion cannot change right now; finish the current task",
        ));
    }
    Ok((life, vitals))
}

/// Appends a journal row. A row with a `dedup_key` that already exists is skipped
/// (only the uniqueness conflict is ignored; every other constraint still fails).
/// Returns the new `seq`, or `None` when deduplicated.
#[allow(clippy::too_many_arguments)]
pub(crate) fn journal(
    c: &Connection,
    life_id: &str,
    kind: &str,
    dedup_key: Option<&str>,
    data: &Value,
    task_id: Option<&str>,
    request_id: Option<&str>,
    at_ms: i64,
) -> Result<Option<i64>> {
    let n = c.execute(
        "INSERT INTO life_events (life_id, kind, dedup_key, data, task_id, request_id, at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT (life_id, kind, dedup_key) DO NOTHING",
        params![
            life_id,
            kind,
            dedup_key,
            data.to_string(),
            task_id,
            request_id,
            at_ms
        ],
    )?;
    Ok((n == 1).then(|| c.last_insert_rowid()))
}

/// Per-life XP and accepted-task count (spec §6.4 "Growth"): XP is the life's legacy demo
/// XP (`demo_xp_rewards`, which keeps counting) plus its treat XP (`activity_xp`, 5 per given
/// treat); the count is its accepted v0.2 tasks.
pub(crate) fn life_xp(c: &Connection, life_id: &str) -> Result<(u32, u32)> {
    let (xp, n): (i64, i64) = c.query_row(
        "SELECT (SELECT COALESCE(SUM(amount), 0) FROM demo_xp_rewards WHERE life_id = ?1)
              + (SELECT COALESCE(SUM(amount), 0) FROM activity_xp WHERE life_id = ?1),
            (SELECT count(*) FROM demo_xp_rewards WHERE life_id = ?1)",
        [life_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok((
        u32::try_from(xp).map_err(|_| StorageError::Corrupt)?,
        u32::try_from(n).map_err(|_| StorageError::Corrupt)?,
    ))
}

/// What earned the XP an evolution is journaled for.
#[derive(Clone, Copy, Debug)]
pub(crate) enum EvolvedBy<'a> {
    /// A v0.2 task acceptance (`task_id`; `legacy-tasks` only).
    #[cfg(feature = "legacy-tasks")]
    Task(&'a str),
    /// A treat given by `treats.give` (its request id).
    Treat { request_id: &'a str },
}

/// Journals each stage newly reached by `old_xp + gained` as `evolved` (once per life and
/// stage, unseen: `dedup_key 'stage:<n>'` is unique per life whatever earned it). A treat
/// adds `"source":"treat"` and its request id (`schema-v5.sql` step 8). Returns whether any
/// row was added.
pub(crate) fn record_evolution(
    c: &Connection,
    life_id: &str,
    by: EvolvedBy<'_>,
    old_xp: u32,
    gained: u32,
    now: i64,
) -> Result<bool> {
    let rules = &RULES_V1.steady;
    let new_xp = old_xp.saturating_add(gained);
    let mut added = false;
    for stage in stages_crossed(old_xp, new_xp, rules) {
        let mut data = json!({
            "from": stage - 1, "to": stage, "xp": new_xp, "level": level_for_xp(new_xp, rules)
        });
        let (task_id, request_id) = match by {
            #[cfg(feature = "legacy-tasks")]
            EvolvedBy::Task(task_id) => (Some(task_id), None),
            EvolvedBy::Treat { request_id } => {
                data["source"] = json!("treat");
                (None, Some(request_id))
            }
        };
        added |= journal(
            c,
            life_id,
            "evolved",
            Some(&format!("stage:{stage}")),
            &data,
            task_id,
            request_id,
            now,
        )?
        .is_some();
    }
    Ok(added)
}

/// Stages journaled by the acceptance of `task_id`, ascending.
#[cfg(feature = "legacy-tasks")]
pub(crate) fn evolved_by_task(c: &Connection, task_id: &str) -> Result<Vec<u8>> {
    let mut stmt = c.prepare(
        "SELECT dedup_key FROM life_events WHERE task_id = ?1 AND kind = 'evolved' ORDER BY seq",
    )?;
    let keys = stmt
        .query_map([task_id], |r| r.get::<_, Option<String>>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    keys.into_iter()
        .map(|k| {
            k.as_deref()
                .and_then(|k| k.strip_prefix("stage:"))
                .and_then(|n| n.parse().ok())
                .ok_or(StorageError::Corrupt)
        })
        .collect()
}

/// The care action that fills a need.
pub(crate) fn action_for(need: Category) -> &'static str {
    match need {
        Category::Hunger => "food",
        Category::Thirst => "water",
        Category::MentalHealth => "play",
        Category::Hygiene => "wash",
    }
}

/// This database's identity: a digest of its first life's id. Life #1 is created with a
/// random id by the lifecycle migration (or a fresh install) and is never changed or
/// deleted (terminal-life triggers), so the value is stable for one database and differs
/// for a recreated database or one re-migrated from a pre-lifecycle backup. Readers of
/// `bar-state.json` compare `state_version` only within one `db_id`.
pub(crate) fn db_id(c: &Connection) -> Result<String> {
    let first: String = c.query_row("SELECT life_id FROM lives WHERE ordinal = 1", [], |r| {
        r.get(0)
    })?;
    let digest = crate::sys::sha256_hex(format!("codepet-db-id:{first}").as_bytes());
    Ok(format!("db-{}", &digest[..24]))
}

pub(crate) fn state_version(c: &Connection) -> Result<i64> {
    Ok(
        c.query_row("SELECT state_version FROM meta WHERE id = 1", [], |r| {
            r.get(0)
        })?,
    )
}

/// True while a task holds the active slot. Only the retired task pipeline can occupy it
/// (`legacy-tasks`); the v5 migration closed every slot task, so in the default build the
/// slot is always free: `death_pending` is never created, the `slot_busy` blocker never
/// triggers and the frozen reason `task` never occurs (spec §6.5).
#[cfg(feature = "legacy-tasks")]
pub(crate) fn slot_busy(c: &Connection) -> Result<bool> {
    Ok(c.query_row(
        &format!("SELECT EXISTS(SELECT 1 FROM tasks WHERE state IN {SLOT_STATES})"),
        [],
        |r| r.get(0),
    )?)
}
/// See the `legacy-tasks` variant: the default build has no slot holder.
#[cfg(not(feature = "legacy-tasks"))]
pub(crate) fn slot_busy(_c: &Connection) -> Result<bool> {
    Ok(false)
}

/// True when a pending end can finalize now (the slot is free and CodePet is On).
pub(crate) fn death_due(c: &Connection) -> Result<bool> {
    Ok(lifecycle_store::load(c)?.death_pending && !slot_busy(c)? && crate::app::enabled(c)?)
}

fn rules_view(r: &Rules) -> RulesView {
    RulesView {
        decay_per_hour: r.decay_per_hour,
        full_hold_ms: r.full_hold_ms,
        warning_at: r.warning_at,
        lethal: Category::ALL
            .into_iter()
            .filter(|c| r.is_lethal(*c))
            .map(|c| c.as_str().to_owned())
            .collect(),
        critical_after_ms: r.critical_after_ms,
        grace_ms: r.grace_ms,
        tick_ms: TICK_MS,
        idle_timeout_s: IDLE_TIMEOUT_S,
        settle_cap_ms: SETTLE_CAP_MS,
    }
}

/// Everything the snapshot, outcomes and `bar-state.json` say about lives, read from
/// committed rows in the caller's transaction.
pub(crate) struct LifeBlock {
    pub life: Option<LifeView>,
    pub lifecycle: LifecycleView,
    pub unseen_events: Vec<UnseenEvent>,
    pub memorial: MemorialBrief,
    /// Rust's half of the bar recorder's eligibility (§5.1): On, alive, armed, not paused,
    /// no slot task, not pending. Sleep deliberately does not clear it.
    pub presence_wanted: bool,
}

impl LifeBlock {
    pub(crate) fn load(c: &Connection) -> Result<Self> {
        let life = active_life(c)?;
        let row = lifecycle_store::load(c)?;
        let paused: bool =
            c.query_row("SELECT decay_paused FROM pet_state WHERE id = 1", [], |r| {
                r.get(0)
            })?;
        let sleeping: bool = c.query_row(
            "SELECT sleep_started_at_ms IS NOT NULL FROM pet_care WHERE id = 1",
            [],
            |r| r.get(0),
        )?;
        let busy = slot_busy(c)?;
        let enabled = crate::app::enabled(c)?;
        // `protocol.json` `snapshot_v03.frozen_reason_precedence`.
        let frozen_reason = match &life {
            None => "no_life",
            Some(_) if !enabled => "off",
            Some(_) if row.death_pending => "pending",
            Some(l) if !l.armed => "unarmed",
            Some(_) if paused => "paused",
            Some(_) if busy => "task",
            Some(_) if sleeping => "sleeping",
            Some(_) => "none",
        };
        let presence_wanted = enabled
            && life
                .as_ref()
                .is_some_and(|l| l.armed && !paused && !busy && !row.death_pending);
        let lifecycle = lifecycle_view(life.as_ref(), &row, frozen_reason);
        let life_view = life
            .as_ref()
            .map(|l| life_view(c, l, &row, paused))
            .transpose()?;
        let unseen_events = match &life {
            Some(l) => unseen_events(c, &l.life_id)?,
            None => Vec::new(),
        };
        Ok(Self {
            life: life_view,
            lifecycle,
            unseen_events,
            memorial: memorial_brief(c)?,
            presence_wanted,
        })
    }
}

fn life_view(c: &Connection, l: &LifeRow, row: &VitalsRow, paused: bool) -> Result<LifeView> {
    let (xp, tasks_accepted) = life_xp(c, &l.life_id)?;
    let level = level_for_xp(xp, &l.rules);
    let stage = stage_for_xp(xp, &l.rules);
    Ok(LifeView {
        life_id: l.life_id.clone(),
        ordinal: l.ordinal,
        name: l.name.clone(),
        species: l.species.clone(),
        appearance: l.appearance.clone(),
        origin: l.origin.clone(),
        status: l.status.as_str().to_owned(),
        mode: l.mode.as_str().to_owned(),
        armed: l.armed,
        pace: l.pace.as_str().to_owned(),
        paused,
        fainted: row.vitals.fainted,
        death_pending: row.death_pending,
        revision: l.revision,
        adopted_at_ms: l.adopted_at_ms,
        xp,
        level,
        next_level_xp: next_level_xp(level, &l.rules),
        stage,
        stage_key: stage_key(stage).unwrap_or("young").to_owned(),
        next_stage: next_stage(xp, &l.rules).map(|n| NextStageView {
            stage: n.stage,
            min_level: n.min_level,
            xp: n.xp,
        }),
        tasks_accepted,
    })
}

fn lifecycle_view(life: Option<&LifeRow>, row: &VitalsRow, frozen_reason: &str) -> LifecycleView {
    let rules = life.map_or(&RULES_V1.steady, |l| &l.rules);
    let v = &row.vitals;
    let windows = life.map(|l| lc::windows(v, &l.rules, l.mode));
    let needs = Category::ALL
        .into_iter()
        .map(|need| {
            let i = need.index();
            let w = windows.map(|w| w[i]);
            NeedView {
                need: need.as_str().to_owned(),
                action: action_for(need).to_owned(),
                lethal: rules.is_lethal(need),
                meter: v.meters[i],
                hold_ms: v.hold_ms[i],
                neglect_ms: v.neglect_ms[i],
                state: w
                    .map_or_else(
                        || {
                            if v.meters[i] == 0 {
                                "empty"
                            } else if v.meters[i] <= rules.warning_at {
                                "low"
                            } else {
                                "ok"
                            }
                        },
                        |w| w.state.as_str(),
                    )
                    .to_owned(),
                until_warning_ms: w.and_then(|w| w.until_warning_ms),
                until_critical_ms: w.and_then(|w| w.until_critical_ms),
                // An unarmed life has no countdown to an end.
                until_end_ms: w
                    .filter(|_| life.is_some_and(|l| l.armed))
                    .and_then(|w| w.until_end_ms),
            }
        })
        .collect();
    let critical = life.and(v.critical).map(|c| CriticalView {
        need: c.need.as_str().to_owned(),
        action: action_for(c.need).to_owned(),
        grace_ms: c.grace_ms,
        elapsed_ms: c.elapsed_ms,
        remaining_ms: (c.grace_ms - c.elapsed_ms).max(0),
        since_ms: row.critical_since_ms.unwrap_or(0),
    });
    LifecycleView {
        ruleset: RULESET_V1.to_owned(),
        frozen_reason: frozen_reason.to_owned(),
        attended_total_ms: row.attended_total_ms,
        needs,
        critical,
        rules: rules_view(rules),
    }
}

fn unseen_events(c: &Connection, life_id: &str) -> Result<Vec<UnseenEvent>> {
    let mut stmt = c.prepare(
        "SELECT seq, dedup_key, at_ms FROM life_events
         WHERE life_id = ?1 AND kind = 'evolved' AND seen_at_ms IS NULL ORDER BY seq",
    )?;
    let rows = stmt
        .query_map([life_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(seq, key, at_ms)| {
            let stage = key
                .as_deref()
                .and_then(|k| k.strip_prefix("stage:"))
                .and_then(|n| n.parse().ok())
                .ok_or(StorageError::Corrupt)?;
            Ok(UnseenEvent {
                seq,
                kind: "evolved".into(),
                stage,
                at_ms,
            })
        })
        .collect()
}

fn summary_from(text: &str) -> Result<LifeSummary> {
    serde_json::from_str(text).map_err(|_| StorageError::Corrupt)
}

fn memorial_brief(c: &Connection) -> Result<MemorialBrief> {
    let count: i64 = c.query_row(
        "SELECT count(*) FROM lives WHERE status IN ('dead','retired')",
        [],
        |r| r.get(0),
    )?;
    let latest: Option<String> = c
        .query_row(
            "SELECT final_summary FROM lives WHERE status IN ('dead','retired')
             ORDER BY ordinal DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(MemorialBrief {
        count: u32::try_from(count).map_err(|_| StorageError::Corrupt)?,
        latest: latest.as_deref().map(summary_from).transpose()?,
    })
}

/// The memorial record of `l` ending now.
fn final_summary(
    c: &Connection,
    l: &LifeRow,
    row: &VitalsRow,
    end_cause: &str,
    now: i64,
) -> Result<LifeSummary> {
    let (xp, tasks_accepted) = life_xp(c, &l.life_id)?;
    let stage = stage_for_xp(xp, &l.rules);
    let m = row.vitals.meters;
    Ok(LifeSummary {
        life_id: l.life_id.clone(),
        name: l.name.clone(),
        species: l.species.clone(),
        appearance: l.appearance.clone(),
        ordinal: l.ordinal,
        mode: l.mode.as_str().to_owned(),
        pace: l.pace.as_str().to_owned(),
        final_level: level_for_xp(xp, &l.rules),
        final_stage: stage,
        stage_name_key: format!("{}.{}", l.appearance, stage_key(stage).unwrap_or("young")),
        life_xp: xp,
        tasks_accepted,
        adopted_at_ms: l.adopted_at_ms,
        ended_at_ms: now,
        end_cause: end_cause.to_owned(),
        final_meters: MetersView {
            hunger: m[0],
            thirst: m[1],
            mental_health: m[2],
            hygiene: m[3],
        },
        attended_ms: row.attended_total_ms,
    })
}

/// Ends `l` (dead or retired) exactly once and releases the vitals row. No other table
/// is touched except the care revision bump that invalidates pending approvals.
fn end_life(
    c: &Connection,
    l: &LifeRow,
    row: &VitalsRow,
    status: &str,
    end_cause: &str,
    from: &str,
    now: i64,
) -> Result<bool> {
    let summary = final_summary(c, l, row, end_cause, now)?;
    let text = serde_json::to_string(&summary)
        .map_err(|_| StorageError::Unavailable("cannot encode memorial"))?;
    // The status guard makes the end exactly once.
    let n = c.execute(
        &format!(
            "UPDATE lives SET status = ?1, ended_at_ms = ?2, end_cause = ?3, final_summary = ?4,
                revision = revision + 1 WHERE life_id = ?5 AND status IN {from}"
        ),
        params![status, now, end_cause, text, l.life_id],
    )?;
    if n != 1 {
        return Ok(false);
    }
    let kind = if status == "dead" { "died" } else { "retired" };
    let data = match row.vitals.critical {
        Some(crit) if status == "dead" => json!({"need": crit.need.as_str()}),
        _ => json!({}),
    };
    journal(c, &l.life_id, kind, Some("end"), &data, None, None, now)?;
    c.execute(
        "UPDATE life_vitals SET life_id = NULL, hold_hunger = 0, hold_thirst = 0,
            hold_mental_health = 0, hold_hygiene = 0, carry_hunger = 0, carry_thirst = 0,
            carry_mental_health = 0, carry_hygiene = 0, neglect_hunger_ms = 0,
            neglect_thirst_ms = 0, critical_need = NULL, critical_grace_ms = NULL,
            critical_elapsed_ms = 0, critical_since_ms = NULL, fainted = 0, death_pending = 0,
            attended_total_ms = 0 WHERE id = 1",
        [],
    )?;
    care::bump(c)?;
    Ok(true)
}

/// Spec §8.1: a pending death becomes final once no task holds the slot. Runs last in
/// every write, so XP granted in the same transaction lands first. Returns whether a
/// life ended.
pub(crate) fn finalize_if_due(c: &Connection, at: &TimeMark) -> Result<bool> {
    let row = lifecycle_store::load(c)?;
    // E2: no death while Off; a pending end waits for On like everything else.
    if !row.death_pending || slot_busy(c)? || !crate::app::enabled(c)? {
        return Ok(false);
    }
    let life = active_life(c)?.ok_or(StorageError::Corrupt)?;
    let need = row.vitals.critical.ok_or(StorageError::Corrupt)?.need;
    let cause = format!("neglect:{}", need.as_str());
    let ended = end_life(c, &life, &row, "dead", &cause, "('critical')", at.wall)?;
    if ended {
        fault("death_before_commit");
    }
    Ok(ended)
}

/// Inserts a new adopted life at level 1, 0 XP, young, meters 70.00, armed with `req.mode`
/// (the caller checked that no life is living, the slot is free and the rules were
/// acknowledged), resets the care block (welcome routine available again) and journals
/// `born` and `armed`. Returns the new life id. Shared by `pet.adopt` and `pet.start`.
fn insert_adopted_life(
    c: &Connection,
    at: &TimeMark,
    request_id: &str,
    req: &AdoptRequest<'_>,
    name: &str,
) -> Result<String> {
    let now = at.wall;
    let rules = RULES_V1.for_pace(req.pace);
    let life_id = new_life_id()?;
    let ordinal: i64 = c.query_row("SELECT COALESCE(MAX(ordinal), 0) + 1 FROM lives", [], |r| {
        r.get(0)
    })?;
    let inserted = c.execute(
        "INSERT INTO lives (life_id, ordinal, name, species, appearance, origin,
                status, mode, armed, armed_at_ms, pace, ruleset, decay_hunger,
                decay_thirst, decay_mental_health, decay_hygiene, full_hold_ms,
                warning_at, critical_after_ms, grace_ms, adopted_at_ms,
                adoption_request_id)
             VALUES (?1, ?2, ?3, ?4, ?4, 'adopted', 'healthy', ?5, 1, ?6, ?7, ?8, ?9,
                ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?6, ?17)",
        params![
            life_id,
            ordinal,
            name,
            req.species,
            req.mode.as_str(),
            now,
            req.pace.as_str(),
            RULESET_V1,
            rules.decay_per_hour[0],
            rules.decay_per_hour[1],
            rules.decay_per_hour[2],
            rules.decay_per_hour[3],
            rules.full_hold_ms,
            rules.warning_at,
            rules.critical_after_ms,
            rules.grace_ms,
            request_id
        ],
    );
    match inserted {
        // The partial unique index is the last line against a second life.
        Err(e) if e.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) => {
            return Err(StorageError::InvalidState("a companion is already living"))
        }
        other => other?,
    };
    c.execute(
        "UPDATE pet_state SET hunger = ?1, thirst = ?1, mental_health = ?1,
                hygiene = ?1, decay_paused = 0, updated_at_ms = ?2 WHERE id = 1",
        params![METER_START, now],
    )?;
    c.execute(
        "UPDATE pet_care SET starts_since_bathroom = 0, work_sessions = 0,
                execution_ms = 0, rest_required = 0, sleep_started_at_ms = NULL,
                sleep_wall_ms = NULL, sleep_boot = NULL, sleep_steady_ms = NULL,
                sleep_remaining_ms = 120000, welcome_care_available = 1 WHERE id = 1",
        [],
    )?;
    let fresh = Vitals::from_meters([METER_START; 4], rules);
    lifecycle_store::save(
        c,
        &VitalsRow {
            life_id: Some(life_id.clone()),
            vitals: fresh,
            critical_since_ms: None,
            death_pending: false,
            attended_total_ms: 0,
        },
        now,
    )?;
    care::bump(c)?;
    journal(
        c,
        &life_id,
        "born",
        None,
        &json!({"origin": "adopted", "species": req.species,
                "mode": req.mode.as_str(), "pace": req.pace.as_str()}),
        None,
        Some(request_id),
        now,
    )?;
    journal(
        c,
        &life_id,
        "armed",
        None,
        &json!({"mode": req.mode.as_str()}),
        None,
        Some(request_id),
        now,
    )?;
    Ok(life_id)
}

/// `pet.start` on a living unarmed life: the look, an optional rename, then the rules
/// with `pet.rules` semantics (mode + arm), each journaled as its own method would.
fn arm_existing(
    c: &Connection,
    at: &TimeMark,
    request_id: &str,
    life: &LifeRow,
    req: &StartRequest<'_>,
    name: Option<&str>,
) -> Result<()> {
    if life.appearance != req.species {
        c.execute(
            "UPDATE lives SET appearance = ?1, revision = revision + 1 WHERE life_id = ?2",
            params![req.species, life.life_id],
        )?;
        journal(
            c,
            &life.life_id,
            "appearance_changed",
            None,
            &json!({"from": life.appearance, "to": req.species}),
            None,
            Some(request_id),
            at.wall,
        )?;
    }
    if let Some(name) = name.filter(|n| *n != life.name) {
        c.execute(
            "UPDATE lives SET name = ?1, revision = revision + 1 WHERE life_id = ?2",
            params![name, life.life_id],
        )?;
    }
    c.execute(
        "UPDATE lives SET mode = ?1, armed = 1, armed_at_ms = COALESCE(armed_at_ms, ?2),
            revision = revision + 1 WHERE life_id = ?3",
        params![req.mode.as_str(), at.wall, life.life_id],
    )?;
    journal(
        c,
        &life.life_id,
        "armed",
        None,
        &json!({"mode": req.mode.as_str()}),
        None,
        Some(request_id),
        at.wall,
    )?;
    if life.mode != req.mode {
        journal(
            c,
            &life.life_id,
            "mode_changed",
            None,
            &json!({"from": life.mode.as_str(), "to": req.mode.as_str()}),
            None,
            Some(request_id),
            at.wall,
        )?;
    }
    Ok(())
}

/// Canonical params JSON for lifecycle idempotency: fixed key order, serde_json escaping.
fn canonical(fields: &[(&str, Value)]) -> String {
    let body: Vec<String> = fields
        .iter()
        .map(|(k, v)| format!("{}:{}", Value::from(*k), v))
        .collect();
    format!("{{{}}}", body.join(","))
}

impl Store {
    fn life_outcome(c: &Connection, request_id: &str, changed: bool) -> Result<LifeOutcome> {
        let block = LifeBlock::load(c)?;
        Ok(LifeOutcome {
            request_id: request_id.to_owned(),
            changed,
            life: block.life,
            lifecycle: block.lifecycle,
            memorial: block.memorial,
        })
    }

    /// Runs one idempotent lifecycle mutation: prologue, replay check, `body`, epilogue
    /// (finalize, `state_version++`), outcome, receipt, then commit.
    fn life_command(
        &mut self,
        request_id: &str,
        method: &str,
        params: &str,
        fault_point: Option<&str>,
        body: impl FnOnce(&Connection, &TimeMark) -> Result<bool>,
    ) -> Result<LifeOutcome> {
        let sha = sys::sha256_hex(params.as_bytes());
        let (tx, at, ended) = self.write_settled()?;
        let (outcome, replayed) = replay_or_run(&tx, request_id, method, &sha, at.wall, || {
            // `protocol.json` `off_behaviour`: nothing may age, arm, grow or end a life while
            // Off (`pet.select` and `pet.events_seen` stay allowed).
            if matches!(
                method,
                "pet.adopt" | "pet.rules" | "pet.configure_life" | "pet.retire"
            ) {
                crate::app::ensure_on(&tx)?;
            }
            // A request made while the companion was still living never adopts over an end
            // that only this command's settlement made final: the memorial comes first.
            if ended && method == "pet.adopt" {
                return Err(StorageError::PassedAway);
            }
            let changed = body(&tx, &at)?;
            seal(&tx, &at)?;
            Self::life_outcome(&tx, request_id, changed)
        })
        .map_err(|e| passed_away_if(ended, e))?;
        if replayed {
            return Ok(outcome);
        }
        if let Some(point) = fault_point {
            fault(point);
        }
        tx.commit()?;
        Ok(outcome)
    }

    /// `pet.select`: cosmetic appearance switch within the living companion's life. It
    /// touches only `lives.appearance`/`revision` and the journal: never meters, vitals,
    /// care, sleep, tasks, XP or approvals. Allowed while sleeping, critical, paused and
    /// during any task state; refused for a pending end.
    pub fn select_appearance(
        &mut self,
        request_id: &str,
        life_id: &str,
        pet_id: &str,
    ) -> Result<LifeOutcome> {
        if !is_species(pet_id) {
            return Err(StorageError::InvalidParams("unknown companion"));
        }
        let params = canonical(&[("life_id", life_id.into()), ("pet_id", pet_id.into())]);
        self.life_command(request_id, "pet.select", &params, None, |c, at| {
            let (life, _) = living_target(c, life_id)?;
            if life.appearance == pet_id {
                return Ok(false);
            }
            c.execute(
                "UPDATE lives SET appearance = ?1, revision = revision + 1
                 WHERE life_id = ?2 AND status IN ('healthy','warning','critical')",
                params![pet_id, life.life_id],
            )?;
            journal(
                c,
                &life.life_id,
                "appearance_changed",
                None,
                &json!({"from": life.appearance, "to": pet_id}),
                None,
                Some(request_id),
                at.wall,
            )?;
            Ok(true)
        })
    }

    /// `pet.adopt`: an explicit new life at level 1, 0 XP, young, meters 70.00, armed with
    /// the acknowledged rules. Requires no living companion and a free slot.
    pub fn adopt(&mut self, request_id: &str, req: &AdoptRequest<'_>) -> Result<LifeOutcome> {
        if !is_species(req.species) {
            return Err(StorageError::InvalidParams("unknown companion"));
        }
        let raw_name = req
            .name
            .or_else(|| default_name(req.species))
            .ok_or(StorageError::InvalidParams("unknown companion"))?;
        let name = normalize_pet_name(raw_name).map_err(StorageError::InvalidParams)?;
        let params = canonical(&[
            ("acknowledge_rules", req.acknowledge_rules.into()),
            ("mode", req.mode.as_str().into()),
            ("name", req.name.map_or(Value::Null, Value::from)),
            ("pace", req.pace.as_str().into()),
            ("species", req.species.into()),
        ]);
        self.life_command(
            request_id,
            "pet.adopt",
            &params,
            Some("adopt_before_commit"),
            |c, at| {
                if active_life(c)?.is_some() {
                    return Err(StorageError::InvalidState("a companion is already living"));
                }
                if slot_busy(c)? {
                    return Err(StorageError::InvalidState(
                        "finish or discard the current task before adopting",
                    ));
                }
                if !req.acknowledge_rules {
                    return Err(StorageError::AcknowledgementRequired);
                }
                insert_adopted_life(c, at, request_id, req, &name)?;
                Ok(true)
            },
        )
    }

    /// `pet.rules`: arms an unarmed life (needs `acknowledge_rules`) and/or changes its
    /// mode. Permadeath → gentle is allowed any time while living; gentle → permadeath
    /// needs the acknowledgement and a healthy companion. Neglect, carries and holds are
    /// never reset.
    pub fn set_rules(
        &mut self,
        request_id: &str,
        life_id: &str,
        mode: Mode,
        acknowledge_rules: bool,
    ) -> Result<LifeOutcome> {
        let params = canonical(&[
            ("acknowledge_rules", acknowledge_rules.into()),
            ("life_id", life_id.into()),
            ("mode", mode.as_str().into()),
        ]);
        self.life_command(request_id, "pet.rules", &params, None, |c, at| {
            let (life, _) = living_target(c, life_id)?;
            let arming = !life.armed;
            let switching = life.mode != mode;
            if (arming || (switching && mode == Mode::Permadeath)) && !acknowledge_rules {
                return Err(StorageError::AcknowledgementRequired);
            }
            if switching && mode == Mode::Permadeath && life.status != LifeStatus::Healthy {
                return Err(StorageError::InvalidState(
                    "switch to permadeath only while the companion is healthy",
                ));
            }
            if !arming && !switching {
                return Ok(false);
            }
            c.execute(
                "UPDATE lives SET mode = ?1, armed = 1, armed_at_ms = COALESCE(armed_at_ms, ?2),
                    revision = revision + 1 WHERE life_id = ?3",
                params![mode.as_str(), at.wall, life.life_id],
            )?;
            if arming {
                journal(
                    c,
                    &life.life_id,
                    "armed",
                    None,
                    &json!({"mode": mode.as_str()}),
                    None,
                    Some(request_id),
                    at.wall,
                )?;
            }
            if switching {
                journal(
                    c,
                    &life.life_id,
                    "mode_changed",
                    None,
                    &json!({"from": life.mode.as_str(), "to": mode.as_str()}),
                    None,
                    Some(request_id),
                    at.wall,
                )?;
            }
            Ok(true)
        })
    }

    /// `pet.configure_life`: leniency-only per-life settings (D7c). A stricter value is
    /// refused; a running frozen grace becomes `max(frozen, new)`.
    pub fn configure_life(
        &mut self,
        request_id: &str,
        life_id: &str,
        expected_revision: i64,
        l: &Leniency,
    ) -> Result<LifeOutcome> {
        if l.grace_hours.is_none() && l.critical_after_hours.is_none() && l.decay_percent.is_none()
        {
            return Err(StorageError::InvalidParams(
                "set at least one of grace_hours, critical_after_hours or decay_percent",
            ));
        }
        let hours = |h: Option<u16>| h.map(|h| i64::from(h) * HOUR_MS);
        let opt = |v: Option<u16>| v.map_or(Value::Null, Value::from);
        let params = canonical(&[
            ("critical_after_hours", opt(l.critical_after_hours)),
            (
                "decay_percent",
                l.decay_percent.map_or(Value::Null, Value::from),
            ),
            ("expected_revision", expected_revision.into()),
            ("grace_hours", opt(l.grace_hours)),
            ("life_id", life_id.into()),
        ]);
        self.life_command(request_id, "pet.configure_life", &params, None, |c, at| {
            let (life, mut row) = living_target(c, life_id)?;
            if life.revision != expected_revision {
                return Err(StorageError::StaleRevision);
            }
            let next = life
                .rules
                .with_leniency(
                    hours(l.grace_hours),
                    hours(l.critical_after_hours),
                    l.decay_percent,
                )
                .map_err(|_| {
                    StorageError::InvalidParams(
                        "settings can only become more lenient for a living companion",
                    )
                })?;
            if next == life.rules {
                return Ok(false);
            }
            c.execute(
                "UPDATE lives SET decay_hunger = ?1, decay_thirst = ?2, decay_mental_health = ?3,
                    decay_hygiene = ?4, critical_after_ms = ?5, grace_ms = ?6,
                    revision = revision + 1 WHERE life_id = ?7 AND revision = ?8",
                params![
                    next.decay_per_hour[0],
                    next.decay_per_hour[1],
                    next.decay_per_hour[2],
                    next.decay_per_hour[3],
                    next.critical_after_ms,
                    next.grace_ms,
                    life.life_id,
                    life.revision
                ],
            )?;
            row.vitals = lc::on_leniency(&row.vitals, &next);
            lifecycle_store::save(c, &row, at.wall)?;
            journal(
                c,
                &life.life_id,
                "configured",
                None,
                &json!({"grace_ms": next.grace_ms, "critical_after_ms": next.critical_after_ms,
                    "decay_per_hour": next.decay_per_hour}),
                None,
                Some(request_id),
                at.wall,
            )?;
            Ok(true)
        })
    }

    /// `pet.retire`: voluntary, terminal and journaled with a memorial. Requires the typed
    /// name, a free slot, and a companion that is neither critical nor pending.
    pub fn retire(
        &mut self,
        request_id: &str,
        life_id: &str,
        confirm_name: &str,
    ) -> Result<LifeOutcome> {
        let params = canonical(&[
            ("confirm_name", confirm_name.into()),
            ("life_id", life_id.into()),
        ]);
        self.life_command(request_id, "pet.retire", &params, None, |c, at| {
            let (life, row) = living_target(c, life_id)?;
            if normalize_pet_name(confirm_name).ok().as_deref() != Some(life.name.as_str()) {
                return Err(StorageError::InvalidParams(
                    "the typed name does not match the companion's name",
                ));
            }
            if slot_busy(c)? {
                return Err(StorageError::InvalidState(
                    "finish or discard the current task before retiring",
                ));
            }
            if life.status == LifeStatus::Critical || row.vitals.critical.is_some() {
                return Err(StorageError::InvalidState(
                    "care for the companion before retiring them",
                ));
            }
            if !end_life(
                c,
                &life,
                &row,
                "retired",
                "retired",
                "('healthy','warning')",
                at.wall,
            )? {
                return Err(StorageError::InvalidState("companion changed concurrently"));
            }
            Ok(true)
        })
    }

    /// `pet.events_seen`: marks the life's evolutions with `seq <= through_seq` as seen,
    /// once. Idempotent; a restart never replays a seen celebration.
    pub fn events_seen(
        &mut self,
        request_id: &str,
        life_id: &str,
        through_seq: i64,
    ) -> Result<LifeOutcome> {
        if through_seq < 0 {
            return Err(StorageError::InvalidParams(
                "through_seq must not be negative",
            ));
        }
        let params = canonical(&[
            ("life_id", life_id.into()),
            ("through_seq", through_seq.into()),
        ]);
        self.life_command(request_id, "pet.events_seen", &params, None, |c, at| {
            if life_by_id(c, life_id)?.is_none() {
                return Err(StorageError::StaleRevision);
            }
            let n = c.execute(
                "UPDATE life_events SET seen_at_ms = ?1 WHERE life_id = ?2 AND kind = 'evolved'
                 AND seq <= ?3 AND seen_at_ms IS NULL",
                params![at.wall, life_id, through_seq],
            )?;
            Ok(n > 0)
        })
    }

    /// `pet.start` (spec §6.2, §2.1): one transaction behind **Let's go!**. With no living
    /// companion it adopts `species` (pace steady, `name` or the species' default name).
    /// With a living unarmed one (the owner's upgraded Ginger) it selects the look
    /// `species`, renames when `name` is given, sets the rules with `pet.rules` semantics and
    /// arms. Then it spends the welcome routine when it is still available and the pet is
    /// awake. `life_id`, when given, must name the living companion. Refused while Off;
    /// `ALREADY_ARMED` for an armed companion; `ACKNOWLEDGEMENT_REQUIRED` without the
    /// acknowledgement; `NAME_INVALID` for a name core refuses. Returns the snapshot.
    pub fn start_pet(&mut self, request_id: &str, req: &StartRequest<'_>) -> Result<Snapshot> {
        if !is_species(req.species) {
            return Err(StorageError::InvalidParams("unknown companion"));
        }
        let name = req
            .name
            .map(|n| normalize_pet_name(n).map_err(StorageError::NameInvalid))
            .transpose()?;
        let params = canonical(&[
            ("acknowledge_rules", req.acknowledge_rules.into()),
            ("life_id", req.life_id.map_or(Value::Null, Value::from)),
            ("mode", req.mode.as_str().into()),
            ("name", req.name.map_or(Value::Null, Value::from)),
            ("species", req.species.into()),
        ]);
        let mut point = None;
        let snapshot = self.v03_command(
            request_id,
            "pet.start",
            &params,
            V03Policy::RefusedWhileOff,
            None,
            |c, at, ended| {
                match active_life(c)? {
                    None => {
                        // Never adopt over an end that only this click's settlement made
                        // final: the memorial comes first (as `pet.adopt`).
                        if ended {
                            return Err(StorageError::PassedAway);
                        }
                        if req.life_id.is_some() {
                            return Err(StorageError::LifeNotFound);
                        }
                        if slot_busy(c)? {
                            return Err(StorageError::InvalidState(
                                "finish or discard the current task before adopting",
                            ));
                        }
                        if !req.acknowledge_rules {
                            return Err(StorageError::AcknowledgementRequired);
                        }
                        let default = default_name(req.species)
                            .ok_or(StorageError::InvalidParams("unknown companion"))?;
                        let name = name.as_deref().unwrap_or(default);
                        let adopt = AdoptRequest {
                            species: req.species,
                            name: Some(name),
                            mode: req.mode,
                            pace: Pace::Steady,
                            acknowledge_rules: true,
                        };
                        insert_adopted_life(c, at, request_id, &adopt, name)?;
                        point = Some("adopt_before_commit");
                    }
                    Some(life) => {
                        if req.life_id.is_some_and(|id| id != life.life_id) {
                            return Err(StorageError::LifeNotFound);
                        }
                        if life.armed {
                            return Err(StorageError::AlreadyArmed);
                        }
                        if !req.acknowledge_rules {
                            return Err(StorageError::AcknowledgementRequired);
                        }
                        let vitals = lifecycle_store::load(c)?;
                        if vitals.death_pending {
                            return Err(StorageError::InvalidState(
                                "the companion cannot change right now; finish the current task",
                            ));
                        }
                        if req.mode == Mode::Permadeath && life.status != LifeStatus::Healthy {
                            return Err(StorageError::InvalidState(
                                "switch to permadeath only while the companion is healthy",
                            ));
                        }
                        arm_existing(c, at, request_id, &life, req, name.as_deref())?;
                    }
                }
                // The welcome routine (fills every need once), unless asleep.
                let (welcome, sleeping): (bool, bool) = c.query_row(
                    "SELECT welcome_care_available, sleep_started_at_ms IS NOT NULL
                     FROM pet_care WHERE id = 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                if welcome && !sleeping {
                    crate::store::fill_care(c, at, "routine", false)?;
                }
                care::bump(c)?;
                if let Some(point) = point {
                    fault(point);
                }
                Ok(true)
            },
        )?;
        Ok(snapshot)
    }

    /// `pet.rename` (spec §6.2): renames the living companion `life_id`. The name is
    /// validated by core's `normalize_pet_name` before the UPDATE (`NAME_INVALID`), so the
    /// `lives.name` CHECK never fires. Cosmetic: allowed while Off, sleeping or fainted.
    pub fn rename(&mut self, request_id: &str, life_id: &str, name: &str) -> Result<Snapshot> {
        let normalized = normalize_pet_name(name).map_err(StorageError::NameInvalid)?;
        let params = canonical(&[("life_id", life_id.into()), ("name", name.into())]);
        self.v03_command(
            request_id,
            "pet.rename",
            &params,
            V03Policy::AllowedWhileOff,
            None,
            |c, _, _| {
                let life = active_life(c)?.ok_or(StorageError::NoActivePet)?;
                if life.life_id != life_id {
                    return Err(StorageError::LifeNotFound);
                }
                if life.name == normalized {
                    return Ok(false);
                }
                c.execute(
                    "UPDATE lives SET name = ?1, revision = revision + 1
                     WHERE life_id = ?2 AND status IN ('healthy','warning','critical')",
                    params![normalized, life.life_id],
                )?;
                Ok(true)
            },
        )
    }

    /// `pet.memorial`: one read-only page of ended lives, newest ordinal first.
    pub fn memorial(
        &self,
        before_ordinal: Option<u32>,
        limit: Option<u32>,
    ) -> Result<MemorialPage> {
        let limit = match limit.unwrap_or(MAX_MEMORIAL_PAGE) {
            0 => return Err(StorageError::InvalidParams("limit must be at least 1")),
            n if n > MAX_MEMORIAL_PAGE => {
                return Err(StorageError::InvalidParams("limit too large"))
            }
            n => n,
        };
        let tx = self.conn.unchecked_transaction()?;
        let before = before_ordinal.map_or(i64::MAX, i64::from);
        let mut stmt = tx.prepare(
            "SELECT ordinal, final_summary FROM lives WHERE status IN ('dead','retired')
             AND ordinal < ?1 ORDER BY ordinal DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![before, i64::from(limit) + 1], |r| {
                Ok((r.get::<_, u32>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        let more = rows.len() > limit as usize;
        let entries = rows
            .iter()
            .take(limit as usize)
            .map(|(_, s)| summary_from(s))
            .collect::<Result<Vec<_>>>()?;
        let next_before_ordinal = more
            .then(|| rows.get(limit as usize - 1).map(|(o, _)| *o))
            .flatten();
        tx.commit()?;
        Ok(MemorialPage {
            entries,
            next_before_ordinal,
        })
    }
}
