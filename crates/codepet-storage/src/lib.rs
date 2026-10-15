//! Durable SQLite state for CodePet milestone 1 (mock execution only).
//!
//! Every mutation runs in `BEGIN IMMEDIATE`, which serializes writers across processes, and
//! every task state change appends a `task_events` row in the same transaction. Nothing here
//! launches a provider, spawns a process, or reads or writes a selected project folder.
use std::fmt;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

mod care;
mod model;
mod paths;
mod schema;
mod store;
mod sys;

pub use care::{BATHROOM_LIMIT, DEMO_XP, EXECUTION_LIMIT_MS, SESSION_LIMIT, SLEEP_MS};
pub use model::{
    AcceptOutcome, CareOutcome, CareView, DecayView, Event, EventData, EventsPage, HistoryCursor,
    HistoryPage, MasteryView, MetersView, MockOptions, MockOutcome, PauseView, PreflightResult,
    PreflightSummary, ProgressionView, ProviderInfo, ReceiptView, RecoverResult, RecoveryChange,
    ReviewBrief, ReviewView, SettingsView, Snapshot, StartResult, TaskBrief, TaskInput,
    TaskSummary, XpView,
};
pub use paths::StatePaths;
pub use store::{AcceptRequest, SealOutcome, Step, Store};
pub use sys::Supervisor;

/// Current SQLite `user_version`.
pub const SCHEMA_VERSION: i64 = 3;
pub const DB_FILE: &str = "codepet.sqlite3";
pub const TOKEN_TTL_MS: i64 = 5 * 60 * 1000;
/// Responses must fit one 64 KiB frame, so stored text is bounded by its JSON-encoded size
/// (quotes and escapes included). A full task summary is at most draft + artifact + receipt
/// (override reason ≤ 512 × 6 bytes encoded) + fixed fields, about 61 KiB.
pub const MAX_DRAFT_JSON_BYTES: usize = 32 * 1024;
/// JSON-encoded bound on one review artifact (milestone 1 has no artifact paging).
pub const MAX_ARTIFACT_JSON_BYTES: usize = 24 * 1024;
/// JSON-encoded bound on the instructions excerpt in a `TaskBrief`.
pub const MAX_EXCERPT_JSON_BYTES: usize = 256;
/// JSON-encoded budget for the briefs in one `history.list` page.
pub const MAX_HISTORY_PAGE_JSON_BYTES: usize = 48 * 1024;
pub const MAX_OVERRIDE_REASON_BYTES: usize = codepet_core::MAX_OVERRIDE_REASON_BYTES;
pub const MAX_MOCK_STEP_MS: u32 = 2000;
pub const MAX_EVENTS_PAGE: u32 = 100;
pub const MAX_HISTORY_PAGE: u32 = 50;
pub const RECENT_TASKS: u32 = 10;
pub const MAX_REQUEST_ID_BYTES: usize = 128;
/// Names accepted by `CODEPET_FAULT` in debug builds.
pub const FAULT_POINTS: [&str; 5] = [
    "accept_before_commit",
    "accept_after_commit",
    "start_after_commit",
    "supervisor_running",
    "review_before_commit",
];

pub type Result<T, E = StorageError> = std::result::Result<T, E>;

/// Typed storage failures; `code()` maps 1:1 onto protocol error codes.
#[derive(Debug)]
pub enum StorageError {
    Busy,
    InvalidState(&'static str),
    StaleRevision,
    UnsupportedProvider,
    NotFound,
    TokenInvalid,
    TokenExpired,
    ReviewChanged,
    /// The stored artifact no longer matches the sealed review digest.
    ReviewDamaged,
    AcknowledgementRequired,
    OverrideRequired,
    IdempotencyConflict,
    InvalidParams(&'static str),
    InvalidPath(&'static str),
    Capacity,
    ResyncRequired,
    /// Database `user_version` this build cannot handle; the file was not modified.
    SchemaUnsupported(i64),
    /// `quick_check` failed or the file is not a database; the file was not modified.
    Corrupt,
    /// State directory or database file failed an ownership/mode/symlink check.
    UnsafeStateDir(String),
    Unavailable(&'static str),
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
}
impl StorageError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Busy => "BUSY",
            Self::InvalidState(_) => "INVALID_STATE",
            Self::StaleRevision => "STALE_REVISION",
            Self::UnsupportedProvider => "UNSUPPORTED_PROVIDER",
            Self::NotFound => "NOT_FOUND",
            Self::TokenInvalid => "TOKEN_INVALID",
            Self::TokenExpired => "TOKEN_EXPIRED",
            Self::ReviewChanged | Self::ReviewDamaged => "REVIEW_CHANGED",
            Self::AcknowledgementRequired => "ACKNOWLEDGEMENT_REQUIRED",
            Self::OverrideRequired => "OVERRIDE_REQUIRED",
            Self::IdempotencyConflict => "IDEMPOTENCY_CONFLICT",
            Self::InvalidParams(_) | Self::InvalidPath(_) => "INVALID_PARAMS",
            Self::Capacity => "CAPACITY",
            Self::ResyncRequired => "RESYNC_REQUIRED",
            Self::SchemaUnsupported(_) => "SCHEMA_UNSUPPORTED",
            Self::Corrupt
            | Self::UnsafeStateDir(_)
            | Self::Unavailable(_)
            | Self::Sqlite(_)
            | Self::Io(_) => "STORAGE_UNAVAILABLE",
        }
    }
    /// True only for lock contention that a later explicit retry may clear.
    pub fn retryable(&self) -> bool {
        matches!(self, Self::Sqlite(e) if matches!(
            e.sqlite_error_code(),
            Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
        ))
    }
}
impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => f.write_str("Another task occupies the active slot."),
            Self::InvalidState(why) => write!(f, "Invalid task state: {why}."),
            Self::StaleRevision => f.write_str("The revision changed. Reload and review it."),
            Self::UnsupportedProvider => f.write_str("Provider is not supported in this version."),
            Self::NotFound => f.write_str("Task not found."),
            Self::TokenInvalid => f.write_str("Preflight token is invalid or already used."),
            Self::TokenExpired => f.write_str("Preflight token expired. Run preflight again."),
            Self::ReviewChanged => f.write_str("The review changed. Review it again."),
            Self::ReviewDamaged => f.write_str(
                "The saved review no longer matches its sealed digest. Run recovery; it cannot be accepted.",
            ),
            Self::AcknowledgementRequired => f.write_str("Review acknowledgement is required."),
            Self::OverrideRequired => f.write_str(
                "Unverified or failed checks need a non-empty override reason.",
            ),
            Self::IdempotencyConflict => {
                f.write_str("Request id was already used with different parameters.")
            }
            Self::InvalidParams(why) => write!(f, "Invalid parameters: {why}."),
            Self::InvalidPath(why) => write!(f, "Invalid path: {why}."),
            Self::Capacity => f.write_str("Capacity limit reached."),
            Self::ResyncRequired => f.write_str("Event cursor is ahead of storage; resync."),
            Self::SchemaUnsupported(v) => {
                write!(
                    f,
                    "Database schema version {v} is not supported; left unchanged."
                )
            }
            Self::Corrupt => f.write_str("Database failed its integrity check; left unchanged."),
            Self::UnsafeStateDir(why) => write!(f, "Unsafe state directory: {why}."),
            Self::Unavailable(why) => write!(f, "Storage unavailable: {why}."),
            Self::Sqlite(e) => write!(f, "Storage unavailable: {e}"),
            Self::Io(e) => write!(f, "Storage unavailable: {e}"),
        }
    }
}
impl std::error::Error for StorageError {}
impl From<rusqlite::Error> for StorageError {
    fn from(e: rusqlite::Error) -> Self {
        match e.sqlite_error_code() {
            Some(rusqlite::ErrorCode::NotADatabase | rusqlite::ErrorCode::DatabaseCorrupt) => {
                Self::Corrupt
            }
            _ => Self::Sqlite(e),
        }
    }
}
impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Wall clock in Unix milliseconds; injectable so tests can expire tokens.
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> i64;
    /// Milliseconds since boot *including* suspend (CLOCK_BOOTTIME) and a clock identity,
    /// when available. Used for nap settlement, where suspended time counts as rest. A
    /// missing identity uses bounded wall-clock reconciliation instead, useful for
    /// deterministic tests.
    fn steady_time(&self) -> Option<(String, i64)> {
        None
    }
    /// Monotonic milliseconds *excluding* suspend (CLOCK_MONOTONIC) and a clock identity,
    /// when available. Used for execution accounting, where a suspended laptop must not
    /// be charged as supervised execution. Same fallback as `steady_time`.
    fn execution_time(&self) -> Option<(String, i64)> {
        None
    }
}

fn boot_id() -> Option<String> {
    let boot = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").ok()?;
    Some(boot.trim().to_owned())
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;
impl Clock for SystemClock {
    fn steady_time(&self) -> Option<(String, i64)> {
        let boot = boot_id()?;
        let uptime = std::fs::read_to_string("/proc/uptime").ok()?;
        let seconds = uptime.split_whitespace().next()?;
        let (whole, fraction) = seconds.split_once('.').unwrap_or((seconds, ""));
        let millis: i64 = whole.parse::<i64>().ok()?.checked_mul(1000)?;
        let fraction = format!("{fraction:0<3}");
        Some((
            boot,
            millis.checked_add(fraction[..3].parse::<i64>().ok()?)?,
        ))
    }
    fn execution_time(&self) -> Option<(String, i64)> {
        let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        let millis = now
            .tv_sec
            .checked_mul(1000)?
            .checked_add(now.tv_nsec / 1_000_000)?;
        // A distinct identity keeps CLOCK_MONOTONIC values from ever being compared with
        // CLOCK_BOOTTIME cursors written by an earlier build.
        Some((format!("{}/monotonic", boot_id()?), millis))
    }
    fn now_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
    }
}
/// Settable clock for tests.
#[derive(Debug, Default)]
pub struct ManualClock(AtomicI64);
impl ManualClock {
    pub fn new(ms: i64) -> Self {
        Self(AtomicI64::new(ms))
    }
    pub fn set(&self, ms: i64) {
        self.0.store(ms, Ordering::SeqCst);
    }
    pub fn advance(&self, ms: i64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}
impl Clock for ManualClock {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// Crash-injection point. `CODEPET_FAULT=<name>` aborts the process here, in debug builds
/// only; release builds ignore the variable. Storage already calls every point in
/// `FAULT_POINTS` at its documented place.
pub fn fault(name: &str) {
    debug_assert!(FAULT_POINTS.contains(&name), "unknown fault point {name}");
    #[cfg(test)]
    test_fault::hit(name);
    #[cfg(debug_assertions)]
    if std::env::var_os("CODEPET_FAULT").is_some_and(|v| v == name) {
        std::process::abort();
    }
}

#[cfg(test)]
mod test_fault {
    use std::cell::Cell;
    thread_local!(static PANIC_AT: Cell<Option<&'static str>> = const { Cell::new(None) });
    /// Unit tests panic (unwinding drops the open transaction) instead of aborting.
    pub fn arm(name: Option<&'static str>) {
        PANIC_AT.with(|p| p.set(name));
    }
    pub fn hit(name: &str) {
        if PANIC_AT.with(|p| p.get()) == Some(name) {
            panic!("injected fault {name}");
        }
    }
}

#[cfg(test)]
mod tests;
