//! `bar-state.json` export (spec §10): bar-safe contents, bounds, mode 0600, and a file
//! that only ever moves forward in `state_version`.
use super::*;
use codepet_core::lifecycle::{Mode, Pace, HOUR_MS};

fn read_bar(env: &Env) -> Value {
    serde_json::from_slice(&fs::read(env.paths.bar_state_path()).unwrap()).unwrap()
}

#[test]
fn export_writes_the_contract_shape_with_mode_0600() {
    let env = Env::new();
    let store = env.open();
    assert!(store.export_bar_state().unwrap());
    let path = env.paths.bar_state_path();
    assert_eq!(path, env.paths.state_dir.join("bar-state.json"));
    assert_eq!(mode(&path), 0o600);
    let v = read_bar(&env);
    assert_eq!(
        keys(&v),
        set(&[
            "schema",
            "state_version",
            "generated_at_ms",
            "app_version",
            "ruleset",
            "life",
            "meters",
            "needs",
            "critical",
            "care",
            "task",
            "presence",
            "unseen_events",
            "memorial"
        ])
    );
    assert_eq!(v["schema"], "codepet.bar.v1");
    assert_eq!(v["app_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(v["ruleset"], "lifecycle-v1");
    assert_eq!(v["generated_at_ms"], T0);
    assert_eq!(v["state_version"], store.snapshot().unwrap().state_version);
    assert_eq!(
        keys(&v["life"]),
        set(&[
            "life_id",
            "ordinal",
            "name",
            "appearance",
            "stage",
            "level",
            "xp",
            "next_stage_xp",
            "status",
            "mode",
            "armed",
            "pace",
            "paused",
            "fainted",
            "death_pending"
        ])
    );
    assert_eq!(v["life"]["name"], "Ginger");
    assert_eq!(v["life"]["next_stage_xp"], 80);
    assert_eq!(v["needs"].as_array().unwrap().len(), 4);
    assert_eq!(
        keys(&v["needs"][0]),
        set(&[
            "need",
            "action",
            "state",
            "until_critical_ms",
            "until_end_ms"
        ])
    );
    assert_eq!(
        keys(&v["care"]),
        set(&[
            "sleeping",
            "sleep_remaining_ms",
            "sleep_started_at_ms",
            "sleep_due",
            "bathroom_due",
            "ready_to_work",
            "readiness_reasons"
        ])
    );
    assert_eq!(
        v["presence"],
        serde_json::json!({"wanted": false, "life_id": v["life"]["life_id"],
            "tick_ms": 60000, "idle_timeout_s": 180})
    );
    assert_eq!(
        v["memorial"],
        serde_json::json!({"count": 0, "latest_name": null})
    );
    assert!(v["task"].is_null() && v["critical"].is_null());
    // Nothing newer: skipped, file untouched.
    let bytes = fs::read(&path).unwrap();
    assert!(!store.export_bar_state().unwrap());
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn export_is_bar_safe_and_bounded() {
    let env = Env::new();
    let mut store = env.open();
    let mut input = env.input(Category::Hygiene);
    input.draft.instructions = "SECRET-INSTRUCTIONS-TEXT".into();
    make_ready(&mut store, &env);
    let t = store.create_task(&input).unwrap();
    let p = store.preflight(&t.task_id, 1).unwrap();
    store
        .start(&t.task_id, 1, &p.preflight_token, sup())
        .unwrap();
    assert!(store.export_bar_state().unwrap());
    let text = fs::read_to_string(env.paths.bar_state_path()).unwrap();
    assert!(!text.contains("SECRET-INSTRUCTIONS-TEXT"));
    assert!(!text.contains(env.project.to_str().unwrap()));
    assert!(!text.contains(&t.task_id));
    assert!(text.len() <= MAX_BAR_STATE_BYTES);
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        v["task"],
        serde_json::json!({"state": "preparing", "category": "hygiene"})
    );
    let reasons = v["care"]["readiness_reasons"].as_array().unwrap();
    assert!(reasons.len() <= MAX_READINESS_REASONS);
    assert!(reasons
        .iter()
        .all(|r| r.as_str().unwrap().len() <= MAX_REASON_BYTES));
}

#[test]
fn readiness_reasons_are_cut_at_char_boundaries() {
    let env = Env::new();
    let mut store = env.open();
    let id = life_id(&store);
    store.retire("r", &id, "Ginger").unwrap();
    let long = "\u{1F43E}".repeat(24);
    store
        .adopt(
            "a",
            &AdoptRequest {
                species: "owl",
                name: Some(&long),
                mode: Mode::Permadeath,
                pace: Pace::Steady,
                acknowledge_rules: true,
            },
        )
        .unwrap();
    // Every condition at once: critical, sleep due, bathroom due, all meters low.
    let c = env.raw();
    c.execute(
        "UPDATE pet_state SET hunger = 0, thirst = 0, mental_health = 0, hygiene = 0",
        [],
    )
    .unwrap();
    c.execute(
        "UPDATE pet_care SET rest_required = 1, starts_since_bathroom = 2",
        [],
    )
    .unwrap();
    c.execute(
        "UPDATE life_vitals SET neglect_thirst_ms = 7200000, critical_need = 'thirst',
         critical_grace_ms = 21600000, critical_since_ms = 5",
        [],
    )
    .unwrap();
    c.execute(
        "UPDATE lives SET status = 'critical' WHERE status = 'healthy'",
        [],
    )
    .unwrap();
    store.set_paused(true).unwrap();
    assert!(store.export_bar_state().unwrap());
    let v = read_bar(&env);
    let reasons = v["care"]["readiness_reasons"].as_array().unwrap();
    assert!(reasons.len() <= MAX_READINESS_REASONS);
    assert!(reasons[0].as_str().unwrap().starts_with(&long));
    assert_eq!(v["critical"]["need"], "thirst");
    assert_eq!(v["life"]["status"], "critical");
    assert_eq!(bar_export::byte_prefix("ab\u{e9}", 3), "ab");
    assert_eq!(bar_export::byte_prefix("ab\u{e9}", 4), "ab\u{e9}");
}

#[test]
fn the_file_only_moves_forward() {
    let env = Env::new();
    let mut a = env.open();
    let b = env.open();
    assert!(a.export_bar_state().unwrap());
    a.care("c1", "food").unwrap();
    assert!(b.export_bar_state().unwrap());
    let newest = read_bar(&env)["state_version"].as_i64().unwrap();
    // An older or equal state never overwrites a newer file.
    assert!(!a.export_bar_state().unwrap());
    let path = env.paths.bar_state_path();
    let future = serde_json::json!({"schema": "codepet.bar.v1", "state_version": newest + 100});
    fs::write(&path, future.to_string()).unwrap();
    a.care("c2", "water").unwrap();
    assert!(!a.export_bar_state().unwrap());
    assert_eq!(read_bar(&env)["state_version"], newest + 100);
    // An unreadable file counts as version 0 and is replaced.
    fs::write(&path, "{broken").unwrap();
    assert!(a.export_bar_state().unwrap());
    assert!(read_bar(&env)["state_version"].as_i64().unwrap() > newest);
}

#[test]
fn stale_temp_files_and_symlinks_never_block_or_redirect_the_export() {
    let env = Env::new();
    let store = env.open();
    let dir = &env.paths.state_dir;
    let stale = dir.join(format!(
        "bar-state.json.tmp-{}-deadbeef",
        std::process::id()
    ));
    fs::write(&stale, "left by a crash").unwrap();
    let target = env.root.join("victim.txt");
    fs::write(&target, "keep me").unwrap();
    symlink(&target, env.paths.bar_state_path()).unwrap();
    assert!(store.export_bar_state().unwrap());
    // The link itself was replaced by a regular file; its target is untouched.
    let meta = fs::symlink_metadata(env.paths.bar_state_path()).unwrap();
    assert!(meta.is_file());
    assert_eq!(fs::read_to_string(&target).unwrap(), "keep me");
    assert_eq!(fs::read_to_string(&stale).unwrap(), "left by a crash");
    let temps = fs::read_dir(dir)
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".tmp-")
        })
        .count();
    assert_eq!(temps, 1);
}

#[test]
fn presence_wanted_follows_the_rust_side_eligibility() {
    let env = Env::with_boot();
    let mut store = env.open();
    let wanted = |store: &Store| {
        store.export_bar_state().unwrap();
        read_bar(&env)["presence"]["wanted"].as_bool().unwrap()
    };
    // Unarmed: not wanted.
    assert!(!wanted(&store));
    arm(&mut store, Mode::Permadeath);
    assert!(wanted(&store));
    // Sleep deliberately does not clear it (a nap ends without any write).
    store.care("routine", "routine").unwrap();
    store.sleep("nap").unwrap();
    assert!(wanted(&store));
    store.wake("wake").unwrap();
    store.set_paused(true).unwrap();
    assert!(!wanted(&store));
    store.set_paused(false).unwrap();
    assert!(wanted(&store));
    let task = task_in_state(&mut store, &env, State::ReviewReady);
    assert!(!wanted(&store));
    let digest = store.review(&task).unwrap().review.unwrap().digest;
    store.discard(&task, &digest).unwrap();
    assert!(wanted(&store));
    let id = life_id(&store);
    store.retire("r", &id, "Ginger").unwrap();
    assert!(!wanted(&store));
    let v = read_bar(&env);
    assert!(v["life"].is_null() && v["presence"]["life_id"].is_null());
    assert_eq!(v["memorial"]["latest_name"], "Ginger");
}

#[test]
fn export_never_settles() {
    let env = Env::with_boot();
    let (store, id) = armed(&env);
    env.pass(2 * HOUR_MS);
    env.write_presence("bar-export-0001", &id, 2 * HOUR_MS);
    let tables = ["pet_state", "life_vitals", "presence_sessions", "meta"];
    let before: Vec<String> = tables.iter().map(|t| env.dump(t)).collect();
    assert!(store.export_bar_state().unwrap());
    let after: Vec<String> = tables.iter().map(|t| env.dump(t)).collect();
    assert_eq!(before, after);
    assert_eq!(read_bar(&env)["meters"]["thirst"], 10000);
}
