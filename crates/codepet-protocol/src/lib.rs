//! Versioned, bounded CodePet protocol: newline-delimited JSON envelopes shared by short
//! commands and a later job socket. Parsing and framing only; no transport, no IO policy.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fmt;

mod frame;
mod params;
#[cfg(feature = "legacy-tasks")]
mod params_legacy;
mod strict;

pub use frame::{read_frame, FrameError, FrameReader};
pub use params::*;
#[cfg(feature = "legacy-tasks")]
pub use params_legacy::*;

pub const PROTOCOL_VERSION: u16 = 1;
/// Maximum frame size in bytes, counting the trailing newline.
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
/// Maximum frame size in bytes without the trailing newline.
pub const MAX_FRAME_PAYLOAD_BYTES: usize = MAX_FRAME_BYTES - 1;
pub const MAX_ID_LEN: usize = 128;
/// Error messages are truncated to this many bytes so error frames always fit.
pub const MAX_MESSAGE_BYTES: usize = 1024;
pub const NOT_IMPLEMENTED_MESSAGE: &str = "not implemented in mock milestone";
/// Message of the v0.3 skeleton's `NOT_IMPLEMENTED` refusals (removed when each method's
/// work package lands).
pub const V03_PENDING_MESSAGE: &str = "not implemented yet in this v0.3 build";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    Busy,
    InvalidState,
    StaleRevision,
    UnsupportedProvider,
    AuthUnavailable,
    SandboxUnavailable,
    ScopeChanged,
    BaselineConflict,
    DiskLimit,
    OutputLimit,
    CancelIncomplete,
    ApplyRecoveryRequired,
    ProtocolMismatch,
    InvalidRequest,
    UnknownMethod,
    InvalidParams,
    NotFound,
    TokenInvalid,
    TokenExpired,
    ReviewChanged,
    AcknowledgementRequired,
    OverrideRequired,
    IdempotencyConflict,
    ResyncRequired,
    Capacity,
    StorageUnavailable,
    SchemaUnsupported,
    Internal,
    /// No living companion: care, sleep, wake, pause and life methods need one.
    NoActivePet,
    // ---- v0.3 (appended; earlier positions are stable) ----
    /// The method is declared by the protocol but this build does not implement it yet.
    NotImplemented,
    /// `pet.start` on a life that is already armed; the UI just opens Home.
    AlreadyArmed,
    /// `pet.start` / `pet.rename`: the name fails the pet-name rule.
    NameInvalid,
    /// `treats.give` with an empty jar.
    NoTreats,
    /// `treats.give` while the pet is asleep, paused or fainted.
    PetNeedsCare,
    /// `clients.connect` / `clients.disconnect` / `clients.test` while the bar runs with a test
    /// state home.
    TestState,
}
impl ErrorCode {
    pub const ALL: [Self; 35] = [
        Self::Busy,
        Self::InvalidState,
        Self::StaleRevision,
        Self::UnsupportedProvider,
        Self::AuthUnavailable,
        Self::SandboxUnavailable,
        Self::ScopeChanged,
        Self::BaselineConflict,
        Self::DiskLimit,
        Self::OutputLimit,
        Self::CancelIncomplete,
        Self::ApplyRecoveryRequired,
        Self::ProtocolMismatch,
        Self::InvalidRequest,
        Self::UnknownMethod,
        Self::InvalidParams,
        Self::NotFound,
        Self::TokenInvalid,
        Self::TokenExpired,
        Self::ReviewChanged,
        Self::AcknowledgementRequired,
        Self::OverrideRequired,
        Self::IdempotencyConflict,
        Self::ResyncRequired,
        Self::Capacity,
        Self::StorageUnavailable,
        Self::SchemaUnsupported,
        Self::Internal,
        Self::NoActivePet,
        Self::NotImplemented,
        Self::AlreadyArmed,
        Self::NameInvalid,
        Self::NoTreats,
        Self::PetNeedsCare,
        Self::TestState,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Busy => "BUSY",
            Self::InvalidState => "INVALID_STATE",
            Self::StaleRevision => "STALE_REVISION",
            Self::UnsupportedProvider => "UNSUPPORTED_PROVIDER",
            Self::AuthUnavailable => "AUTH_UNAVAILABLE",
            Self::SandboxUnavailable => "SANDBOX_UNAVAILABLE",
            Self::ScopeChanged => "SCOPE_CHANGED",
            Self::BaselineConflict => "BASELINE_CONFLICT",
            Self::DiskLimit => "DISK_LIMIT",
            Self::OutputLimit => "OUTPUT_LIMIT",
            Self::CancelIncomplete => "CANCEL_INCOMPLETE",
            Self::ApplyRecoveryRequired => "APPLY_RECOVERY_REQUIRED",
            Self::ProtocolMismatch => "PROTOCOL_MISMATCH",
            Self::InvalidRequest => "INVALID_REQUEST",
            Self::UnknownMethod => "UNKNOWN_METHOD",
            Self::InvalidParams => "INVALID_PARAMS",
            Self::NotFound => "NOT_FOUND",
            Self::TokenInvalid => "TOKEN_INVALID",
            Self::TokenExpired => "TOKEN_EXPIRED",
            Self::ReviewChanged => "REVIEW_CHANGED",
            Self::AcknowledgementRequired => "ACKNOWLEDGEMENT_REQUIRED",
            Self::OverrideRequired => "OVERRIDE_REQUIRED",
            Self::IdempotencyConflict => "IDEMPOTENCY_CONFLICT",
            Self::ResyncRequired => "RESYNC_REQUIRED",
            Self::Capacity => "CAPACITY",
            Self::StorageUnavailable => "STORAGE_UNAVAILABLE",
            Self::SchemaUnsupported => "SCHEMA_UNSUPPORTED",
            Self::Internal => "INTERNAL",
            Self::NoActivePet => "NO_ACTIVE_PET",
            Self::NotImplemented => "NOT_IMPLEMENTED",
            Self::AlreadyArmed => "ALREADY_ARMED",
            Self::NameInvalid => "NAME_INVALID",
            Self::NoTreats => "NO_TREATS",
            Self::PetNeedsCare => "PET_NEEDS_CARE",
            Self::TestState => "TEST_STATE",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == s)
    }
    /// Default hint: the identical request may succeed later without user action.
    pub fn retryable(self) -> bool {
        matches!(self, Self::Busy | Self::Capacity | Self::StorageUnavailable)
    }
}
impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Method {
    Hello,
    StateGet,
    EventsSubscribe,
    ProvidersInspect,
    TaskCreate,
    TaskGet,
    TaskUpdate,
    TaskPreflight,
    TaskStart,
    TaskCancel,
    TaskReview,
    TaskLogs,
    TaskApply,
    TaskAcceptArtifact,
    TaskDiscard,
    TaskRollback,
    TaskRecover,
    HistoryList,
    SettingsUpdate,
    PetPause,
    PetCare,
    PetSleep,
    PetWake,
    PetSelect,
    PetAdopt,
    PetRules,
    PetConfigureLife,
    PetRetire,
    PetEventsSeen,
    PetMemorial,
    LifecycleRules,
    // ---- v0.3 (appended; spec §6.2) ----
    PetStart,
    PetRename,
    TreatsGive,
    AppSetEnabled,
    AppSetHelpers,
    AgentForgetHistory,
    ClientsStatus,
    ClientsConnect,
    ClientsDisconnect,
    ClientsTest,
    PetMemories,
}
impl Method {
    pub const ALL: [Self; 42] = [
        Self::Hello,
        Self::StateGet,
        Self::EventsSubscribe,
        Self::ProvidersInspect,
        Self::TaskCreate,
        Self::TaskGet,
        Self::TaskUpdate,
        Self::TaskPreflight,
        Self::TaskStart,
        Self::TaskCancel,
        Self::TaskReview,
        Self::TaskLogs,
        Self::TaskApply,
        Self::TaskAcceptArtifact,
        Self::TaskDiscard,
        Self::TaskRollback,
        Self::TaskRecover,
        Self::HistoryList,
        Self::SettingsUpdate,
        Self::PetPause,
        Self::PetCare,
        Self::PetSleep,
        Self::PetWake,
        Self::PetSelect,
        Self::PetAdopt,
        Self::PetRules,
        Self::PetConfigureLife,
        Self::PetRetire,
        Self::PetEventsSeen,
        Self::PetMemorial,
        Self::LifecycleRules,
        Self::PetStart,
        Self::PetRename,
        Self::TreatsGive,
        Self::AppSetEnabled,
        Self::AppSetHelpers,
        Self::AgentForgetHistory,
        Self::ClientsStatus,
        Self::ClientsConnect,
        Self::ClientsDisconnect,
        Self::ClientsTest,
        Self::PetMemories,
    ];
    /// The v0.3 methods (spec §6.2 "Added"), in `ALL` order.
    pub const V03_ADDED: [Self; 11] = [
        Self::PetStart,
        Self::PetRename,
        Self::TreatsGive,
        Self::AppSetEnabled,
        Self::AppSetHelpers,
        Self::AgentForgetHistory,
        Self::ClientsStatus,
        Self::ClientsConnect,
        Self::ClientsDisconnect,
        Self::ClientsTest,
        Self::PetMemories,
    ];
    /// The v0.2 methods v0.3 unroutes (spec §6.2 "Removed"; `hello` stays, only its
    /// supervisor handshake is retired). The default build parses none of them (they answer
    /// `UNKNOWN_METHOD`); they route again only with the non-default `legacy-tasks` feature,
    /// for non-default compatibility.
    pub const V03_REMOVED: [Self; 17] = [
        Self::EventsSubscribe,
        Self::ProvidersInspect,
        Self::TaskCreate,
        Self::TaskGet,
        Self::TaskUpdate,
        Self::TaskPreflight,
        Self::TaskStart,
        Self::TaskCancel,
        Self::TaskReview,
        Self::TaskLogs,
        Self::TaskApply,
        Self::TaskAcceptArtifact,
        Self::TaskDiscard,
        Self::TaskRollback,
        Self::TaskRecover,
        Self::HistoryList,
        Self::SettingsUpdate,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hello => "hello",
            Self::StateGet => "state.get",
            Self::EventsSubscribe => "events.subscribe",
            Self::ProvidersInspect => "providers.inspect",
            Self::TaskCreate => "task.create",
            Self::TaskGet => "task.get",
            Self::TaskUpdate => "task.update",
            Self::TaskPreflight => "task.preflight",
            Self::TaskStart => "task.start",
            Self::TaskCancel => "task.cancel",
            Self::TaskReview => "task.review",
            Self::TaskLogs => "task.logs",
            Self::TaskApply => "task.apply",
            Self::TaskAcceptArtifact => "task.accept_artifact",
            Self::TaskDiscard => "task.discard",
            Self::TaskRollback => "task.rollback",
            Self::TaskRecover => "task.recover",
            Self::HistoryList => "history.list",
            Self::SettingsUpdate => "settings.update",
            Self::PetPause => "pet.pause",
            Self::PetCare => "pet.care",
            Self::PetSleep => "pet.sleep",
            Self::PetWake => "pet.wake",
            Self::PetSelect => "pet.select",
            Self::PetAdopt => "pet.adopt",
            Self::PetRules => "pet.rules",
            Self::PetConfigureLife => "pet.configure_life",
            Self::PetRetire => "pet.retire",
            Self::PetEventsSeen => "pet.events_seen",
            Self::PetMemorial => "pet.memorial",
            Self::PetMemories => "pet.memories",
            Self::LifecycleRules => "lifecycle.rules",
            Self::PetStart => "pet.start",
            Self::PetRename => "pet.rename",
            Self::TreatsGive => "treats.give",
            Self::AppSetEnabled => "app.set_enabled",
            Self::AppSetHelpers => "app.set_helpers",
            Self::AgentForgetHistory => "agent.forget_history",
            Self::ClientsStatus => "clients.status",
            Self::ClientsConnect => "clients.connect",
            Self::ClientsDisconnect => "clients.disconnect",
            Self::ClientsTest => "clients.test",
        }
    }
    /// The v0.3 methods whose work package has not landed in this build: they validate their
    /// params, then answer `NOT_IMPLEMENTED` before opening storage. Empty since WP3 (the
    /// clients methods).
    pub const V03_PENDING: [Self; 0] = [];
    /// Parses a routed method name. A method of [`Self::V03_REMOVED`] parses only with the
    /// `legacy-tasks` feature; otherwise it is unknown, exactly like a name that never existed.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|m| m.as_str() == s)
            .filter(|m| m.is_routed())
    }
    /// False for the retired task-pipeline methods in the default build (spec §6.5).
    pub fn is_routed(self) -> bool {
        cfg!(feature = "legacy-tasks") || !Self::V03_REMOVED.contains(&self)
    }
    /// True when the method may change durable state (including preflight tokens).
    /// `state.get` counts: it may settle presence, a persisted nap, a due death or
    /// checkpoint live execution. Every method is classified explicitly, so a new
    /// variant cannot compile without a decision here.
    pub fn is_mutating(self) -> bool {
        match self {
            Self::Hello
            | Self::EventsSubscribe
            | Self::ProvidersInspect
            | Self::TaskGet
            | Self::TaskReview
            | Self::TaskLogs
            | Self::HistoryList
            | Self::PetMemorial
            | Self::PetMemories
            | Self::LifecycleRules => false,
            Self::StateGet
            | Self::TaskCreate
            | Self::TaskUpdate
            | Self::TaskPreflight
            | Self::TaskStart
            | Self::TaskCancel
            | Self::TaskApply
            | Self::TaskAcceptArtifact
            | Self::TaskDiscard
            | Self::TaskRollback
            | Self::TaskRecover
            | Self::SettingsUpdate
            | Self::PetPause
            | Self::PetCare
            | Self::PetSleep
            | Self::PetWake
            | Self::PetSelect
            | Self::PetAdopt
            | Self::PetRules
            | Self::PetConfigureLife
            | Self::PetRetire
            | Self::PetEventsSeen
            // `clients.status` writes `client_hooks` and re-exports; the clients methods
            // start a journaled job.
            | Self::PetStart
            | Self::PetRename
            | Self::TreatsGive
            | Self::AppSetEnabled
            | Self::AppSetHelpers
            | Self::AgentForgetHistory
            | Self::ClientsStatus
            | Self::ClientsConnect
            | Self::ClientsDisconnect
            | Self::ClientsTest => true,
        }
    }
    /// False for methods that parse but are refused: `task.logs`, `task.apply` and
    /// `task.rollback` with `INVALID_STATE` (`legacy-tasks` only), and the v0.3 methods of
    /// [`Self::V03_PENDING`] with `NOT_IMPLEMENTED` until their work package lands.
    pub fn is_implemented(self) -> bool {
        !matches!(self, Self::TaskLogs | Self::TaskApply | Self::TaskRollback)
            && !Self::V03_PENDING.contains(&self)
    }
}
impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl Serialize for Method {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}
impl<'de> Deserialize<'de> for Method {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).ok_or_else(|| serde::de::Error::custom("unknown method"))
    }
}

/// A typed protocol failure with the request id when it could be recovered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolError {
    pub code: ErrorCode,
    pub message: String,
    pub id: Option<String>,
}
impl ProtocolError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: bounded_message(message.into()),
            id: None,
        }
    }
    pub fn with_id(mut self, id: Option<String>) -> Self {
        self.id = id;
        self
    }
    pub fn to_response(&self) -> Response {
        Response::err(self.id.clone(), self.code, self.message.clone())
    }
}
impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for ProtocolError {}

/// Truncates at a char boundary to at most `MAX_MESSAGE_BYTES`, marking the cut.
pub fn bounded_message(mut message: String) -> String {
    if message.len() > MAX_MESSAGE_BYTES {
        let marker = "…";
        let mut end = MAX_MESSAGE_BYTES - marker.len();
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
        message.push_str(marker);
    }
    message
}

pub fn valid_id(id: &str) -> bool {
    (1..=MAX_ID_LEN).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub v: u16,
    pub id: String,
    pub method: Method,
    #[serde(default = "empty_object")]
    pub params: Value,
}
fn empty_object() -> Value {
    Value::Object(Map::new())
}
impl Request {
    pub fn new(id: impl Into<String>, method: Method, params: Value) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            id: id.into(),
            method,
            params,
        }
    }
    pub fn to_frame(&self) -> Result<String, ProtocolError> {
        encode_frame(self)
    }
}

/// Parses one request frame (with or without its trailing newline).
///
/// Duplicate object keys anywhere are rejected. A version other than 1 yields
/// `PROTOCOL_MISMATCH` before unknown-field checks so newer clients get a clear answer.
pub fn parse_request(frame: &str) -> Result<Request, ProtocolError> {
    let text = frame.strip_suffix('\n').unwrap_or(frame);
    let invalid = |m: &str| ProtocolError::new(ErrorCode::InvalidRequest, m);
    if text.len() > MAX_FRAME_PAYLOAD_BYTES {
        return Err(invalid("frame exceeds 65536 bytes"));
    }
    if text.contains('\0') || text.contains('\n') {
        return Err(invalid("frame contains NUL or embedded newline"));
    }
    let value = strict::from_str(text).map_err(|e| {
        invalid(if e.contains("duplicate") {
            "duplicate object key"
        } else {
            "malformed JSON"
        })
    })?;
    let Value::Object(obj) = &value else {
        return Err(invalid("request must be a JSON object"));
    };
    let id = obj
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| valid_id(s))
        .map(str::to_owned);
    match obj.get("v") {
        Some(Value::Number(n)) if n.as_u64() == Some(u64::from(PROTOCOL_VERSION)) => {}
        Some(Value::Number(_)) => {
            return Err(ProtocolError::new(
                ErrorCode::ProtocolMismatch,
                "unsupported protocol version; this build speaks v1",
            )
            .with_id(id))
        }
        Some(_) => return Err(invalid("field v must be a number").with_id(id)),
        None => return Err(invalid("missing field v").with_id(id)),
    }
    if id.is_none() {
        return Err(invalid("id must be 1..=128 characters of [A-Za-z0-9._:-]"));
    }
    if let Some(k) = obj
        .keys()
        .find(|k| !matches!(k.as_str(), "v" | "id" | "method" | "params"))
    {
        let k = if k.len() > 64 { "(long name)" } else { k };
        return Err(invalid(&format!("unknown field {k:?}")).with_id(id));
    }
    let method = match obj.get("method") {
        Some(Value::String(m)) => Method::parse(m).ok_or_else(|| {
            ProtocolError::new(ErrorCode::UnknownMethod, "unknown method").with_id(id.clone())
        })?,
        Some(_) => return Err(invalid("method must be a string").with_id(id)),
        None => return Err(invalid("missing field method").with_id(id)),
    };
    match obj.get("params") {
        None | Some(Value::Object(_)) => {}
        Some(_) => return Err(invalid("params must be a JSON object").with_id(id)),
    }
    let request: Request = serde_json::from_value(value)
        .map_err(|_| invalid("invalid request envelope").with_id(id))?;
    debug_assert_eq!(request.method, method);
    Ok(request)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub v: u16,
    pub id: Option<String>,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBody>,
}
impl Response {
    pub fn ok(id: impl Into<String>, result: Value) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            id: Some(id.into()),
            ok: true,
            result: Some(result),
            error: None,
        }
    }
    /// Error response with the code's default `retryable` and a bounded message.
    pub fn err(id: Option<String>, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            id,
            ok: false,
            result: None,
            error: Some(ErrorBody {
                code,
                message: bounded_message(message.into()),
                retryable: code.retryable(),
            }),
        }
    }
    /// One line with a trailing newline; `OUTPUT_LIMIT` error if it would exceed the frame.
    pub fn to_frame(&self) -> Result<String, ProtocolError> {
        encode_frame(self).map_err(|e| e.with_id(self.id.clone()))
    }
    /// Like `to_frame`, but replaces an oversize response with an `OUTPUT_LIMIT` error frame.
    pub fn to_frame_or_limit(&self) -> String {
        self.to_frame().unwrap_or_else(|e| {
            let id = self.id.clone().filter(|i| valid_id(i));
            encode_frame(&Response::err(id, e.code, e.message))
                .expect("bounded error response fits in a frame")
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub v: u16,
    pub event: String,
    pub seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub data: Value,
}
impl Event {
    pub fn new(event: impl Into<String>, seq: u64, task_id: Option<String>, data: Value) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            event: event.into(),
            seq,
            task_id,
            data,
        }
    }
    pub fn to_frame(&self) -> Result<String, ProtocolError> {
        encode_frame(self)
    }
}

/// Serializes to one JSON line plus `\n`, refusing frames over `MAX_FRAME_BYTES`.
pub fn encode_frame<T: Serialize + ?Sized>(value: &T) -> Result<String, ProtocolError> {
    let mut line = serde_json::to_string(value)
        .map_err(|_| ProtocolError::new(ErrorCode::Internal, "failed to encode frame"))?;
    if line.len() > MAX_FRAME_PAYLOAD_BYTES {
        return Err(ProtocolError::new(
            ErrorCode::OutputLimit,
            "response exceeds the 65536-byte frame limit",
        ));
    }
    line.push('\n');
    Ok(line)
}

#[cfg(test)]
mod tests;
