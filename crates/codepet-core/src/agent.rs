//! Coding-client activity (v0.3 spec §5.8, §6.4): the shared vocabulary of the hook,
//! the fold and the export. Pure; no I/O.
//!
//! WP0 lands only the declarations every work package shares: client ids, event kinds,
//! session and app states, and the numeric rules. The constants are mirrored in
//! `docs/v0.3/contracts/agent_vectors.json` (checked by a test below). WP2 adds the pure
//! functions and runs their vector sections: normalization, event ids, `hmac_keys`,
//! interactivity, `turn_qualifies`, `caps_allow`, the session machine, the fold outcomes and
//! `pet_state` (the `aggregate` section that builds `bar-state.json`'s `agent` block). The
//! shared face priority is not Rust: it is generated into `petstate.js` (WP6).

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
    /// Idempotent state changes (`agent_vectors.json` `event_ids.state_kinds`). A real
    /// duplicate delivery of one is harmless, but a legitimate repeat can be byte-identical
    /// (a second permission prompt for the same tool), so they are never deduped by the
    /// 3 s bucket: their event-id bucket is the millisecond ([`event_bucket`]).
    pub fn is_state_change(self) -> bool {
        matches!(
            self,
            Self::SessionStart | Self::Waiting | Self::IdlePrompt | Self::SessionEnd
        )
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

/// The event id's (and dedupe key's) time bucket: `floor(at_ms / 3000)`, or `at_ms`
/// itself for a state change ([`EventKind::is_state_change`]; an unknown kind uses the
/// 3 s bucket).
pub fn event_bucket(kind: &str, at_ms: i64) -> i64 {
    if EventKind::parse(kind).is_some_and(EventKind::is_state_change) {
        at_ms
    } else {
        at_ms.div_euclid(EVENT_BUCKET_MS)
    }
}
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

// ---- Normalization (§5.8, agent_vectors.json "normalization") ----------------------------

/// The top-level keys the hook's tokenizer keeps. Every other key is skipped unread.
/// (`completed`/`failed`/`interrupted` are not read: the Hermes plugin sends the kind.)
pub const ALLOWED_KEYS: [&str; 23] = [
    "hook_event_name",
    "hookEventName",
    "session_id",
    "sessionId",
    "prompt_id",
    "promptId",
    "turn_id",
    "tool_use_id",
    "toolUseId",
    "notification_type",
    "notificationType",
    "tool_name",
    "toolName",
    "reason",
    "error",
    "agent_id",
    "subagentType",
    "event",
    "tools",
    "interactive",
    "approval_key",
    "codepet_v",
    "codepet_test",
];

/// One allow-listed top-level value as the tokenizer saw it. Strings longer than
/// [`MAX_ID_FIELD_BYTES`] are never kept (they become [`Field::Other`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Field {
    /// A decoded JSON string of at most [`MAX_ID_FIELD_BYTES`] bytes.
    Str(String),
    /// A JSON integer without fraction or exponent that fits `i64`.
    Int(i64),
    Bool(bool),
    Null,
    /// Anything else: a longer string, a fraction, an object or an array.
    Other,
}

/// The allow-listed top-level fields of one hook payload (last occurrence wins).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Payload {
    fields: Vec<(&'static str, Field)>,
}
impl Payload {
    /// Records `key` (ignored unless it is in [`ALLOWED_KEYS`]); a repeated key replaces
    /// the earlier value, like a JSON parser.
    pub fn set(&mut self, key: &str, value: Field) {
        let Some(key) = ALLOWED_KEYS.iter().copied().find(|k| *k == key) else {
            return;
        };
        match self.fields.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => self.fields.push((key, value)),
        }
    }
    pub fn get(&self, key: &str) -> Option<&Field> {
        self.fields.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }
    fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
    fn str(&self, key: &str) -> Option<&str> {
        match self.get(key) {
            Some(Field::Str(s)) => Some(s),
            _ => None,
        }
    }
    /// The first present key's string value (`p.get(a, p.get(b))`).
    fn first_str(&self, keys: &[&str]) -> Option<&str> {
        keys.iter().find(|k| self.has(k)).and_then(|k| self.str(k))
    }
}

/// Why the hook dropped an event before the inbox.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropReason {
    /// `agent_id` / `subagentType` present.
    Child,
    /// `codepet_test` does not match `$STATE/test.nonce`.
    TestNonce,
    /// An event name, notification type or synthesized payload CodePet does not map.
    Unmapped,
    /// A missing or invalid id field.
    BadId,
}
impl DropReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Child => "child",
            Self::TestNonce => "test_nonce",
            Self::Unmapped => "unmapped",
            Self::BadId => "bad_id",
        }
    }
}

/// A normalized event: enums and raw ids only (the raw ids live only in the inbox file).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Normalized {
    pub kind: EventKind,
    pub session: Option<String>,
    pub turn: Option<String>,
    pub tool: Option<String>,
    /// Notification or error class (`agent_events.class`).
    pub class: Option<&'static str>,
    /// Hermes approval key (only without `session`).
    pub akey: Option<String>,
    /// Completed tools counted by a plugin (`turn_done` of OpenCode/Hermes), else 0.
    pub tools: u32,
    /// The synthesized payload's `interactive`; `None` for clients whose interactivity the
    /// hook decides from the environment and `/proc`.
    pub interactive: Option<bool>,
    pub test: bool,
}

/// `error` classes kept verbatim; anything else is `unknown`.
pub const ERROR_CLASSES: [&str; 8] = [
    "rate_limit",
    "authentication_failed",
    "invalid_request",
    "server_error",
    "max_output_tokens",
    "overloaded",
    "billing_error",
    "unknown",
];

/// Grok's snake_case `hookEventName` → the PascalCase name.
pub const GROK_SNAKE_EVENT_NAMES: [(&str, &str); 8] = [
    ("session_start", "SessionStart"),
    ("user_prompt_submit", "UserPromptSubmit"),
    ("pre_tool_use", "PreToolUse"),
    ("notification", "Notification"),
    ("stop", "Stop"),
    ("stop_failure", "StopFailure"),
    ("stop_cancelled", "StopCancelled"),
    ("session_end", "SessionEnd"),
];

/// The event names each hook-config client registers (a truncated payload of another
/// name is dropped, never guessed).
fn registered_event(client: ClientId, name: &str) -> bool {
    let names: &[&str] = match client {
        ClientId::ClaudeCode => &[
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "Notification",
            "Stop",
            "StopFailure",
            "SessionEnd",
        ],
        ClientId::Codex => &[
            "UserPromptSubmit",
            "PermissionRequest",
            "PostToolUse",
            "Stop",
            "Interrupt",
            "SessionEnd",
        ],
        ClientId::GrokCli => &[
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "Notification",
            "Stop",
            "StopFailure",
            "StopCancelled",
            "SessionEnd",
        ],
        ClientId::OpenCode | ClientId::Hermes => &[],
    };
    names.contains(&name)
}

/// `[A-Za-z0-9._:-]{1,128}`.
pub fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_ID_FIELD_BYTES
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
}

/// A ready-check nonce: 32 lowercase hex characters.
pub fn valid_nonce(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Compares in time independent of where the first difference is.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Normalizes one payload (§5.8). `client` is the argv id (authoritative). `nonce` is
/// called only when the payload carries `codepet_test` and returns the content of
/// `$STATE/test.nonce`, if any. `truncated` says the payload did not end within
/// [`MAX_HOOK_READ_BYTES`]: such an event becomes `activity` when its registered event
/// name and session id were found, otherwise it is dropped.
pub fn normalize(
    client: ClientId,
    p: &Payload,
    truncated: bool,
    nonce: impl FnOnce() -> Option<String>,
) -> Result<Normalized, DropReason> {
    if p.has("agent_id") || p.has("subagentType") {
        return Err(DropReason::Child);
    }
    let mut test = false;
    if let Some(t) = p.get("codepet_test") {
        let Field::Str(t) = t else {
            return Err(DropReason::TestNonce);
        };
        if !valid_nonce(t) {
            return Err(DropReason::TestNonce);
        }
        match nonce() {
            Some(n) if constant_time_eq(n.as_bytes(), t.as_bytes()) => test = true,
            _ => return Err(DropReason::TestNonce),
        }
    }
    // `ident`: the first present key decides; a present key with an invalid value is bad.
    let ident = |keys: &[&str]| -> Result<Option<String>, DropReason> {
        match keys.iter().find(|k| p.has(k)) {
            None => Ok(None),
            Some(k) => match p.str(k) {
                Some(v) if valid_id(v) => Ok(Some(v.to_owned())),
                _ => Err(DropReason::BadId),
            },
        }
    };
    if matches!(client, ClientId::OpenCode | ClientId::Hermes) {
        let kind = p.str("event").and_then(EventKind::parse);
        let version_ok = matches!(p.get("codepet_v"), Some(Field::Int(1)));
        let Some(mut kind) = kind.filter(|_| version_ok) else {
            return Err(DropReason::Unmapped);
        };
        let session = ident(&["session_id"])?;
        let turn = ident(&["turn_id"])?;
        let akey = if client == ClientId::Hermes {
            ident(&["approval_key"])?
        } else {
            None
        };
        if akey.is_some() {
            if session.is_some() {
                return Err(DropReason::BadId);
            }
            if !matches!(kind, EventKind::Waiting | EventKind::Activity) {
                return Err(DropReason::Unmapped);
            }
        } else if session.is_none() {
            return Err(DropReason::BadId);
        }
        let tools = match p.get("tools") {
            Some(Field::Int(n)) if (0..=1000).contains(n) => u32::try_from(*n).unwrap_or(0),
            _ => 0,
        };
        let interactive = match p.get("interactive") {
            Some(Field::Bool(b)) => *b,
            _ => true,
        };
        if truncated {
            kind = EventKind::Activity;
        }
        return Ok(Normalized {
            kind,
            session,
            turn,
            tool: None,
            class: None,
            akey,
            tools: if truncated { 0 } else { tools },
            interactive: Some(interactive),
            test,
        });
    }
    let name: Option<&str> = if client == ClientId::GrokCli {
        match p.get("hook_event_name") {
            Some(Field::Str(s)) => Some(s),
            None | Some(Field::Null) => p.str("hookEventName").and_then(|snake| {
                GROK_SNAKE_EVENT_NAMES
                    .iter()
                    .find(|(s, _)| *s == snake)
                    .map(|(_, pascal)| *pascal)
            }),
            Some(_) => None,
        }
    } else {
        p.str("hook_event_name")
    };
    let session = ident(&["session_id", "sessionId"])?;
    let turn = ident(&["prompt_id", "promptId", "turn_id"])?;
    let tool = ident(&["tool_use_id", "toolUseId"])?;
    let Some(session) = session else {
        return Err(DropReason::BadId);
    };
    let base = Normalized {
        kind: EventKind::Activity,
        session: Some(session),
        turn,
        tool: None,
        class: None,
        akey: None,
        tools: 0,
        interactive: None,
        test,
    };
    let Some(name) = name else {
        return Err(DropReason::Unmapped);
    };
    if truncated {
        // The fields that decide the kind may lie beyond the cut: record activity only.
        if !registered_event(client, name) {
            return Err(DropReason::Unmapped);
        }
        return Ok(Normalized { tool, ..base });
    }
    let note = p.first_str(&["notification_type", "notificationType"]);
    let tool_name = p.first_str(&["tool_name", "toolName"]);
    let error_class = || {
        let e = p.str("error").unwrap_or("unknown");
        ERROR_CLASSES
            .iter()
            .copied()
            .find(|c| *c == e)
            .unwrap_or("unknown")
    };
    let out = |kind: EventKind| Normalized {
        kind,
        ..base.clone()
    };
    let keyless = |kind: EventKind, class: Option<&'static str>| Normalized {
        kind,
        turn: None,
        class,
        ..base.clone()
    };
    let waiting_class = |n: Option<&str>, allowed: &[&'static str]| {
        n.and_then(|n| allowed.iter().copied().find(|a| *a == n))
    };
    let unmapped = Err(DropReason::Unmapped);
    match client {
        ClientId::ClaudeCode => match name {
            "SessionStart" => Ok(keyless(EventKind::SessionStart, None)),
            "UserPromptSubmit" => Ok(out(EventKind::TurnStart)),
            "PreToolUse" => Ok(Normalized {
                tool,
                ..out(EventKind::Activity)
            }),
            "Notification" => {
                const WAITING: [&str; 4] = [
                    "permission_prompt",
                    "elicitation_dialog",
                    "elicitation_url_dialog",
                    "agent_needs_input",
                ];
                if let Some(c) = waiting_class(note, &WAITING) {
                    Ok(keyless(EventKind::Waiting, Some(c)))
                } else if note == Some("idle_prompt") {
                    Ok(keyless(EventKind::IdlePrompt, Some("idle_prompt")))
                } else {
                    unmapped
                }
            }
            "Stop" => Ok(out(EventKind::TurnDone)),
            "StopFailure" => Ok(Normalized {
                class: Some(error_class()),
                ..out(EventKind::TurnFailed)
            }),
            "SessionEnd" => Ok(keyless(EventKind::SessionEnd, None)),
            _ => unmapped,
        },
        ClientId::Codex => match name {
            "UserPromptSubmit" => Ok(out(EventKind::TurnStart)),
            "PermissionRequest" => Ok(out(EventKind::Waiting)),
            "PostToolUse" => Ok(Normalized {
                tool,
                ..out(EventKind::Activity)
            }),
            "Stop" => Ok(out(EventKind::TurnDone)),
            "Interrupt" => Ok(out(EventKind::TurnCancelled)),
            "SessionEnd" => Ok(out(EventKind::SessionEnd)),
            _ => unmapped,
        },
        ClientId::GrokCli => match name {
            "SessionStart" => Ok(keyless(EventKind::SessionStart, None)),
            "UserPromptSubmit" => Ok(out(EventKind::TurnStart)),
            "PreToolUse" => {
                if matches!(tool_name, Some("AskUserQuestion" | "ask_user_question")) {
                    Ok(Normalized {
                        tool,
                        class: Some("ask_user"),
                        ..out(EventKind::Waiting)
                    })
                } else {
                    Ok(Normalized {
                        tool,
                        ..out(EventKind::Activity)
                    })
                }
            }
            "Notification" => match note {
                Some("permission_prompt") => {
                    Ok(keyless(EventKind::Waiting, Some("permission_prompt")))
                }
                Some("idle_prompt") => Ok(keyless(EventKind::IdlePrompt, Some("idle_prompt"))),
                _ => unmapped,
            },
            "Stop" => match p.str("reason") {
                Some("end_turn") => Ok(out(EventKind::TurnDone)),
                Some("channel_closed" | "shutdown") => Ok(keyless(EventKind::SessionEnd, None)),
                _ => unmapped,
            },
            "StopFailure" => Ok(Normalized {
                class: Some(error_class()),
                ..out(EventKind::TurnFailed)
            }),
            "StopCancelled" => Ok(out(EventKind::TurnCancelled)),
            "SessionEnd" => Ok(keyless(EventKind::SessionEnd, None)),
            _ => unmapped,
        },
        ClientId::OpenCode | ClientId::Hermes => unmapped,
    }
}

// ---- Interactivity (§6.4, agent_vectors.json "interactivity") ----------------------------

/// Grok tokens that mean a single-turn, headless run (`grok --help`, notes.md §1.2).
pub const GROK_HEADLESS_FLAGS: [&str; 6] = [
    "-p",
    "--single",
    "--prompt-file",
    "--prompt-json",
    "--output-format",
    "--json-schema",
];
/// `--flag=value` forms of the Grok headless flags.
pub const GROK_HEADLESS_PREFIXES: [&str; 5] = [
    "--single=",
    "--prompt-file=",
    "--prompt-json=",
    "--output-format=",
    "--json-schema=",
];
/// How far up the process tree the hook looks for the client (`comm`), excluding itself.
pub const ANCESTOR_DEPTH: usize = 4;

/// The `comm` of the process whose argv decides interactivity, for the clients that the
/// hook classifies from `/proc` (OpenCode and Hermes send it in the payload).
pub fn ancestor_comm(client: ClientId) -> Option<&'static str> {
    match client {
        ClientId::ClaudeCode => Some("claude"),
        ClientId::Codex => Some("codex"),
        ClientId::GrokCli => Some("grok"),
        ClientId::OpenCode | ClientId::Hermes => None,
    }
}

/// Positive headless evidence → `false`; no evidence (or `/proc` unreadable, `argv` =
/// `None`) → `true`. `entrypoint` is `CLAUDE_CODE_ENTRYPOINT` (Claude Code only);
/// `argv` is the nearest [`ancestor_comm`] process's argv, `argv[0]` included.
pub fn interactive(client: ClientId, entrypoint: Option<&str>, argv: Option<&[String]>) -> bool {
    let args = argv.map_or(&[][..], |a| a.get(1..).unwrap_or(&[]));
    match client {
        ClientId::ClaudeCode => match entrypoint {
            Some(ep) => !ep.starts_with("sdk"),
            None => !args.iter().any(|t| t == "-p" || t == "--print"),
        },
        ClientId::Codex => {
            !(args.first().is_some_and(|t| t == "e")
                || args.iter().any(|t| t == "exec" || t == "review"))
        }
        ClientId::GrokCli => {
            let flag = args.iter().any(|t| {
                GROK_HEADLESS_FLAGS.contains(&t.as_str())
                    || GROK_HEADLESS_PREFIXES.iter().any(|p| t.starts_with(p))
            });
            let pair = args
                .windows(2)
                .any(|w| w[0] == "agent" && w[1] == "headless");
            !(flag || pair)
        }
        ClientId::OpenCode | ClientId::Hermes => true,
    }
}

// ---- Treats (§6.4, "turn_qualifies" and "caps") -------------------------------------------

/// A finished turn qualifies for a treat (X2): the matching turn was open, it had a tool
/// call or lasted [`MIN_TURN_MS`], the session is interactive and it is not a test.
pub fn turn_qualifies(
    open: bool,
    tools: u32,
    duration_ms: i64,
    interactive: bool,
    test: bool,
) -> bool {
    open && (tools >= 1 || duration_ms >= MIN_TURN_MS) && interactive && !test
}

/// The cap verdict of a qualifying turn (X3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapVerdict {
    Allow,
    JarFull,
    CappedSession,
    CappedHour,
    CappedDay,
}
impl CapVerdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::JarFull => "jar_full",
            Self::CappedSession => "capped_session",
            Self::CappedHour => "capped_hour",
            Self::CappedDay => "capped_day",
        }
    }
}

/// Rolling hour of the hourly cap.
pub const HOUR_MS: i64 = 60 * 60 * 1000;
/// Rolling day of the daily cap.
pub const DAY_MS: i64 = 24 * HOUR_MS;

/// The marker of an earn time kept by "Forget coding history" (owner decision 2026-10-16,
/// `notes.md` §7 item 1): such a time is `ceil_hour(earned) + FORGOTTEN_MARK_MS`, so it is
/// never earlier than the real earn (every cap can only get stricter) and it is never on an
/// hour boundary, so retention can tell it from a time it coarsened itself and clears it
/// (sets 0) once it has left the [`DAY_MS`] window.
pub const FORGOTTEN_MARK_MS: i64 = 1;

/// The earn time "Forget coding history" keeps for a treat earned at `earned_ms`, as seen at
/// `now_ms`: rounded up to the end of its hour plus [`FORGOTTEN_MARK_MS`] while the earn
/// can still count toward a cap (`earned_ms > now_ms - DAY_MS`, the daily window is
/// half-open), else 0 (forgotten, never counts). 0 stays 0, and a kept time maps to itself,
/// so Forget is idempotent.
pub fn forgotten_earn_ms(earned_ms: i64, now_ms: i64) -> i64 {
    if earned_ms <= 0 || earned_ms <= now_ms.saturating_sub(DAY_MS) {
        return 0;
    }
    if earned_ms.rem_euclid(HOUR_MS) == FORGOTTEN_MARK_MS {
        return earned_ms;
    }
    let ceil = earned_ms
        .div_euclid(HOUR_MS)
        .saturating_add(i64::from(earned_ms.rem_euclid(HOUR_MS) != 0))
        .saturating_mul(HOUR_MS);
    ceil.saturating_add(FORGOTTEN_MARK_MS)
}

/// The `earned_at_ms` an ordinary earn at `at_ms` is stored with: at least 1 (0 means
/// forgotten), and never [`FORGOTTEN_MARK_MS`] past an hour, so only a time kept by Forget
/// carries the marker. Moving such an earn 1 ms later only makes the caps stricter.
pub fn ordinary_earn_ms(at_ms: i64) -> i64 {
    let t = at_ms.max(1);
    if t.rem_euclid(HOUR_MS) == FORGOTTEN_MARK_MS {
        t.saturating_add(1)
    } else {
        t
    }
}

/// The verdict from precomputed counts (the storage form): `session_recent` = this
/// session earned within [`CAP_SESSION_MS`]; `hour`/`day` = earns in the rolling windows.
/// Every window is half-open and forgotten rows (`earned_at_ms = 0`) never count; a time
/// kept by Forget ([`forgotten_earn_ms`]) counts like any other.
pub fn caps_verdict(jar: u32, session_recent: bool, hour: u32, day: u32) -> CapVerdict {
    if jar >= JAR_MAX {
        CapVerdict::JarFull
    } else if session_recent {
        CapVerdict::CappedSession
    } else if hour >= CAP_PER_HOUR {
        CapVerdict::CappedHour
    } else if day >= CAP_PER_DAY {
        CapVerdict::CappedDay
    } else {
        CapVerdict::Allow
    }
}

/// `caps_allow` over earn times (the vector form of [`caps_verdict`]).
pub fn caps_allow(at_ms: i64, jar: u32, session_earned: &[i64], all_earned: &[i64]) -> CapVerdict {
    let within = |e: i64, window: i64| e > 0 && e > at_ms - window;
    let count = |window: i64| {
        u32::try_from(all_earned.iter().filter(|e| within(**e, window)).count()).unwrap_or(u32::MAX)
    };
    caps_verdict(
        jar,
        session_earned.iter().any(|e| within(*e, CAP_SESSION_MS)),
        count(HOUR_MS),
        count(DAY_MS),
    )
}

// ---- The session machine (§6.4, "sessions") -----------------------------------------------

/// The open turn of a session. Keys are opaque (the fold passes HMACs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenTurn {
    pub key: String,
    pub started_ms: i64,
    /// Distinct tools counted so far (plus a reopened turn's earlier count).
    pub tools: u32,
}

/// The last finished turn of a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoneTurn {
    pub key: String,
    pub started_ms: i64,
    pub done_ms: i64,
    /// Counted tools plus the plugin's `tools` of its `turn_done`, plus late tools.
    pub tools: u32,
    pub earned: bool,
    /// The late-tool rule re-ran the treat check already.
    pub rechecked: bool,
}

/// One `agent_sessions` row (approval rows use [`approval_transition`] instead).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub state: SessionState,
    pub state_since_ms: i64,
    pub last_event_ms: i64,
    pub open: Option<OpenTurn>,
    pub done: Option<DoneTurn>,
    pub error_at_ms: Option<i64>,
}
impl Session {
    /// A new row created by an event at `at_ms`.
    pub fn new(at_ms: i64) -> Self {
        Self {
            state: SessionState::Idle,
            state_since_ms: at_ms,
            last_event_ms: at_ms,
            open: None,
            done: None,
            error_at_ms: None,
        }
    }
    fn set_state(&mut self, state: SessionState, at_ms: i64) {
        if self.state != state {
            self.state = state;
            self.state_since_ms = at_ms;
        }
    }
}

/// One event as the machine sees it.
#[derive(Clone, Copy, Debug)]
pub struct SessionEvent<'a> {
    pub kind: EventKind,
    pub at_ms: i64,
    /// The turn key. A `turn_start` always has one (the fold derives one for keyless
    /// starts, `~ev:<event_id>`).
    pub turn: Option<&'a str>,
    /// The tool key, if the event names a tool.
    pub tool: Option<&'a str>,
    /// The plugin's `tools` on `turn_done`.
    pub payload_tools: u32,
}

/// A treat check the caller must run (it owns the jar and the durable caps).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreatCheck {
    pub turn_key: String,
    pub tools: u32,
    pub duration_ms: i64,
    /// The time the caps are evaluated at (the turn's `turn_done` time).
    pub at_ms: i64,
    /// A late-tool re-check of the last finished turn (rule (b)).
    pub recheck: bool,
}

/// What one event did beyond the row itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Transition {
    /// The event's tool was counted toward this turn (a distinct new tool).
    pub counted_tool_turn: Option<String>,
    /// A treat check to run; on success the caller sets `done.earned`.
    pub treat: Option<TreatCheck>,
    /// A `turn_done` closed the matching turn (`finished_seq` for interactive sessions).
    pub finished: bool,
}

/// Applies one event to a session row (§6.4 table, closer matching, late-tool rules).
/// `seen(turn, tool, since_ms)` tells whether `tool` was already counted toward `turn` by
/// an event at or after `since_ms` (the fold answers it from `agent_events`).
pub fn apply_session_event(
    s: &mut Session,
    ev: &SessionEvent<'_>,
    seen: impl Fn(&str, &str, i64) -> bool,
) -> Transition {
    let at = ev.at_ms;
    let mut t = Transition::default();
    s.last_event_ms = s.last_event_ms.max(at);
    match ev.kind {
        EventKind::SessionStart | EventKind::IdlePrompt => s.set_state(SessionState::Idle, at),
        EventKind::Waiting => s.set_state(SessionState::Waiting, at),
        EventKind::TurnStart => {
            // A still-open turn closes neutrally: it is simply replaced.
            let key = ev.turn.unwrap_or_default().to_owned();
            s.open = Some(OpenTurn {
                key,
                started_ms: at,
                tools: 0,
            });
            s.set_state(SessionState::Working, at);
        }
        EventKind::Activity => {
            let open_matches = s
                .open
                .as_ref()
                .is_some_and(|o| ev.turn.is_none_or(|k| k == o.key));
            let late_new_tool = match (&s.done, ev.tool, ev.turn) {
                (Some(d), Some(tool), Some(key)) => {
                    key == d.key && !seen(&d.key, tool, d.started_ms)
                }
                _ => false,
            };
            if open_matches {
                let o = s.open.as_mut().expect("checked above");
                if let Some(tool) = ev.tool {
                    if !seen(&o.key, tool, o.started_ms) {
                        o.tools = o.tools.saturating_add(1);
                        t.counted_tool_turn = Some(o.key.clone());
                    }
                }
                if matches!(s.state, SessionState::Waiting | SessionState::Idle) {
                    s.set_state(SessionState::Working, at);
                }
            } else if late_new_tool {
                let d = s.done.as_mut().expect("checked above");
                t.counted_tool_turn = Some(d.key.clone());
                if at <= d.done_ms + LATE_TOOL_MS {
                    // (b) a late async tool event of the finished turn.
                    d.tools = d.tools.saturating_add(1);
                    if !d.earned && !d.rechecked {
                        d.rechecked = true;
                        t.treat = Some(TreatCheck {
                            turn_key: d.key.clone(),
                            tools: d.tools,
                            duration_ms: d.done_ms - d.started_ms,
                            at_ms: d.done_ms,
                            recheck: true,
                        });
                    }
                } else {
                    // (c) continuation (e.g. a Stop veto): the turn reopens.
                    s.open = Some(OpenTurn {
                        key: d.key.clone(),
                        started_ms: d.started_ms,
                        tools: d.tools.saturating_add(1),
                    });
                    s.set_state(SessionState::Working, at);
                }
            } else if s.open.is_none() && s.state == SessionState::Waiting {
                // (d) the question was answered and nothing known is running.
                s.set_state(SessionState::Idle, at);
            }
        }
        EventKind::TurnDone | EventKind::TurnFailed | EventKind::TurnCancelled => {
            let matches = s.open.as_ref().is_some_and(|o| match ev.turn {
                Some(k) => k == o.key,
                None => o.started_ms < at,
            });
            if matches {
                let o = s.open.take().expect("checked above");
                match ev.kind {
                    EventKind::TurnDone => {
                        s.set_state(SessionState::Idle, at);
                        t.finished = true;
                        let tools = o.tools.saturating_add(ev.payload_tools);
                        let prev_earned =
                            s.done.as_ref().is_some_and(|d| d.key == o.key && d.earned);
                        t.treat = Some(TreatCheck {
                            turn_key: o.key.clone(),
                            tools,
                            duration_ms: at - o.started_ms,
                            at_ms: at,
                            recheck: false,
                        });
                        s.done = Some(DoneTurn {
                            key: o.key,
                            started_ms: o.started_ms,
                            done_ms: at,
                            tools,
                            earned: prev_earned,
                            rechecked: false,
                        });
                    }
                    EventKind::TurnFailed => {
                        s.set_state(SessionState::Error, at);
                        s.error_at_ms = Some(at);
                    }
                    _ => s.set_state(SessionState::Idle, at),
                }
            }
        }
        EventKind::SessionEnd => {
            s.open = None;
            s.set_state(SessionState::Ended, at);
        }
    }
    t
}

/// A Hermes approval row (keyed by its approval key): `waiting` → waiting, `activity` →
/// idle; any other kind leaves it alone (`None`).
pub fn approval_transition(kind: EventKind) -> Option<SessionState> {
    match kind {
        EventKind::Waiting => Some(SessionState::Waiting),
        EventKind::Activity => Some(SessionState::Idle),
        _ => None,
    }
}

// ---- Fold outcomes (§6.4, "fold") ---------------------------------------------------------

/// `agent_events.outcome`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoldOutcome {
    Folded,
    DroppedOff,
    DroppedOld,
    DroppedChild,
    Test,
    DroppedTest,
    NoLife,
}
impl FoldOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Folded => "folded",
            Self::DroppedOff => "dropped_off",
            Self::DroppedOld => "dropped_old",
            Self::DroppedChild => "dropped_child",
            Self::Test => "test",
            Self::DroppedTest => "dropped_test",
            Self::NoLife => "no_life",
        }
    }
}

/// The fold's gate for one non-duplicate record, in the contract order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoldGate {
    /// Recorded with this outcome and nothing else changes.
    Stop(FoldOutcome),
    /// A real event: client record (+hello), then the session unless there is no life.
    Record { at_ms: i64 },
}

/// `enabled` = `meta.enabled`; `test_running` = `meta.test_state = 'running'`. A future
/// time beyond [`FUTURE_CLAMP_MS`] is clamped to `now_ms`.
pub fn fold_gate(
    enabled: bool,
    at_ms: i64,
    now_ms: i64,
    test: bool,
    test_running: bool,
) -> FoldGate {
    if !enabled {
        return FoldGate::Stop(FoldOutcome::DroppedOff);
    }
    if at_ms < now_ms - MAX_EVENT_AGE_MS {
        return FoldGate::Stop(FoldOutcome::DroppedOld);
    }
    let at_ms = if at_ms > now_ms + FUTURE_CLAMP_MS {
        now_ms
    } else {
        at_ms
    };
    if test {
        return FoldGate::Stop(if test_running {
            FoldOutcome::Test
        } else {
            FoldOutcome::DroppedTest
        });
    }
    FoldGate::Record { at_ms }
}

// ---- The pet-state aggregate (§6.4, "aggregate") ------------------------------------------

/// What `pet_state` reads of one `agent_sessions` row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRow {
    pub client: ClientId,
    pub approval: bool,
    pub interactive: bool,
    pub state: SessionState,
    pub state_since_ms: i64,
    pub last_event_ms: i64,
    pub error_at_ms: Option<i64>,
}

/// One entry of `agent.apps`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppStatus {
    pub client: ClientId,
    pub state: AppState,
    pub since_ms: i64,
    pub stale_at_ms: i64,
}

/// `agent.state`, `apps`, `stale_at_ms`, `error_at_ms`, `error_app` and `helpers_busy`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Aggregate {
    pub state: AppState,
    pub apps: Vec<AppStatus>,
    pub stale_at_ms: Option<i64>,
    pub error_at_ms: Option<i64>,
    pub error_app: Option<ClientId>,
    pub helpers_busy: bool,
}

/// The aggregate behind the export's `agent` block, from committed rows (pure).
pub fn pet_state(rows: &[SessionRow], count_helpers: bool, now_ms: i64) -> Aggregate {
    let expiry = |r: &SessionRow| -> Option<(AppState, i64)> {
        match r.state {
            SessionState::Working => Some((AppState::Working, r.last_event_ms + STALE_WORKING_MS)),
            SessionState::Waiting => Some((AppState::Waiting, r.last_event_ms + STALE_WAITING_MS)),
            SessionState::Error => r
                .error_at_ms
                .map(|e| (AppState::Error, e + ERROR_RECENT_MS)),
            SessionState::Idle | SessionState::Ended => None,
        }
    };
    let mut apps: Vec<AppStatus> = Vec::new();
    for client in ClientId::ALL {
        let counting: Vec<(AppState, i64, i64)> = rows
            .iter()
            .filter(|r| r.client == client && r.interactive)
            .filter_map(|r| expiry(r).map(|(st, x)| (st, x, r.state_since_ms)))
            .filter(|(_, x, _)| now_ms < *x)
            .collect();
        let Some(top) = counting.iter().map(|(st, _, _)| *st).max() else {
            continue;
        };
        let win = counting.iter().filter(|(st, _, _)| *st == top);
        let since_ms = win
            .clone()
            .map(|(_, _, since)| *since)
            .min()
            .unwrap_or(now_ms);
        let stale_at_ms = win.map(|(_, x, _)| *x).max().unwrap_or(now_ms);
        apps.push(AppStatus {
            client,
            state: top,
            since_ms,
            stale_at_ms,
        });
    }
    apps.sort_by_key(|a| (a.since_ms, a.client));
    let error = rows
        .iter()
        .filter(|r| r.interactive && !r.approval)
        .filter_map(|r| r.error_at_ms.map(|e| (e, r.client)))
        .filter(|(e, _)| now_ms < e + ERROR_RECENT_MS)
        .min_by_key(|(e, c)| (std::cmp::Reverse(*e), *c));
    let helpers_busy = count_helpers
        && rows.iter().any(|r| {
            !r.interactive
                && r.state == SessionState::Working
                && now_ms < r.last_event_ms + STALE_WORKING_MS
        });
    Aggregate {
        state: apps.iter().map(|a| a.state).max().unwrap_or(AppState::Idle),
        stale_at_ms: apps.iter().map(|a| a.stale_at_ms).min(),
        apps,
        error_at_ms: error.map(|(e, _)| e),
        error_app: error.map(|(_, c)| c),
        helpers_busy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const VECTORS: &str = include_str!("../../../docs/v0.3/contracts/agent_vectors.json");

    fn vectors() -> Value {
        serde_json::from_str(VECTORS).expect("agent_vectors.json parses")
    }

    #[test]
    fn forget_keeps_a_marked_hour_coarsened_earn_time_for_one_day() {
        let now = 1_792_123_876_111_i64;
        let hour_end = |t: i64| (t / HOUR_MS + 1) * HOUR_MS;
        // Within the day: rounded up to the end of its hour, plus the marker.
        let e = now - 10 * 60_000;
        let kept = forgotten_earn_ms(e, now);
        assert_eq!(kept, hour_end(e) + FORGOTTEN_MARK_MS);
        assert!(kept >= e && kept - e <= HOUR_MS);
        assert_eq!(kept.rem_euclid(HOUR_MS), FORGOTTEN_MARK_MS);
        // Idempotent: a kept time maps to itself; an exact hour gets only the marker.
        assert_eq!(forgotten_earn_ms(kept, now), kept);
        let on_hour = (now / HOUR_MS) * HOUR_MS;
        assert_eq!(forgotten_earn_ms(on_hour, now), on_hour + FORGOTTEN_MARK_MS);
        // Out of the (half-open) daily window, or already forgotten: 0.
        assert_eq!(forgotten_earn_ms(now - DAY_MS, now), 0);
        assert_eq!(
            forgotten_earn_ms(now - DAY_MS + 1, now),
            hour_end(now - DAY_MS + 1) + 1
        );
        assert_eq!(forgotten_earn_ms(0, now), 0);
        // A kept time never loosens a cap: every earn still counts where it counted before.
        for minutes in [0_i64, 1, 29, 59, 61, 600, 1439] {
            let e = now - minutes * 60_000;
            let k = forgotten_earn_ms(e, now);
            for at in [now, now + 20_000, now + HOUR_MS, now + DAY_MS - 1] {
                for window in [CAP_SESSION_MS, HOUR_MS, DAY_MS] {
                    if e > at - window {
                        assert!(k > at - window, "{minutes} min, window {window}");
                    }
                }
            }
        }
    }

    #[test]
    fn an_ordinary_earn_never_carries_the_forget_marker() {
        let hour = 1_792_123_876_111_i64 / HOUR_MS * HOUR_MS;
        // hh:00:00.001 would look kept by Forget: it moves 1 ms later (only stricter caps).
        assert_eq!(ordinary_earn_ms(hour + FORGOTTEN_MARK_MS), hour + 2);
        assert_ne!(
            ordinary_earn_ms(hour + FORGOTTEN_MARK_MS).rem_euclid(HOUR_MS),
            FORGOTTEN_MARK_MS
        );
        for t in [hour, hour + 2, hour + 59_999, hour + HOUR_MS - 1] {
            assert_eq!(ordinary_earn_ms(t), t);
        }
        assert_eq!(
            ordinary_earn_ms(0),
            2,
            "0 means forgotten, and 1 is the marker"
        );
        assert_eq!(ordinary_earn_ms(-5), 2);
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
        let state_kinds: Vec<&str> = v["event_ids"]["state_kinds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap())
            .collect();
        for k in EventKind::ALL {
            assert_eq!(
                k.is_state_change(),
                state_kinds.contains(&k.as_str()),
                "{k:?}"
            );
            let bucket = event_bucket(k.as_str(), 1_792_123_876_111);
            let want = if k.is_state_change() {
                1_792_123_876_111
            } else {
                1_792_123_876_111 / EVENT_BUCKET_MS
            };
            assert_eq!(bucket, want, "{k:?}");
        }
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

    // ---- WP2 vector replays ---------------------------------------------------------

    pub(crate) fn payload_of(v: &Value) -> Payload {
        let mut p = Payload::default();
        for (k, v) in v.as_object().expect("payload is an object") {
            let f = match v {
                Value::String(s) if s.len() <= MAX_ID_FIELD_BYTES => Field::Str(s.clone()),
                Value::Number(n) if n.is_i64() => Field::Int(n.as_i64().unwrap()),
                Value::Bool(b) => Field::Bool(*b),
                Value::Null => Field::Null,
                _ => Field::Other,
            };
            p.set(k, f);
        }
        p
    }

    fn opt_str(v: &Value) -> Option<String> {
        v.as_str().map(str::to_owned)
    }

    #[test]
    fn normalization_vectors() {
        let v = vectors();
        let vecs = v["normalization"]["vectors"].as_array().unwrap();
        assert!(vecs.len() >= 70);
        for vec in vecs {
            let client = ClientId::parse(vec["client"].as_str().unwrap()).unwrap();
            let nonce = opt_str(&vec["nonce"]);
            let got = normalize(client, &payload_of(&vec["payload"]), false, || nonce);
            let expect = vec["expect"].as_object().unwrap();
            let ctx = format!("{client:?} {}", vec["payload"]);
            match got {
                Err(reason) => {
                    assert_eq!(
                        expect.get("drop").and_then(Value::as_str),
                        Some(reason.as_str()),
                        "{ctx}"
                    );
                }
                Ok(n) => {
                    assert!(!expect.contains_key("drop"), "{ctx}: kept as {n:?}");
                    for (k, want) in expect {
                        let ok = match k.as_str() {
                            "kind" => want == n.kind.as_str(),
                            "session" => opt_str(want) == n.session,
                            "turn" => opt_str(want) == n.turn,
                            "tool" => opt_str(want) == n.tool,
                            "class" => want.as_str() == n.class,
                            "akey" => opt_str(want) == n.akey,
                            "tools" => want.as_u64() == Some(u64::from(n.tools)),
                            "interactive" => want.as_bool() == n.interactive,
                            "test" => want.as_bool() == Some(n.test),
                            other => panic!("{ctx}: unknown expect key {other}"),
                        };
                        assert!(ok, "{ctx}: {k} = {n:?}, expected {want}");
                    }
                }
            }
        }
    }

    #[test]
    fn truncated_payloads_become_activity_or_drop() {
        let mut p = Payload::default();
        p.set("hook_event_name", Field::Str("Stop".into()));
        p.set("session_id", Field::Str("s1".into()));
        p.set("prompt_id", Field::Str("p1".into()));
        let n = normalize(ClientId::ClaudeCode, &p, true, || None).unwrap();
        assert_eq!(
            (n.kind, n.turn.as_deref()),
            (EventKind::Activity, Some("p1"))
        );
        // An unregistered name or a missing session is dropped, never guessed.
        let mut q = p.clone();
        q.set("hook_event_name", Field::Str("SubagentStop".into()));
        assert_eq!(
            normalize(ClientId::ClaudeCode, &q, true, || None),
            Err(DropReason::Unmapped)
        );
        let mut r = Payload::default();
        r.set("hook_event_name", Field::Str("PreToolUse".into()));
        assert_eq!(
            normalize(ClientId::ClaudeCode, &r, true, || None),
            Err(DropReason::BadId)
        );
        // The child rule still runs first.
        let mut c = p.clone();
        c.set("agent_id", Field::Other);
        assert_eq!(
            normalize(ClientId::ClaudeCode, &c, true, || None),
            Err(DropReason::Child)
        );
    }

    #[test]
    fn the_nonce_is_read_only_for_test_payloads() {
        let mut p = Payload::default();
        p.set("hook_event_name", Field::Str("Stop".into()));
        p.set("session_id", Field::Str("s1".into()));
        let n = normalize(ClientId::ClaudeCode, &p, false, || panic!("read the nonce"));
        assert!(!n.unwrap().test);
        // Keys outside the allow-list are never stored, even if set.
        p.set("prompt", Field::Str("secret".into()));
        assert!(p.get("prompt").is_none());
    }

    #[test]
    fn interactivity_vectors() {
        let v = vectors();
        let mut ran = 0;
        for vec in v["interactivity"]["vectors"].as_array().unwrap() {
            let client = ClientId::parse(vec["client"].as_str().unwrap()).unwrap();
            let got = match client {
                // The Hermes plugin decides (WP3's Python tests replay these).
                ClientId::Hermes => continue,
                ClientId::OpenCode => vec["payload_interactive"].as_bool() != Some(false),
                _ => {
                    let argv: Option<Vec<String>> = vec["ancestor_argv"]
                        .as_array()
                        .map(|a| a.iter().map(|t| t.as_str().unwrap().to_owned()).collect());
                    let ep = vec["env"]["CLAUDE_CODE_ENTRYPOINT"].as_str();
                    interactive(client, ep, argv.as_deref())
                }
            };
            assert_eq!(Value::Bool(got), vec["expect"], "{vec}");
            ran += 1;
        }
        assert!(ran >= 25);
        let rules = &v["interactivity"];
        let flags: Vec<&str> = rules["grok_headless_flags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f.as_str().unwrap())
            .collect();
        assert_eq!(flags, GROK_HEADLESS_FLAGS);
        let prefixes: Vec<&str> = rules["grok_headless_prefixes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f.as_str().unwrap())
            .collect();
        assert_eq!(prefixes, GROK_HEADLESS_PREFIXES);
    }

    #[test]
    fn treat_vectors() {
        let v = vectors();
        for vec in v["turn_qualifies"]["vectors"].as_array().unwrap() {
            let got = turn_qualifies(
                vec["open"].as_bool().unwrap(),
                u32::try_from(vec["tools"].as_u64().unwrap()).unwrap(),
                vec["duration_ms"].as_i64().unwrap(),
                vec["interactive"].as_bool().unwrap(),
                vec["test"].as_bool().unwrap(),
            );
            assert_eq!(Value::Bool(got), vec["expect"], "{vec}");
        }
        let times = |a: &Value| -> Vec<i64> {
            a.as_array()
                .unwrap()
                .iter()
                .map(|t| t.as_i64().unwrap())
                .collect()
        };
        for vec in v["caps"]["vectors"].as_array().unwrap() {
            let got = caps_allow(
                vec["at"].as_i64().unwrap(),
                u32::try_from(vec["jar"].as_u64().unwrap()).unwrap(),
                &times(&vec["session_earned"]),
                &times(&vec["all_earned"]),
            );
            assert_eq!(got.as_str(), vec["expect"], "{}", vec["name"]);
        }
    }

    /// The reference `World` of check_contracts.py over the pure machine: an in-memory
    /// jar, earn list and counted-tool log stand in for the database.
    struct World {
        jar: u32,
        earned: Vec<(String, i64)>,
        earned_keys: std::collections::HashSet<(String, String)>,
        finished_seq: u32,
        finished_jar_full: bool,
        sessions: std::collections::HashMap<String, Session>,
        approvals: std::collections::BTreeMap<String, SessionState>,
        counted: Vec<(String, String, String, i64)>,
    }
    impl World {
        fn new(jar: u32) -> Self {
            Self {
                jar,
                earned: Vec::new(),
                earned_keys: Default::default(),
                finished_seq: 0,
                finished_jar_full: false,
                sessions: Default::default(),
                approvals: Default::default(),
                counted: Vec::new(),
            }
        }
        fn treat(&mut self, sid: &str, check: &TreatCheck, inter: bool) -> bool {
            if self
                .earned_keys
                .contains(&(sid.to_owned(), check.turn_key.clone()))
                || !turn_qualifies(true, check.tools, check.duration_ms, inter, false)
            {
                return false;
            }
            let session: Vec<i64> = self
                .earned
                .iter()
                .filter(|(s, _)| s == sid)
                .map(|(_, e)| *e)
                .collect();
            let all: Vec<i64> = self.earned.iter().map(|(_, e)| *e).collect();
            let verdict = caps_allow(check.at_ms, self.jar, &session, &all);
            self.finished_jar_full = verdict == CapVerdict::JarFull;
            if verdict != CapVerdict::Allow {
                return false;
            }
            self.earned.push((sid.to_owned(), check.at_ms));
            self.earned_keys
                .insert((sid.to_owned(), check.turn_key.clone()));
            self.jar += 1;
            true
        }
        fn apply(&mut self, sid: &str, inter: bool, e: &Value) -> bool {
            let kind = EventKind::parse(e["kind"].as_str().unwrap()).unwrap();
            if let Some(akey) = e["akey"].as_str() {
                if let Some(st) = approval_transition(kind) {
                    self.approvals.insert(akey.to_owned(), st);
                }
                return false;
            }
            let at = e["at"].as_i64().unwrap();
            let ev = SessionEvent {
                kind,
                at_ms: at,
                turn: e["turn"].as_str(),
                tool: e["tool"].as_str(),
                payload_tools: u32::try_from(e["tools"].as_u64().unwrap_or(0)).unwrap(),
            };
            let counted = &self.counted;
            let s = self
                .sessions
                .entry(sid.to_owned())
                .or_insert_with(|| Session::new(at));
            let t = apply_session_event(s, &ev, |turn, tool, since| {
                counted.iter().any(|(cs, ct, ctool, cat)| {
                    cs == sid && ct == turn && ctool == tool && *cat >= since
                })
            });
            if let Some(turn) = &t.counted_tool_turn {
                self.counted.push((
                    sid.to_owned(),
                    turn.clone(),
                    ev.tool.unwrap().to_owned(),
                    at,
                ));
            }
            if t.finished && inter {
                self.finished_seq += 1;
                self.finished_jar_full = false;
            }
            let mut earned = false;
            if let Some(check) = &t.treat {
                if self.treat(sid, check, inter) {
                    earned = true;
                    let d = self.sessions.get_mut(sid).unwrap().done.as_mut().unwrap();
                    d.earned = true;
                }
            }
            earned
        }
    }

    #[test]
    fn session_vectors() {
        let v = vectors();
        let vecs = v["sessions"]["vectors"].as_array().unwrap();
        assert!(vecs.len() >= 20);
        for vec in vecs {
            let name = vec["name"].as_str().unwrap();
            let mut w = World::new(u32::try_from(vec["jar"].as_u64().unwrap_or(0)).unwrap());
            let sid = vec["session"].as_str().unwrap_or("s1");
            let inter = vec["interactive"].as_bool().unwrap_or(true);
            for (i, e) in vec["events"].as_array().unwrap().iter().enumerate() {
                let got = w.apply(sid, inter, e);
                let where_ = format!("{name} event {i}");
                if let Some(expect) = e["expect"].as_str() {
                    let st = w.sessions.get(sid).map_or("idle", |s| s.state.as_str());
                    assert_eq!(st, expect, "{where_}: state");
                }
                assert_eq!(
                    got,
                    e["earned"].as_bool().unwrap_or(false),
                    "{where_}: earned"
                );
                if let Some(want) = e["expect_approval"].as_str() {
                    let akey = e["akey"].as_str().unwrap();
                    assert_eq!(w.approvals[akey].as_str(), want, "{where_}: approval");
                }
            }
            let f = &vec["final"];
            let s = w.sessions.get(sid);
            assert_eq!(
                w.earned.len() as u64,
                f["treats"].as_u64().unwrap(),
                "{name}"
            );
            assert_eq!(
                u64::from(w.finished_seq),
                f["finished_seq"].as_u64().unwrap(),
                "{name}"
            );
            let open = s.and_then(|s| s.open.as_ref()).map(|o| o.key.as_str());
            assert_eq!(open, f["open_turn"].as_str(), "{name}: open_turn");
            let done_tools = s.and_then(|s| s.done.as_ref()).map_or(0, |d| d.tools);
            assert_eq!(
                u64::from(done_tools),
                f["last_done_tools"].as_u64().unwrap(),
                "{name}: last_done_tools"
            );
            if let Some(err) = f.get("error_at_ms") {
                assert_eq!(s.and_then(|s| s.error_at_ms), err.as_i64(), "{name}");
            }
            if let Some(rows) = f.get("approval_rows") {
                for (k, want) in rows.as_object().unwrap() {
                    assert_eq!(Some(w.approvals[k].as_str()), want.as_str(), "{name}");
                }
            }
            if let Some(full) = f.get("finished_jar_full") {
                assert_eq!(Some(w.finished_jar_full), full.as_bool(), "{name}");
            }
        }
    }

    #[test]
    fn fold_vectors() {
        let v = vectors();
        let now = v["fold"]["now"].as_i64().unwrap();
        for vec in v["fold"]["vectors"].as_array().unwrap() {
            let name = vec["name"].as_str().unwrap();
            let e = &vec["event"];
            let test = e["test"].as_bool().unwrap_or(false);
            let running = vec["test_state"].as_str() == Some("running");
            let gate = fold_gate(
                vec["enabled"].as_bool().unwrap(),
                e["at"].as_i64().unwrap(),
                now,
                test,
                running,
            );
            let (outcome, record, hello, at) = match gate {
                FoldGate::Stop(o) => (o, false, false, None),
                FoldGate::Record { at_ms } => {
                    let o = if vec["life"].as_bool().unwrap() {
                        FoldOutcome::Folded
                    } else {
                        FoldOutcome::NoLife
                    };
                    (
                        o,
                        true,
                        !vec["first_seen"].as_bool().unwrap_or(false),
                        Some(at_ms),
                    )
                }
            };
            let x = &vec["expect"];
            assert_eq!(outcome.as_str(), x["outcome"], "{name}");
            assert_eq!(Value::Bool(record), x["client_record"], "{name}");
            assert_eq!(Value::Bool(hello), x["hello"], "{name}");
            if let Some(want) = x.get("at") {
                assert_eq!(at, want.as_i64(), "{name}");
            }
        }
    }

    pub(crate) fn session_row_of(r: &Value) -> SessionRow {
        SessionRow {
            client: ClientId::parse(r["client"].as_str().unwrap()).unwrap(),
            approval: r["approval"].as_i64().unwrap() == 1,
            interactive: r["interactive"].as_i64().unwrap() == 1,
            state: SessionState::parse(r["state"].as_str().unwrap()).unwrap(),
            state_since_ms: r["state_since_ms"].as_i64().unwrap(),
            last_event_ms: r["last_event_ms"].as_i64().unwrap(),
            error_at_ms: r["error_at_ms"].as_i64(),
        }
    }

    #[test]
    fn aggregate_vectors() {
        let v = vectors();
        let vecs = v["aggregate"]["vectors"].as_array().unwrap();
        assert!(vecs.len() >= 10);
        for vec in vecs {
            let rows: Vec<SessionRow> = vec["sessions"]
                .as_array()
                .unwrap()
                .iter()
                .map(session_row_of)
                .collect();
            let a = pet_state(
                &rows,
                vec["count_helpers"].as_bool().unwrap(),
                vec["now"].as_i64().unwrap(),
            );
            let got = serde_json::json!({
                "state": a.state.as_str(),
                "apps": a.apps.iter().map(|x| serde_json::json!({
                    "id": x.client.as_str(),
                    "state": x.state.as_str(),
                    "since_ms": x.since_ms,
                    "stale_at_ms": x.stale_at_ms,
                })).collect::<Vec<_>>(),
                "stale_at_ms": a.stale_at_ms,
                "error_at_ms": a.error_at_ms,
                "error_app": a.error_app.map(ClientId::as_str),
                "helpers_busy": a.helpers_busy,
            });
            assert_eq!(got, vec["expect"], "{}", vec["name"]);
        }
    }

    #[test]
    fn allow_list_matches_the_spec() {
        // §5.8 minus completed/failed/interrupted (notes.md §2 item 21); `source` and
        // `stop_hook_active` are allowed by the spec but never needed, so never read.
        for k in [
            "prompt",
            "cwd",
            "transcript_path",
            "tool_input",
            "message",
            "model",
        ] {
            assert!(!ALLOWED_KEYS.contains(&k), "{k}");
        }
        let v = vectors();
        let errors: Vec<&str> = ERROR_CLASSES.to_vec();
        assert_eq!(errors.len(), 8);
        let snake = v["normalization"]["grok_snake_event_names"]
            .as_object()
            .unwrap();
        assert_eq!(snake.len(), GROK_SNAKE_EVENT_NAMES.len());
        for (s, pascal) in GROK_SNAKE_EVENT_NAMES {
            assert_eq!(snake[s], pascal);
        }
    }
}
