//! The treat jar (spec §6.4 "Treat check" and "Growth", §11 decision 1): real finished turns
//! drop treats into the living life's jar; tapping **Treats** (`treats.give`) is the explicit
//! acceptance that turns one treat into 5 XP, exactly once (invariant R).
//!
//! Durable rules, all computed from `activity_turns` (so they survive restarts, X3):
//! - a turn earns at most one treat (`UNIQUE(session_key, turn_key)`, X1);
//! - the jar holds at most [`JAR_MAX`] unspent treats;
//! - at most one treat per session per 30 s, 12 per rolling hour and 48 per rolling 24 h,
//!   over every life (adopting a new pet never resets the caps). Windows are half-open: an
//!   earn exactly one window ago no longer counts. Forgotten rows (`earned_at_ms = 0`) never
//!   count.
//!
//! With the existing curve (`level_xp` 0/80/200/400/700, stages at levels 2 and 4) a pet is
//! grown after 16 treats and mature after 80. The fold (WP2) earns through [`earn`]; nothing
//! here runs a settlement or touches care, arming or life state (E1).
use crate::life::{self, EvolvedBy};
use crate::model::Snapshot;
use crate::store::V03Policy;
use crate::{fault, lifecycle_store, Result, StorageError, Store};
use codepet_core::agent::{CAP_PER_DAY, CAP_PER_HOUR, CAP_SESSION_MS, JAR_MAX, TREAT_XP};
use rusqlite::{params, Connection, OptionalExtension};

const HOUR_MS: i64 = 60 * 60 * 1000;
const DAY_MS: i64 = 24 * HOUR_MS;

/// The cap verdict for one qualifying turn (`agent_vectors.json` `caps`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapCheck {
    Allow,
    /// The jar already holds [`JAR_MAX`] unspent treats; the UI wiggles the jar.
    JarFull,
    CappedSession,
    CappedHour,
    CappedDay,
}
impl CapCheck {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::JarFull => "jar_full",
            Self::CappedSession => "capped_session",
            Self::CappedHour => "capped_hour",
            Self::CappedDay => "capped_day",
        }
    }
}

/// The pure cap rule, in order: jar full, then the session's 30 s, then 12 per rolling hour,
/// then 48 per rolling 24 h. `session_earned` / `all_earned` are `earned_at_ms` values; 0 is
/// a forgotten row and never counts.
pub fn caps_allow(at: i64, jar: u32, session_earned: &[i64], all_earned: &[i64]) -> CapCheck {
    let within = |t: i64, window: i64| t > 0 && t > at - window;
    if jar >= JAR_MAX {
        CapCheck::JarFull
    } else if session_earned.iter().any(|&t| within(t, CAP_SESSION_MS)) {
        CapCheck::CappedSession
    } else if all_earned.iter().filter(|&&t| within(t, HOUR_MS)).count() >= CAP_PER_HOUR as usize {
        CapCheck::CappedHour
    } else if all_earned.iter().filter(|&&t| within(t, DAY_MS)).count() >= CAP_PER_DAY as usize {
        CapCheck::CappedDay
    } else {
        CapCheck::Allow
    }
}

/// Unspent treats of `life_id`.
pub(crate) fn jar(c: &Connection, life_id: &str) -> Result<u32> {
    let n: i64 = c.query_row(
        "SELECT count(*) FROM activity_turns WHERE life_id = ?1 AND given_at_ms IS NULL",
        [life_id],
        |r| r.get(0),
    )?;
    u32::try_from(n).map_err(|_| StorageError::Corrupt)
}

/// The living life's jar for the export and the snapshot: `(jar, full)`, `(0, false)` with
/// no living life.
pub(crate) fn jar_of_living(c: &Connection) -> Result<(u32, bool)> {
    let Some(l) = life::active_life(c)? else {
        return Ok((0, false));
    };
    let n = jar(c, &l.life_id)?.min(JAR_MAX);
    Ok((n, n >= JAR_MAX))
}

/// [`caps_allow`] over the durable rows: the jar of `life_id` and every life's earns.
pub(crate) fn check(c: &Connection, life_id: &str, session_key: &str, at: i64) -> Result<CapCheck> {
    let jar = jar(c, life_id)?;
    if jar >= JAR_MAX {
        return Ok(CapCheck::JarFull);
    }
    let count = |sql: &str, p: &[&dyn rusqlite::ToSql]| -> Result<i64> {
        Ok(c.query_row(sql, p, |r| r.get(0))?)
    };
    let session = count(
        "SELECT count(*) FROM activity_turns WHERE session_key = ?1 AND earned_at_ms > 0
         AND earned_at_ms > ?2",
        &[&session_key, &(at - CAP_SESSION_MS)],
    )?;
    let window = |ms: i64| -> Result<i64> {
        count(
            "SELECT count(*) FROM activity_turns WHERE earned_at_ms > 0 AND earned_at_ms > ?1",
            &[&(at - ms)],
        )
    };
    Ok(if session > 0 {
        CapCheck::CappedSession
    } else if window(HOUR_MS)? >= i64::from(CAP_PER_HOUR) {
        CapCheck::CappedHour
    } else if window(DAY_MS)? >= i64::from(CAP_PER_DAY) {
        CapCheck::CappedDay
    } else {
        CapCheck::Allow
    })
}

/// What [`earn`] did with one qualifying finished turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Earned {
    /// One treat dropped into the jar.
    Treat,
    /// This (session, turn) already earned (X1); nothing changed.
    Duplicate,
    /// No treat: the jar is full (the export's `finished_jar_full`).
    JarFull,
    /// No treat: a durable cap applies.
    Capped(CapCheck),
    /// No treat: CodePet is Off (X2; the fold drops such events before this).
    Off,
}

/// Drops one treat for a qualifying turn into `life_id`'s jar if the jar and the caps allow
/// it (called by the fold inside its transaction; `session_key` / `turn_key` are the 32-hex
/// HMAC keys, `client` a supported client id). The caller has already decided that the turn
/// qualifies (`agent::turn_qualifies`) and that CodePet is On (X2).
// The fold (WP2) is the caller; until it lands only the tests use it.
#[allow(dead_code)]
pub(crate) fn earn(
    c: &Connection,
    life_id: &str,
    client: &str,
    session_key: &str,
    turn_key: &str,
    at: i64,
) -> Result<Earned> {
    let exists: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM activity_turns WHERE session_key = ?1 AND turn_key = ?2)",
        params![session_key, turn_key],
        |r| r.get(0),
    )?;
    if exists {
        return Ok(Earned::Duplicate);
    }
    if !crate::app::enabled(c)? {
        return Ok(Earned::Off);
    }
    match check(c, life_id, session_key, at)? {
        CapCheck::Allow => {}
        CapCheck::JarFull => return Ok(Earned::JarFull),
        capped => return Ok(Earned::Capped(capped)),
    }
    c.execute(
        "INSERT INTO activity_turns (session_key, turn_key, client, life_id, earned_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![session_key, turn_key, client, life_id, at.max(1)],
    )?;
    Ok(Earned::Treat)
}

impl Store {
    /// `treats.give` (spec §6.2): gives **one** treat to the living companion `life_id`.
    /// Marks the oldest unspent treat given and inserts its `activity_xp` row (5 XP) in the
    /// same transaction, then journals any stage it reaches (`evolved`, `source: treat`).
    /// Exactly once per request id (receipt replay). Refused while Off (`INVALID_STATE`), with
    /// an empty jar (`NO_TREATS`) and while the companion is asleep, paused or fainted
    /// (`PET_NEEDS_CARE`).
    pub fn give_treat(&mut self, request_id: &str, life_id: &str) -> Result<Snapshot> {
        let params = format!("{{\"life_id\":{}}}", serde_json::Value::from(life_id));
        self.v03_command(
            request_id,
            "treats.give",
            &params,
            V03Policy::RefusedWhileOff,
            Some("treat_before_commit"),
            |c, at, _| {
                let life = life::active_life(c)?.ok_or(StorageError::NoActivePet)?;
                if life.life_id != life_id {
                    return Err(StorageError::LifeNotFound);
                }
                let vitals = lifecycle_store::load(c)?;
                if vitals.death_pending {
                    return Err(StorageError::InvalidState(
                        "the companion cannot eat right now",
                    ));
                }
                let oldest: Option<i64> = c
                    .query_row(
                        "SELECT id FROM activity_turns WHERE life_id = ?1 AND given_at_ms IS NULL
                         ORDER BY id LIMIT 1",
                        [&life.life_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                let Some(row) = oldest else {
                    return Err(StorageError::NoTreats);
                };
                let (sleeping, paused): (bool, bool) = c.query_row(
                    "SELECT (SELECT sleep_started_at_ms IS NOT NULL FROM pet_care WHERE id = 1),
                        (SELECT decay_paused FROM pet_state WHERE id = 1)",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                if sleeping || paused || vitals.vitals.fainted {
                    return Err(StorageError::PetNeedsCare);
                }
                let (old_xp, _) = life::life_xp(c, &life.life_id)?;
                let n = c.execute(
                    "UPDATE activity_turns SET given_at_ms = ?1 WHERE id = ?2
                     AND given_at_ms IS NULL",
                    params![at.wall.max(1), row],
                )?;
                if n != 1 {
                    return Err(StorageError::InvalidState("the treat changed concurrently"));
                }
                c.execute(
                    "INSERT INTO activity_xp (life_id, turn_row_id, amount, granted_at_ms,
                        request_id) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![life.life_id, row, TREAT_XP, at.wall, request_id],
                )?;
                if life::record_evolution(
                    c,
                    &life.life_id,
                    EvolvedBy::Treat { request_id },
                    old_xp,
                    TREAT_XP,
                    at.wall,
                )? {
                    fault("evolve_before_commit");
                }
                Ok(true)
            },
        )
    }
}
