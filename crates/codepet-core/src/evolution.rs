//! XP → level → stage derivation. Stage is always derived from a life's current
//! authoritative XP; journal rows are history and never drive it. Pure; no I/O.

use crate::lifecycle::Rules;

/// Stable stage keys, index `stage - 1`.
pub const STAGE_KEYS: [&str; 3] = ["young", "grown", "mature"];

/// The next stage a life can reach and the XP it needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NextStage {
    pub stage: u8,
    pub min_level: u8,
    pub xp: u32,
}

/// 1..=5. Identical to the v0.1 `xp_view` count of thresholds at or below `xp`.
pub fn level_for_xp(xp: u32, r: &Rules) -> u8 {
    r.level_xp.iter().filter(|&&t| xp >= t).count().max(1) as u8
}

/// XP needed for the level after `level`; `None` at the top level. Same indexing as
/// the v0.1 `xp_view` (`thresholds.get(level)`).
pub fn next_level_xp(level: u8, r: &Rules) -> Option<u32> {
    r.level_xp.get(usize::from(level)).copied()
}

/// 1..=3: the highest stage whose minimum level is reached.
pub fn stage_for_level(level: u8, r: &Rules) -> u8 {
    r.stage_min_level
        .iter()
        .filter(|&&m| level >= m)
        .count()
        .max(1) as u8
}

pub fn stage_for_xp(xp: u32, r: &Rules) -> u8 {
    stage_for_level(level_for_xp(xp, r), r)
}

/// The first stage above the current one, or `None` at the final stage.
pub fn next_stage(xp: u32, r: &Rules) -> Option<NextStage> {
    let current = stage_for_xp(xp, r);
    let idx = usize::from(current);
    let min_level = *r.stage_min_level.get(idx)?;
    let need = *r.level_xp.get(usize::from(min_level).checked_sub(1)?)?;
    Some(NextStage {
        stage: current + 1,
        min_level,
        xp: need,
    })
}

/// Stages newly reached when XP moves from `old_xp` to `new_xp`, ascending.
/// Empty when XP does not rise across a stage boundary (including any decrease).
pub fn stages_crossed(old_xp: u32, new_xp: u32, r: &Rules) -> Vec<u8> {
    let from = stage_for_xp(old_xp, r);
    let to = stage_for_xp(new_xp, r);
    (from + 1..=to).collect()
}

/// The stable key for a stage (1..=3).
pub fn stage_key(stage: u8) -> Option<&'static str> {
    STAGE_KEYS.get(usize::from(stage).checked_sub(1)?).copied()
}
