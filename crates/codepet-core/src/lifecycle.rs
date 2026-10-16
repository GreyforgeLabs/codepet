//! Ruleset `lifecycle-v1` and the integer-exact attended-time engine (spec §3, §4.3, §6).
//!
//! Pure and deterministic: no clocks, no I/O, no floating point. Storage supplies the
//! credited attended interval and persists the returned vitals and events. Every
//! number is a ruleset value; a life copies its effective rules at adoption and
//! may only become more lenient later.

use crate::{Category, METER_MAX};

pub const RULESET_V1: &str = "lifecycle-v1";
pub const HOUR_MS: i64 = 3_600_000;
pub const TICK_MS: i64 = 60_000;
pub const SETTLE_CAP_MS: i64 = 28_800_000;
/// Presence idle look-back in seconds (UI contract, fixed).
pub const IDLE_TIMEOUT_S: i64 = 180;
/// Hard bound on `advance` iterations; any remainder past it is discarded (never charged).
pub const MAX_ADVANCE_ITERATIONS: usize = 32;

/// Validation bounds (Rust `Rules::validate` and the storage SQL CHECKs agree).
pub const DECAY_MAX: u32 = 25_000;
pub const FULL_HOLD_MS_RANGE: (i64, i64) = (60_000, 14_400_000);
pub const WARNING_AT_RANGE: (u16, u16) = (1_000, 5_000);
pub const CRITICAL_AFTER_MS_RANGE: (i64, i64) = (300_000, 86_400_000);
pub const GRACE_MS_RANGE: (i64, i64) = (900_000, 172_800_000);
/// `pet.configure_life` `decay_percent` range (scales current rates down only).
pub const DECAY_PERCENT_RANGE: (u8, u8) = (25, 100);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Permadeath,
    Gentle,
}
impl Mode {
    pub const ALL: [Self; 2] = [Self::Permadeath, Self::Gentle];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Permadeath => "permadeath",
            Self::Gentle => "gentle",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.as_str() == s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pace {
    Steady,
    Playtest,
}
impl Pace {
    pub const ALL: [Self; 2] = [Self::Steady, Self::Playtest];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Steady => "steady",
            Self::Playtest => "playtest",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifeStatus {
    Healthy,
    Warning,
    Critical,
    Dead,
    Retired,
}
impl LifeStatus {
    pub const ALL: [Self; 5] = [
        Self::Healthy,
        Self::Warning,
        Self::Critical,
        Self::Dead,
        Self::Retired,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Warning => "warning",
            Self::Critical => "critical",
            Self::Dead => "dead",
            Self::Retired => "retired",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }
    /// Living statuses (the `lives_one_living` partial index).
    pub fn is_living(self) -> bool {
        matches!(self, Self::Healthy | Self::Warning | Self::Critical)
    }
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Dead | Self::Retired)
    }
}

/// Effective per-life parameters plus the shared level and stage curves.
/// Index order of the 4-arrays is [`Category::ALL`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rules {
    pub version: &'static str,
    /// Hundredths per attended hour.
    pub decay_per_hour: [u32; 4],
    pub full_hold_ms: i64,
    pub warning_at: u16,
    pub lethal: [bool; 4],
    pub critical_after_ms: i64,
    pub grace_ms: i64,
    pub level_xp: [u32; 5],
    pub stage_min_level: [u8; 3],
}

/// The two v1 presets. A life copies one of them at adoption or migration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RulesetV1 {
    pub steady: Rules,
    pub playtest: Rules,
}
impl RulesetV1 {
    pub fn for_pace(&self, pace: Pace) -> &Rules {
        match pace {
            Pace::Steady => &self.steady,
            Pace::Playtest => &self.playtest,
        }
    }
}

pub const RULES_V1: RulesetV1 = RulesetV1 {
    steady: Rules {
        version: RULESET_V1,
        decay_per_hour: [1000, 1250, 600, 500],
        full_hold_ms: 3_600_000,
        warning_at: 3000,
        lethal: [true, true, false, false],
        critical_after_ms: 7_200_000,
        grace_ms: 21_600_000,
        level_xp: [0, 80, 200, 400, 700],
        stage_min_level: [1, 2, 4],
    },
    playtest: Rules {
        version: RULESET_V1,
        decay_per_hour: [20000, 25000, 12000, 10000],
        full_hold_ms: 180_000,
        warning_at: 3000,
        lethal: [true, true, false, false],
        critical_after_ms: 360_000,
        grace_ms: 1_080_000,
        level_xp: [0, 80, 200, 400, 700],
        stage_min_level: [1, 2, 4],
    },
};

const MORE_LENIENT_ONLY: &str = "settings can only become more lenient for a living companion";

impl Rules {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != RULESET_V1 {
            return Err("unknown ruleset version");
        }
        if self.decay_per_hour.iter().any(|&d| d > DECAY_MAX) {
            return Err("decay_per_hour must be 0..=25000");
        }
        if !(FULL_HOLD_MS_RANGE.0..=FULL_HOLD_MS_RANGE.1).contains(&self.full_hold_ms) {
            return Err("full_hold_ms out of range");
        }
        if !(WARNING_AT_RANGE.0..=WARNING_AT_RANGE.1).contains(&self.warning_at) {
            return Err("warning_at out of range");
        }
        if !self.lethal.iter().any(|&l| l) {
            return Err("at least one need must be lethal");
        }
        if self.lethal[Category::MentalHealth.index()] || self.lethal[Category::Hygiene.index()] {
            return Err("only hunger and thirst may be lethal");
        }
        if !(CRITICAL_AFTER_MS_RANGE.0..=CRITICAL_AFTER_MS_RANGE.1)
            .contains(&self.critical_after_ms)
        {
            return Err("critical_after_ms out of range");
        }
        if !(GRACE_MS_RANGE.0..=GRACE_MS_RANGE.1).contains(&self.grace_ms) {
            return Err("grace_ms out of range");
        }
        if self.grace_ms < self.critical_after_ms {
            return Err("grace_ms must be at least critical_after_ms");
        }
        if self.level_xp[0] != 0 || self.level_xp.windows(2).any(|w| w[0] >= w[1]) {
            return Err("level_xp must start at 0 and strictly increase");
        }
        let levels = self.level_xp.len() as u8;
        if self.stage_min_level[0] != 1
            || self.stage_min_level.windows(2).any(|w| w[0] > w[1])
            || self.stage_min_level.iter().any(|&m| m == 0 || m > levels)
        {
            return Err("stage_min_level must start at 1, not decrease and stay within 1..=5");
        }
        Ok(())
    }

    /// True when `self` can replace `old` under the leniency-only rule (D7c):
    /// grace ≥, critical_after ≥, each decay rate ≤, and every other field identical.
    pub fn is_more_lenient_or_equal(&self, old: &Rules) -> bool {
        self.grace_ms >= old.grace_ms
            && self.critical_after_ms >= old.critical_after_ms
            && self
                .decay_per_hour
                .iter()
                .zip(old.decay_per_hour.iter())
                .all(|(new, old)| new <= old)
            && self.version == old.version
            && self.full_hold_ms == old.full_hold_ms
            && self.warning_at == old.warning_at
            && self.lethal == old.lethal
            && self.level_xp == old.level_xp
            && self.stage_min_level == old.stage_min_level
    }

    /// Applies a `pet.configure_life` request (§3 "Leniency"). Each `Some` value is a
    /// new absolute setting; a stricter value, an out-of-range value or a
    /// `decay_percent` outside 25..=100 is refused. `decay_percent` scales the current
    /// rates down (floor). A raised `critical_after_ms` above `grace_ms` raises
    /// `grace_ms` to match, so the result always validates when the request is lenient.
    pub fn with_leniency(
        &self,
        grace_ms: Option<i64>,
        critical_after_ms: Option<i64>,
        decay_percent: Option<u8>,
    ) -> Result<Rules, &'static str> {
        let mut next = self.clone();
        if let Some(g) = grace_ms {
            if g < self.grace_ms {
                return Err(MORE_LENIENT_ONLY);
            }
            next.grace_ms = g;
        }
        if let Some(c) = critical_after_ms {
            if c < self.critical_after_ms {
                return Err(MORE_LENIENT_ONLY);
            }
            next.critical_after_ms = c;
            next.grace_ms = next.grace_ms.max(c);
        }
        if let Some(p) = decay_percent {
            if !(DECAY_PERCENT_RANGE.0..=DECAY_PERCENT_RANGE.1).contains(&p) {
                return Err("decay_percent must be 25..=100");
            }
            for rate in &mut next.decay_per_hour {
                *rate = *rate * u32::from(p) / 100;
            }
        }
        next.validate()?;
        if !next.is_more_lenient_or_equal(self) {
            return Err(MORE_LENIENT_ONLY);
        }
        Ok(next)
    }

    pub fn is_lethal(&self, need: Category) -> bool {
        self.lethal[need.index()]
    }
}

/// A running critical period. `grace_ms` is frozen at entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Critical {
    pub need: Category,
    pub grace_ms: i64,
    pub elapsed_ms: i64,
}

/// The clock state of the active life. Meters are hundredths (10000 = full).
/// `carry` is the fractional decay not yet taken from a meter, in
/// hundredths × ms per hour (`0..HOUR_MS`). Only lethal entries of `neglect_ms` move.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Vitals {
    pub meters: [u16; 4],
    pub hold_ms: [i64; 4],
    pub carry: [i64; 4],
    pub neglect_ms: [i64; 4],
    pub critical: Option<Critical>,
    pub fainted: bool,
}

impl Vitals {
    /// Fresh vitals for the given meters: every full meter starts a satiety hold
    /// (spec §7 data step 4); nothing is backdated.
    pub fn from_meters(meters: [u16; 4], r: &Rules) -> Self {
        let mut hold_ms = [0; 4];
        for (hold, meter) in hold_ms.iter_mut().zip(meters) {
            if meter >= METER_MAX {
                *hold = r.full_hold_ms;
            }
        }
        Self {
            meters,
            hold_ms,
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    EnteredCritical(Category),
    Recovered,
    Fainted,
    Died(Category),
}
impl Event {
    /// Stable journal kind.
    pub fn kind(self) -> &'static str {
        match self {
            Self::EnteredCritical(_) => "entered_critical",
            Self::Recovered => "recovered",
            Self::Fainted => "fainted",
            Self::Died(_) => "died",
        }
    }
    pub fn need(self) -> Option<Category> {
        match self {
            Self::EnteredCritical(c) | Self::Died(c) => Some(c),
            Self::Recovered | Self::Fainted => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Advance {
    pub vitals: Vitals,
    pub events: Vec<Event>,
    pub charged_ms: i64,
    pub discarded_ms: i64,
}

/// Charges `delta_ms` of credited attended time (clamped to `0..=SETTLE_CAP_MS`).
/// `charged_ms + discarded_ms` always equals the clamped delta. The step that enters
/// critical, and the step that ends a permadeath life, discard the rest (D7a).
pub fn advance(v: &Vitals, r: &Rules, mode: Mode, delta_ms: i64) -> Advance {
    advance_traced(v, r, mode, delta_ms).0
}

/// [`advance`] plus the iteration count, for the termination tests.
/// The parallel 4-array index loops deliberately mirror the spec §4.3 pseudocode.
#[allow(clippy::needless_range_loop)]
pub(crate) fn advance_traced(v: &Vitals, r: &Rules, mode: Mode, delta_ms: i64) -> (Advance, usize) {
    let mut s = v.clone();
    let mut events = Vec::new();
    let mut remaining = delta_ms.clamp(0, SETTLE_CAP_MS);
    let mut charged = 0_i64;
    let mut discarded = 0_i64;
    let mut iterations = 0_usize;
    let rate = r.decay_per_hour.map(i64::from);
    while remaining > 0 {
        if iterations == MAX_ADVANCE_ITERATIONS {
            discarded += remaining;
            break;
        }
        iterations += 1;
        let mut step = remaining;
        for i in 0..4 {
            if s.hold_ms[i] > 0 {
                step = step.min(s.hold_ms[i]);
            } else if rate[i] > 0 && s.meters[i] > 0 {
                let need = i64::from(s.meters[i]) * HOUR_MS - s.carry[i];
                step = step.min(ceil_div(need, rate[i]));
            }
        }
        match s.critical {
            None => {
                for j in 0..4 {
                    if r.lethal[j] && s.meters[j] == 0 && s.hold_ms[j] == 0 {
                        step = step.min(r.critical_after_ms - s.neglect_ms[j]);
                    }
                }
            }
            Some(c) if !s.fainted => step = step.min(c.grace_ms - c.elapsed_ms),
            Some(_) => {}
        }
        let step = step.max(1);
        let neglecting: [bool; 4] =
            std::array::from_fn(|j| r.lethal[j] && s.meters[j] == 0 && s.hold_ms[j] == 0);
        for i in 0..4 {
            if s.hold_ms[i] > 0 {
                s.hold_ms[i] -= step;
            } else {
                let t = rate[i] * step + s.carry[i];
                let meter = (i64::from(s.meters[i]) - t / HOUR_MS).max(0);
                s.meters[i] = meter as u16;
                s.carry[i] = if meter == 0 { 0 } else { t % HOUR_MS };
            }
        }
        for j in 0..4 {
            if neglecting[j] {
                s.neglect_ms[j] += step;
            }
        }
        if let (Some(c), false) = (s.critical.as_mut(), s.fainted) {
            c.elapsed_ms += step;
        }
        remaining -= step;
        charged += step;
        if let (Some(c), false) = (s.critical, s.fainted) {
            if c.elapsed_ms >= c.grace_ms {
                match mode {
                    Mode::Permadeath => {
                        events.push(Event::Died(c.need));
                        discarded += remaining;
                        break;
                    }
                    Mode::Gentle => {
                        s.fainted = true;
                        events.push(Event::Fainted);
                    }
                }
            }
        }
        if s.critical.is_none() {
            if let Some(need) = most_neglected(&s, r) {
                s.critical = Some(Critical {
                    need,
                    grace_ms: r.grace_ms,
                    elapsed_ms: 0,
                });
                events.push(Event::EnteredCritical(need));
                discarded += remaining;
                break;
            }
        }
    }
    (
        Advance {
            vitals: s,
            events,
            charged_ms: charged,
            discarded_ms: discarded,
        },
        iterations,
    )
}

/// The lethal need with the largest neglect at or beyond `critical_after_ms`;
/// ties resolve in [`Category::ALL`] order.
fn most_neglected(s: &Vitals, r: &Rules) -> Option<Category> {
    let mut best: Option<Category> = None;
    for c in Category::ALL {
        let j = c.index();
        if r.lethal[j]
            && s.neglect_ms[j] >= r.critical_after_ms
            && best.is_none_or(|b| s.neglect_ms[j] > s.neglect_ms[b.index()])
        {
            best = Some(c);
        }
    }
    best
}

/// Lethal needs (in [`Category::ALL`] order) whose neglect is at or beyond `critical_after_ms`.
fn still_critical(s: &Vitals, r: &Rules) -> Vec<Category> {
    Category::ALL
        .into_iter()
        .filter(|c| r.lethal[c.index()] && s.neglect_ms[c.index()] >= r.critical_after_ms)
        .collect()
}

fn ceil_div(a: i64, b: i64) -> i64 {
    // Callers pass b > 0. A non-positive numerator means "already there".
    if a <= 0 {
        0
    } else {
        (a + b - 1) / b
    }
}

/// Matching care: each filled need becomes full with a fresh satiety hold, zero
/// carry and (if lethal) zero neglect. A running critical changes only when its own
/// need is filled: it clears when no lethal need is still at or beyond
/// `critical_after_ms`, and otherwise moves to that need, keeping the frozen grace and
/// elapsed time (no fresh grace is granted). Unrelated care never touches it, even when a
/// leniency change has since raised `critical_after_ms` past the critical need's
/// neglect: [`on_leniency`] only extends a running grace, and [`advance`] keeps counting it.
pub fn apply_care(v: &Vitals, r: &Rules, filled: &[Category]) -> (Vitals, Vec<Event>) {
    let mut s = v.clone();
    let mut events = Vec::new();
    for c in filled {
        let i = c.index();
        s.meters[i] = METER_MAX;
        s.hold_ms[i] = r.full_hold_ms;
        s.carry[i] = 0;
        if r.lethal[i] {
            s.neglect_ms[i] = 0;
        }
    }
    if let Some(crit) = s.critical.filter(|c| filled.contains(&c.need)) {
        match still_critical(&s, r).first() {
            None => {
                s.critical = None;
                s.fainted = false;
                events.push(Event::Recovered);
            }
            Some(&next) => s.critical = Some(Critical { need: next, ..crit }),
        }
    }
    (s, events)
}

#[cfg(feature = "legacy-tasks")]
/// A Start debit: every satiety hold ends (the debited meters are no longer full).
pub fn on_debit(v: &Vitals) -> Vitals {
    Vitals {
        hold_ms: [0; 4],
        ..v.clone()
    }
}

#[cfg(feature = "legacy-tasks")]
/// A refund restored some meters. `v.meters` must already hold the refunded values;
/// each listed need that is now full gets its satiety hold back. Others are unchanged.
pub fn on_refund(v: &Vitals, r: &Rules, restored: &[Category]) -> Vitals {
    let mut s = v.clone();
    for c in restored {
        let i = c.index();
        if s.meters[i] >= METER_MAX {
            s.hold_ms[i] = r.full_hold_ms;
        }
    }
    s
}

/// A leniency change took effect: a frozen grace already running becomes
/// `max(frozen, r.grace_ms)`, so a deadline can only move later. A running critical is
/// never cleared here, even when `r.critical_after_ms` now exceeds its need's neglect:
/// only care for that need ends it (see [`apply_care`]).
pub fn on_leniency(v: &Vitals, r: &Rules) -> Vitals {
    let mut s = v.clone();
    if let Some(c) = s.critical.as_mut() {
        c.grace_ms = c.grace_ms.max(r.grace_ms);
    }
    s
}

/// Healthy | Warning | Critical only; Dead and Retired are storage states.
pub fn status(v: &Vitals, r: &Rules) -> LifeStatus {
    if v.critical.is_some() {
        LifeStatus::Critical
    } else if v.meters.iter().any(|&m| m <= r.warning_at)
        || (0..4).any(|j| r.lethal[j] && v.neglect_ms[j] > 0)
    {
        LifeStatus::Warning
    } else {
        LifeStatus::Healthy
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NeedState {
    Ok,
    Low,
    Empty,
    Critical,
}
impl NeedState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Low => "low",
            Self::Empty => "empty",
            Self::Critical => "critical",
        }
    }
}

/// Attended-time estimates for one need. `None` means "does not happen under these
/// rules" (zero decay, a non-lethal need, or gentle mode for `until_end_ms`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NeedWindow {
    pub need: Category,
    pub lethal: bool,
    pub state: NeedState,
    pub until_warning_ms: Option<i64>,
    pub until_critical_ms: Option<i64>,
    pub until_end_ms: Option<i64>,
}

/// Per-need estimates of attended time, assuming no care and no other change.
///
/// - `until_warning_ms`: until the meter is ≤ `warning_at` (0 when already there).
/// - `until_critical_ms` (lethal only): until this need's neglect reaches
///   `critical_after_ms` (0 when already there).
/// - `until_end_ms` (lethal, permadeath only): for a running critical, the frozen
///   grace left for the critical need and for any lethal need also past
///   `critical_after_ms`; otherwise `until_critical_ms + grace_ms`.
///
/// `until_end_ms` is also `None` for an unarmed life; the caller (which knows the
/// armed flag) clears it there.
pub fn windows(v: &Vitals, r: &Rules, mode: Mode) -> [NeedWindow; 4] {
    Category::ALL.map(|need| {
        let i = need.index();
        let lethal = r.lethal[i];
        let meter = i64::from(v.meters[i]);
        let hold = v.hold_ms[i].max(0);
        let rate = i64::from(r.decay_per_hour[i]);
        let is_critical_need = v.critical.is_some_and(|c| c.need == need);
        let state = if is_critical_need {
            NeedState::Critical
        } else if meter == 0 {
            NeedState::Empty
        } else if meter <= i64::from(r.warning_at) {
            NeedState::Low
        } else {
            NeedState::Ok
        };
        let until_warning_ms = if meter <= i64::from(r.warning_at) {
            Some(0)
        } else if rate == 0 {
            None
        } else {
            let need_t = (meter - i64::from(r.warning_at)) * HOUR_MS - v.carry[i];
            Some(hold + ceil_div(need_t, rate))
        };
        let until_zero = if meter == 0 {
            Some(hold)
        } else if rate == 0 {
            None
        } else {
            Some(hold + ceil_div(meter * HOUR_MS - v.carry[i], rate))
        };
        let until_critical_ms = if !lethal {
            None
        } else if v.neglect_ms[i] >= r.critical_after_ms {
            Some(0)
        } else {
            until_zero.map(|z| z + (r.critical_after_ms - v.neglect_ms[i].max(0)))
        };
        let until_end_ms = match (mode, lethal) {
            (Mode::Gentle, _) | (_, false) => None,
            (Mode::Permadeath, true) => match v.critical {
                Some(c) if is_critical_need || v.neglect_ms[i] >= r.critical_after_ms => {
                    Some((c.grace_ms - c.elapsed_ms).max(0))
                }
                _ => until_critical_ms.map(|u| u + r.grace_ms),
            },
        };
        NeedWindow {
            need,
            lethal,
            state,
            until_warning_ms,
            until_critical_ms,
            until_end_ms,
        }
    })
}
