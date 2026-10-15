//! Typed request params. Enum-valued fields stay strings; the CLI validates them with core.
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

/// Params for methods that take none: `state.get`, `providers.inspect`, `task.recover`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmptyParams {}
impl Validate for EmptyParams {}
pub type StateGetParams = EmptyParams;
pub type ProvidersInspectParams = EmptyParams;
pub type TaskRecoverParams = EmptyParams;

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
