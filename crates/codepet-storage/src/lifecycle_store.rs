//! The active life's clock state: vitals I/O, attended-presence settlement (spec §5.2),
//! and the care/debit/refund hooks (spec §8.2). Every function runs inside the caller's
//! `BEGIN IMMEDIATE` transaction. No timer, thread, file write or deletion exists here.
use crate::care::{self, TimeMark};
use crate::life::{self, LifeRow};
use crate::presence_io::{self, PresenceRead};
use crate::{fault, Result, StatePaths, StorageError};
use codepet_core::lifecycle::{self as lc, Critical, Event, Vitals, SETTLE_CAP_MS, TICK_MS};
use codepet_core::Category;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;

/// The singleton `life_vitals` row joined with the `pet_state` meters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VitalsRow {
    pub life_id: Option<String>,
    pub vitals: Vitals,
    pub critical_since_ms: Option<i64>,
    pub death_pending: bool,
    pub attended_total_ms: i64,
}

pub(crate) fn load(c: &Connection) -> Result<VitalsRow> {
    type Raw = (
        Option<String>,
        [u16; 4],
        [i64; 4],
        [i64; 4],
        [i64; 2],
        (Option<String>, Option<i64>, i64, Option<i64>),
        (bool, bool, i64),
    );
    let raw: Raw = c.query_row(
        "SELECT v.life_id, p.hunger, p.thirst, p.mental_health, p.hygiene,
            v.hold_hunger, v.hold_thirst, v.hold_mental_health, v.hold_hygiene,
            v.carry_hunger, v.carry_thirst, v.carry_mental_health, v.carry_hygiene,
            v.neglect_hunger_ms, v.neglect_thirst_ms,
            v.critical_need, v.critical_grace_ms, v.critical_elapsed_ms, v.critical_since_ms,
            v.fainted, v.death_pending, v.attended_total_ms
         FROM life_vitals v, pet_state p WHERE v.id = 1 AND p.id = 1",
        [],
        |r| {
            Ok((
                r.get(0)?,
                [r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?],
                [r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?],
                [r.get(9)?, r.get(10)?, r.get(11)?, r.get(12)?],
                [r.get(13)?, r.get(14)?],
                (r.get(15)?, r.get(16)?, r.get(17)?, r.get(18)?),
                (r.get(19)?, r.get(20)?, r.get(21)?),
            ))
        },
    )?;
    let (life_id, meters, hold_ms, carry, neglect, crit, (fainted, death_pending, attended)) = raw;
    let (need, grace, elapsed, since) = crit;
    let critical = match (need, grace) {
        (None, None) => None,
        (Some(need), Some(grace_ms)) => Some(Critical {
            need: Category::parse(&need)
                .filter(|c| matches!(c, Category::Hunger | Category::Thirst))
                .ok_or(StorageError::Corrupt)?,
            grace_ms,
            elapsed_ms: elapsed,
        }),
        _ => return Err(StorageError::Corrupt),
    };
    Ok(VitalsRow {
        life_id,
        vitals: Vitals {
            meters,
            hold_ms,
            carry,
            neglect_ms: [neglect[0], neglect[1], 0, 0],
            critical,
            fainted,
        },
        critical_since_ms: since,
        death_pending,
        attended_total_ms: attended,
    })
}

/// Persists `row`. Meters go to `pet_state` (touching `updated_at_ms` only when a meter
/// actually changed); everything else to `life_vitals`. Returns whether a meter changed.
pub(crate) fn save(c: &Connection, row: &VitalsRow, now: i64) -> Result<bool> {
    let before = load(c)?.vitals.meters;
    let m = row.vitals.meters;
    let changed = before != m;
    if changed {
        c.execute(
            "UPDATE pet_state SET hunger = ?1, thirst = ?2, mental_health = ?3, hygiene = ?4,
                updated_at_ms = ?5 WHERE id = 1",
            params![m[0], m[1], m[2], m[3], now],
        )?;
    }
    let v = &row.vitals;
    // Only lethal needs (hunger, thirst) accrue neglect; core never moves the others.
    debug_assert!(v.neglect_ms[2] == 0 && v.neglect_ms[3] == 0);
    let crit = v.critical;
    c.execute(
        "UPDATE life_vitals SET life_id = ?1,
            hold_hunger = ?2, hold_thirst = ?3, hold_mental_health = ?4, hold_hygiene = ?5,
            carry_hunger = ?6, carry_thirst = ?7, carry_mental_health = ?8, carry_hygiene = ?9,
            neglect_hunger_ms = ?10, neglect_thirst_ms = ?11,
            critical_need = ?12, critical_grace_ms = ?13, critical_elapsed_ms = ?14,
            critical_since_ms = ?15, fainted = ?16, death_pending = ?17, attended_total_ms = ?18
         WHERE id = 1",
        params![
            row.life_id,
            v.hold_ms[0],
            v.hold_ms[1],
            v.hold_ms[2],
            v.hold_ms[3],
            v.carry[0],
            v.carry[1],
            v.carry[2],
            v.carry[3],
            v.neglect_ms[0],
            v.neglect_ms[1],
            crit.map(|c| c.need.as_str()),
            crit.map(|c| c.grace_ms),
            crit.map_or(0, |c| c.elapsed_ms),
            crit.and(row.critical_since_ms),
            v.fainted,
            row.death_pending,
            row.attended_total_ms
        ],
    )?;
    Ok(changed)
}

/// Re-derives `lives.status` (healthy, warning or critical) from the vitals and journals
/// a transition as `health_changed`.
pub(crate) fn rederive_status(c: &Connection, life: &LifeRow, v: &Vitals, now: i64) -> Result<()> {
    let next = lc::status(v, &life.rules);
    if next == life.status {
        return Ok(());
    }
    let n = c.execute(
        "UPDATE lives SET status = ?1 WHERE life_id = ?2 AND status = ?3",
        params![next.as_str(), life.life_id, life.status.as_str()],
    )?;
    if n != 1 {
        return Err(StorageError::InvalidState("companion changed concurrently"));
    }
    life::journal(
        c,
        &life.life_id,
        "health_changed",
        None,
        &json!({"from": life.status.as_str(), "to": next.as_str()}),
        None,
        None,
        now,
    )?;
    Ok(())
}

fn reject(c: &Connection) -> Result<()> {
    c.execute(
        "UPDATE meta SET presence_rejections = presence_rejections + 1 WHERE id = 1",
        [],
    )?;
    Ok(())
}

/// Spec §5.2: charges the bar's attended presence to the living companion, at most once
/// per session and capped by CLOCK_BOOTTIME elapsed since the previous settlement. Any
/// uncertainty (no boot clock, a refused file, a blocker) charges nothing.
pub(crate) fn settle_presence(c: &Connection, paths: &StatePaths, at: &TimeMark) -> Result<()> {
    // 1. Clock guard: without a boot clock, time stays frozen and nothing is consumed.
    let (Some(boot), Some(steady)) = (at.boot.as_deref(), at.steady) else {
        return Ok(());
    };
    let now = at.wall;
    let cursor: Option<(Option<String>, Option<i64>)> = c
        .query_row(
            "SELECT last_boot, last_steady_ms FROM presence_clock WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (last_boot, last_steady) = cursor.clone().unwrap_or((None, None));
    let same_boot = last_boot.as_deref() == Some(boot);
    let cap = match (same_boot, last_steady) {
        (true, Some(last)) => steady.saturating_sub(last).saturating_add(TICK_MS).max(0),
        _ => SETTLE_CAP_MS,
    }
    .min(SETTLE_CAP_MS);
    // A same-boot cursor never moves backwards.
    let next_steady = match (same_boot, last_steady) {
        (true, Some(last)) => steady.max(last),
        _ => steady,
    };
    if cursor.is_some() {
        c.execute(
            "UPDATE presence_clock SET last_boot = ?1, last_steady_ms = ?2, last_wall_ms = ?3
             WHERE id = 1",
            params![boot, next_steady, now],
        )?;
    } else {
        c.execute(
            "INSERT INTO presence_clock (id, last_boot, last_steady_ms, last_wall_ms)
             VALUES (1, ?1, ?2, ?3)",
            params![boot, next_steady, now],
        )?;
    }
    // 2. The sidecar.
    let doc = match presence_io::read(&paths.presence_path(), now) {
        PresenceRead::Missing => return Ok(()),
        PresenceRead::Refused(_) => return reject(c),
        PresenceRead::Doc(doc) => doc,
    };
    // 3. The append-only ledger: each session's attended time is consumed exactly once.
    let consumed: Option<i64> = c
        .query_row(
            "SELECT consumed_ms FROM presence_sessions WHERE session_id = ?1",
            [&doc.session],
            |r| r.get(0),
        )
        .optional()?;
    if consumed.is_none() {
        c.execute(
            "INSERT INTO presence_sessions (session_id, consumed_ms, first_seen_at_ms,
                last_settled_at_ms) VALUES (?1, 0, ?2, ?2)",
            params![doc.session, now],
        )?;
    }
    let available = doc.attended_ms - consumed.unwrap_or(0);
    if available < 0 {
        return reject(c);
    }
    if available == 0 {
        return Ok(());
    }
    // 4. Cap by boot time.
    let mut credit = available.min(cap);
    let mut discard = available - credit;
    // 5. Blockers discard everything: the state they describe held across the interval,
    // because every state change is itself a settling write.
    let life = life::active_life(c)?;
    let mut row = load(c)?;
    let paused: bool = c.query_row("SELECT decay_paused FROM pet_state WHERE id = 1", [], |r| {
        r.get(0)
    })?;
    // Off is a blocker too (spec §7.3): the time is discarded, never banked, so On resumes
    // with no catch-up.
    let blocked = match &life {
        None => true,
        Some(l) => {
            l.life_id != doc.life_id
                || !l.armed
                || paused
                || row.death_pending
                || life::slot_busy(c)?
                || !crate::app::enabled(c)?
        }
    };
    if blocked {
        discard += credit;
        credit = 0;
    }
    // 6. Nap: the remaining rest is discarded, the rest is credited (evaluated before
    // `settle_sleep`, so a finished nap can never freeze time).
    if credit > 0 {
        let (sleeping, remaining): (bool, i64) = c.query_row(
            "SELECT sleep_started_at_ms IS NOT NULL, sleep_remaining_ms FROM pet_care
             WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if sleeping {
            let nap = credit.min(remaining.max(0));
            credit -= nap;
            discard += nap;
        }
    }
    // 7. Charge.
    let mut charged = 0;
    if let (Some(life), true) = (life, credit > 0) {
        let adv = lc::advance(&row.vitals, &life.rules, life.mode, credit);
        charged = adv.charged_ms;
        discard += credit - adv.charged_ms;
        row.vitals = adv.vitals;
        row.attended_total_ms = row.attended_total_ms.saturating_add(adv.charged_ms);
        for event in &adv.events {
            match *event {
                Event::EnteredCritical(need) => {
                    row.critical_since_ms = Some(now);
                    journal_need(c, &life, "critical", need, now)?;
                }
                Event::Fainted => {
                    let need = row.vitals.critical.map_or(Category::Thirst, |c| c.need);
                    journal_need(c, &life, "fainted", need, now)?;
                }
                Event::Recovered => {
                    row.critical_since_ms = None;
                    life::journal(
                        c,
                        &life.life_id,
                        "recovered",
                        None,
                        &json!({}),
                        None,
                        None,
                        now,
                    )?;
                }
                Event::Died(need) => {
                    row.death_pending = true;
                    journal_need(c, &life, "death_pending", need, now)?;
                }
            }
        }
        if save(c, &row, now)? {
            care::bump(c)?;
        }
        rederive_status(c, &life, &row.vitals, now)?;
    }
    c.execute(
        "UPDATE presence_sessions SET consumed_ms = ?1, credited_ms = credited_ms + ?2,
            discarded_ms = discarded_ms + ?3, last_settled_at_ms = ?4 WHERE session_id = ?5",
        params![doc.attended_ms, charged, discard, now, doc.session],
    )?;
    fault("presence_before_commit");
    Ok(())
}

fn journal_need(
    c: &Connection,
    life: &LifeRow,
    kind: &str,
    need: Category,
    now: i64,
) -> Result<()> {
    life::journal(
        c,
        &life.life_id,
        kind,
        None,
        &json!({"need": need.as_str()}),
        None,
        None,
        now,
    )?;
    Ok(())
}

/// Matching care filled `filled`: full meters, fresh satiety holds, lethal neglect reset,
/// and recovery when no lethal need is still critical. No fresh grace is ever granted.
pub(crate) fn on_care(c: &Connection, filled: &[Category], now: i64) -> Result<()> {
    let Some(life) = life::active_life(c)? else {
        return Ok(());
    };
    let mut row = load(c)?;
    let (v, events) = lc::apply_care(&row.vitals, &life.rules, filled);
    row.vitals = v;
    if row.vitals.critical.is_none() {
        row.critical_since_ms = None;
    }
    for event in events {
        if event == Event::Recovered {
            life::journal(
                c,
                &life.life_id,
                "recovered",
                None,
                &json!({}),
                None,
                None,
                now,
            )?;
        }
    }
    save(c, &row, now)?;
    rederive_status(c, &life, &row.vitals, now)
}

#[cfg(feature = "legacy-tasks")]
/// A Start debit: binds the task to the living companion and ends every satiety hold.
pub(crate) fn on_debit(c: &Connection, task_id: &str, now: i64) -> Result<()> {
    let life = life::active_life(c)?.ok_or(StorageError::NoActivePet)?;
    c.execute(
        "UPDATE tasks SET life_id = ?1 WHERE id = ?2",
        params![life.life_id, task_id],
    )?;
    let mut row = load(c)?;
    row.vitals = lc::on_debit(&row.vitals);
    save(c, &row, now)?;
    rederive_status(c, &life, &row.vitals, now)
}

#[cfg(feature = "legacy-tasks")]
/// A pre-execution refund restored the meters of `task_id`'s life: each meter now full
/// gets its satiety hold back. A refund for a life that is no longer active touches no
/// vitals.
pub(crate) fn on_refund(c: &Connection, task_id: &str, now: i64) -> Result<()> {
    let Some(life) = life::active_life(c)? else {
        return Ok(());
    };
    let owner: Option<String> = c
        .query_row("SELECT life_id FROM tasks WHERE id = ?1", [task_id], |r| {
            r.get(0)
        })
        .optional()?
        .flatten();
    if owner.as_deref() != Some(life.life_id.as_str()) {
        return Ok(());
    }
    let mut row = load(c)?;
    row.vitals = lc::on_refund(&row.vitals, &life.rules, &Category::ALL);
    save(c, &row, now)?;
    rederive_status(c, &life, &row.vitals, now)
}
