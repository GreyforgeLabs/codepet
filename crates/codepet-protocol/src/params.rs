//! Typed request params. Enum-valued fields stay strings; the CLI validates them with core.
//! The lifecycle methods (`pet.select`, `pet.adopt`, `pet.rules`, `pet.configure_life`,
//! `pet.retire`, `pet.events_seen`, `pet.memorial`) also check their closed string sets and
//! ranges here, so a malformed request is refused before the state directory is opened.
use crate::{ErrorCode, ProtocolError};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_EVENTS_LIMIT: u32 = 100;
pub const MAX_HISTORY_LIMIT: u32 = 50;
pub const MAX_STEP_MS: u32 = 2000;
/// Upper bound for task ids, review digests and preflight tokens.
pub const MAX_TOKEN_LEN: usize = 128;
/// Must equal `codepet_core::MAX_OVERRIDE_REASON_BYTES` (checked by a CLI test).
pub const MAX_OVERRIDE_REASON_BYTES: usize = 512;
pub const MAX_CLIENT_LEN: usize = 128;
pub const MAX_HELLO_VERSIONS: usize = 16;
/// Largest `pet.memorial` page, and the default page size.
pub const MAX_MEMORIAL_LIMIT: u32 = 20;
/// Raw byte bound for a pet name or a typed retirement confirmation. Core's
/// `normalize_pet_name` applies the real rule (1..=24 characters, at most 96 bytes after
/// trimming and stripping control and bidi characters); this only bounds the input.
pub const MAX_PET_NAME_INPUT_BYTES: usize = 256;
/// Must equal `codepet_core::species::SPECIES_IDS`, in order (checked by a CLI test).
pub const PET_IDS: [&str; 15] = [
    "cat",
    "dog",
    "capybara",
    "penguin",
    "robot",
    "iguana",
    "spider",
    "snake",
    "goldfish",
    "rabbit",
    "turtle",
    "frog",
    "owl",
    "red_panda",
    "axolotl",
];
/// Life modes, as in `codepet_core::lifecycle::Mode`.
pub const LIFE_MODES: [&str; 2] = ["permadeath", "gentle"];
/// Adoption paces, as in `codepet_core::lifecycle::Pace`.
pub const LIFE_PACES: [&str; 2] = ["steady", "playtest"];
/// `pet.configure_life` hour bounds: core's `Rules::validate` caps `grace_ms` at
/// 172,800,000 (48 h) and `critical_after_ms` at 86,400,000 (24 h).
pub const MAX_GRACE_HOURS: u16 = 48;
pub const MAX_CRITICAL_AFTER_HOURS: u16 = 24;
/// `pet.configure_life` may slow decay to this percent of the current rates, never faster.
pub const MIN_DECAY_PERCENT: u8 = 25;
pub const MAX_DECAY_PERCENT: u8 = 100;
/// Serde error text quoted in `INVALID_PARAMS` messages is cut to this many bytes.
const MAX_SERDE_DETAIL_BYTES: usize = 160;

/// Range and length checks that serde types alone cannot express.
pub trait Validate {
    fn validate(&self) -> Result<(), ProtocolError> {
        Ok(())
    }
}

/// Deserializes params from a JSON object. Errors are `INVALID_PARAMS` with bounded text.
pub fn params_from_value<T: DeserializeOwned>(value: Value) -> Result<T, ProtocolError> {
    if !value.is_object() {
        return Err(invalid_params("params must be a JSON object"));
    }
    serde_json::from_value(value).map_err(|e| {
        let mut detail = e.to_string();
        if detail.len() > MAX_SERDE_DETAIL_BYTES {
            let mut end = MAX_SERDE_DETAIL_BYTES;
            while !detail.is_char_boundary(end) {
                end -= 1;
            }
            detail.truncate(end);
            detail.push('…');
        }
        invalid_params(&format!("invalid params: {detail}"))
    })
}

/// `params_from_value` followed by `Validate::validate`.
pub fn parse_params<T: DeserializeOwned + Validate>(value: Value) -> Result<T, ProtocolError> {
    let params: T = params_from_value(value)?;
    params.validate()?;
    Ok(params)
}

fn invalid_params(message: &str) -> ProtocolError {
    ProtocolError::new(ErrorCode::InvalidParams, message)
}

fn check_token(field: &str, value: &str) -> Result<(), ProtocolError> {
    if value.is_empty() || value.len() > MAX_TOKEN_LEN {
        return Err(invalid_params(&format!(
            "{field} must be 1..={MAX_TOKEN_LEN} bytes"
        )));
    }
    Ok(())
}

fn check_limit(limit: Option<u32>, max: u32) -> Result<(), ProtocolError> {
    match limit {
        Some(l) if !(1..=max).contains(&l) => {
            Err(invalid_params(&format!("limit must be 1..={max}")))
        }
        _ => Ok(()),
    }
}

/// Params for methods with no fields, including `pet.sleep` and `pet.wake`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmptyParams {}
impl Validate for EmptyParams {}
pub type StateGetParams = EmptyParams;
pub type ProvidersInspectParams = EmptyParams;
pub type TaskRecoverParams = EmptyParams;
pub type PetSleepParams = EmptyParams;
pub type PetWakeParams = EmptyParams;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelloParams {
    #[serde(default)]
    pub client: Option<String>,
    #[serde(default)]
    pub versions: Option<Vec<u16>>,
}
impl Validate for HelloParams {
    fn validate(&self) -> Result<(), ProtocolError> {
        if self
            .client
            .as_ref()
            .is_some_and(|c| c.len() > MAX_CLIENT_LEN)
        {
            return Err(invalid_params(&format!(
                "client must be at most {MAX_CLIENT_LEN} bytes"
            )));
        }
        if self
            .versions
            .as_ref()
            .is_some_and(|v| v.len() > MAX_HELLO_VERSIONS)
        {
            return Err(invalid_params(&format!(
                "versions must list at most {MAX_HELLO_VERSIONS} entries"
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MockParams {
    #[serde(default)]
    pub outcome: Option<String>,
    #[serde(default)]
    pub step_ms: Option<u32>,
}
impl Validate for MockParams {
    fn validate(&self) -> Result<(), ProtocolError> {
        if self.step_ms.is_some_and(|s| s > MAX_STEP_MS) {
            return Err(invalid_params(&format!(
                "mock.step_ms must be 0..={MAX_STEP_MS}"
            )));
        }
        if self.outcome.as_ref().is_some_and(|o| o.len() > 64) {
            return Err(invalid_params("mock.outcome is too long"));
        }
        Ok(())
    }
}

/// Draft fields shared by `task.create` and `task.update`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftParams {
    pub category: String,
    pub provider: String,
    pub folder: String,
    pub scope: Vec<String>,
    pub instructions: String,
    #[serde(default)]
    pub mock: Option<MockParams>,
}
impl Validate for DraftParams {
    fn validate(&self) -> Result<(), ProtocolError> {
        self.mock.as_ref().map_or(Ok(()), Validate::validate)
    }
}
pub type TaskCreateParams = DraftParams;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskUpdateParams {
    pub task_id: String,
    pub expected_revision: u64,
    pub category: String,
    pub provider: String,
    pub folder: String,
    pub scope: Vec<String>,
    pub instructions: String,
    #[serde(default)]
    pub mock: Option<MockParams>,
}
impl TaskUpdateParams {
    pub fn draft(&self) -> DraftParams {
        DraftParams {
            category: self.category.clone(),
            provider: self.provider.clone(),
            folder: self.folder.clone(),
            scope: self.scope.clone(),
            instructions: self.instructions.clone(),
            mock: self.mock.clone(),
        }
    }
}
impl Validate for TaskUpdateParams {
    fn validate(&self) -> Result<(), ProtocolError> {
        check_token("task_id", &self.task_id)?;
        self.mock.as_ref().map_or(Ok(()), Validate::validate)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskPreflightParams {
    pub task_id: String,
    pub revision: u64,
}
impl Validate for TaskPreflightParams {
    fn validate(&self) -> Result<(), ProtocolError> {
        check_token("task_id", &self.task_id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskStartParams {
    pub task_id: String,
    pub revision: u64,
    pub preflight_token: String,
}
impl Validate for TaskStartParams {
    fn validate(&self) -> Result<(), ProtocolError> {
        check_token("task_id", &self.task_id)?;
        check_token("preflight_token", &self.preflight_token)
    }
}

/// Params naming one task: `task.get`, `task.cancel`, `task.review`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskIdParams {
    pub task_id: String,
}
impl Validate for TaskIdParams {
    fn validate(&self) -> Result<(), ProtocolError> {
        check_token("task_id", &self.task_id)
    }
}
pub type TaskGetParams = TaskIdParams;
pub type TaskCancelParams = TaskIdParams;
pub type TaskReviewParams = TaskIdParams;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskAcceptArtifactParams {
    pub task_id: String,
    pub review_digest: String,
    pub acknowledged: bool,
    #[serde(default)]
    pub override_reason: Option<String>,
}
impl Validate for TaskAcceptArtifactParams {
    fn validate(&self) -> Result<(), ProtocolError> {
        check_token("task_id", &self.task_id)?;
        check_token("review_digest", &self.review_digest)?;
        if self
            .override_reason
            .as_ref()
            .is_some_and(|r| r.trim().len() > MAX_OVERRIDE_REASON_BYTES || r.contains('\0'))
        {
            return Err(invalid_params(&format!(
                "override_reason must be at most {MAX_OVERRIDE_REASON_BYTES} bytes after trimming, without NUL"
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskDiscardParams {
    pub task_id: String,
    pub review_digest: String,
}
impl Validate for TaskDiscardParams {
    fn validate(&self) -> Result<(), ProtocolError> {
        check_token("task_id", &self.task_id)?;
        check_token("review_digest", &self.review_digest)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventsSubscribeParams {
    pub after_seq: u64,
    #[serde(default)]
    pub limit: Option<u32>,
}
impl EventsSubscribeParams {
    pub fn limit_or_default(&self) -> u32 {
        self.limit.unwrap_or(MAX_EVENTS_LIMIT)
    }
}
impl Validate for EventsSubscribeParams {
    fn validate(&self) -> Result<(), ProtocolError> {
        check_limit(self.limit, MAX_EVENTS_LIMIT)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryListParams {
    #[serde(default)]
    pub before_ms: Option<i64>,
    /// Tie-breaker for tasks created in the same millisecond: the `next` cursor of a page.
    #[serde(default)]
    pub before_task_id: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
}
impl HistoryListParams {
    pub fn limit_or_default(&self) -> u32 {
        self.limit.unwrap_or(MAX_HISTORY_LIMIT)
    }
}
impl Validate for HistoryListParams {
    fn validate(&self) -> Result<(), ProtocolError> {
        if let Some(id) = &self.before_task_id {
            check_token("before_task_id", id)?;
            if self.before_ms.is_none() {
                return Err(invalid_params("before_task_id requires before_ms"));
            }
        }
        check_limit(self.limit, MAX_HISTORY_LIMIT)
    }
}

/// Settings ranges are enforced by core's `Settings::validate`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsUpdateParams {
    pub expected_revision: u64,
    pub decay_enabled: bool,
    pub decay_per_hour: [u16; 4],
    pub timeout_seconds: u32,
}
impl Validate for SettingsUpdateParams {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PetPauseParams {
    pub paused: bool,
}
impl Validate for PetPauseParams {}

/// Explicit care actions. Validation runs before the state directory is opened.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PetCareParams {
    pub action: String,
}
impl Validate for PetCareParams {
    fn validate(&self) -> Result<(), ProtocolError> {
        if matches!(
            self.action.as_str(),
            "food" | "water" | "play" | "wash" | "bathroom" | "routine"
        ) {
            Ok(())
        } else {
            Err(invalid_params(
                "action must be food, water, play, wash, bathroom or routine",
            ))
        }
    }
}

/// True when `id` is one of the 15 companion ids.
pub fn is_pet_id(id: &str) -> bool {
    PET_IDS.contains(&id)
}

fn check_pet_id(field: &str, value: &str) -> Result<(), ProtocolError> {
    if is_pet_id(value) {
        Ok(())
    } else {
        Err(invalid_params(&format!(
            "{field} must be one of the 15 companion ids"
        )))
    }
}

fn check_mode(value: &str) -> Result<(), ProtocolError> {
    if LIFE_MODES.contains(&value) {
        Ok(())
    } else {
        Err(invalid_params("mode must be permadeath or gentle"))
    }
}

/// Bounds raw name input; content rules (trim, strip, 1..=24 characters) are core's.
fn check_name_input(field: &str, value: &str) -> Result<(), ProtocolError> {
    if value.trim().is_empty() || value.len() > MAX_PET_NAME_INPUT_BYTES || value.contains('\0') {
        return Err(invalid_params(&format!(
            "{field} must be 1..={MAX_PET_NAME_INPUT_BYTES} bytes of text, not blank, without NUL"
        )));
    }
    Ok(())
}

/// `pet.select`: cosmetic appearance switch within the life `life_id`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PetSelectParams {
    pub life_id: String,
    pub pet_id: String,
}
impl Validate for PetSelectParams {
    fn validate(&self) -> Result<(), ProtocolError> {
        check_token("life_id", &self.life_id)?;
        check_pet_id("pet_id", &self.pet_id)
    }
}

/// `pet.adopt`. `acknowledge_rules: false` is well-formed; storage answers it with
/// `ACKNOWLEDGEMENT_REQUIRED`. A missing `name` means the species' default name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PetAdoptParams {
    pub species: String,
    #[serde(default)]
    pub name: Option<String>,
    pub mode: String,
    #[serde(default)]
    pub pace: Option<String>,
    pub acknowledge_rules: bool,
}
impl PetAdoptParams {
    /// The pace, defaulting to `steady`.
    pub fn pace_or_default(&self) -> &str {
        self.pace.as_deref().unwrap_or("steady")
    }
}
impl Validate for PetAdoptParams {
    fn validate(&self) -> Result<(), ProtocolError> {
        check_pet_id("species", &self.species)?;
        if let Some(name) = &self.name {
            check_name_input("name", name)?;
        }
        check_mode(&self.mode)?;
        match self.pace.as_deref() {
            Some(p) if !LIFE_PACES.contains(&p) => {
                Err(invalid_params("pace must be steady or playtest"))
            }
            _ => Ok(()),
        }
    }
}

/// `pet.rules`: arm an unarmed life and/or change its mode.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PetRulesParams {
    pub life_id: String,
    pub mode: String,
    pub acknowledge_rules: bool,
}
impl Validate for PetRulesParams {
    fn validate(&self) -> Result<(), ProtocolError> {
        check_token("life_id", &self.life_id)?;
        check_mode(&self.mode)
    }
}

/// `pet.configure_life`: leniency-only per-life settings. At least one setting is required;
/// whether the change is more lenient is decided by storage against the life's rules.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PetConfigureLifeParams {
    pub life_id: String,
    pub expected_revision: u64,
    #[serde(default)]
    pub grace_hours: Option<u16>,
    #[serde(default)]
    pub critical_after_hours: Option<u16>,
    #[serde(default)]
    pub decay_percent: Option<u8>,
}
impl Validate for PetConfigureLifeParams {
    fn validate(&self) -> Result<(), ProtocolError> {
        check_token("life_id", &self.life_id)?;
        if self.grace_hours.is_none()
            && self.critical_after_hours.is_none()
            && self.decay_percent.is_none()
        {
            return Err(invalid_params(
                "set at least one of grace_hours, critical_after_hours or decay_percent",
            ));
        }
        if self
            .grace_hours
            .is_some_and(|h| !(1..=MAX_GRACE_HOURS).contains(&h))
        {
            return Err(invalid_params(&format!(
                "grace_hours must be 1..={MAX_GRACE_HOURS}"
            )));
        }
        if self
            .critical_after_hours
            .is_some_and(|h| !(1..=MAX_CRITICAL_AFTER_HOURS).contains(&h))
        {
            return Err(invalid_params(&format!(
                "critical_after_hours must be 1..={MAX_CRITICAL_AFTER_HOURS}"
            )));
        }
        if self
            .decay_percent
            .is_some_and(|p| !(MIN_DECAY_PERCENT..=MAX_DECAY_PERCENT).contains(&p))
        {
            return Err(invalid_params(&format!(
                "decay_percent must be {MIN_DECAY_PERCENT}..={MAX_DECAY_PERCENT}"
            )));
        }
        Ok(())
    }
}

/// `pet.retire`: `confirm_name` must match the life's name (checked by storage).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PetRetireParams {
    pub life_id: String,
    pub confirm_name: String,
}
impl Validate for PetRetireParams {
    fn validate(&self) -> Result<(), ProtocolError> {
        check_token("life_id", &self.life_id)?;
        check_name_input("confirm_name", &self.confirm_name)
    }
}

/// `pet.events_seen`: marks evolved events up to and including `through_seq` as seen.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PetEventsSeenParams {
    pub life_id: String,
    pub through_seq: i64,
}
impl Validate for PetEventsSeenParams {
    fn validate(&self) -> Result<(), ProtocolError> {
        check_token("life_id", &self.life_id)?;
        if self.through_seq < 0 {
            return Err(invalid_params("through_seq must not be negative"));
        }
        Ok(())
    }
}

/// `pet.memorial`: one read-only page of ended lives, newest ordinal first.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PetMemorialParams {
    #[serde(default)]
    pub before_ordinal: Option<u32>,
    #[serde(default)]
    pub limit: Option<u32>,
}
impl PetMemorialParams {
    pub fn limit_or_default(&self) -> u32 {
        self.limit.unwrap_or(MAX_MEMORIAL_LIMIT)
    }
}
impl Validate for PetMemorialParams {
    fn validate(&self) -> Result<(), ProtocolError> {
        check_limit(self.limit, MAX_MEMORIAL_LIMIT)
    }
}

/// `lifecycle.rules` takes no params.
pub type LifecycleRulesParams = EmptyParams;
