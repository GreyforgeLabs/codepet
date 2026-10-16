//! Explicit pet care, bounded rest settlement, and isolated simulation progression.
//! No timer, worker, provider launch, or project access exists in this module.
use crate::model::{CareView, MasteryView, MetersView, ProgressionView, XpView};
#[cfg(feature = "legacy-tasks")]
use crate::{life, lifecycle_store};
use crate::{Clock, Result, StorageError};
use codepet_core::evolution::{level_for_xp, next_level_xp};
use codepet_core::lifecycle::RULES_V1;
#[cfg(feature = "legacy-tasks")]
use codepet_core::{Category, State};
#[cfg(feature = "legacy-tasks")]
use rusqlite::OptionalExtension;
use rusqlite::{params, Connection};

pub const SLEEP_MS: i64 = 120_000;
pub const EXECUTION_LIMIT_MS: i64 = 45 * 60 * 1000;
pub const SESSION_LIMIT: u32 = 3;
pub const BATHROOM_LIMIT: u32 = 2;
pub const DEMO_XP: u32 = 20;

/// One command's time sample. `boot`/`steady` is CLOCK_BOOTTIME (includes suspend) and
/// settles naps; `run_clock`/`run_steady` is CLOCK_MONOTONIC (excludes suspend) and
/// measures supervised execution, so a suspended laptop is never charged as work.
#[derive(Clone)]
pub(crate) struct TimeMark {
    pub wall: i64,
    pub boot: Option<String>,
    pub steady: Option<i64>,
    /// Execution clock of the retired task pipeline (`legacy-tasks` only).
    #[cfg(feature = "legacy-tasks")]
    pub run_clock: Option<String>,
    #[cfg(feature = "legacy-tasks")]
    pub run_steady: Option<i64>,
}
impl TimeMark {
    pub fn new(clock: &dyn Clock) -> Self {
        let split = |t: Option<(String, i64)>| match t {
            Some((id, ms)) => (Some(id), Some(ms)),
            None => (None, None),
        };
        let (boot, steady) = split(clock.steady_time());
        #[cfg(feature = "legacy-tasks")]
        let (run_clock, run_steady) = split(clock.execution_time());
        Self {
            wall: clock.now_ms(),
            boot,
            steady,
            #[cfg(feature = "legacy-tasks")]
            run_clock,
            #[cfg(feature = "legacy-tasks")]
            run_steady,
        }
    }
    /// Rest elapsed since a nap cursor, counting suspended time.
    fn rest_elapsed(&self, wall: i64, boot: Option<&str>, steady: Option<i64>, cap: i64) -> i64 {
        elapsed(
            (self.wall, self.boot.as_deref(), self.steady),
            (wall, boot, steady),
            cap,
        )
    }
    /// Execution elapsed since a segment cursor, excluding suspended time.
    #[cfg(feature = "legacy-tasks")]
    fn run_elapsed(&self, wall: i64, clock: Option<&str>, steady: Option<i64>, cap: i64) -> i64 {
        elapsed(
            (self.wall, self.run_clock.as_deref(), self.run_steady),
            (wall, clock, steady),
            cap,
        )
    }
}

/// Prefer the same clock identity's monotonic delta. Otherwise (another boot, or no
/// monotonic source) use bounded wall time, never negative elapsed time. The caller
/// preserves high-water marks after rollback.
fn elapsed(
    (now_wall, now_id, now_steady): (i64, Option<&str>, Option<i64>),
    (wall, id, steady): (i64, Option<&str>, Option<i64>),
    cap: i64,
) -> i64 {
    let delta = if now_id.is_some() && now_id == id {
        now_steady.zip(steady).map(|(a, b)| a.saturating_sub(b))
    } else {
        None
    };
    delta
        .unwrap_or_else(|| now_wall.saturating_sub(wall))
        .clamp(0, cap)
}

pub(crate) fn invalidate(c: &Connection) -> Result<()> {
    c.execute(
        "UPDATE preflight_tokens SET status = 'invalidated' WHERE status = 'pending'",
        [],
    )?;
    Ok(())
}

pub(crate) fn bump(c: &Connection) -> Result<()> {
    c.execute(
        "UPDATE pet_care SET revision = revision + 1 WHERE id = 1",
        [],
    )?;
    invalidate(c)
}

pub(crate) fn settle_sleep(c: &Connection, at: &TimeMark) -> Result<bool> {
    type SleepRow = (Option<i64>, Option<i64>, Option<String>, Option<i64>, i64);
    let (started, wall, boot, steady, remaining): SleepRow = c.query_row(
        "SELECT sleep_started_at_ms, sleep_wall_ms, sleep_boot, sleep_steady_ms,
         sleep_remaining_ms FROM pet_care WHERE id = 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )?;
    if started.is_none() {
        return Ok(false);
    }
    let wall = wall.ok_or(StorageError::Corrupt)?;
    let elapsed = at.rest_elapsed(wall, boot.as_deref(), steady, remaining);
    if elapsed == 0 {
        return Ok(false);
    }
    if elapsed >= remaining {
        c.execute(
            "UPDATE pet_care SET work_sessions = 0, execution_ms = 0,
            rest_required = 0, sleep_remaining_ms = 120000, sleep_started_at_ms = NULL,
            sleep_wall_ms = NULL, sleep_boot = NULL, sleep_steady_ms = NULL WHERE id = 1",
            [],
        )?;
        bump(c)?;
    } else {
        c.execute(
            "UPDATE pet_care SET sleep_remaining_ms = sleep_remaining_ms - ?1,
            sleep_wall_ms = ?2, sleep_boot = ?3, sleep_steady_ms = ?4 WHERE id = 1",
            params![elapsed, at.wall.max(wall), at.boot, at.steady],
        )?;
    }
    // Partial countdown settlement does not change start eligibility or approvals.
    // Only the completed-rest transition invalidates a preflight.
    Ok(true)
}

/// The care block. In the default v0.3 build the nap and bathroom requirements never come
/// due (spec §7.2: the retired readiness gates, `legacy-tasks` only); `sleeping` and the
/// nap countdown are real.
#[cfg_attr(not(feature = "legacy-tasks"), allow(unused_variables))]
pub(crate) fn view(c: &Connection, meters: MetersView) -> Result<CareView> {
    #[cfg_attr(not(feature = "legacy-tasks"), allow(unused_mut))]
    let mut v = c.query_row(
        "SELECT revision, starts_since_bathroom, work_sessions,
        execution_ms, rest_required, sleep_remaining_ms, sleep_started_at_ms,
        welcome_care_available FROM pet_care WHERE id = 1",
        [],
        |r| {
            let starts: u32 = r.get(1)?;
            let work: u32 = r.get(2)?;
            let execution: i64 = r.get(3)?;
            let required: bool = r.get(4)?;
            let started: Option<i64> = r.get(6)?;
            let legacy = cfg!(feature = "legacy-tasks");
            Ok(CareView {
                revision: r.get(0)?,
                bathroom_due: legacy && starts >= BATHROOM_LIMIT,
                starts_since_bathroom: starts,
                sleeping: started.is_some(),
                sleep_due: legacy
                    && (required || work >= SESSION_LIMIT || execution >= EXECUTION_LIMIT_MS),
                sleep_remaining_ms: r.get(5)?,
                sleep_started_at_ms: started,
                work_sessions: work,
                execution_ms: execution,
                #[cfg(feature = "legacy-tasks")]
                ready_to_work: false,
                #[cfg(feature = "legacy-tasks")]
                readiness_reasons: vec![],
                welcome_care_available: r.get(7)?,
            })
        },
    )?;
    #[cfg(feature = "legacy-tasks")]
    readiness(c, meters, &mut v)?;
    Ok(v)
}

/// The v0.2 readiness gate of `CareView` (`legacy-tasks` only).
#[cfg(feature = "legacy-tasks")]
fn readiness(c: &Connection, meters: MetersView, v: &mut CareView) -> Result<()> {
    if let Some(reason) = lifecycle_reason(c)? {
        v.readiness_reasons.push(reason);
    }
    if v.sleeping {
        v.readiness_reasons
            .push("Resting: finish the nap before starting work".into());
    } else if v.sleep_due {
        v.readiness_reasons
            .push("Needs a nap: put the pet to bed".into());
    }
    for (value, reason) in [
        (meters.hunger, "Needs food"),
        (meters.thirst, "Needs water"),
        (meters.mental_health, "Needs play"),
        (meters.hygiene, "Needs a wash"),
    ] {
        if value != 10000 {
            v.readiness_reasons.push(reason.into());
        }
    }
    if v.bathroom_due {
        v.readiness_reasons
            .push("Needs a bathroom break; washing alone will not clear it".into());
    }
    let busy: bool = c.query_row(
        &format!(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE state IN {})",
            crate::schema::SLOT_STATES
        ),
        [],
        |r| r.get(0),
    )?;
    if busy {
        v.readiness_reasons
            .push("Finish or discard the current task first".into());
    }
    v.ready_to_work = v.readiness_reasons.is_empty();
    Ok(())
}

#[cfg(feature = "legacy-tasks")]
/// The lifecycle gate, in order: no living companion → a pending end → a critical need.
/// Plain text only; durations and deaths are never predicted here.
fn lifecycle_reason(c: &Connection) -> Result<Option<String>> {
    let Some(life) = life::active_life(c)? else {
        return Ok(Some("Adopt a companion to start work".into()));
    };
    let vitals = lifecycle_store::load(c)?;
    if vitals.death_pending {
        // Care cannot change the outcome now (it is refused while the end is pending), so the
        // copy must not promise a recovery; it matches LifeText.pendingText.
        return Ok(Some(format!(
            "New work waits until the current task is closed; {}'s grace time has run out.",
            life.name
        )));
    }
    if let Some(critical) = vitals.vitals.critical {
        return Ok(Some(format!(
            "{} urgently needs {}; care before starting work",
            life.name,
            life::action_for(critical.need)
        )));
    }
    Ok(None)
}

#[cfg(feature = "legacy-tasks")]
/// Start and preflight refuse without a living, settled companion (spec §6).
pub(crate) fn ensure_lifecycle_ready(c: &Connection) -> Result<()> {
    if life::active_life(c)?.is_none() {
        return Err(StorageError::NoActivePet);
    }
    let vitals = lifecycle_store::load(c)?;
    if vitals.death_pending {
        return Err(StorageError::InvalidState(
            "new work waits until the current task is closed; the companion's grace time has run out",
        ));
    }
    if vitals.vitals.critical.is_some() {
        return Err(StorageError::InvalidState(
            "the companion urgently needs care; care before starting work",
        ));
    }
    Ok(())
}

#[cfg(feature = "legacy-tasks")]
pub(crate) fn ensure_ready(c: &Connection, meters: MetersView) -> Result<()> {
    ensure_lifecycle_ready(c)?;
    let v = view(c, meters)?;
    let why = if v.sleeping {
        "pet is sleeping; finish its nap before starting work"
    } else if v.sleep_due {
        "pet needs a nap; put it to bed"
    } else if v.bathroom_due {
        "pet needs a bathroom break; use bathroom or care routine"
    } else if meters.hunger != 10000 {
        "pet needs food; all care bars must be full"
    } else if meters.thirst != 10000 {
        "pet needs water; all care bars must be full"
    } else if meters.mental_health != 10000 {
        "pet needs play; all care bars must be full"
    } else if meters.hygiene != 10000 {
        "pet needs a wash; all care bars must be full"
    } else {
        return Ok(());
    };
    Err(StorageError::InvalidState(why))
}

#[cfg(feature = "legacy-tasks")]
pub(crate) fn debit(c: &Connection, task_id: &str, now: i64) -> Result<()> {
    c.execute(
        "INSERT INTO task_care_usage (task_id) VALUES (?1)",
        [task_id],
    )?;
    c.execute(
        "UPDATE pet_state SET hunger = hunger - 1000, thirst = thirst - 1000,
        mental_health = mental_health - 500, hygiene = hygiene - 1000, updated_at_ms = ?1
        WHERE id = 1",
        [now],
    )?;
    c.execute(
        "UPDATE pet_care SET starts_since_bathroom = starts_since_bathroom + 1,
        work_sessions = work_sessions + 1 WHERE id = 1",
        [],
    )?;
    // Starting another task changes readiness but must not consume or invalidate the
    // other task's preflight; the active slot is still authoritative under this lock.
    c.execute(
        "UPDATE pet_care SET revision = revision + 1 WHERE id = 1",
        [],
    )?;
    // Bind the task to the living companion and end every satiety hold.
    lifecycle_store::on_debit(c, task_id, now)
}

#[cfg(feature = "legacy-tasks")]
/// Running and verification are actual supervised execution phases. Waiting, preparation,
/// review and cancellation waiting earn no execution duration. Recovery never invents
/// unobserved runtime after the last durable checkpoint.
pub(crate) fn transition(
    c: &Connection,
    task_id: &str,
    from: State,
    to: State,
    at: &TimeMark,
) -> Result<()> {
    type Usage = (bool, bool, i64, Option<i64>, Option<String>, Option<i64>);
    let usage: Option<Usage> = c
        .query_row(
            "SELECT executed, refunded, execution_ms,
        segment_wall_ms, segment_boot, segment_steady_ms FROM task_care_usage WHERE task_id = ?1",
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
    let Some((executed, refunded, previous, wall, boot, steady)) = usage else {
        return Ok(());
    };
    let running = matches!(to, State::Running | State::Verifying);
    let elapsed = if to != State::Interrupted && matches!(from, State::Running | State::Verifying) {
        wall.map_or(0, |w| {
            at.run_elapsed(
                w,
                boot.as_deref(),
                steady,
                EXECUTION_LIMIT_MS.saturating_sub(previous),
            )
        })
    } else {
        0
    };
    let newly_executed = executed || running;
    // Preserve a high-water cursor even for an older caller timestamp. The writer
    // samples after locking, but this also fails conservatively on clock regression.
    let next_wall = running.then_some(at.wall.max(wall.unwrap_or(at.wall)));
    let next_steady = if running && at.run_clock.as_deref() == boot.as_deref() {
        match (at.run_steady, steady) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        }
    } else if running {
        at.run_steady
    } else {
        None
    };
    c.execute(
        "UPDATE task_care_usage SET executed = ?1, execution_ms = execution_ms + ?2,
        segment_wall_ms = ?3, segment_boot = ?4, segment_steady_ms = ?5 WHERE task_id = ?6",
        params![
            newly_executed,
            elapsed,
            next_wall,
            if running {
                at.run_clock.as_deref()
            } else {
                None
            },
            next_steady,
            task_id
        ],
    )?;
    if elapsed > 0 {
        let before: i64 =
            c.query_row("SELECT execution_ms FROM pet_care WHERE id = 1", [], |r| {
                r.get(0)
            })?;
        c.execute(
            "UPDATE pet_care SET execution_ms = MIN(execution_ms + ?1, ?2) WHERE id = 1",
            params![elapsed, EXECUTION_LIMIT_MS],
        )?;
        if before < EXECUTION_LIMIT_MS && before.saturating_add(elapsed) >= EXECUTION_LIMIT_MS {
            // Recording execution (a snapshot checkpoint or a supervisor step) never
            // invalidates other tasks' approvals: the slot is busy, and Start re-checks
            // readiness under the writer lock, like the start debit. Only accrual that
            // makes a nap due changes readiness, so only it moves the care revision.
            c.execute(
                "UPDATE pet_care SET revision = revision + 1 WHERE id = 1",
                [],
            )?;
        }
    }
    if !newly_executed
        && !refunded
        && matches!(to, State::Failed | State::Cancelled | State::Interrupted)
    {
        c.execute(
            "UPDATE task_care_usage SET refunded = 1 WHERE task_id = ?1",
            [task_id],
        )?;
        c.execute(
            "UPDATE pet_state SET hunger = MIN(hunger + 1000, 10000),
            thirst = MIN(thirst + 1000, 10000), mental_health = MIN(mental_health + 500, 10000),
            hygiene = MIN(hygiene + 1000, 10000), updated_at_ms = ?1 WHERE id = 1",
            [at.wall],
        )?;
        // A meter restored to full gets its satiety hold back.
        lifecycle_store::on_refund(c, task_id, at.wall)?;
        c.execute(
            "UPDATE pet_care SET work_sessions = MAX(work_sessions - 1, 0),
            starts_since_bathroom = MAX(starts_since_bathroom - 1, 0) WHERE id = 1",
            [],
        )?;
        bump(c)?;
    }
    Ok(())
}

/// The running or verifying task whose supervisor is still alive, if any.
#[cfg(feature = "legacy-tasks")]
fn live_execution(c: &Connection) -> Result<Option<(String, State)>> {
    type RunningRow = (String, String, u32, i64);
    let row: Option<RunningRow> = c
        .query_row(
            "SELECT id, state, supervisor_pid, supervisor_start_time FROM tasks
         WHERE state IN ('running','verifying') AND supervisor_pid IS NOT NULL
         AND supervisor_start_time IS NOT NULL LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((id, state, pid, start_time)) = row else {
        return Ok(None);
    };
    let start_time = u64::try_from(start_time).map_err(|_| StorageError::Corrupt)?;
    if !(crate::Supervisor { pid, start_time }).is_alive() {
        return Ok(None);
    }
    Ok(Some((
        id,
        State::parse(&state).ok_or(StorageError::Corrupt)?,
    )))
}

/// True when a snapshot has persisted state to settle: a nap in progress or a live
/// supervised execution segment. Otherwise `state.get` stays a plain read.
pub(crate) fn snapshot_needs_settlement(c: &Connection) -> Result<bool> {
    let sleeping: bool = c.query_row(
        "SELECT sleep_started_at_ms IS NOT NULL FROM pet_care WHERE id = 1",
        [],
        |r| r.get(0),
    )?;
    #[cfg(feature = "legacy-tasks")]
    let sleeping = sleeping || live_execution(c)?.is_some();
    Ok(sleeping)
}

#[cfg(feature = "legacy-tasks")]
/// An explicit snapshot may checkpoint a live supervisor, but never extrapolates
/// execution after a crash. Persisted segment cursors make later transitions idempotent.
pub(crate) fn checkpoint_running(c: &Connection, at: &TimeMark) -> Result<()> {
    if let Some((id, state)) = live_execution(c)? {
        transition(c, &id, state, state, at)?;
    }
    Ok(())
}

#[cfg(feature = "legacy-tasks")]
pub(crate) fn track(category: Category) -> &'static str {
    match category {
        Category::Hunger => "bugfix",
        Category::Thirst => "quality_performance",
        Category::MentalHealth => "learning_specification",
        Category::Hygiene => "cleanup",
    }
}

fn xp_view(mastery: MasteryView) -> XpView {
    let xp = mastery
        .bugfix
        .saturating_add(mastery.quality_performance)
        .saturating_add(mastery.learning_specification)
        .saturating_add(mastery.cleanup);
    // The shared lifecycle-v1 curve [0, 80, 200, 400, 700]; output is byte-identical to
    // the v0.1 inline thresholds (tested).
    let rules = &RULES_V1.steady;
    let level = level_for_xp(xp, rules);
    XpView {
        lifetime_xp: xp,
        level,
        next_level_xp: next_level_xp(level, rules),
        mastery,
    }
}

pub(crate) fn progression(c: &Connection) -> Result<ProgressionView> {
    let mut mastery = MasteryView::default();
    let mut stmt = c.prepare("SELECT track, SUM(amount) FROM demo_xp_rewards GROUP BY track")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)))? {
        let (track, amount) = row?;
        match track.as_str() {
            "bugfix" => mastery.bugfix = amount,
            "quality_performance" => mastery.quality_performance = amount,
            "learning_specification" => mastery.learning_specification = amount,
            "cleanup" => mastery.cleanup = amount,
            _ => return Err(StorageError::Corrupt),
        }
    }
    Ok(ProgressionView {
        real: xp_view(MasteryView::default()),
        demo: xp_view(mastery),
    })
}
