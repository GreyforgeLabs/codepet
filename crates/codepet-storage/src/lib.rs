//! Durable SQLite state for CodePet (mock execution only) and the v0.2 companion lifecycle.
//!
//! Every mutation runs in `BEGIN IMMEDIATE`, which serializes writers across processes, and
//! every task state change appends a `task_events` row in the same transaction. Every write
//! transaction settles attended presence and rest first and finalizes a due death last (see
//! `docs/lifecycle.md`). Nothing here launches a provider, spawns a process, or reads or
//! writes a selected project folder.
use std::fmt;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

mod agent_export;
mod agent_store;
mod app;
mod bar_export;
mod care;
mod clients_store;
mod life;
mod lifecycle_store;
mod model;
mod paths;
mod presence_io;
mod schema;
mod spool;
mod store;
mod sys;
#[cfg(any(test, feature = "test-support"))]
pub mod testing;
mod treats;

pub use agent_export::{
    AgentExport, AppExport, ClientExport, ClientsExport, JobExport, TestExport, TreatsExport,
};
pub use bar_export::{
    BAR_SCHEMA, BAR_SCHEMA_V2, MAX_BAR_STATE_BYTES, MAX_READINESS_REASONS, MAX_REASON_BYTES,
};
pub use care::{BATHROOM_LIMIT, DEMO_XP, EXECUTION_LIMIT_MS, SESSION_LIMIT, SLEEP_MS};
pub use clients_store::{
    ClientRow, DoctorMeta, JobClientResult, JobTarget, StatusUpdate, JOB_KINDS, REASONS,
    STEP_CODEX_HOOKS_OFF, STEP_CODEX_TRUST,
};
/// Lifecycle enums used by the life API, re-exported for callers of `adopt`/`set_rules`.
pub use codepet_core::lifecycle::{Mode, Pace};
pub use life::{
    lifecycle_rules_json, lifecycle_rules_sha256, AdoptRequest, Leniency, StartRequest,
    MAX_MEMORIAL_PAGE,
};
pub use model::{
    AcceptOutcome, CareOutcome, CareView, CriticalView, DecayView, Event, EventData, EventsPage,
    HistoryCursor, HistoryPage, LifeOutcome, LifeSummary, LifeView, LifecycleView, MasteryView,
    MemorialBrief, MemorialPage, MetersView, MockOptions, MockOutcome, NeedView, NextStageView,
    PauseView, PreflightResult, PreflightSummary, ProgressionView, ProviderInfo, ReceiptView,
    RecoverResult, RecoveryChange, ReviewBrief, ReviewView, RulesView, SettingsView, Snapshot,
    StartResult, TaskBrief, TaskInput, TaskSummary, UnseenEvent, XpView,
};
pub use paths::{
    StatePaths, BAR_STATE_FILE, CLIENTS_DIR, DISCONNECTED_SUFFIX, FOLD_BACKOFF_FILE,
    FOLD_LOCK_FILE, HELPERS_FLAG, INBOX_DIR, INBOX_OVERFLOW_FILE, INTEGRATIONS_DIR, JOBS_DIR,
    OFF_FLAG, PRESENCE_FILE, SHIM_REL_PATH, STAGING_DIR, TEST_NONCE_FILE,
};
#[cfg(feature = "legacy-tasks")]
pub use store::{AcceptRequest, SealOutcome, Step};
pub use store::{MigrationReport, Store};
pub use sys::Supervisor;

/// Lowercase hex SHA-256 of `data` (Codex trust hashes, shim digests in `doctor`).
pub fn sha256_hex(data: &[u8]) -> String {
    sys::sha256_hex(data)
}
pub use treats::{caps_allow, CapCheck, Earned};
// v0.3 ingestion (WP2): the hook inbox and the fold.
pub use agent_store::{FoldBatch, AGENT_SCHEMA_VERSION};
pub use spool::{
    event_id, hmac_key, payload_digest, Inbox, InboxRecord, InboxScan, SpoolWrite, INBOX_EXT,
};

/// Current SQLite `user_version`.
pub const SCHEMA_VERSION: i64 = 5;
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
/// Idempotency receipts kept for `pet.care`, `pet.sleep` and `pet.wake`: the most recent
/// ones, pruned in the transaction that adds a new one. Acceptance receipts are never
/// pruned. A retry of a care request older than this window runs as a new action.
pub const MAX_CARE_RECEIPTS: i64 = 256;
/// Names accepted by `CODEPET_FAULT` in debug builds. v0.3 (spec §6.5) retires the task
/// pipeline's points (`accept_*`, `start_after_commit`, `supervisor_running`,
/// `review_before_commit`; still honoured by the `legacy-tasks` build, see
/// [`LEGACY_FAULT_POINTS`]) and adds `fold_before_commit`, `treat_before_commit`,
/// `enable_before_commit` and `connect_after_artifact`.
pub const FAULT_POINTS: [&str; 8] = [
    "death_before_commit",
    "adopt_before_commit",
    "evolve_before_commit",
    "presence_before_commit",
    "fold_before_commit",
    "treat_before_commit",
    "enable_before_commit",
    "connect_after_artifact",
];
/// The retired task pipeline's fault points (`legacy-tasks` only; deleted in v0.3.1).
#[cfg(feature = "legacy-tasks")]
pub const LEGACY_FAULT_POINTS: [&str; 5] = [
    "accept_before_commit",
    "accept_after_commit",
    "start_after_commit",
    "supervisor_running",
    "review_before_commit",
];
/// Bounded idempotency receipts for frequent lifecycle clicks (`pet.select`,
/// `pet.events_seen`, `pet.rules`, `pet.configure_life`), pruned like care receipts.
/// Adoption and retirement receipts are kept.
pub const MAX_LIFE_RECEIPTS: i64 = 256;

pub type Result<T, E = StorageError> = std::result::Result<T, E>;

/// Typed storage failures; `code()` maps 1:1 onto protocol error codes.
#[derive(Debug)]
pub enum StorageError {
    Busy,
    /// No living companion: care, sleep, pause and selection need one.
    NoActivePet,
    /// `INVALID_STATE` "CodePet is off": a method that would age, arm, grow or end a life
    /// while `meta.enabled = 0` (`protocol.json` `off_behaviour`).
    Off,
    /// `pet.start` on a life that is already armed; the UI just opens Home.
    AlreadyArmed,
    /// `pet.start` / `pet.rename`: the name fails core's `normalize_pet_name`.
    NameInvalid(&'static str),
    /// `treats.give` with an empty jar.
    NoTreats,
    /// `treats.give` while the pet is asleep, paused or fainted.
    PetNeedsCare,
    /// `NOT_FOUND` for a `life_id` that does not name the living companion.
    LifeNotFound,
    /// `BUSY`: the fold holds `fold.lock` (`agent.forget_history`); retry shortly.
    FoldRunning,
    /// `BUSY`: a Connect, Disconnect or ready-check job already runs for that client.
    JobRunning,
    /// `NO_ACTIVE_PET` for a command whose own settlement made a pending end final: the
    /// end is committed and the command refused.
    PassedAway,
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
    /// An older database that only `codepet migrate` (or `state.get`) may upgrade; the file
    /// was not modified.
    MigrationRequired(i64),
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
            Self::Busy | Self::FoldRunning | Self::JobRunning => "BUSY",
            Self::NoActivePet | Self::PassedAway => "NO_ACTIVE_PET",
            Self::InvalidState(_) | Self::Off => "INVALID_STATE",
            Self::AlreadyArmed => "ALREADY_ARMED",
            Self::NameInvalid(_) => "NAME_INVALID",
            Self::NoTreats => "NO_TREATS",
            Self::PetNeedsCare => "PET_NEEDS_CARE",
            Self::LifeNotFound => "NOT_FOUND",
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
            Self::SchemaUnsupported(_) | Self::MigrationRequired(_) => "SCHEMA_UNSUPPORTED",
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
            Self::NoActivePet => f.write_str(
                "No companion is living. Adopt a companion whenever you're ready.",
            ),
            Self::PassedAway => f.write_str(
                "Your companion's grace time ran out and they have passed away. \
                 Their memorial is kept; adopt a companion whenever you're ready.",
            ),
            Self::InvalidState(why) => write!(f, "Invalid state: {why}."),
            Self::Off => f.write_str("CodePet is off"),
            Self::AlreadyArmed => f.write_str("The companion has already started."),
            Self::NameInvalid(why) => write!(f, "That name is not allowed: {why}."),
            Self::NoTreats => f.write_str("The treat jar is empty."),
            Self::PetNeedsCare => {
                f.write_str("The companion needs care first (asleep, paused or fainted).")
            }
            Self::LifeNotFound => f.write_str("That companion was not found."),
            Self::JobRunning => {
                f.write_str("That app is already being connected or checked; try again shortly.")
            }
            Self::FoldRunning => {
                f.write_str("Coding activity is being recorded right now; try again shortly.")
            }
            Self::StaleRevision => f.write_str("The revision changed. Reload and review it."),
            Self::UnsupportedProvider => f.write_str("Provider is not supported in this version."),
            Self::NotFound => f.write_str("Not found."),
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
            Self::MigrationRequired(v) => write!(
                f,
                "Database schema version {v} needs `codepet migrate` first; left unchanged."
            ),
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
/// Settable clock for tests. By default it has no boot clock (`steady_time() == None`),
/// exactly as before v0.2; `set_steady` adds a settable `(boot id, CLOCK_BOOTTIME ms)` pair
/// so lifecycle tests can drive presence settlement deterministically.
#[derive(Debug, Default)]
pub struct ManualClock {
    wall: AtomicI64,
    steady: Mutex<Option<(String, i64)>>,
}
impl ManualClock {
    pub fn new(ms: i64) -> Self {
        Self {
            wall: AtomicI64::new(ms),
            steady: Mutex::new(None),
        }
    }
    /// Sets the wall clock only (a wall jump); the boot clock is unchanged.
    pub fn set(&self, ms: i64) {
        self.wall.store(ms, Ordering::SeqCst);
    }
    /// Time passes: the wall clock and, when set, the boot clock advance together.
    pub fn advance(&self, ms: i64) {
        self.wall.fetch_add(ms, Ordering::SeqCst);
        if let Some((_, steady)) = self.steady_guard().as_mut() {
            *steady += ms;
        }
    }
    /// Sets (or replaces) the boot clock identity and its milliseconds since boot.
    pub fn set_steady(&self, boot: &str, ms: i64) {
        *self.steady_guard() = Some((boot.to_owned(), ms));
    }
    /// Removes the boot clock again (`steady_time() == None`).
    pub fn clear_steady(&self) {
        *self.steady_guard() = None;
    }
    /// Advances only the boot clock (for example a suspend, which the wall clock also
    /// sees, or a wall clock that stepped back while real time passed).
    pub fn advance_steady(&self, ms: i64) {
        if let Some((_, steady)) = self.steady_guard().as_mut() {
            *steady += ms;
        }
    }
    fn steady_guard(&self) -> std::sync::MutexGuard<'_, Option<(String, i64)>> {
        self.steady
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
impl Clock for ManualClock {
    fn now_ms(&self) -> i64 {
        self.wall.load(Ordering::SeqCst)
    }
    fn steady_time(&self) -> Option<(String, i64)> {
        self.steady_guard().clone()
    }
}

/// Crash-injection point. `CODEPET_FAULT=<name>` aborts the process here, in debug builds
/// only; release builds ignore the variable. Storage already calls every point in
/// `FAULT_POINTS` at its documented place.
///
/// The process marks itself non-dumpable first. With a piped `core_pattern`
/// (systemd-coredump), `ulimit -c 0` still journals every abort as a crash, and desktop
/// crash watchers then report each injected fault; a non-dumpable process skips the
/// core-dump path in the kernel entirely.
pub fn fault(name: &str) {
    #[cfg(feature = "legacy-tasks")]
    let known = FAULT_POINTS.contains(&name) || LEGACY_FAULT_POINTS.contains(&name);
    #[cfg(not(feature = "legacy-tasks"))]
    let known = FAULT_POINTS.contains(&name);
    debug_assert!(known, "unknown fault point {name}");
    #[cfg(test)]
    test_fault::hit(name);
    #[cfg(debug_assertions)]
    if std::env::var_os("CODEPET_FAULT").is_some_and(|v| v == name) {
        #[cfg(target_os = "linux")]
        let _ =
            rustix::process::set_dumpable_behavior(rustix::process::DumpableBehavior::NotDumpable);
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
