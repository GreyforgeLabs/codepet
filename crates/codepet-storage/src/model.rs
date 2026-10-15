//! Result shapes from docs/milestone-1.md. Enums serialize as their stable core strings.
use codepet_core::{Category, Draft, Provider, State, Verification};
use serde::{Deserialize, Serialize};

macro_rules! str_serde {
    ($module:ident, $ty:ty) => {
        pub(crate) mod $module {
            use super::*;
            pub fn serialize<S: serde::Serializer>(v: &$ty, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(v.as_str())
            }
            #[allow(dead_code)]
            pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<$ty, D::Error> {
                let s = String::deserialize(d)?;
                <$ty>::parse(&s).ok_or_else(|| serde::de::Error::custom("unknown enum value"))
            }
        }
    };
}
str_serde!(state_str, State);
str_serde!(category_str, Category);
str_serde!(provider_str, Provider);
str_serde!(verification_str, Verification);
str_serde!(outcome_str, MockOutcome);

pub(crate) mod opt_state_str {
    use super::*;
    pub fn serialize<S: serde::Serializer>(v: &Option<State>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(v) => s.serialize_str(v.as_str()),
            None => s.serialize_none(),
        }
    }
}

/// Mock-only outcome selected in the draft. No outcome claims `passed`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MockOutcome {
    #[default]
    Artifact,
    ProviderFailure,
    CheckFailed,
}
impl MockOutcome {
    pub const ALL: [Self; 3] = [Self::Artifact, Self::ProviderFailure, Self::CheckFailed];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Artifact => "artifact",
            Self::ProviderFailure => "provider_failure",
            Self::CheckFailed => "check_failed",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|o| o.as_str() == s)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MockOptions {
    #[serde(with = "outcome_str")]
    pub outcome: MockOutcome,
    /// Delay between supervisor steps, 0..=2000.
    pub step_ms: u32,
}

/// Draft fields for `task.create` / `task.update`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskInput {
    pub draft: Draft,
    pub mock: MockOptions,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReviewView {
    pub digest: String,
    #[serde(with = "verification_str")]
    pub verification: Verification,
    /// Recovery replaced missing or malformed review metadata so the task can be abandoned.
    /// Such a review was never sealed this way and can never be accepted.
    pub metadata_repaired: bool,
    pub artifact: String,
    pub simulated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptView {
    pub task_id: String,
    #[serde(with = "category_str")]
    pub category: Category,
    pub amount_hundredths: u16,
    #[serde(with = "verification_str")]
    pub verification: Verification,
    pub override_reason: Option<String>,
    pub request_id: String,
    pub accepted_at_ms: i64,
    /// Isolated simulation progression; never verified-work experience.
    #[serde(default)]
    pub demo_xp: u32,
    #[serde(default)]
    pub real_xp: u32,
    #[serde(default)]
    pub progression_track: Option<String>,
    /// The life that performed the task (`tasks.life_id`); `None` in pre-v0.2 receipts.
    #[serde(default)]
    pub life_id: Option<String>,
    /// That life's level just before and after this acceptance (0 = unknown, pre-v0.2).
    #[serde(default)]
    pub level_before: u8,
    #[serde(default)]
    pub level_after: u8,
    /// Stages this acceptance newly reached, ascending (journaled once per life and stage).
    #[serde(default)]
    pub evolved_to: Vec<u8>,
    /// `"life_ended"` when an eligible reward was withheld because the performing life had
    /// already ended; never transferred to another life.
    #[serde(default)]
    pub xp_withheld: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TaskSummary {
    pub task_id: String,
    pub revision: i64,
    #[serde(with = "state_str")]
    pub state: State,
    #[serde(with = "category_str")]
    pub category: Category,
    #[serde(with = "provider_str")]
    pub provider: Provider,
    pub folder: String,
    pub scope: Vec<String>,
    pub instructions: String,
    pub mock: MockOptions,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub review: Option<ReviewView>,
    pub receipt: Option<ReceiptView>,
}

/// Bounded list form of a task for `state.get`, `history.list` and `task.recover`: no
/// folder, scope or artifact text, and an instructions excerpt. `task.get` / `task.review`
/// return the full `TaskSummary`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TaskBrief {
    pub task_id: String,
    pub revision: i64,
    #[serde(with = "state_str")]
    pub state: State,
    #[serde(with = "category_str")]
    pub category: Category,
    #[serde(with = "provider_str")]
    pub provider: Provider,
    pub mock: MockOptions,
    /// Prefix of the instructions whose JSON encoding fits in 256 bytes.
    pub instructions_excerpt: String,
    pub instructions_bytes: usize,
    pub instructions_sha256: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub review: Option<ReviewBrief>,
    pub receipt: Option<ReceiptView>,
}

/// Review identity without the artifact text. The digest is enough to discard.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReviewBrief {
    pub digest: String,
    #[serde(with = "verification_str")]
    pub verification: Verification,
    /// See `ReviewView::metadata_repaired`.
    pub metadata_repaired: bool,
    pub simulated: bool,
    pub artifact_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetersView {
    pub hunger: u16,
    pub thirst: u16,
    pub mental_health: u16,
    pub hygiene: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct DecayView {
    pub enabled: bool,
    pub paused: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SettingsView {
    pub revision: i64,
    pub decay_enabled: bool,
    pub decay_per_hour: [u16; 4],
    pub timeout_seconds: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ProviderInfo {
    #[serde(with = "provider_str")]
    pub id: Provider,
    pub label: &'static str,
    pub available: bool,
    pub note: &'static str,
}
impl ProviderInfo {
    pub fn all() -> Vec<Self> {
        Provider::ALL
            .into_iter()
            .map(|p| Self {
                id: p,
                label: p.label(),
                available: p.supported(),
                note: if p.supported() {
                    "Simulation only; runs no command"
                } else {
                    "Not yet supported on this version"
                },
            })
            .collect()
    }
}

/// The exact `state.get` result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Snapshot {
    pub schema_version: i64,
    /// `meta.state_version`: increases with every committed write (and every settling
    /// `state.get`); `bar-state.json` carries the same counter.
    pub state_version: i64,
    pub protocol_version: u16,
    pub last_seq: i64,
    pub meters: MetersView,
    pub care: CareView,
    pub progression: ProgressionView,
    pub decay: DecayView,
    pub settings: SettingsView,
    pub active_task: Option<TaskBrief>,
    pub recent: Vec<TaskBrief>,
    pub providers: Vec<ProviderInfo>,
    /// The living companion, or `null` when none is living.
    pub life: Option<LifeView>,
    pub lifecycle: LifecycleView,
    /// The living companion's evolutions no UI has acknowledged yet (`pet.events_seen`).
    pub unseen_events: Vec<UnseenEvent>,
    pub memorial: MemorialBrief,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PreflightSummary {
    #[serde(with = "category_str")]
    pub category: Category,
    #[serde(with = "provider_str")]
    pub provider: Provider,
    pub folder: String,
    pub scope: Vec<String>,
    pub instructions_sha256: String,
    pub network: &'static str,
    pub writable: &'static str,
    pub timeout_seconds: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PreflightResult {
    pub task_id: String,
    pub revision: i64,
    /// Returned once; only its SHA-256 is stored.
    pub preflight_token: String,
    pub expires_at_ms: i64,
    pub summary: PreflightSummary,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StartResult {
    pub task_id: String,
    #[serde(with = "state_str")]
    pub state: State,
    pub seq: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptOutcome {
    pub receipt: ReceiptView,
    pub duplicate: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RecoveryChange {
    pub task_id: String,
    #[serde(with = "state_str")]
    pub from: State,
    #[serde(with = "state_str")]
    pub to: State,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RecoverResult {
    pub changed: Vec<RecoveryChange>,
    /// Tasks whose missing or malformed review metadata was replaced with abandonment
    /// metadata, including tasks whose state did not change. They can now be abandoned.
    pub repaired: Vec<String>,
    pub active_task: Option<TaskBrief>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EventData {
    #[serde(with = "state_str")]
    pub state: State,
    /// Previous state; `null` for task creation. Equal to `state` for a draft update or
    /// for a recovery repair of review metadata that left the state unchanged.
    #[serde(serialize_with = "opt_state_str::serialize")]
    pub from: Option<State>,
    pub revision: i64,
    pub at_ms: i64,
}

/// A persisted `task.state` event, serialized as the protocol event envelope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Event {
    pub v: u16,
    pub event: &'static str,
    pub seq: i64,
    pub task_id: String,
    pub data: EventData,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EventsPage {
    pub events: Vec<Event>,
    pub last_seq: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HistoryPage {
    pub tasks: Vec<TaskBrief>,
    /// Cursor for the next (older) page, or `null` when no older task exists.
    pub next: Option<HistoryCursor>,
}

/// `history.list` position: tasks before `before_ms`, or created in that millisecond but
/// stored before `before_task_id`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HistoryCursor {
    pub before_ms: i64,
    pub before_task_id: String,
}

/// `pet.pause` result: Pause/Travel of the living companion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PauseView {
    pub paused: bool,
    #[serde(default)]
    pub life_id: Option<String>,
}

/// Authoritative care/readiness. Values are settled only by explicit short commands.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CareView {
    pub revision: i64,
    pub bathroom_due: bool,
    pub starts_since_bathroom: u32,
    pub sleeping: bool,
    pub sleep_due: bool,
    pub sleep_remaining_ms: i64,
    pub sleep_started_at_ms: Option<i64>,
    pub work_sessions: u32,
    pub execution_ms: i64,
    pub ready_to_work: bool,
    pub readiness_reasons: Vec<String>,
    pub welcome_care_available: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CareOutcome {
    pub request_id: String,
    pub action: String,
    pub changed: bool,
    pub meters: MetersView,
    pub care: CareView,
    /// The lifecycle after this action; `None` in receipts stored before v0.2.
    #[serde(default)]
    pub lifecycle: Option<LifecycleView>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct MasteryView {
    pub bugfix: u32,
    pub quality_performance: u32,
    pub learning_specification: u32,
    pub cleanup: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct XpView {
    pub lifetime_xp: u32,
    pub level: u8,
    pub next_level_xp: Option<u32>,
    pub mastery: MasteryView,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProgressionView {
    pub real: XpView,
    pub demo: XpView,
}

/// The next stage a life can reach (`evolution::NextStage`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NextStageView {
    pub stage: u8,
    pub min_level: u8,
    pub xp: u32,
}

/// One companion life. XP, level and stage are per life: `xp` is exactly the sum of the
/// life's `demo_xp_rewards` rows and `tasks_accepted` is their count.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifeView {
    pub life_id: String,
    pub ordinal: u32,
    pub name: String,
    pub species: String,
    pub appearance: String,
    pub origin: String,
    pub status: String,
    pub mode: String,
    pub armed: bool,
    pub pace: String,
    pub paused: bool,
    pub fainted: bool,
    pub death_pending: bool,
    pub revision: i64,
    pub adopted_at_ms: i64,
    pub xp: u32,
    pub level: u8,
    pub next_level_xp: Option<u32>,
    pub stage: u8,
    pub stage_key: String,
    pub next_stage: Option<NextStageView>,
    pub tasks_accepted: u32,
}

/// Attended-time view of one need. Durations are active (attended) time, never wall time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeedView {
    pub need: String,
    pub action: String,
    pub lethal: bool,
    pub meter: u16,
    pub hold_ms: i64,
    pub neglect_ms: i64,
    /// `ok`, `low`, `empty` or `critical`.
    pub state: String,
    pub until_warning_ms: Option<i64>,
    pub until_critical_ms: Option<i64>,
    /// `None` for gentle or unarmed lives and for non-lethal needs.
    pub until_end_ms: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CriticalView {
    pub need: String,
    pub action: String,
    /// Frozen at critical entry; only a leniency change can raise it.
    pub grace_ms: i64,
    pub elapsed_ms: i64,
    pub remaining_ms: i64,
    pub since_ms: i64,
}

/// The living companion's effective rules plus the fixed presence contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RulesView {
    pub decay_per_hour: [u32; 4],
    pub full_hold_ms: i64,
    pub warning_at: u16,
    pub lethal: Vec<String>,
    pub critical_after_ms: i64,
    pub grace_ms: i64,
    pub tick_ms: i64,
    pub idle_timeout_s: i64,
    pub settle_cap_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleView {
    pub ruleset: String,
    /// `none`, `no_life`, `pending`, `unarmed`, `paused`, `task` or `sleeping`.
    pub frozen_reason: String,
    pub attended_total_ms: i64,
    pub needs: Vec<NeedView>,
    pub critical: Option<CriticalView>,
    pub rules: RulesView,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnseenEvent {
    pub seq: i64,
    pub kind: String,
    pub stage: u8,
    pub at_ms: i64,
}

/// The memorial record of an ended life (`lives.final_summary`), written once.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifeSummary {
    pub life_id: String,
    pub name: String,
    pub species: String,
    pub appearance: String,
    pub ordinal: u32,
    pub mode: String,
    pub pace: String,
    pub final_level: u8,
    pub final_stage: u8,
    /// `<appearance>.<stage key>`, e.g. `cat.grown`; UIs map it to the stage name.
    pub stage_name_key: String,
    pub life_xp: u32,
    pub tasks_accepted: u32,
    pub adopted_at_ms: i64,
    pub ended_at_ms: i64,
    /// `neglect:hunger`, `neglect:thirst` or `retired`.
    pub end_cause: String,
    pub final_meters: MetersView,
    pub attended_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorialBrief {
    pub count: u32,
    pub latest: Option<LifeSummary>,
}

/// `pet.memorial`: ended lives, newest ordinal first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorialPage {
    pub entries: Vec<LifeSummary>,
    pub next_before_ordinal: Option<u32>,
}

/// Result of every mutating lifecycle method (`pet.select`, `pet.adopt`, `pet.rules`,
/// `pet.configure_life`, `pet.retire`, `pet.events_seen`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifeOutcome {
    pub request_id: String,
    pub changed: bool,
    pub life: Option<LifeView>,
    pub lifecycle: LifecycleView,
    pub memorial: MemorialBrief,
}
