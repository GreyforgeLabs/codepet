//! The v0.3 blocks of `bar-state.json` (spec §8, `docs/v0.3/contracts/bar.v2.schema.json`):
//! `enabled`, `agent`, `treats` and `clients`.
//!
//! WP0 serialises them with defaults (CodePet on, nothing seen, empty jar, no client
//! records). WP1 fills `enabled` and `treats`, WP2 fills `agent`, WP3 fills `clients`, each
//! from committed rows and without settling anything.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// `agent`: what the connected coding apps are doing, for the bar, Home and the overlay.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentExport {
    /// The highest app state (`idle` < `error` < `working` < `waiting`) over counting
    /// sessions; `idle` when none count.
    pub state: String,
    /// One entry per app with a counting session, sorted by `since_ms`, then client order.
    pub apps: Vec<AppExport>,
    /// Earliest `stale_at_ms` of `apps`; the UI arms one single-shot timer for it.
    pub stale_at_ms: Option<i64>,
    pub finished_seq: i64,
    pub finished_at_ms: Option<i64>,
    pub finished_app: Option<String>,
    /// The last finished turn earned a treat.
    pub finished_treat: bool,
    /// The last finished turn found the jar full.
    pub finished_jar_full: bool,
    pub hello_seq: i64,
    pub hello_at_ms: Option<i64>,
    pub hello_app: Option<String>,
    /// The latest ready check, or `null` before the first one.
    pub test: Option<TestExport>,
    pub error_at_ms: Option<i64>,
    pub error_app: Option<String>,
    /// A non-interactive session is working and "Count background helpers" is on.
    pub helpers_busy: bool,
}
impl Default for AgentExport {
    fn default() -> Self {
        Self {
            state: "idle".to_owned(),
            apps: Vec::new(),
            stale_at_ms: None,
            finished_seq: 0,
            finished_at_ms: None,
            finished_app: None,
            finished_treat: false,
            finished_jar_full: false,
            hello_seq: 0,
            hello_at_ms: None,
            hello_app: None,
            test: None,
            error_at_ms: None,
            error_app: None,
            helpers_busy: false,
        }
    }
}

/// One app in `agent.apps`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppExport {
    /// A supported client id.
    pub id: String,
    /// `working`, `waiting` or `error` (an `idle` app is omitted).
    pub state: String,
    pub since_ms: i64,
    pub stale_at_ms: i64,
}

/// `agent.test`: the ready check (`clients.test`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestExport {
    /// `running`, `done` or `failed`.
    pub state: String,
    pub at_ms: i64,
}

/// `treats`: the jar of the living life.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreatsExport {
    /// Unspent treats, `0..=10`.
    pub jar: u32,
    /// `jar` has reached the cap.
    pub full: bool,
}

/// `clients`: one entry per client CodePet has a record for, keyed by client id.
pub type ClientsExport = BTreeMap<String, ClientExport>;

/// One client in `clients`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientExport {
    pub connected: bool,
    /// `codex_trust` or `codex_hooks_off`, else `null`.
    pub needs_step: Option<String>,
    pub last_seen_ms: Option<i64>,
    /// First real event since the last Connect; `null` means "Waiting for {app}".
    pub first_seen_ms: Option<i64>,
    /// Reason of the last failed Connect (`not_installed`, `too_old`, `blocked`,
    /// `settings_changed`, `ready_check`), else `null`.
    pub failed_reason: Option<String>,
    /// The running job for this client, else `null`.
    pub job: Option<JobExport>,
}

/// `clients.<id>.job`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobExport {
    pub id: String,
    /// `connect`, `disconnect` or `test`.
    pub kind: String,
    /// `running` (finished jobs are reflected in the client record instead).
    pub state: String,
}
