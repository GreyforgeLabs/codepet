// Task-pipeline helpers stay for the `legacy-tasks` build (deleted in v0.3.1); the default
// build compiles them without callers.
#![cfg_attr(not(feature = "legacy-tasks"), allow(dead_code))]

use super::*;
use codepet_core::{Category, Draft, Provider, State};
#[cfg(feature = "legacy-tasks")]
use codepet_core::{Settings, Verification};
use rusqlite::Connection;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;

const T0: i64 = 1_700_000_000_000;
const ARTIFACT: &str = "Mock explanation artifact";

struct Env {
    _tmp: TempDir,
    root: PathBuf,
    home: PathBuf,
    project: PathBuf,
    paths: StatePaths,
    clock: Arc<ManualClock>,
}
impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let home = root.join("home");
        let project = root.join("project");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&project).unwrap();
        let xdg = root.join("state");
        let paths = StatePaths::resolve(|k| match k {
            "XDG_STATE_HOME" => Some(xdg.clone().into()),
            "HOME" => Some(home.clone().into()),
            _ => None,
        })
        .unwrap();
        Self {
            _tmp: tmp,
            root,
            home,
            project,
            paths,
            clock: Arc::new(ManualClock::new(T0)),
        }
    }
    fn open(&self) -> Store {
        Store::open(&self.paths, self.clock.clone()).unwrap()
    }
    fn try_open(&self) -> Result<Store> {
        Store::open(&self.paths, self.clock.clone())
    }
    fn input(&self, category: Category) -> TaskInput {
        self.input_at(category, &self.project)
    }
    fn input_at(&self, category: Category, folder: &Path) -> TaskInput {
        TaskInput {
            draft: Draft {
                category,
                provider: Provider::Mock,
                folder: folder.to_str().unwrap().to_owned(),
                scope: vec!["src".into()],
                instructions: "Explain the task state machine".into(),
            },
            mock: MockOptions::default(),
        }
    }
    fn raw(&self) -> Connection {
        Connection::open(&self.paths.db_path).unwrap()
    }
    fn count(&self, sql: &str) -> i64 {
        self.raw().query_row(sql, [], |r| r.get(0)).unwrap()
    }
}

fn sup() -> Supervisor {
    Supervisor::current().unwrap()
}
fn dead_sup() -> Supervisor {
    let s = sup();
    Supervisor {
        pid: s.pid,
        start_time: s.start_time + 1,
    }
}

#[cfg(feature = "legacy-tasks")]
fn make_ready(store: &mut Store, env: &Env) {
    let state = store.snapshot().unwrap();
    if state.care.sleep_due && state.active_task.is_none() {
        store.sleep(&sys::uuid_v4().unwrap()).unwrap();
        env.clock.advance(SLEEP_MS);
        store.snapshot().unwrap();
    }
    store.care(&sys::uuid_v4().unwrap(), "routine").unwrap();
}

#[cfg(feature = "legacy-tasks")]
fn ready(store: &mut Store, env: &Env, category: Category) -> (String, i64, String) {
    make_ready(store, env);
    let t = store.create_task(&env.input(category)).unwrap();
    let p = store.preflight(&t.task_id, t.revision).unwrap();
    (t.task_id, t.revision, p.preflight_token)
}
#[cfg(feature = "legacy-tasks")]
fn started(store: &mut Store, env: &Env, category: Category, s: Supervisor) -> String {
    let (id, rev, token) = ready(store, env, category);
    let r = store.start(&id, rev, &token, s).unwrap();
    assert_eq!(r.state, State::Preparing);
    id
}
#[cfg(feature = "legacy-tasks")]
fn reviewed(
    store: &mut Store,
    env: &Env,
    category: Category,
    verification: Verification,
) -> (String, String) {
    let id = started(store, env, category, sup());
    let s = sup();
    assert!(matches!(
        store
            .supervisor_step(&id, s, State::Preparing, State::Running)
            .unwrap(),
        Step::Advanced { .. }
    ));
    store
        .supervisor_step(&id, s, State::Running, State::Verifying)
        .unwrap();
    let simulated = verification != Verification::Passed;
    match store
        .seal_review(&id, s, ARTIFACT, verification, simulated)
        .unwrap()
    {
        SealOutcome::Sealed { digest, .. } => (id, digest),
        SealOutcome::Cancelling => panic!("unexpected cancel"),
    }
}
#[cfg(feature = "legacy-tasks")]
fn accept(
    store: &mut Store,
    id: &str,
    digest: &str,
    request_id: &str,
    reason: Option<&str>,
) -> Result<AcceptOutcome> {
    store.accept_artifact(&AcceptRequest {
        request_id,
        task_id: id,
        review_digest: digest,
        acknowledged: true,
        override_reason: reason,
    })
}
fn meters(store: &Store) -> MetersView {
    store.snapshot().unwrap().meters
}
fn keys(v: &Value) -> BTreeSet<&str> {
    v.as_object().unwrap().keys().map(String::as_str).collect()
}
fn set(k: &[&'static str]) -> BTreeSet<&'static str> {
    k.iter().copied().collect()
}
fn mode(p: &Path) -> u32 {
    fs::symlink_metadata(p).unwrap().permissions().mode() & 0o7777
}

#[test]
fn fresh_open_has_defaults_and_safe_modes() {
    let env = Env::new();
    let store = env.open();
    let s = store.snapshot().unwrap();
    assert_eq!(
        s.meters,
        MetersView {
            hunger: 7000,
            thirst: 7000,
            mental_health: 7000,
            hygiene: 7000
        }
    );
    #[cfg(feature = "legacy-tasks")]
    {
        assert_eq!(
            s.decay,
            DecayView {
                enabled: false,
                paused: false
            }
        );
        let d = Settings::default();
        assert_eq!(s.settings.revision, 1);
        assert_eq!(s.settings.decay_enabled, d.decay_enabled);
        assert_eq!(s.settings.decay_per_hour, d.decay_per_hour);
        assert_eq!(s.settings.timeout_seconds, d.timeout_seconds);
        assert!(s.active_task.is_none() && s.recent.is_empty());
        assert_eq!(s.providers.len(), 5);
        assert!(s
            .providers
            .iter()
            .all(|p| p.available == (p.id == Provider::Mock)));
    }
    assert_eq!(
        (s.schema_version, s.protocol_version, s.last_seq),
        (SCHEMA_VERSION, 1, 0)
    );
    assert_eq!(mode(&env.paths.state_dir), 0o700);
    assert_eq!(mode(&env.paths.db_path), 0o600);
    assert_eq!(env.paths.state_dir, env.root.join("state/codepet"));
    assert_eq!(env.count("PRAGMA user_version"), SCHEMA_VERSION);
    let journal: String = env
        .raw()
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(journal, "wal");
    assert_eq!(env.count("SELECT count(*) FROM apply_transactions"), 0);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn snapshot_json_shape_matches_contract() {
    let env = Env::new();
    let mut store = env.open();
    let v = serde_json::to_value(store.snapshot().unwrap()).unwrap();
    assert_eq!(
        keys(&v),
        set(&[
            "schema_version",
            "state_version",
            "db_id",
            "protocol_version",
            "last_seq",
            "meters",
            "care",
            "progression",
            "decay",
            "settings",
            "active_task",
            "recent",
            "providers",
            "life",
            "lifecycle",
            "unseen_events",
            "memorial"
        ])
    );
    assert_eq!(
        keys(&v["meters"]),
        set(&["hunger", "thirst", "mental_health", "hygiene"])
    );
    assert_eq!(keys(&v["decay"]), set(&["enabled", "paused"]));
    assert_eq!(
        v["settings"],
        serde_json::json!({"revision": 1, "decay_enabled": false,
            "decay_per_hour": [100, 100, 50, 50], "timeout_seconds": 1800})
    );
    assert!(v["active_task"].is_null());
    assert_eq!(
        v["providers"][0],
        serde_json::json!({"id": "mock", "label": "Mock", "available": true,
            "note": "Simulation only; runs no command"})
    );
    assert_eq!(
        v["providers"][1],
        serde_json::json!({"id": "codex", "label": "Codex", "available": false,
            "note": "Not yet supported on this version"})
    );
    let ids: Vec<_> = v["providers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["label"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["Mock", "Codex", "OpenCode", "Hermes", "Claude Code"]);

    let (id, digest) = reviewed(
        &mut store,
        &env,
        Category::MentalHealth,
        Verification::NotVerified,
    );
    let v = serde_json::to_value(store.snapshot().unwrap()).unwrap();
    // state.get lists task briefs; the full summary comes from task.get / task.review.
    let task = &v["active_task"];
    assert_eq!(
        keys(task),
        set(&[
            "task_id",
            "revision",
            "state",
            "category",
            "provider",
            "mock",
            "instructions_excerpt",
            "instructions_bytes",
            "instructions_sha256",
            "created_at_ms",
            "updated_at_ms",
            "review",
            "receipt"
        ])
    );
    assert_eq!(task["state"], "review_ready");
    assert_eq!(task["category"], "mental_health");
    assert_eq!(
        task["mock"],
        serde_json::json!({"outcome": "artifact", "step_ms": 0})
    );
    assert_eq!(
        task["instructions_excerpt"],
        "Explain the task state machine"
    );
    assert_eq!(task["instructions_bytes"], 30);
    assert_eq!(
        task["instructions_sha256"],
        sys::sha256_hex(b"Explain the task state machine")
    );
    assert_eq!(
        task["review"],
        serde_json::json!({"digest": digest, "verification": "not_verified",
            "metadata_repaired": false, "simulated": true, "artifact_bytes": ARTIFACT.len()})
    );
    assert!(task["receipt"].is_null());
    assert_eq!(v["recent"].as_array().unwrap().len(), 1);
    assert_eq!(v["recent"][0], *task);
    let full = serde_json::to_value(store.task(&id).unwrap()).unwrap();
    assert_eq!(
        keys(&full),
        set(&[
            "task_id",
            "revision",
            "state",
            "category",
            "provider",
            "folder",
            "scope",
            "instructions",
            "mock",
            "created_at_ms",
            "updated_at_ms",
            "review",
            "receipt"
        ])
    );
    assert_eq!(
        full["review"],
        serde_json::json!({"digest": digest, "verification": "not_verified",
            "metadata_repaired": false, "artifact": ARTIFACT, "simulated": true})
    );

    let out = accept(&mut store, &id, &digest, "req-1", Some("useful")).unwrap();
    let a = serde_json::to_value(&out).unwrap();
    assert_eq!(keys(&a), set(&["receipt", "duplicate"]));
    let life_id = store.snapshot().unwrap().life.unwrap().life_id;
    assert_eq!(
        a["receipt"],
        serde_json::json!({"task_id": id, "category": "mental_health",
            "amount_hundredths": 0, "demo_xp": 20, "real_xp": 0,
            "progression_track": "learning_specification", "verification": "not_verified",
            "override_reason": "useful", "request_id": "req-1", "accepted_at_ms": T0,
            "life_id": life_id, "level_before": 1, "level_after": 1, "evolved_to": [],
            "xp_withheld": null})
    );
    let v = serde_json::to_value(store.snapshot().unwrap()).unwrap();
    assert!(v["active_task"].is_null());
    assert_eq!(v["recent"][0]["receipt"], a["receipt"]);
    assert_eq!(v["meters"]["mental_health"], 9500);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn reopen_persists_meters_tasks_and_settings() {
    let env = Env::new();
    let id = {
        let mut store = env.open();
        let (id, digest) = reviewed(&mut store, &env, Category::Hygiene, Verification::Failed);
        accept(
            &mut store,
            &id,
            &digest,
            "r",
            Some("reviewed failing check"),
        )
        .unwrap();
        store.set_paused(true).unwrap();
        let s = Settings {
            decay_enabled: true,
            ..Settings::default()
        };
        store.update_settings(1, &s).unwrap();
        id
    };
    let store = env.open();
    let snap = store.snapshot().unwrap();
    assert_eq!(snap.meters.hygiene, 9000);
    assert_eq!(snap.meters.hunger, 9000);
    assert!(snap.decay.paused && snap.decay.enabled);
    assert_eq!(snap.settings.revision, 2);
    let t = store.task(&id).unwrap();
    assert_eq!(t.state, State::AcceptedArtifact);
    assert_eq!(
        t.receipt.unwrap().override_reason.as_deref(),
        Some("reviewed failing check")
    );
    assert!(snap.last_seq > 0);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn stale_revisions_are_refused() {
    let env = Env::new();
    let mut store = env.open();
    make_ready(&mut store, &env);
    let t = store.create_task(&env.input(Category::Hunger)).unwrap();
    let p = store.preflight(&t.task_id, 1).unwrap();
    let u = store
        .update_task(&t.task_id, 1, &env.input(Category::Thirst))
        .unwrap();
    assert_eq!((u.revision, u.category), (2, Category::Thirst));
    assert!(matches!(
        store.update_task(&t.task_id, 1, &env.input(Category::Hunger)),
        Err(StorageError::StaleRevision)
    ));
    assert!(matches!(
        store.preflight(&t.task_id, 1),
        Err(StorageError::StaleRevision)
    ));
    assert!(matches!(
        store.start(&t.task_id, 1, &p.preflight_token, sup()),
        Err(StorageError::StaleRevision)
    ));
    // The old token was invalidated by the update even for the current revision.
    assert!(matches!(
        store.start(&t.task_id, 2, &p.preflight_token, sup()),
        Err(StorageError::TokenInvalid)
    ));
    let p = store.preflight(&t.task_id, 2).unwrap();
    store
        .start(&t.task_id, 2, &p.preflight_token, sup())
        .unwrap();
    assert!(matches!(
        store.update_task(&t.task_id, 2, &env.input(Category::Hunger)),
        Err(StorageError::InvalidState(_))
    ));
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn tokens_are_one_use_and_task_bound() {
    let env = Env::new();
    let mut store = env.open();
    let (a, rev_a, tok_a) = ready(&mut store, &env, Category::Hunger);
    let (b, rev_b, tok_b) = ready(&mut store, &env, Category::Thirst);
    let zeros = "0".repeat(64);
    let upper = tok_a.to_uppercase();
    let longer = format!("{tok_a}0");
    for bad in ["", "zz", zeros.as_str(), upper.as_str(), longer.as_str()] {
        assert!(matches!(
            store.start(&a, rev_a, bad, sup()),
            Err(StorageError::TokenInvalid)
        ));
    }
    assert!(matches!(
        store.start(&b, rev_b, &tok_a, sup()),
        Err(StorageError::TokenInvalid)
    ));
    store.start(&a, rev_a, &tok_a, sup()).unwrap();
    // Replay of the consumed token.
    assert!(matches!(
        store.start(&a, rev_a, &tok_a, sup()),
        Err(StorageError::InvalidState(_))
    ));
    assert_eq!(
        env.count("SELECT count(*) FROM preflight_tokens WHERE status = 'consumed'"),
        1
    );
    // Only hashes are stored.
    assert_eq!(
        env.count(&format!(
            "SELECT count(*) FROM preflight_tokens WHERE token_sha256 IN ('{tok_a}', '{tok_b}')"
        )),
        0
    );
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn consumed_token_cannot_restart_after_slot_frees() {
    let env = Env::new();
    let mut store = env.open();
    let (id, rev, token) = ready(&mut store, &env, Category::Hunger);
    store.start(&id, rev, &token, sup()).unwrap();
    // Force the row back to draft to prove the consumed status itself refuses reuse.
    env.raw()
        .execute("UPDATE tasks SET state = 'draft' WHERE id = ?1", [&id])
        .unwrap();
    assert!(matches!(
        store.start(&id, rev, &token, sup()),
        Err(StorageError::TokenInvalid)
    ));
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn token_expires_by_clock() {
    let env = Env::new();
    let mut store = env.open();
    let (id, rev, token) = ready(&mut store, &env, Category::Hunger);
    env.clock.advance(TOKEN_TTL_MS);
    assert!(matches!(
        store.start(&id, rev, &token, sup()),
        Err(StorageError::TokenExpired)
    ));
    let p = store.preflight(&id, rev).unwrap();
    assert_eq!(p.expires_at_ms, T0 + 2 * TOKEN_TTL_MS);
    env.clock.advance(TOKEN_TTL_MS - 1);
    store.start(&id, rev, &p.preflight_token, sup()).unwrap();
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn token_refused_after_clock_steps_back() {
    let env = Env::new();
    let mut store = env.open();
    let (id, rev, token) = ready(&mut store, &env, Category::Hunger);
    // Two hours back: without the check this token would live until T0 + 5 min.
    env.clock.set(T0 - 2 * 60 * 60 * 1000);
    assert!(matches!(
        store.start(&id, rev, &token, sup()),
        Err(StorageError::TokenExpired)
    ));
    env.clock.set(T0 - 1);
    assert!(matches!(
        store.start(&id, rev, &token, sup()),
        Err(StorageError::TokenExpired)
    ));
    // Nothing was consumed: the same token works once the clock is sane again.
    env.clock.set(T0);
    store.start(&id, rev, &token, sup()).unwrap();
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn newer_preflight_supersedes_older() {
    let env = Env::new();
    let mut store = env.open();
    let (id, rev, old) = ready(&mut store, &env, Category::Hunger);
    let new = store.preflight(&id, rev).unwrap().preflight_token;
    assert_ne!(old, new);
    assert!(matches!(
        store.start(&id, rev, &old, sup()),
        Err(StorageError::TokenInvalid)
    ));
    store.start(&id, rev, &new, sup()).unwrap();
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn settings_update_invalidates_tokens() {
    let env = Env::new();
    let mut store = env.open();
    let (id, rev, token) = ready(&mut store, &env, Category::Hunger);
    let s = store.update_settings(1, &Settings::default()).unwrap();
    assert_eq!(s.revision, 2);
    assert!(matches!(
        store.start(&id, rev, &token, sup()),
        Err(StorageError::TokenInvalid)
    ));
    assert!(matches!(
        store.update_settings(1, &Settings::default()),
        Err(StorageError::StaleRevision)
    ));
    let bad = Settings {
        timeout_seconds: 0,
        ..Settings::default()
    };
    assert!(matches!(
        store.update_settings(2, &bad),
        Err(StorageError::InvalidParams(_))
    ));
    let token = store.preflight(&id, rev).unwrap();
    assert_eq!(token.summary.timeout_seconds, 1800);
    store
        .start(&id, rev, &token.preflight_token, sup())
        .unwrap();
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn preflight_summary_and_path_policy() {
    let env = Env::new();
    let mut store = env.open();
    let (id, rev, _) = ready(&mut store, &env, Category::MentalHealth);
    let p = store.preflight(&id, rev).unwrap();
    assert_eq!(p.preflight_token.len(), 64);
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(
        keys(&v),
        set(&[
            "task_id",
            "revision",
            "preflight_token",
            "expires_at_ms",
            "summary"
        ])
    );
    assert_eq!(
        keys(&v["summary"]),
        set(&[
            "category",
            "provider",
            "folder",
            "scope",
            "instructions_sha256",
            "network",
            "writable",
            "timeout_seconds"
        ])
    );
    assert_eq!(v["summary"]["network"], "none (mock)");
    assert_eq!(
        v["summary"]["writable"],
        "none (mock: no project files are read or written)"
    );
    assert_eq!(
        p.summary.instructions_sha256,
        sys::sha256_hex(b"Explain the task state machine")
    );
    let state_parent = env.paths.state_dir.parent().unwrap().to_path_buf();
    let inside = env.paths.state_dir.join("x");
    let home_slash = PathBuf::from(format!("{}/", env.home.display()));
    for folder in [
        env.home.clone(),
        home_slash,
        state_parent,
        env.paths.state_dir.clone(),
        inside,
        env.root.clone(),
    ] {
        let t = store
            .create_task(&env.input_at(Category::Hunger, &folder))
            .unwrap();
        assert!(
            matches!(
                store.preflight(&t.task_id, 1),
                Err(StorageError::InvalidPath(_))
            ),
            "{folder:?}"
        );
    }
    // A symlink to HOME is refused through its canonical form.
    let link = env.root.join("home-link");
    symlink(&env.home, &link).unwrap();
    let t = store
        .create_task(&env.input_at(Category::Hunger, &link))
        .unwrap();
    assert!(matches!(
        store.preflight(&t.task_id, 1),
        Err(StorageError::InvalidPath(_))
    ));
    // A symlinked parent into the state dir is resolved through its deepest existing
    // ancestor, even when the last components do not exist.
    let state_link = env.root.join("state-link");
    symlink(&env.paths.state_dir, &state_link).unwrap();
    for folder in [state_link.join("sub"), state_link.join("a/b/c")] {
        let t = store
            .create_task(&env.input_at(Category::Hunger, &folder))
            .unwrap();
        assert!(
            matches!(
                store.preflight(&t.task_id, 1),
                Err(StorageError::InvalidPath(_))
            ),
            "{folder:?}"
        );
    }
    // Drafts may name any provider; preflight fails closed for non-mock ones.
    for provider in Provider::ALL.into_iter().filter(|p| !p.supported()) {
        let mut input = env.input(Category::Hunger);
        input.draft.provider = provider;
        let t = store.create_task(&input).unwrap();
        assert!(matches!(
            store.preflight(&t.task_id, 1),
            Err(StorageError::UnsupportedProvider)
        ));
    }
    assert!(matches!(
        store.preflight("missing", 1),
        Err(StorageError::NotFound)
    ));
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn start_rechecks_the_folder() {
    let env = Env::new();
    let mut store = env.open();
    let folder = env.root.join("later-link");
    fs::create_dir(&folder).unwrap();
    let t = store
        .create_task(&env.input_at(Category::Hunger, &folder))
        .unwrap();
    let p = store.preflight(&t.task_id, 1).unwrap();
    // Between preflight and Start the folder becomes a symlink into the state dir.
    fs::remove_dir(&folder).unwrap();
    symlink(&env.paths.state_dir, &folder).unwrap();
    assert!(matches!(
        store.start(&t.task_id, 1, &p.preflight_token, sup()),
        Err(StorageError::InvalidPath(_))
    ));
    assert!(store.snapshot().unwrap().active_task.is_none());
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn unknown_home_fails_closed_and_account_home_is_refused() {
    let env = Env::new();
    let mut store = env.open();
    let (id, rev, _) = ready(&mut store, &env, Category::Hunger);
    // No HOME and no passwd entry: nothing to compare against, so preflight refuses.
    let mut paths = env.paths.clone();
    paths.home = None;
    paths.account_home = None;
    let mut blind = Store::open(&paths, env.clock.clone()).unwrap();
    assert!(matches!(
        blind.preflight(&id, rev),
        Err(StorageError::Unavailable(_))
    ));
    // HOME unset (or unusable) but the passwd home is known: that home is refused.
    paths.account_home = Some(env.home.clone());
    let mut store = Store::open(&paths, env.clock.clone()).unwrap();
    store.preflight(&id, rev).unwrap();
    let t = store
        .create_task(&env.input_at(Category::Hunger, &env.home))
        .unwrap();
    assert!(matches!(
        store.preflight(&t.task_id, 1),
        Err(StorageError::InvalidPath(_))
    ));
}

#[test]
fn passwd_home_lookup() {
    let passwd = "root:x:0:0::/root:/bin/sh\n\
                  broken:line\n\
                  u:x:1000:1000:U,,,:/home/u:/bin/zsh\n\
                  e:x:1001:1001:::/bin/sh\n";
    assert_eq!(
        sys::passwd_home(passwd, 1000),
        Some(PathBuf::from("/home/u"))
    );
    assert_eq!(sys::passwd_home(passwd, 0), Some(PathBuf::from("/root")));
    assert_eq!(sys::passwd_home(passwd, 1001), None);
    assert_eq!(sys::passwd_home(passwd, 4242), None);
    let p = StatePaths::from_env().unwrap();
    if let Some(h) = &p.account_home {
        assert!(h.is_absolute());
    }
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn draft_input_is_validated() {
    let env = Env::new();
    let mut store = env.open();
    let mut input = env.input(Category::Hunger);
    input.draft.folder = "relative".into();
    assert!(matches!(
        store.create_task(&input),
        Err(StorageError::InvalidPath(_))
    ));
    let mut input = env.input(Category::Hunger);
    input.draft.instructions = "  ".into();
    assert!(matches!(
        store.create_task(&input),
        Err(StorageError::InvalidParams(_))
    ));
    let mut input = env.input(Category::Hunger);
    input.mock.step_ms = MAX_MOCK_STEP_MS + 1;
    assert!(matches!(
        store.create_task(&input),
        Err(StorageError::InvalidParams(_))
    ));
    input.mock = MockOptions {
        outcome: MockOutcome::CheckFailed,
        step_ms: MAX_MOCK_STEP_MS,
    };
    input.draft.scope = vec!["src".into(), "docs/a.md".into()];
    let t = store.create_task(&input).unwrap();
    assert_eq!(t.mock, input.mock);
    assert_eq!(t.scope, input.draft.scope);
    assert_eq!(env.count("SELECT count(*) FROM tasks"), 1);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn single_active_slot_across_states_and_handles() {
    let env = Env::new();
    let mut a = env.open();
    let mut b = env.open();
    let (x, rx, tx) = ready(&mut a, &env, Category::Hunger);
    let (y, ry, ty) = ready(&mut b, &env, Category::Thirst);
    a.start(&x, rx, &tx, sup()).unwrap();
    assert!(matches!(
        b.start(&y, ry, &ty, sup()),
        Err(StorageError::Busy)
    ));
    // A Busy start does not consume the token.
    assert_eq!(
        env.count("SELECT count(*) FROM preflight_tokens WHERE status = 'pending'"),
        1
    );
    let s = sup();
    a.supervisor_step(&x, s, State::Preparing, State::Running)
        .unwrap();
    a.supervisor_step(&x, s, State::Running, State::WaitingForUser)
        .unwrap();
    assert!(matches!(
        b.start(&y, ry, &ty, sup()),
        Err(StorageError::Busy)
    ));
    a.supervisor_step(&x, s, State::WaitingForUser, State::Running)
        .unwrap();
    a.supervisor_step(&x, s, State::Running, State::Verifying)
        .unwrap();
    let SealOutcome::Sealed { digest, .. } = a
        .seal_review(&x, s, ARTIFACT, Verification::NotVerified, true)
        .unwrap()
    else {
        panic!()
    };
    // review_ready still holds the slot.
    assert!(matches!(
        b.start(&y, ry, &ty, sup()),
        Err(StorageError::Busy)
    ));
    assert_eq!(b.snapshot().unwrap().active_task.unwrap().task_id, x);
    b.discard(&x, &digest).unwrap();
    assert!(matches!(
        b.start(&y, ry, &ty, sup()),
        Err(StorageError::InvalidState(_))
    ));
    make_ready(&mut b, &env);
    let token = b.preflight(&y, ry).unwrap();
    b.start(&y, ry, &token.preflight_token, sup()).unwrap();
    assert_eq!(a.snapshot().unwrap().active_task.unwrap().task_id, y);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn unique_index_enforces_slot_even_without_checks() {
    let env = Env::new();
    let mut store = env.open();
    let x = store.create_task(&env.input(Category::Hunger)).unwrap();
    let y = store.create_task(&env.input(Category::Hunger)).unwrap();
    let raw = env.raw();
    raw.execute(
        "UPDATE tasks SET state = 'running' WHERE id = ?1",
        [&x.task_id],
    )
    .unwrap();
    let err = raw
        .execute(
            "UPDATE tasks SET state = 'recovery_required' WHERE id = ?1",
            [&y.task_id],
        )
        .unwrap_err();
    assert_eq!(
        err.sqlite_error_code(),
        Some(rusqlite::ErrorCode::ConstraintViolation)
    );
    raw.execute(
        "UPDATE tasks SET state = 'failed' WHERE id = ?1",
        [&y.task_id],
    )
    .unwrap();
    // Enum and meter CHECK constraints.
    assert!(raw
        .execute(
            "UPDATE tasks SET state = 'bogus' WHERE id = ?1",
            [&y.task_id]
        )
        .is_err());
    assert!(raw
        .execute("UPDATE pet_state SET hunger = 10001", [])
        .is_err());
    assert!(raw.execute("UPDATE pet_state SET hunger = -1", []).is_err());
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn slot_state_list_matches_core() {
    for s in State::ALL {
        assert_eq!(
            schema::SLOT_STATES.contains(&format!("'{}'", s.as_str())),
            s.occupies_slot(),
            "{s:?}"
        );
    }
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn acceptance_rewards_exactly_once() {
    let env = Env::new();
    let mut store = env.open();
    let (id, digest) = reviewed(
        &mut store,
        &env,
        Category::MentalHealth,
        Verification::NotVerified,
    );
    let first = accept(&mut store, &id, &digest, "req-1", Some("useful")).unwrap();
    assert!(!first.duplicate);
    assert_eq!(meters(&store).mental_health, 9500);
    // Same request id: the stored response, byte for byte.
    let again = accept(&mut store, &id, &digest, "req-1", Some("useful")).unwrap();
    assert_eq!(again, first);
    // Different request id: the recorded receipt, flagged duplicate, no new reward.
    let dup = accept(&mut store, &id, &digest, "req-2", Some("other words")).unwrap();
    assert!(dup.duplicate);
    assert_eq!(dup.receipt, first.receipt);
    let mut other = env.open();
    let dup2 = accept(&mut other, &id, &digest, "req-3", Some("x")).unwrap();
    assert_eq!(dup2.receipt, first.receipt);
    assert_eq!(meters(&store).mental_health, 9500);
    assert_eq!(env.count("SELECT count(*) FROM demo_xp_rewards"), 1);
    assert_eq!(env.count("SELECT count(*) FROM acceptances"), 1);
    assert_eq!(
        env.count("SELECT count(*) FROM request_results WHERE method = 'task.accept_artifact'"),
        3
    );
    // Wrong digest or missing acknowledgement still fail for an accepted task.
    assert!(matches!(
        accept(&mut store, &id, "00", "req-4", Some("x")),
        Err(StorageError::ReviewChanged)
    ));
    // Rewards table refuses a second row for a task.
    assert!(env
        .raw()
        .execute(
            "INSERT INTO demo_xp_rewards (task_id, track, amount, granted_at_ms)
             VALUES (?1, 'bugfix', 20, 0)",
            [&id],
        )
        .is_err());
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn idempotency_conflict_on_changed_params() {
    let env = Env::new();
    let mut store = env.open();
    let (id, digest) = reviewed(
        &mut store,
        &env,
        Category::Thirst,
        Verification::NotVerified,
    );
    accept(&mut store, &id, &digest, "req", Some("ok")).unwrap();
    assert!(matches!(
        accept(&mut store, &id, &digest, "req", Some("ok ")),
        Err(StorageError::IdempotencyConflict)
    ));
    assert!(matches!(
        accept(&mut store, &id, &digest, "req", None),
        Err(StorageError::IdempotencyConflict)
    ));
    let (id2, digest2) = {
        // Free slot already (accepted is terminal); a new task reusing the request id conflicts.
        reviewed(
            &mut store,
            &env,
            Category::Thirst,
            Verification::NotVerified,
        )
    };
    assert!(matches!(
        accept(&mut store, &id2, &digest2, "req", Some("ok")),
        Err(StorageError::IdempotencyConflict)
    ));
    assert_eq!(meters(&store).thirst, 9000);
    let long_id = "x".repeat(129);
    for bad in ["", "has space", long_id.as_str(), "ü"] {
        assert!(matches!(
            accept(&mut store, &id2, &digest2, bad, Some("ok")),
            Err(StorageError::InvalidParams(_))
        ));
    }
    accept(&mut store, &id2, &digest2, &"a".repeat(128), Some("ok")).unwrap();
    assert_eq!(meters(&store).thirst, 9000);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn override_rules_mirror_core() {
    let env = Env::new();
    let mut store = env.open();
    let (id, digest) = reviewed(&mut store, &env, Category::Hunger, Verification::Failed);
    let no_ack = store.accept_artifact(&AcceptRequest {
        request_id: "a",
        task_id: &id,
        review_digest: &digest,
        acknowledged: false,
        override_reason: Some("fine"),
    });
    assert!(matches!(no_ack, Err(StorageError::AcknowledgementRequired)));
    assert!(matches!(
        accept(&mut store, &id, "deadbeef", "b", Some("fine")),
        Err(StorageError::ReviewChanged)
    ));
    let long = "x".repeat(MAX_OVERRIDE_REASON_BYTES + 1);
    for reason in [None, Some(""), Some("   ")] {
        assert!(
            matches!(
                accept(&mut store, &id, &digest, "c", reason),
                Err(StorageError::OverrideRequired)
            ),
            "{reason:?}"
        );
    }
    // A reason that was given but is too long (in UTF-8 bytes) or has NUL is a parameter
    // error, not a missing reason: 200 CJK characters are 600 bytes.
    let cjk = "\u{4e2d}".repeat(200);
    for reason in [long.as_str(), "a\0b", cjk.as_str()] {
        assert!(
            matches!(
                accept(&mut store, &id, &digest, "c", Some(reason)),
                Err(StorageError::InvalidParams(_))
            ),
            "{reason:?}"
        );
    }
    assert_eq!(
        MAX_OVERRIDE_REASON_BYTES,
        codepet_core::MAX_OVERRIDE_REASON_BYTES
    );
    // Failed attempts store nothing and grant nothing.
    assert_eq!(
        env.count("SELECT count(*) FROM request_results WHERE method = 'task.accept_artifact'"),
        0
    );
    assert_eq!(meters(&store).hunger, 9000);
    let max = format!("  {}  ", "y".repeat(MAX_OVERRIDE_REASON_BYTES));
    let out = accept(&mut store, &id, &digest, "c", Some(&max)).unwrap();
    assert_eq!(
        out.receipt.override_reason.as_deref(),
        Some("y".repeat(MAX_OVERRIDE_REASON_BYTES).as_str())
    );
    assert_eq!(out.receipt.verification, Verification::Failed);

    // A non-simulated passed check needs no reason; an over-long one is still refused.
    let (id, digest) = reviewed(&mut store, &env, Category::Hunger, Verification::Passed);
    assert!(matches!(
        accept(&mut store, &id, &digest, "d", Some(&long)),
        Err(StorageError::InvalidParams(_))
    ));
    let out = accept(&mut store, &id, &digest, "e", None).unwrap();
    assert_eq!(out.receipt.override_reason, None);
    assert_eq!(meters(&store).hunger, 9000);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn discard_failure_and_cancel_never_reward() {
    let env = Env::new();
    let mut store = env.open();
    make_ready(&mut store, &env);
    let before = meters(&store);
    // Discard.
    let (id, digest) = reviewed(
        &mut store,
        &env,
        Category::Hunger,
        Verification::NotVerified,
    );
    assert!(matches!(
        store.discard(&id, "wrong"),
        Err(StorageError::ReviewChanged)
    ));
    assert_eq!(store.discard(&id, &digest).unwrap().state, State::Discarded);
    assert_eq!(store.discard(&id, &digest).unwrap().state, State::Discarded);
    assert!(matches!(
        accept(&mut store, &id, &digest, "late", Some("x")),
        Err(StorageError::InvalidState(_))
    ));
    // Failure.
    let id = started(&mut store, &env, Category::Thirst, sup());
    store
        .supervisor_step(&id, sup(), State::Preparing, State::Running)
        .unwrap();
    assert!(matches!(
        store.fail(&id, sup()).unwrap(),
        Step::Advanced { .. }
    ));
    assert_eq!(store.task(&id).unwrap().state, State::Failed);
    assert!(matches!(
        store.review(&id),
        Err(StorageError::InvalidState(_))
    ));
    // Cancel: idempotent, then confirmed by the supervisor.
    let draft = store.create_task(&env.input(Category::Hygiene)).unwrap();
    assert!(matches!(
        store.cancel(&draft.task_id),
        Err(StorageError::InvalidState(_))
    ));
    let id = started(&mut store, &env, Category::Hygiene, sup());
    assert_eq!(store.cancel(&id).unwrap().state, State::Cancelling);
    assert_eq!(store.cancel(&id).unwrap().state, State::Cancelling);
    assert_eq!(
        store
            .supervisor_step(&id, sup(), State::Preparing, State::Running)
            .unwrap(),
        Step::Cancelling
    );
    assert!(matches!(
        store.confirm_cancelled(&id, dead_sup()),
        Err(StorageError::InvalidState(_))
    ));
    store.confirm_cancelled(&id, sup()).unwrap();
    assert_eq!(store.cancel(&id).unwrap().state, State::Cancelled);
    assert!(matches!(
        store.confirm_cancelled(&id, sup()),
        Err(StorageError::InvalidState(_))
    ));
    assert_eq!(meters(&store), before);
    assert_eq!(env.count("SELECT count(*) FROM rewards"), 0);
    assert!(store.snapshot().unwrap().active_task.is_none());
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn acceptance_does_not_refill_care() {
    let env = Env::new();
    let mut store = env.open();
    for i in 0..3 {
        let (id, digest) = reviewed(
            &mut store,
            &env,
            Category::Hygiene,
            Verification::NotVerified,
        );
        let before = meters(&store);
        let r = accept(&mut store, &id, &digest, &format!("r{i}"), Some("ok")).unwrap();
        assert_eq!(r.receipt.amount_hundredths, 0);
        assert_eq!(r.receipt.demo_xp, 20);
        assert_eq!(meters(&store), before);
    }
    assert_eq!(env.count("SELECT count(*) FROM rewards"), 0);
    assert_eq!(
        store.snapshot().unwrap().progression.demo.mastery.cleanup,
        60
    );
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn supervisor_steps_are_compare_and_set() {
    let env = Env::new();
    let mut store = env.open();
    let id = started(&mut store, &env, Category::Hunger, sup());
    let s = sup();
    assert!(matches!(
        store.supervisor_step(&id, s, State::Running, State::Verifying),
        Err(StorageError::InvalidState(_))
    ));
    assert!(matches!(
        store.supervisor_step(&id, dead_sup(), State::Preparing, State::Running),
        Err(StorageError::InvalidState(_))
    ));
    for to in [
        State::ReviewReady,
        State::Cancelled,
        State::AcceptedArtifact,
        State::Interrupted,
        State::Failed,
    ] {
        assert!(matches!(
            store.supervisor_step(&id, s, State::Preparing, to),
            Err(StorageError::InvalidParams(_))
        ));
    }
    // Preparing → Verifying skips a phase and is not in the transition table.
    assert!(matches!(
        store.supervisor_step(&id, s, State::Preparing, State::Verifying),
        Err(StorageError::InvalidState(_))
    ));
    store
        .supervisor_step(&id, s, State::Preparing, State::Running)
        .unwrap();
    assert!(matches!(
        store.seal_review(&id, s, ARTIFACT, Verification::NotVerified, true),
        Err(StorageError::InvalidState(_))
    ));
    store
        .supervisor_step(&id, s, State::Running, State::Verifying)
        .unwrap();
    // The bound is JSON-encoded size: quotes count, and a control character costs 6 bytes.
    let big = "x".repeat(MAX_ARTIFACT_JSON_BYTES - 1);
    let escaped = "\u{1}".repeat(MAX_ARTIFACT_JSON_BYTES / 6);
    for (artifact, verification, simulated) in [
        ("", Verification::NotVerified, true),
        ("  \n", Verification::NotVerified, true),
        ("a\0", Verification::NotVerified, true),
        (big.as_str(), Verification::NotVerified, true),
        (escaped.as_str(), Verification::NotVerified, true),
        ("ok", Verification::Passed, true),
    ] {
        assert!(matches!(
            store.seal_review(&id, s, artifact, verification, simulated),
            Err(StorageError::InvalidParams(_))
        ));
    }
    store.cancel(&id).unwrap();
    assert_eq!(
        store
            .seal_review(&id, s, ARTIFACT, Verification::NotVerified, true)
            .unwrap(),
        SealOutcome::Cancelling
    );
    assert_eq!(store.fail(&id, s).unwrap(), Step::Cancelling);
    store.confirm_cancelled(&id, s).unwrap();
    assert_eq!(env.count("SELECT count(*) FROM artifacts"), 0);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn artifact_at_size_limit_is_sealed() {
    let env = Env::new();
    let mut store = env.open();
    let id = started(&mut store, &env, Category::Hunger, sup());
    let s = sup();
    store
        .supervisor_step(&id, s, State::Preparing, State::Running)
        .unwrap();
    store
        .supervisor_step(&id, s, State::Running, State::Verifying)
        .unwrap();
    let big = "x".repeat(MAX_ARTIFACT_JSON_BYTES - 2);
    assert!(matches!(
        store
            .seal_review(&id, s, &big, Verification::NotVerified, true)
            .unwrap(),
        SealOutcome::Sealed { .. }
    ));
    assert_eq!(
        store.review(&id).unwrap().review.unwrap().artifact.len(),
        MAX_ARTIFACT_JSON_BYTES - 2
    );
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn recover_interrupts_dead_supervisors_only() {
    let env = Env::new();
    let mut store = env.open();
    let id = started(&mut store, &env, Category::Hunger, dead_sup());
    // A live supervisor is never disturbed.
    {
        let other = Env::new();
        let mut s2 = other.open();
        let live = started(&mut s2, &other, Category::Hunger, sup());
        s2.supervisor_step(&live, sup(), State::Preparing, State::Running)
            .unwrap();
        let r = s2.recover().unwrap();
        assert!(r.changed.is_empty());
        assert_eq!(r.active_task.unwrap().state, State::Running);
    }
    let r = store.recover().unwrap();
    assert_eq!(
        r.changed,
        [RecoveryChange {
            task_id: id.clone(),
            from: State::Preparing,
            to: State::Interrupted
        }]
    );
    assert!(r.active_task.is_none());
    assert!(store.recover().unwrap().changed.is_empty());
    // A cancelling task with a lost supervisor is interrupted, not cancelled.
    let id = started(&mut store, &env, Category::Hunger, sup());
    store.cancel(&id).unwrap();
    env.raw()
        .execute("UPDATE tasks SET supervisor_pid = 0 WHERE id = ?1", [&id])
        .unwrap();
    let r = store.recover().unwrap();
    assert_eq!(r.changed[0].from, State::Cancelling);
    assert_eq!(store.task(&id).unwrap().state, State::Interrupted);
    assert_eq!(env.count("SELECT count(*) FROM rewards"), 0);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn recover_flags_tampered_review() {
    let env = Env::new();
    let mut store = env.open();
    let (id, digest) = reviewed(
        &mut store,
        &env,
        Category::Hunger,
        Verification::NotVerified,
    );
    // Intact review survives recovery unchanged.
    assert!(store.recover().unwrap().changed.is_empty());
    assert_eq!(store.task(&id).unwrap().state, State::ReviewReady);
    env.raw()
        .execute(
            "UPDATE artifacts SET content = 'tampered' WHERE task_id = ?1",
            [&id],
        )
        .unwrap();
    // Before recovery runs, the damaged review can be neither read for review nor accepted,
    // even with the original digest; nothing is recorded or rewarded.
    assert!(matches!(
        store.review(&id),
        Err(StorageError::ReviewDamaged)
    ));
    assert!(matches!(
        accept(&mut store, &id, &digest, "a", Some("x")),
        Err(StorageError::ReviewDamaged)
    ));
    assert_eq!(
        env.count("SELECT count(*) FROM request_results WHERE method = 'task.accept_artifact'"),
        0
    );
    assert_eq!(env.count("SELECT count(*) FROM rewards"), 0);
    assert_eq!(store.task(&id).unwrap().state, State::ReviewReady);
    let r = store.recover().unwrap();
    assert_eq!(r.changed[0].to, State::RecoveryRequired);
    let active = r.active_task.unwrap();
    assert_eq!(active.state, State::RecoveryRequired);
    // The brief carries the digest the UI needs to abandon the task, and task.review shows
    // the read-only recovery_required task.
    assert_eq!(active.review.unwrap().digest, digest);
    assert_eq!(store.review(&id).unwrap().state, State::RecoveryRequired);
    assert!(matches!(
        accept(&mut store, &id, &digest, "a", Some("x")),
        Err(StorageError::InvalidState(_))
    ));
    let (x, rx, tx) = ready(&mut store, &env, Category::Thirst);
    assert!(matches!(
        store.start(&x, rx, &tx, sup()),
        Err(StorageError::Busy)
    ));
    // Explicit abandonment frees the slot without a reward.
    assert_eq!(store.discard(&id, &digest).unwrap().state, State::Discarded);
    store.start(&x, rx, &tx, sup()).unwrap();
    assert_eq!(meters(&store).hunger, 9000);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn recover_flags_tampered_verification_column() {
    let env = Env::new();
    let mut store = env.open();
    let (id, _) = reviewed(&mut store, &env, Category::Hunger, Verification::Failed);
    env.raw()
        .execute(
            "UPDATE tasks SET review_verification = 'not_verified' WHERE id = ?1",
            [&id],
        )
        .unwrap();
    assert_eq!(store.recover().unwrap().changed.len(), 1);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn recover_missing_review_metadata_can_be_explicitly_abandoned() {
    for missing in [
        "review_digest",
        "review_verification",
        "both",
        "both_and_check",
    ] {
        let env = Env::new();
        let mut store = env.open();
        let (id, original_digest) =
            reviewed(&mut store, &env, Category::Hunger, Verification::Failed);
        let columns = match missing {
            "both" | "both_and_check" => "review_digest = NULL, review_verification = NULL",
            "review_digest" => "review_digest = NULL",
            _ => "review_verification = NULL",
        };
        env.raw()
            .execute(&format!("UPDATE tasks SET {columns} WHERE id = ?1"), [&id])
            .unwrap();
        if missing == "both_and_check" {
            env.raw()
                .execute("DELETE FROM checks WHERE task_id = ?1", [&id])
                .unwrap();
        }

        let recovered = store.recover().unwrap();
        assert_eq!(recovered.changed[0].to, State::RecoveryRequired);
        assert_eq!(recovered.repaired, vec![id.clone()]);
        let review = recovered
            .active_task
            .unwrap()
            .review
            .expect("recovery must expose the digest needed for explicit abandonment");
        assert_eq!(review.digest.len(), 64);
        assert!(review.digest.bytes().all(|b| b.is_ascii_hexdigit()));
        if missing == "review_verification" {
            assert_eq!(review.digest, original_digest);
        } else {
            assert_ne!(review.digest, original_digest);
        }
        // The recorded check still says `failed`; only a task with no check at all is
        // reported as not verified. Nothing is invented as a pass.
        assert_eq!(
            review.verification,
            if missing == "both_and_check" {
                Verification::NotVerified
            } else {
                Verification::Failed
            }
        );
        assert!(review.metadata_repaired);
        let source: String = env
            .raw()
            .query_row(
                "SELECT verification_source FROM review_repairs WHERE task_id = ?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            source,
            match missing {
                "review_digest" => "stored",
                "both_and_check" => "default",
                _ => "checks",
            }
        );
        // The replacement metadata is durable and is never treated as a new sealed result.
        drop(store);
        let mut store = env.open();
        let again = store.recover().unwrap();
        assert!(again.changed.is_empty() && again.repaired.is_empty());
        let task = store.review(&id).unwrap();
        assert_eq!(task.state, State::RecoveryRequired);
        assert_eq!(task.review.as_ref().unwrap().digest, review.digest);
        assert_eq!(task.review.as_ref().unwrap().artifact, ARTIFACT);
        assert!(matches!(
            accept(
                &mut store,
                &id,
                &review.digest,
                "no-reward",
                Some("acknowledged")
            ),
            Err(StorageError::InvalidState(_))
        ));
        assert!(matches!(
            store.discard(&id, "wrong-digest"),
            Err(StorageError::ReviewChanged)
        ));
        let (next, revision, token) = ready(&mut store, &env, Category::Thirst);
        assert!(matches!(
            store.start(&next, revision, &token, sup()),
            Err(StorageError::Busy)
        ));
        // The care routine in ready() filled every meter, which would cap (and so hide) any
        // refund. Lower each meter below full so a gain on abandonment is visible, and
        // compare the meters immediately before and after the discard.
        env.raw()
            .execute(
                "UPDATE pet_state SET hunger = 6000, thirst = 6100, mental_health = 6200,
                 hygiene = 6300 WHERE id = 1",
                [],
            )
            .unwrap();
        let before_discard = meters(&store);
        assert_eq!(before_discard.hunger, 6000);
        let discarded = store.discard(&id, &review.digest).unwrap();
        assert_eq!(discarded.state, State::Discarded);
        assert_eq!(
            meters(&store),
            before_discard,
            "abandonment must neither refund nor reward any care meter"
        );
        // History keeps marking the review as repaired abandonment metadata.
        assert!(discarded.review.unwrap().metadata_repaired);
        assert!(store.snapshot().unwrap().active_task.is_none());
        // Restore the fixture's full bars (Start requires them; care would also supersede
        // the pending preflight) so the rest of the test checks Start's debit as before.
        env.raw()
            .execute(
                "UPDATE pet_state SET hunger = 10000, thirst = 10000, mental_health = 10000,
                 hygiene = 10000 WHERE id = 1",
                [],
            )
            .unwrap();
        store.start(&next, revision, &token, sup()).unwrap();
        // The care routine filled hunger; Start debits one care step. Abandonment
        // grants neither a care reward nor demo XP.
        assert_eq!(meters(&store).hunger, 9000);
        assert_eq!(env.count("SELECT count(*) FROM rewards"), 0);
        assert_eq!(env.count("SELECT count(*) FROM demo_xp_rewards"), 0);
        assert_eq!(env.count("SELECT count(*) FROM acceptances"), 0);
    }
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn recover_replaces_digests_that_task_discard_could_never_accept() {
    for damaged in [String::new(), "f".repeat(200), "A".repeat(64)] {
        let env = Env::new();
        let mut store = env.open();
        let (id, _) = reviewed(&mut store, &env, Category::Hunger, Verification::Failed);
        env.raw()
            .execute(
                "UPDATE tasks SET review_digest = ?1 WHERE id = ?2",
                rusqlite::params![damaged, id],
            )
            .unwrap();
        let discard_params = |digest: &str| {
            codepet_protocol::parse_params::<codepet_protocol::TaskDiscardParams>(
                serde_json::json!({"task_id": id, "review_digest": digest}),
            )
        };
        if damaged.is_empty() || damaged.len() > 128 {
            // The stored value alone could never get past task.discard's validation.
            assert!(discard_params(&damaged).is_err());
        }

        let recovered = store.recover().unwrap();
        assert_eq!(recovered.changed[0].to, State::RecoveryRequired);
        assert_eq!(recovered.repaired, vec![id.clone()]);
        let review = recovered.active_task.unwrap().review.unwrap();
        assert_ne!(review.digest, damaged);
        assert_eq!(review.digest.len(), 64);
        assert!(review
            .digest
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
        assert_eq!(review.verification, Verification::Failed);
        // The replaced value is preserved verbatim, not destroyed by the repair.
        let (previous, digest, source): (String, String, String) = env
            .raw()
            .query_row(
                "SELECT previous_digest, digest, verification_source FROM review_repairs
                 WHERE task_id = ?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            (previous.as_str(), digest.as_str(), source.as_str()),
            (damaged.as_str(), review.digest.as_str(), "stored")
        );

        // The UI's protocol-level discard now validates and releases the slot.
        let params = discard_params(&review.digest).unwrap();
        assert_eq!(
            store
                .discard(&params.task_id, &params.review_digest)
                .unwrap()
                .state,
            State::Discarded
        );
        assert!(store.snapshot().unwrap().active_task.is_none());
        assert_eq!(env.count("SELECT count(*) FROM demo_xp_rewards"), 0);
    }
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn recover_repairs_already_stranded_recovery_metadata() {
    let env = Env::new();
    let mut store = env.open();
    let (id, _) = reviewed(&mut store, &env, Category::Hunger, Verification::Failed);
    // A previous build may already have recovered the damaged task without providing
    // the metadata needed to abandon it. Re-running recovery must also repair that case.
    env.raw()
        .execute(
            "UPDATE tasks SET state = 'recovery_required', review_digest = NULL,
                review_verification = NULL WHERE id = ?1",
            [&id],
        )
        .unwrap();
    let seq = store.last_seq().unwrap();
    let recovered = store.recover().unwrap();
    assert!(recovered.changed.is_empty());
    // No state changed, but the result reports the repair so the UI can explain it.
    assert_eq!(recovered.repaired, vec![id.clone()]);
    // The repair is a mutation, so it appends one same-state event.
    assert_eq!(store.last_seq().unwrap(), seq + 1);
    let page = store.events_after(seq, None).unwrap();
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].task_id, id);
    assert_eq!(page.events[0].data.from, Some(State::RecoveryRequired));
    assert_eq!(page.events[0].data.state, State::RecoveryRequired);
    // Running recovery again finds nothing left to repair.
    let again = store.recover().unwrap();
    assert!(again.changed.is_empty() && again.repaired.is_empty());
    assert_eq!(store.last_seq().unwrap(), seq + 1);
    let active = recovered.active_task.unwrap();
    assert_eq!(active.state, State::RecoveryRequired);
    let review = active.review.unwrap();
    // Restored from the recorded failed check, never shown as "no checks ran".
    assert_eq!(review.verification, Verification::Failed);
    assert!(review.metadata_repaired);
    assert_eq!(
        store.review(&id).unwrap().review.unwrap().artifact,
        ARTIFACT
    );
    assert!(matches!(
        accept(
            &mut store,
            &id,
            &review.digest,
            "still-blocked",
            Some("acknowledged")
        ),
        Err(StorageError::InvalidState(_))
    ));
    store.discard(&id, &review.digest).unwrap();
    assert!(store.snapshot().unwrap().active_task.is_none());
    assert_eq!(env.count("SELECT count(*) FROM rewards"), 0);
    assert_eq!(env.count("SELECT count(*) FROM demo_xp_rewards"), 0);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn every_state_change_appends_an_event() {
    let env = Env::new();
    let mut store = env.open();
    let (id, digest) = reviewed(
        &mut store,
        &env,
        Category::Hunger,
        Verification::NotVerified,
    );
    accept(&mut store, &id, &digest, "r", Some("ok")).unwrap();
    let page = store.events_after(0, None).unwrap();
    let states: Vec<_> = page.events.iter().map(|e| e.data.state).collect();
    assert_eq!(
        states,
        [
            State::Draft,
            State::Preparing,
            State::Running,
            State::Verifying,
            State::ReviewReady,
            State::AcceptedArtifact
        ]
    );
    assert!(page.events.windows(2).all(|w| w[0].seq < w[1].seq));
    assert_eq!(page.last_seq, page.events.last().unwrap().seq);
    assert_eq!(page.events[0].data.from, None);
    let v = serde_json::to_value(&page.events[1]).unwrap();
    assert_eq!(keys(&v), set(&["v", "event", "seq", "task_id", "data"]));
    assert_eq!(v["v"], 1);
    assert_eq!(v["event"], "task.state");
    assert_eq!(
        v["data"],
        serde_json::json!({"state": "preparing", "from": "draft", "revision": 1, "at_ms": T0})
    );
    let tail = store.events_after(page.last_seq - 2, Some(1)).unwrap();
    assert_eq!(tail.events.len(), 1);
    assert_eq!(tail.events[0].seq, page.last_seq - 1);
    assert!(store
        .events_after(page.last_seq, None)
        .unwrap()
        .events
        .is_empty());
    assert!(matches!(
        store.events_after(page.last_seq + 1, None),
        Err(StorageError::ResyncRequired)
    ));
    for bad in [Some(0), Some(MAX_EVENTS_PAGE + 1)] {
        assert!(matches!(
            store.events_after(0, bad),
            Err(StorageError::InvalidParams(_))
        ));
    }
    assert!(matches!(
        store.events_after(-1, None),
        Err(StorageError::InvalidParams(_))
    ));
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn history_pages_newest_first() {
    let env = Env::new();
    let mut store = env.open();
    let mut ids = Vec::new();
    for _ in 0..3 {
        ids.push(
            store
                .create_task(&env.input(Category::Hunger))
                .unwrap()
                .task_id,
        );
        env.clock.advance(10);
    }
    let page = store.history(None, None, Some(2)).unwrap();
    let got: Vec<_> = page.tasks.iter().map(|t| t.task_id.clone()).collect();
    assert_eq!(got, [ids[2].clone(), ids[1].clone()]);
    let next = page.next.unwrap();
    assert_eq!(next.before_task_id, ids[1]);
    let older = store
        .history(Some(next.before_ms), Some(&next.before_task_id), None)
        .unwrap();
    assert_eq!(older.tasks.len(), 1);
    assert_eq!(older.tasks[0].task_id, ids[0]);
    assert!(older.next.is_none());
    // `before_ms` alone keeps its strict meaning.
    let older = store.history(Some(next.before_ms), None, None).unwrap();
    assert_eq!(older.tasks[0].task_id, ids[0]);
    assert!(matches!(
        store.history(None, None, Some(MAX_HISTORY_PAGE + 1)),
        Err(StorageError::InvalidParams(_))
    ));
    for bad in [(None, Some("x")), (Some(T0), Some("missing"))] {
        assert!(matches!(
            store.history(bad.0, bad.1, None),
            Err(StorageError::InvalidParams(_))
        ));
    }
    for _ in 0..10 {
        store.create_task(&env.input(Category::Thirst)).unwrap();
    }
    assert_eq!(
        store.snapshot().unwrap().recent.len(),
        RECENT_TASKS as usize
    );
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn history_cursor_keeps_tasks_created_in_the_same_millisecond() {
    let env = Env::new();
    let mut store = env.open();
    env.clock.advance(5);
    let older = store.create_task(&env.input(Category::Hunger)).unwrap();
    env.clock.advance(5);
    let mut same = Vec::new();
    for _ in 0..5 {
        same.push(
            store
                .create_task(&env.input(Category::Thirst))
                .unwrap()
                .task_id,
        );
    }
    let mut seen = Vec::new();
    let (mut before_ms, mut before_id) = (None, None::<String>);
    loop {
        let page = store
            .history(before_ms, before_id.as_deref(), Some(2))
            .unwrap();
        seen.extend(page.tasks.iter().map(|t| t.task_id.clone()));
        let Some(next) = page.next else { break };
        before_ms = Some(next.before_ms);
        before_id = Some(next.before_task_id);
    }
    let mut expected: Vec<_> = same.iter().rev().cloned().collect();
    expected.push(older.task_id);
    assert_eq!(seen, expected, "every task exactly once, newest first");
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn draft_capacity_is_bounded() {
    let env = Env::new();
    let mut store = env.open();
    for _ in 0..codepet_core::MAX_TASKS {
        store.create_task(&env.input(Category::Hunger)).unwrap();
    }
    assert!(matches!(
        store.create_task(&env.input(Category::Hunger)),
        Err(StorageError::Capacity)
    ));
}

#[test]
fn pause_is_persisted() {
    let env = Env::new();
    let mut store = env.open();
    let paused = |s: &Snapshot| s.life.as_ref().unwrap().paused;
    assert!(store.set_paused(true).unwrap().paused);
    assert!(paused(&env.open().snapshot().unwrap()));
    assert!(!store.set_paused(false).unwrap().paused);
    assert!(!paused(&store.snapshot().unwrap()));
}

#[test]
fn newer_schema_is_refused_untouched() {
    let env = Env::new();
    env.paths.prepare().unwrap();
    {
        let c = env.raw();
        c.execute_batch("CREATE TABLE future(x); PRAGMA user_version = 5;")
            .unwrap();
    }
    let bytes = fs::read(&env.paths.db_path).unwrap();
    assert!(matches!(
        env.try_open(),
        Err(StorageError::SchemaUnsupported(5))
    ));
    assert_eq!(fs::read(&env.paths.db_path).unwrap(), bytes);
    assert_eq!(fs::read_dir(&env.paths.state_dir).unwrap().count(), 1);
}

#[test]
fn unversioned_foreign_database_is_refused() {
    let env = Env::new();
    env.paths.prepare().unwrap();
    env.raw().execute_batch("CREATE TABLE other(x);").unwrap();
    let bytes = fs::read(&env.paths.db_path).unwrap();
    assert!(matches!(
        env.try_open(),
        Err(StorageError::SchemaUnsupported(0))
    ));
    assert_eq!(fs::read(&env.paths.db_path).unwrap(), bytes);
}

#[test]
fn corrupt_database_is_refused_untouched() {
    let env = Env::new();
    env.paths.prepare().unwrap();
    let garbage: Vec<u8> = (0..8192u32).map(|i| (i * 7 % 251) as u8).collect();
    fs::write(&env.paths.db_path, &garbage).unwrap();
    assert!(matches!(env.try_open(), Err(StorageError::Corrupt)));
    assert_eq!(fs::read(&env.paths.db_path).unwrap(), garbage);

    // A valid header with damaged b-tree pages.
    let env = Env::new();
    drop(env.open());
    {
        let c = env.raw();
        c.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .unwrap();
        c.execute_batch("PRAGMA journal_mode = DELETE").unwrap();
    }
    let mut bytes = fs::read(&env.paths.db_path).unwrap();
    let page = 4096;
    assert!(bytes.len() > 3 * page);
    for b in &mut bytes[page..3 * page] {
        *b = 0xA5;
    }
    fs::write(&env.paths.db_path, &bytes).unwrap();
    assert!(matches!(env.try_open(), Err(StorageError::Corrupt)));
    assert_eq!(fs::read(&env.paths.db_path).unwrap(), bytes);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn migration_backs_up_older_versions_first() {
    fn v5(tx: &rusqlite::Transaction<'_>, _at: &care::TimeMark) -> Result<()> {
        Ok(tx.execute_batch("CREATE TABLE extra (x INTEGER)")?)
    }
    let env = Env::new();
    let id = {
        let mut store = env.open();
        store
            .create_task(&env.input(Category::Hunger))
            .unwrap()
            .task_id
    };
    let migrations: &[schema::Migration] = &[
        schema::MIGRATIONS[0],
        schema::MIGRATIONS[1],
        schema::MIGRATIONS[2],
        schema::MIGRATIONS[3],
        v5,
    ];
    env.clock.set(T0 + 42);
    let store = Store::open_with(&env.paths, env.clock.clone(), migrations).unwrap();
    assert_eq!(store.task(&id).unwrap().task_id, id);
    drop(store);
    assert_eq!(env.count("PRAGMA user_version"), 5);
    assert_eq!(env.count("SELECT count(*) FROM extra"), 0);
    let backup = env
        .paths
        .state_dir
        .join(format!("codepet.sqlite3.bak-v4-{}", T0 + 42));
    assert_eq!(mode(&backup), 0o600);
    let b = Connection::open(&backup).unwrap();
    let v: i64 = b
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, 4);
    let n: i64 = b
        .query_row("SELECT count(*) FROM tasks WHERE id = ?1", [&id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(n, 1);
    // This build (v4) now refuses the upgraded file without touching it.
    let bytes = fs::read(&env.paths.db_path).unwrap();
    assert!(matches!(
        env.try_open(),
        Err(StorageError::SchemaUnsupported(5))
    ));
    assert_eq!(fs::read(&env.paths.db_path).unwrap(), bytes);
    // Reopening at the same version neither migrates nor backs up again.
    Store::open_with(&env.paths, env.clock.clone(), migrations).unwrap();
    let backups = fs::read_dir(&env.paths.state_dir)
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".bak-")
        })
        .count();
    assert_eq!(backups, 1);
}

#[test]
fn unsafe_state_dirs_are_refused_without_chmod() {
    let env = Env::new();
    fs::create_dir_all(&env.paths.state_dir).unwrap();
    fs::set_permissions(&env.paths.state_dir, fs::Permissions::from_mode(0o750)).unwrap();
    assert!(matches!(
        env.try_open(),
        Err(StorageError::UnsafeStateDir(_))
    ));
    assert_eq!(mode(&env.paths.state_dir), 0o750);
    assert!(!env.paths.db_path.exists());

    let env = Env::new();
    let real = env.root.join("elsewhere");
    fs::create_dir(&real).unwrap();
    fs::set_permissions(&real, fs::Permissions::from_mode(0o700)).unwrap();
    fs::create_dir_all(env.paths.state_dir.parent().unwrap()).unwrap();
    symlink(&real, &env.paths.state_dir).unwrap();
    assert!(matches!(
        env.try_open(),
        Err(StorageError::UnsafeStateDir(_))
    ));
    assert_eq!(fs::read_dir(&real).unwrap().count(), 0);

    let env = Env::new();
    env.paths.prepare().unwrap();
    fs::set_permissions(&env.paths.db_path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(matches!(
        env.try_open(),
        Err(StorageError::UnsafeStateDir(_))
    ));
    assert_eq!(mode(&env.paths.db_path), 0o644);

    let env = Env::new();
    env.paths.prepare().unwrap();
    fs::remove_file(&env.paths.db_path).unwrap();
    let target = env.root.join("target.sqlite3");
    symlink(&target, &env.paths.db_path).unwrap();
    assert!(matches!(
        env.try_open(),
        Err(StorageError::UnsafeStateDir(_))
    ));
    assert!(!target.exists());

    let env = Env::new();
    fs::create_dir_all(&env.paths.state_dir).unwrap();
    fs::set_permissions(&env.paths.state_dir, fs::Permissions::from_mode(0o700)).unwrap();
    fs::create_dir(&env.paths.db_path).unwrap();
    assert!(matches!(
        env.try_open(),
        Err(StorageError::UnsafeStateDir(_))
    ));
}

#[test]
fn state_dir_resolution() {
    let env_of = |pairs: &'static [(&'static str, &'static str)]| {
        move |k: &str| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| std::ffi::OsString::from(v))
        }
    };
    let p = StatePaths::resolve(env_of(&[("HOME", "/h/u")])).unwrap();
    assert_eq!(p.state_dir, PathBuf::from("/h/u/.local/state/codepet"));
    assert_eq!(
        p.db_path,
        PathBuf::from("/h/u/.local/state/codepet/codepet.sqlite3")
    );
    assert_eq!(p.home, Some(PathBuf::from("/h/u")));
    let p = StatePaths::resolve(env_of(&[("HOME", "/h/u"), ("XDG_STATE_HOME", "")])).unwrap();
    assert_eq!(p.state_dir, PathBuf::from("/h/u/.local/state/codepet"));
    let p = StatePaths::resolve(env_of(&[("XDG_STATE_HOME", "/x/s")])).unwrap();
    assert_eq!(p.state_dir, PathBuf::from("/x/s/codepet"));
    assert_eq!(p.home, None);
    for bad in [
        &[("XDG_STATE_HOME", "rel/state"), ("HOME", "/h/u")][..],
        &[("XDG_STATE_HOME", "/x/../s")][..],
        &[("HOME", "relative")][..],
        &[][..],
    ] {
        let owned: Vec<_> = bad.to_vec();
        let r = StatePaths::resolve(|k| {
            owned
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| std::ffi::OsString::from(v))
        });
        assert!(matches!(r, Err(StorageError::UnsafeStateDir(_))), "{bad:?}");
    }
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn supervisor_identity_uses_start_time() {
    let me = sup();
    assert_eq!(me.pid, std::process::id());
    assert!(me.is_alive());
    assert!(!dead_sup().is_alive());
    assert!(!Supervisor {
        pid: 0,
        start_time: me.start_time
    }
    .is_alive());
    assert!(!Supervisor {
        pid: u32::MAX,
        start_time: 0
    }
    .is_alive());
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn injected_crash_before_commit_leaves_no_trace() {
    let env = Env::new();
    let (id, digest) = {
        let mut store = env.open();
        reviewed(
            &mut store,
            &env,
            Category::Thirst,
            Verification::NotVerified,
        )
    };
    {
        let mut store = env.open();
        test_fault::arm(Some("accept_before_commit"));
        let r = catch_unwind(AssertUnwindSafe(|| {
            accept(&mut store, &id, &digest, "crash", Some("ok"))
        }));
        test_fault::arm(None);
        assert!(r.is_err());
    }
    let mut store = env.open();
    assert_eq!(store.task(&id).unwrap().state, State::ReviewReady);
    assert_eq!(meters(&store).thirst, 9000);
    for table in ["rewards", "acceptances", "demo_xp_rewards"] {
        assert_eq!(env.count(&format!("SELECT count(*) FROM {table}")), 0);
    }
    assert_eq!(
        env.count("SELECT count(*) FROM request_results WHERE method = 'task.accept_artifact'"),
        0
    );
    let out = accept(&mut store, &id, &digest, "crash", Some("ok")).unwrap();
    assert!(!out.duplicate);
    assert_eq!(meters(&store).thirst, 9000);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn injected_crash_after_commit_keeps_single_reward() {
    let env = Env::new();
    let (id, digest) = {
        let mut store = env.open();
        reviewed(
            &mut store,
            &env,
            Category::Thirst,
            Verification::NotVerified,
        )
    };
    {
        let mut store = env.open();
        test_fault::arm(Some("accept_after_commit"));
        let r = catch_unwind(AssertUnwindSafe(|| {
            accept(&mut store, &id, &digest, "crash", Some("ok"))
        }));
        test_fault::arm(None);
        assert!(r.is_err());
    }
    let mut store = env.open();
    assert_eq!(store.task(&id).unwrap().state, State::AcceptedArtifact);
    let again = accept(&mut store, &id, &digest, "crash", Some("ok")).unwrap();
    assert!(!again.duplicate);
    assert_eq!(again.receipt.request_id, "crash");
    assert_eq!(meters(&store).thirst, 9000);
    assert_eq!(env.count("SELECT count(*) FROM demo_xp_rewards"), 1);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn injected_crash_before_review_commit_stays_verifying() {
    let env = Env::new();
    let id = {
        let mut store = env.open();
        let id = started(&mut store, &env, Category::Hunger, sup());
        store
            .supervisor_step(&id, sup(), State::Preparing, State::Running)
            .unwrap();
        store
            .supervisor_step(&id, sup(), State::Running, State::Verifying)
            .unwrap();
        test_fault::arm(Some("review_before_commit"));
        let r = catch_unwind(AssertUnwindSafe(|| {
            store.seal_review(&id, sup(), ARTIFACT, Verification::NotVerified, true)
        }));
        test_fault::arm(None);
        assert!(r.is_err());
        id
    };
    let mut store = env.open();
    let t = store.task(&id).unwrap();
    assert_eq!(t.state, State::Verifying);
    assert!(t.review.is_none());
    assert_eq!(env.count("SELECT count(*) FROM artifacts"), 0);
    // The supervisor (this process) is still alive, so recovery leaves it alone.
    assert!(store.recover().unwrap().changed.is_empty());
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn injected_crash_after_start_commit_is_recoverable() {
    let env = Env::new();
    let mut store = env.open();
    let (id, rev, token) = ready(&mut store, &env, Category::Hunger);
    test_fault::arm(Some("start_after_commit"));
    let r = catch_unwind(AssertUnwindSafe(|| {
        store.start(&id, rev, &token, dead_sup())
    }));
    test_fault::arm(None);
    assert!(r.is_err());
    let mut store = env.open();
    assert_eq!(store.task(&id).unwrap().state, State::Preparing);
    let r = store.recover().unwrap();
    assert_eq!(r.changed[0].to, State::Interrupted);
}

#[test]
fn fault_points_are_inert_without_env() {
    // CODEPET_FAULT is not set for the test process; every point must be a no-op.
    if std::env::var_os("CODEPET_FAULT").is_none() {
        for name in FAULT_POINTS {
            fault(name);
        }
    }
}

#[test]
fn error_codes_map_to_protocol() {
    let cases: Vec<(StorageError, &str)> = vec![
        (StorageError::Busy, "BUSY"),
        (StorageError::InvalidState("x"), "INVALID_STATE"),
        (StorageError::StaleRevision, "STALE_REVISION"),
        (StorageError::UnsupportedProvider, "UNSUPPORTED_PROVIDER"),
        (StorageError::NotFound, "NOT_FOUND"),
        (StorageError::TokenInvalid, "TOKEN_INVALID"),
        (StorageError::TokenExpired, "TOKEN_EXPIRED"),
        (StorageError::ReviewChanged, "REVIEW_CHANGED"),
        (StorageError::ReviewDamaged, "REVIEW_CHANGED"),
        (
            StorageError::AcknowledgementRequired,
            "ACKNOWLEDGEMENT_REQUIRED",
        ),
        (StorageError::OverrideRequired, "OVERRIDE_REQUIRED"),
        (StorageError::IdempotencyConflict, "IDEMPOTENCY_CONFLICT"),
        (StorageError::InvalidParams("x"), "INVALID_PARAMS"),
        (StorageError::InvalidPath("x"), "INVALID_PARAMS"),
        (StorageError::Capacity, "CAPACITY"),
        (StorageError::ResyncRequired, "RESYNC_REQUIRED"),
        (StorageError::SchemaUnsupported(9), "SCHEMA_UNSUPPORTED"),
        (StorageError::Corrupt, "STORAGE_UNAVAILABLE"),
        (
            StorageError::UnsafeStateDir("x".into()),
            "STORAGE_UNAVAILABLE",
        ),
        (StorageError::Unavailable("x"), "STORAGE_UNAVAILABLE"),
        (
            StorageError::Io(std::io::Error::other("x")),
            "STORAGE_UNAVAILABLE",
        ),
    ];
    for (e, code) in cases {
        assert_eq!(e.code(), code);
        assert!(!e.to_string().is_empty());
        assert!(!e.retryable());
    }
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn ids_are_uuid_v4() {
    let env = Env::new();
    let mut store = env.open();
    let a = store
        .create_task(&env.input(Category::Hunger))
        .unwrap()
        .task_id;
    let b = store
        .create_task(&env.input(Category::Hunger))
        .unwrap()
        .task_id;
    assert_ne!(a, b);
    for id in [a, b] {
        assert_eq!(id.len(), 36);
        let parts: Vec<_> = id.split('-').map(str::len).collect();
        assert_eq!(parts, [8, 4, 4, 4, 12]);
        assert_eq!(&id[14..15], "4");
        assert!(matches!(&id[19..20], "8" | "9" | "a" | "b"));
    }
}

#[test]
fn connection_pragmas_are_applied() {
    let env = Env::new();
    let store = env.open();
    let get = |sql: &str| -> i64 { store.conn().query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(get("PRAGMA foreign_keys"), 1);
    assert_eq!(get("PRAGMA synchronous"), 2);
    assert_eq!(get("PRAGMA busy_timeout"), 5000);
    // Foreign keys reject orphan rows written through the store's connection.
    assert!(store
        .conn()
        .execute(
            "INSERT INTO task_scopes (task_id, position, path) VALUES ('nope', 0, 'src')",
            [],
        )
        .is_err());
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn concurrent_starts_from_separate_connections_admit_one() {
    let env = Env::new();
    let mut handles: Vec<(Store, String, i64, String)> = (0..4)
        .map(|_| {
            let mut s = env.open();
            let (id, rev, token) = ready(&mut s, &env, Category::Hunger);
            (s, id, rev, token)
        })
        .collect();
    let barrier = std::sync::Barrier::new(handles.len());
    let results: Vec<Result<StartResult>> = std::thread::scope(|scope| {
        let joins: Vec<_> = handles
            .iter_mut()
            .map(|(s, id, rev, token)| {
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    s.start(id, *rev, token, sup())
                })
            })
            .collect();
        joins.into_iter().map(|j| j.join().unwrap()).collect()
    });
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(results
        .iter()
        .filter_map(|r| r.as_ref().err())
        .all(|e| matches!(e, StorageError::Busy)));
    assert_eq!(
        env.count(&format!(
            "SELECT count(*) FROM tasks WHERE state IN {}",
            schema::SLOT_STATES
        )),
        1
    );
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn concurrent_accepts_reward_once() {
    let env = Env::new();
    let (id, digest) = {
        let mut store = env.open();
        reviewed(
            &mut store,
            &env,
            Category::Hunger,
            Verification::NotVerified,
        )
    };
    let mut stores: Vec<Store> = (0..4).map(|_| env.open()).collect();
    let barrier = std::sync::Barrier::new(stores.len());
    let outcomes: Vec<AcceptOutcome> = std::thread::scope(|scope| {
        let joins: Vec<_> = stores
            .iter_mut()
            .enumerate()
            .map(|(i, s)| {
                let (barrier, id, digest) = (&barrier, &id, &digest);
                scope.spawn(move || {
                    barrier.wait();
                    accept(s, id, digest, &format!("req-{i}"), Some("ok")).unwrap()
                })
            })
            .collect();
        joins.into_iter().map(|j| j.join().unwrap()).collect()
    });
    assert_eq!(outcomes.iter().filter(|o| !o.duplicate).count(), 1);
    assert!(outcomes.windows(2).all(|w| w[0].receipt == w[1].receipt));
    assert_eq!(meters(&env.open()).hunger, 9000);
    assert_eq!(env.count("SELECT count(*) FROM demo_xp_rewards"), 1);
}

#[cfg(feature = "legacy-tasks")]
/// Builds the largest draft the store accepts: JSON-hostile instructions (quotes and control
/// characters), long scope entries, and a folder padded to exactly `MAX_DRAFT_JSON_BYTES`.
fn max_input(env: &Env) -> TaskInput {
    let mut input = env.input(Category::Hygiene);
    let d = &mut input.draft;
    d.instructions = format!("{}{}", "\"".repeat(8000), "\u{1}".repeat(1000));
    d.scope = (0..4).map(|i| format!("{i}{}", "s".repeat(1999))).collect();
    d.folder = "/p".into();
    let size = |d: &Draft| {
        serde_json::to_vec(&(&d.folder, &d.scope, &d.instructions))
            .unwrap()
            .len()
    };
    let pad = MAX_DRAFT_JSON_BYTES - size(d);
    d.folder.push_str(&"q".repeat(pad));
    assert_eq!(size(d), MAX_DRAFT_JSON_BYTES);
    assert!(d.folder.len() <= 4096);
    input
}

#[cfg(feature = "legacy-tasks")]
/// A maximal draft carried to review with a maximal artifact.
fn max_review(store: &mut Store, env: &Env) -> (String, String) {
    make_ready(store, env);
    let t = store.create_task(&max_input(env)).unwrap();
    let p = store.preflight(&t.task_id, 1).unwrap();
    let s = sup();
    store.start(&t.task_id, 1, &p.preflight_token, s).unwrap();
    store
        .supervisor_step(&t.task_id, s, State::Preparing, State::Running)
        .unwrap();
    store
        .supervisor_step(&t.task_id, s, State::Running, State::Verifying)
        .unwrap();
    let artifact = "\"".repeat((MAX_ARTIFACT_JSON_BYTES - 2) / 2);
    match store
        .seal_review(&t.task_id, s, &artifact, Verification::NotVerified, true)
        .unwrap()
    {
        SealOutcome::Sealed { digest, .. } => (t.task_id, digest),
        SealOutcome::Cancelling => panic!("unexpected cancel"),
    }
}

/// Response envelope around a result: `{"v":1,"id":"<128 bytes>","ok":true,"result":…}\n`.
fn fits_frame<T: serde::Serialize>(what: &str, value: &T) {
    let envelope = r#"{"v":1,"id":"","ok":true,"result":}"#.len() + 128 + 1;
    let n = serde_json::to_vec(value).unwrap().len() + envelope;
    assert!(n <= 65536, "{what}: {n} bytes");
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn draft_budget_is_json_encoded_size() {
    let env = Env::new();
    let mut store = env.open();
    let mut input = max_input(&env);
    let t = store.create_task(&input).unwrap();
    fits_frame("task.create", &t);
    fits_frame("task.preflight", &store.preflight(&t.task_id, 1).unwrap());
    input.draft.folder.push('q');
    assert!(matches!(
        store.create_task(&input),
        Err(StorageError::InvalidParams(_))
    ));
    assert!(matches!(
        store.update_task(&t.task_id, 1, &input),
        Err(StorageError::InvalidParams(_))
    ));
    // 16 KiB of control characters is within core's byte limit but encodes to ~96 KiB.
    let mut escaped = env.input(Category::Hunger);
    escaped.draft.instructions = "\u{1}".repeat(codepet_core::MAX_PROMPT_BYTES);
    assert!(escaped.draft.validate().is_ok());
    assert!(matches!(
        store.create_task(&escaped),
        Err(StorageError::InvalidParams(_))
    ));
    assert_eq!(env.count("SELECT count(*) FROM tasks"), 1);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn maximal_tasks_can_always_be_read_reviewed_and_released() {
    let env = Env::new();
    let mut store = env.open();
    let reason = "\"".repeat(MAX_OVERRIDE_REASON_BYTES);
    // Ten accepted maximal tasks with maximal receipts fill `recent`.
    for i in 0..RECENT_TASKS {
        let (id, digest) = max_review(&mut store, &env);
        let request_id = format!("{i:0>128}");
        let out = accept(&mut store, &id, &digest, &request_id, Some(&reason)).unwrap();
        fits_frame("task.accept_artifact", &out);
        fits_frame("task.get accepted", &store.task(&id).unwrap());
        env.clock.advance(1);
    }
    // An eleventh holds the slot in review_ready.
    let (id, digest) = max_review(&mut store, &env);
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.recent.len(), RECENT_TASKS as usize);
    fits_frame("state.get", &snapshot);
    let brief = snapshot.active_task.unwrap();
    assert_eq!(brief.review.as_ref().unwrap().digest, digest);
    assert!(
        serde_json::to_vec(&brief.instructions_excerpt)
            .unwrap()
            .len()
            <= 256
    );
    fits_frame("task.review", &store.review(&id).unwrap());
    fits_frame("task.recover", &store.recover().unwrap());
    let page = store.history(None, None, None).unwrap();
    fits_frame("history.list", &page);
    assert_eq!(page.tasks.len(), 11);
    // The digest from state.get is enough to release the slot.
    let discarded = store.discard(&id, &digest).unwrap();
    fits_frame("task.discard", &discarded);
    assert!(store.snapshot().unwrap().active_task.is_none());
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn history_pages_stop_at_the_byte_budget() {
    let env = Env::new();
    let mut store = env.open();
    let reason = "\"".repeat(MAX_OVERRIDE_REASON_BYTES);
    let mut accepted = Vec::new();
    for i in 0..40 {
        let (id, digest) = reviewed(&mut store, &env, Category::Thirst, Verification::Failed);
        accept(
            &mut store,
            &id,
            &digest,
            &format!("{i:0>128}"),
            Some(&reason),
        )
        .unwrap();
        accepted.push(id);
        env.clock.advance(1);
    }
    let page = store.history(None, None, None).unwrap();
    assert!(page.tasks.len() < accepted.len(), "{}", page.tasks.len());
    assert!(serde_json::to_vec(&page.tasks).unwrap().len() <= MAX_HISTORY_PAGE_JSON_BYTES);
    fits_frame("history.list", &page);
    let next = page.next.unwrap();
    let rest = store
        .history(Some(next.before_ms), Some(&next.before_task_id), None)
        .unwrap();
    assert_eq!(page.tasks.len() + rest.tasks.len(), accepted.len());
    assert!(rest.next.is_none());
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn json_prefix_matches_serde_encoding() {
    let samples = [
        "plain ascii text",
        "quotes \" and \\ backslashes",
        "\u{1}\u{2}\n\t\r\u{8}\u{c}\u{1f}",
        "多字节 текст 🦀 mixed",
        "",
    ];
    for s in samples {
        for max in 2..80 {
            let p = store::json_prefix(s, max);
            assert!(s.starts_with(p));
            let enc = serde_json::to_string(p).unwrap().len();
            assert!(enc <= max, "{s:?} {max}");
            if p.len() < s.len() {
                let next = s[p.len()..].chars().next().unwrap();
                let longer = &s[..p.len() + next.len_utf8()];
                assert!(serde_json::to_string(longer).unwrap().len() > max);
            }
        }
    }
}

#[path = "care_tests.rs"]
mod care_cases;

// ---- shared lifecycle helpers (used by the v0.2 test modules below) ----------------------

/// Boot clock identity used by lifecycle tests.
const BOOT: &str = "boot-a";

impl Env {
    /// An environment whose clock also has a CLOCK_BOOTTIME pair, so presence settles.
    fn with_boot() -> Self {
        let env = Self::new();
        env.clock.set_steady(BOOT, 1_000_000);
        env
    }
    fn presence_path(&self) -> PathBuf {
        self.paths.presence_path()
    }
    /// Writes a valid `presence.json` (mode 0600) claiming `attended_ms` for `session`.
    fn write_presence(&self, session: &str, life_id: &str, attended_ms: i64) {
        let now = self.clock.now_ms();
        let ticks = (attended_ms + 59_999) / 60_000;
        let doc = serde_json::json!({
            "v": 1, "session": session, "life_id": life_id,
            "started_wall_ms": now - attended_ms, "updated_wall_ms": now,
            "attended_ms": attended_ms, "ticks": ticks
        });
        self.write_presence_raw(&doc.to_string());
    }
    fn write_presence_raw(&self, text: &str) {
        let path = self.presence_path();
        let _ = fs::remove_file(&path);
        fs::write(&path, text).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    /// Time passes for the wall and boot clocks together.
    fn pass(&self, ms: i64) {
        self.clock.advance(ms);
    }
    fn rejections(&self) -> i64 {
        self.count("SELECT presence_rejections FROM meta")
    }
    /// One table's rows, as text, in rowid order.
    fn dump(&self, table: &str) -> String {
        let c = self.raw();
        let mut stmt = c
            .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
            .unwrap();
        let cols = stmt.column_count();
        let rows = stmt
            .query_map([], |r| {
                let mut out = Vec::new();
                for i in 0..cols {
                    let v: rusqlite::types::Value = r.get(i)?;
                    out.push(format!("{v:?}"));
                }
                Ok(out.join("|"))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        rows.join("\n")
    }
}

/// The living companion's id.
fn life_id(store: &Store) -> String {
    store
        .snapshot()
        .unwrap()
        .life
        .expect("a living companion")
        .life_id
}

/// Arms the living companion with `mode` (acknowledging the rules).
fn arm(store: &mut Store, mode: codepet_core::lifecycle::Mode) -> LifeOutcome {
    let id = life_id(store);
    store
        .set_rules(&sys::uuid_v4().unwrap(), &id, mode, true)
        .unwrap()
}

#[path = "bar_export_tests.rs"]
mod bar_export_cases;
#[path = "life_tests.rs"]
mod life_cases;
#[path = "lifecycle_tests.rs"]
mod lifecycle_cases;
#[path = "migration_v4_tests.rs"]
mod migration_v4_cases;
#[path = "presence_tests.rs"]
mod presence_cases;

/// An armed (permadeath) companion with full meters, on a clock with a boot pair.
fn armed(env: &Env) -> (Store, String) {
    let mut store = env.open();
    store.care(&sys::uuid_v4().unwrap(), "routine").unwrap();
    arm(&mut store, codepet_core::lifecycle::Mode::Permadeath);
    let id = life_id(&store);
    (store, id)
}

#[cfg(feature = "legacy-tasks")]
/// Drives a new task into `state` (any slot state) through the public API, plus raw
/// artifact tampering for `recovery_required`. Returns the task id.
fn task_in_state(store: &mut Store, env: &Env, state: State) -> String {
    let s = sup();
    let id = started(store, env, Category::Thirst, s);
    let step = |store: &mut Store, from, to| {
        assert!(matches!(
            store.supervisor_step(&id, s, from, to).unwrap(),
            Step::Advanced { .. }
        ));
    };
    match state {
        State::Preparing => {}
        State::Running => step(store, State::Preparing, State::Running),
        State::WaitingForUser => {
            step(store, State::Preparing, State::Running);
            step(store, State::Running, State::WaitingForUser);
        }
        State::Verifying => {
            step(store, State::Preparing, State::Running);
            step(store, State::Running, State::Verifying);
        }
        State::Cancelling => {
            step(store, State::Preparing, State::Running);
            store.cancel(&id).unwrap();
        }
        State::ReviewReady | State::RecoveryRequired => {
            step(store, State::Preparing, State::Running);
            step(store, State::Running, State::Verifying);
            store
                .seal_review(&id, s, ARTIFACT, Verification::NotVerified, true)
                .unwrap();
            if state == State::RecoveryRequired {
                env.raw()
                    .execute(
                        "UPDATE artifacts SET content = 'tampered' WHERE task_id = ?1",
                        [&id],
                    )
                    .unwrap();
                store.recover().unwrap();
            }
        }
        other => panic!("{other:?} is not a slot state"),
    }
    assert_eq!(store.task(&id).unwrap().state, state);
    id
}

const SLOT_STATE_LIST: [State; 7] = [
    State::Preparing,
    State::Running,
    State::WaitingForUser,
    State::Verifying,
    State::ReviewReady,
    State::Cancelling,
    State::RecoveryRequired,
];
