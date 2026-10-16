//! Coding-client activity (v0.3 spec §5.8, §6.4): the shared vocabulary of the hook,
//! the fold and the export. Pure; no I/O.
//!
//! WP0 lands only the declarations every work package shares: client ids, event kinds,
//! session and app states, and the numeric rules. The constants are mirrored in
//! `docs/v0.3/contracts/agent_vectors.json` (checked by a test below). WP2 adds the pure
//! functions (`turn_qualifies`, `caps_allow`, `pet_state`, normalization) and runs the
//! vectors.

/// A supported coding client. Ids match `~/.config/greyforge/agents/registry.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ClientId {
    ClaudeCode,
    Codex,
    OpenCode,
    Hermes,
    GrokCli,
}
impl ClientId {
    /// Stable order: Helpers rows, export maps and tie-breaks all use it.
    pub const ALL: [Self; 5] = [
        Self::ClaudeCode,
        Self::Codex,
        Self::OpenCode,
        Self::Hermes,
        Self::GrokCli,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::OpenCode => "opencode",
            Self::Hermes => "hermes",
            Self::GrokCli => "grok-cli",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == s)
    }
    /// The name shown next to the app tile (`{app}` in `strings.json`).
    pub fn display_name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
            Self::OpenCode => "OpenCode",
            Self::Hermes => "Hermes",
            Self::GrokCli => "Grok",
        }
    }
    /// Codex needs the guided trust step and never appears on the first-run Connect card.
    pub fn on_first_run_card(self) -> bool {
        self != Self::Codex
    }
}

/// A normalized hook event (§5.8). Anything else is dropped before the inbox.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EventKind {
    SessionStart,
    TurnStart,
    Activity,
    Waiting,
    IdlePrompt,
    TurnDone,
    TurnFailed,
    TurnCancelled,
    SessionEnd,
}
impl EventKind {
    pub const ALL: [Self; 9] = [
        Self::SessionStart,
        Self::TurnStart,
        Self::Activity,
        Self::Waiting,
        Self::IdlePrompt,
        Self::TurnDone,
        Self::TurnFailed,
        Self::TurnCancelled,
        Self::SessionEnd,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SessionStart => "session_start",
            Self::TurnStart => "turn_start",
            Self::Activity => "activity",
            Self::Waiting => "waiting",
            Self::IdlePrompt => "idle_prompt",
            Self::TurnDone => "turn_done",
            Self::TurnFailed => "turn_failed",
            Self::TurnCancelled => "turn_cancelled",
            Self::SessionEnd => "session_end",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }
    /// Kinds that close a turn; they close only the turn whose key matches (§6.4).
    pub fn is_closer(self) -> bool {
        matches!(
            self,
            Self::TurnDone | Self::TurnFailed | Self::TurnCancelled
        )
    }
}

/// The state of one `agent_sessions` row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SessionState {
    Idle,
    Working,
    Waiting,
    Error,
    Ended,
}
impl SessionState {
    pub const ALL: [Self; 5] = [
        Self::Idle,
        Self::Working,
        Self::Waiting,
        Self::Error,
        Self::Ended,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Waiting => "waiting",
            Self::Error => "error",
            Self::Ended => "ended",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

/// The exported state of one app and the global `agent.state` (§6.4 pet-state aggregate).
/// Declared in rising priority: `Ord` is the aggregation order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AppState {
    Idle,
    Error,
    Working,
    Waiting,
}
impl AppState {
    pub const ALL: [Self; 4] = [Self::Idle, Self::Error, Self::Working, Self::Waiting];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Error => "error",
            Self::Working => "working",
            Self::Waiting => "waiting",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

/// Hermes platforms that count as a person at the keyboard (§5.6).
pub const HERMES_INTERACTIVE_PLATFORMS: [&str; 4] = ["cli", "tui", "desktop", "acp"];

// ---- Numeric rules (§5.8, §5.9, §6.4, §8). Mirrored in agent_vectors.json "constants". ----

/// A `working` session stops counting this long after its last event.
pub const STALE_WORKING_MS: i64 = 10 * 60 * 1000;
/// A `waiting` session stops counting this long after its last event.
pub const STALE_WAITING_MS: i64 = 30 * 60 * 1000;
/// A turn without a tool call qualifies for a treat only if it lasted this long.
pub const MIN_TURN_MS: i64 = 30 * 1000;
/// Unspent treats the jar holds.
pub const JAR_MAX: u32 = 10;
/// At most one treat per session per this window.
pub const CAP_SESSION_MS: i64 = 30 * 1000;
/// At most this many treats per rolling hour.
pub const CAP_PER_HOUR: u32 = 12;
/// At most this many treats per rolling 24 hours.
pub const CAP_PER_DAY: u32 = 48;
/// A tool event of the just-finished turn this close after its `turn_done` is a late
/// async delivery, not a continuation.
pub const LATE_TOOL_MS: i64 = 1000;
/// Celebrations show only within this window after the event.
pub const CELEBRATE_WINDOW_MS: i64 = 30 * 1000;
/// A finished turn celebrates only after its app stayed not-working this long.
pub const CELEBRATE_GRACE_MS: i64 = 2000;
/// A failed turn shows "Oops" this long.
pub const ERROR_RECENT_MS: i64 = 2 * 60 * 1000;
/// The `error` animation frame plays this long before the pet idles.
pub const ERROR_FRAME_MS: i64 = 3000;
/// Events further in the future than this are clamped to the fold's clock.
pub const FUTURE_CLAMP_MS: i64 = 2 * 60 * 1000;
/// Events older than this are dropped by the fold.
pub const MAX_EVENT_AGE_MS: i64 = 24 * 60 * 60 * 1000;
/// XP granted by one given treat (`activity_xp.amount`).
pub const TREAT_XP: u32 = 5;
/// The hook reads at most this much of a payload.
pub const MAX_HOOK_READ_BYTES: usize = 64 * 1024;
/// The hook drains stdin to EOF for at most this long.
pub const HOOK_DRAIN_MS: u64 = 250;
/// One inbox record is at most this long.
pub const MAX_INBOX_RECORD_BYTES: usize = 512;
/// More inbox entries than this drop the event and touch `inbox/.overflow`.
pub const MAX_INBOX_FILES: usize = 2048;
/// Allow-listed id fields are at most this long (`[A-Za-z0-9._:-]`).
pub const MAX_ID_FIELD_BYTES: usize = 128;
/// Event-id time bucket for duplicate-delivery dedupe.
pub const EVENT_BUCKET_MS: i64 = 3000;
/// Events per fold transaction.
pub const FOLD_BATCH_EVENTS: usize = 64;
/// Time budget of one fold transaction.
pub const FOLD_BATCH_MS: u64 = 50;
/// Time budget of one fold run.
pub const FOLD_RUN_MS: u64 = 5000;
/// First fold backoff; it doubles up to [`FOLD_BACKOFF_MAX_MS`].
pub const FOLD_BACKOFF_START_MS: i64 = 5000;
pub const FOLD_BACKOFF_MAX_MS: i64 = 10 * 60 * 1000;
/// `agent_events` retention.
pub const AGENT_EVENTS_KEEP_MS: i64 = 7 * 24 * 60 * 60 * 1000;
pub const AGENT_EVENTS_MAX_ROWS: u32 = 20_000;
/// Ended sessions are kept this long.
pub const AGENT_SESSIONS_KEEP_MS: i64 = 30 * 24 * 60 * 60 * 1000;
/// `activity_turns` timestamps older than this are coarsened to the hour.
pub const TURN_TIMES_EXACT_MS: i64 = 24 * 60 * 60 * 1000;
/// Retention runs at most once per this interval, deleting at most
/// [`RETENTION_LIMIT`] rows per statement.
pub const RETENTION_INTERVAL_MS: i64 = 60 * 60 * 1000;
pub const RETENTION_LIMIT: u32 = 500;
/// At most one non-urgent export per this interval.
pub const EXPORT_MIN_INTERVAL_MS: i64 = 250;
/// The ready check sends `turn_done` this long after `turn_start`.
pub const TEST_DONE_DELAY_MS: i64 = 1500;
/// A ready-check event that has not arrived after this long fails the check.
pub const TEST_ARRIVAL_MS: i64 = 5000;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const VECTORS: &str = include_str!("../../../docs/v0.3/contracts/agent_vectors.json");

    fn vectors() -> Value {
        serde_json::from_str(VECTORS).expect("agent_vectors.json parses")
    }

    #[test]
    fn vocabularies_match_the_contract() {
        let v = vectors();
        let ids: Vec<&str> = v["clients"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ClientId::ALL.map(ClientId::as_str));
        for c in v["clients"].as_array().unwrap() {
            let id = ClientId::parse(c["id"].as_str().unwrap()).unwrap();
            assert_eq!(c["name"], id.display_name());
            assert_eq!(c["first_run_card"], id.on_first_run_card());
        }
        let kinds: Vec<&str> = v["kinds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap())
            .collect();
        assert_eq!(kinds, EventKind::ALL.map(EventKind::as_str));
        let states: Vec<&str> = v["session_states"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap())
            .collect();
        assert_eq!(states, SessionState::ALL.map(SessionState::as_str));
        let app: Vec<&str> = v["app_states_rising"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap())
            .collect();
        assert_eq!(app, AppState::ALL.map(AppState::as_str));
        assert!(AppState::Waiting > AppState::Working && AppState::Working > AppState::Error);
        let platforms: Vec<&str> = v["interactivity"]["hermes_platforms"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap())
            .collect();
        assert_eq!(platforms, HERMES_INTERACTIVE_PLATFORMS);
    }

    #[test]
    fn constants_match_the_contract() {
        let v = vectors();
        let c = &v["constants"];
        let int = |k: &str| c[k].as_i64().unwrap_or_else(|| panic!("constant {k}"));
        assert_eq!(int("stale_working_ms"), STALE_WORKING_MS);
        assert_eq!(int("stale_waiting_ms"), STALE_WAITING_MS);
        assert_eq!(int("min_turn_ms"), MIN_TURN_MS);
        assert_eq!(int("jar_max"), i64::from(JAR_MAX));
        assert_eq!(int("cap_session_ms"), CAP_SESSION_MS);
        assert_eq!(int("cap_per_hour"), i64::from(CAP_PER_HOUR));
        assert_eq!(int("cap_per_day"), i64::from(CAP_PER_DAY));
        assert_eq!(int("late_tool_ms"), LATE_TOOL_MS);
        assert_eq!(int("celebrate_window_ms"), CELEBRATE_WINDOW_MS);
        assert_eq!(int("celebrate_grace_ms"), CELEBRATE_GRACE_MS);
        assert_eq!(int("error_recent_ms"), ERROR_RECENT_MS);
        assert_eq!(int("error_frame_ms"), ERROR_FRAME_MS);
        assert_eq!(int("future_clamp_ms"), FUTURE_CLAMP_MS);
        assert_eq!(int("max_event_age_ms"), MAX_EVENT_AGE_MS);
        assert_eq!(int("treat_xp"), i64::from(TREAT_XP));
        assert_eq!(int("max_hook_read_bytes") as usize, MAX_HOOK_READ_BYTES);
        assert_eq!(int("hook_drain_ms") as u64, HOOK_DRAIN_MS);
        assert_eq!(
            int("max_inbox_record_bytes") as usize,
            MAX_INBOX_RECORD_BYTES
        );
        assert_eq!(int("max_inbox_files") as usize, MAX_INBOX_FILES);
        assert_eq!(int("max_id_field_bytes") as usize, MAX_ID_FIELD_BYTES);
        assert_eq!(int("event_bucket_ms"), EVENT_BUCKET_MS);
        assert_eq!(int("fold_batch_events") as usize, FOLD_BATCH_EVENTS);
        assert_eq!(int("fold_batch_ms") as u64, FOLD_BATCH_MS);
        assert_eq!(int("fold_run_ms") as u64, FOLD_RUN_MS);
        assert_eq!(int("fold_backoff_start_ms"), FOLD_BACKOFF_START_MS);
        assert_eq!(int("fold_backoff_max_ms"), FOLD_BACKOFF_MAX_MS);
        assert_eq!(int("agent_events_keep_ms"), AGENT_EVENTS_KEEP_MS);
        assert_eq!(
            int("agent_events_max_rows"),
            i64::from(AGENT_EVENTS_MAX_ROWS)
        );
        assert_eq!(int("agent_sessions_keep_ms"), AGENT_SESSIONS_KEEP_MS);
        assert_eq!(int("turn_times_exact_ms"), TURN_TIMES_EXACT_MS);
        assert_eq!(int("retention_interval_ms"), RETENTION_INTERVAL_MS);
        assert_eq!(int("retention_limit"), i64::from(RETENTION_LIMIT));
        assert_eq!(int("export_min_interval_ms"), EXPORT_MIN_INTERVAL_MS);
        assert_eq!(int("test_done_delay_ms"), TEST_DONE_DELAY_MS);
        assert_eq!(int("test_arrival_ms"), TEST_ARRIVAL_MS);
        assert_eq!(
            c.as_object().unwrap().len(),
            35,
            "every contract constant is mirrored here"
        );
    }

    #[test]
    fn closers_are_the_three_turn_enders() {
        let closers: Vec<_> = EventKind::ALL
            .into_iter()
            .filter(|k| k.is_closer())
            .collect();
        assert_eq!(
            closers,
            [
                EventKind::TurnDone,
                EventKind::TurnFailed,
                EventKind::TurnCancelled
            ]
        );
        assert!(ClientId::ALL
            .into_iter()
            .all(|c| c.on_first_run_card() == (c != ClientId::Codex)));
    }
}
