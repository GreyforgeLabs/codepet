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
    pub simulated: bool,
    pub artifact_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
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
    pub protocol_version: u16,
    pub last_seq: i64,
    pub meters: MetersView,
    pub decay: DecayView,
    pub settings: SettingsView,
    pub active_task: Option<TaskBrief>,
    pub recent: Vec<TaskBrief>,
    pub providers: Vec<ProviderInfo>,
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
    pub active_task: Option<TaskBrief>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EventData {
    #[serde(with = "state_str")]
    pub state: State,
    /// Previous state; `null` for task creation. Equal to `state` for a draft update.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct PauseView {
    pub paused: bool,
}
