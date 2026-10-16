//! `bar-state.json` export (spec §10): bar-safe contents, bounds, mode 0600, and a file
//! that only ever moves forward in `state_version`.
use super::*;
#[cfg(feature = "legacy-tasks")]
use codepet_core::lifecycle::Pace;
use codepet_core::lifecycle::{Mode, HOUR_MS};

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
            "db_id",
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
            "memorial",
            "enabled",
            "agent",
            "treats",
            "clients"
        ])
    );
    assert_eq!(v["schema"], "codepet.bar.v1");
    // v0.3 blocks (spec §8) with their WP0 defaults; v0.2 readers ignore them.
    assert_eq!(v["enabled"], true);
    assert_eq!(
        v["agent"],
        serde_json::json!({
            "state": "idle", "apps": [], "stale_at_ms": null,
            "finished_seq": 0, "finished_at_ms": null, "finished_app": null,
            "finished_treat": false, "finished_jar_full": false,
            "hello_seq": 0, "hello_at_ms": null, "hello_app": null,
            "test": null, "error_at_ms": null, "error_app": null, "helpers_busy": false
        })
    );
    assert_eq!(v["treats"], serde_json::json!({"jar": 0, "full": false}));
    assert_eq!(v["clients"], serde_json::json!({}));
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
            "until_warning_ms",
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

#[cfg(feature = "legacy-tasks")]
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

#[cfg(feature = "legacy-tasks")]
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
    // (Same database: the file names this database's db_id.)
    let db_id = read_bar(&env)["db_id"].clone();
    let future = serde_json::json!({"schema": "codepet.bar.v1", "state_version": newest + 100, "db_id": db_id});
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
    #[cfg(feature = "legacy-tasks")]
    {
        let task = task_in_state(&mut store, &env, State::ReviewReady);
        assert!(!wanted(&store));
        let digest = store.review(&task).unwrap().review.unwrap().digest;
        store.discard(&task, &digest).unwrap();
        assert!(wanted(&store));
    }
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

/// The key set of every object in `v`, recursively; arrays contribute their first element
/// and `null` contributes nothing (an `Option` that happens to be empty).
fn shape(v: &Value, at: &str, out: &mut BTreeSet<String>) {
    match v {
        Value::Object(map) => {
            for (k, child) in map {
                let path = format!("{at}.{k}");
                out.insert(path.clone());
                shape(child, &path, out);
            }
        }
        Value::Array(items) => {
            if let Some(first) = items.first() {
                shape(first, &format!("{at}[]"), out);
            }
        }
        _ => {}
    }
}

#[test]
fn need_windows_carry_until_warning_for_the_bar_prediction() {
    // The bar leader predicts a warning from until_warning_ms between settling commands;
    // without it a non-lethal need was never predicted and a lethal one jumped to critical.
    let env = Env::with_boot();
    let (store, _id) = armed(&env);
    let snap = store.snapshot().unwrap();
    assert!(store.export_bar_state().unwrap());
    let v = read_bar(&env);
    let needs = v["needs"].as_array().unwrap();
    assert_eq!(needs.len(), snap.lifecycle.needs.len());
    for (bar, view) in needs.iter().zip(&snap.lifecycle.needs) {
        assert_eq!(bar["need"], view.need.as_str());
        assert_eq!(
            bar["until_warning_ms"].as_i64(),
            view.until_warning_ms,
            "{}",
            view.need
        );
        assert_eq!(
            bar["until_critical_ms"].as_i64(),
            view.until_critical_ms,
            "{}",
            view.need
        );
    }
    // Every need (mood and hygiene included) has a warning window from full meters.
    assert!(needs
        .iter()
        .all(|n| n["until_warning_ms"].as_i64().unwrap_or(0) > 0));
}

#[test]
fn the_ui_fixture_has_exactly_the_exported_shape() {
    // qml/tests/fixtures/bar-state.v1.json feeds the bar model tests; it must not carry
    // fields the Rust export lacks (or miss ones it has).
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../qml/tests/fixtures/bar-state.v1.json"
    ))
    .unwrap();
    let env = Env::with_boot();
    let (store, _id) = armed(&env);
    assert!(store.export_bar_state().unwrap());
    let real = read_bar(&env);
    let (mut a, mut b) = (BTreeSet::new(), BTreeSet::new());
    shape(&real, "", &mut a);
    shape(&fixture, "", &mut b);
    // Options that are null in a fresh export (critical, task, sleep start, latest name)
    // may be filled in the fixture; everything the export emits must be in the fixture.
    let missing: Vec<_> = a.difference(&b).collect();
    assert!(missing.is_empty(), "fixture lacks {missing:?}");
    let extra: Vec<_> = b
        .difference(&a)
        .filter(|k| {
            !k.starts_with(".critical.")
                && !k.starts_with(".task.")
                && !k.starts_with(".unseen_events[].")
        })
        .collect();
    assert!(
        extra.is_empty(),
        "fixture has fields Rust never emits: {extra:?}"
    );
}

#[test]
fn the_export_has_the_v03_contract_shape() {
    // docs/v0.3/contracts/fixtures/bar-v2/*.json are the frozen v0.3 export states. The
    // WP0 export emits the same keys (the label stays codepet.bar.v1 until both readers
    // accept v2, see docs/v0.3/contracts/notes.md).
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../docs/v0.3/contracts/fixtures/bar-v2/clients-mixed.json"
    ))
    .unwrap();
    let env = Env::with_boot();
    let (store, _id) = armed(&env);
    assert!(store.export_bar_state().unwrap());
    let real = read_bar(&env);
    assert_eq!(real["schema"], BAR_SCHEMA);
    assert_eq!(fixture["schema"], BAR_SCHEMA_V2);
    let (mut a, mut b) = (BTreeSet::new(), BTreeSet::new());
    shape(&real, "", &mut a);
    shape(&fixture, "", &mut b);
    let missing: Vec<_> = a.difference(&b).collect();
    assert!(missing.is_empty(), "contract fixture lacks {missing:?}");
    // The fixture fills options a fresh export leaves empty (apps, test, client records).
    let extra: Vec<_> = b
        .difference(&a)
        .filter(|k| {
            !k.starts_with(".critical.")
                && !k.starts_with(".agent.apps[].")
                && !k.starts_with(".agent.test.")
                && !k.starts_with(".clients.")
        })
        .collect();
    assert!(extra.is_empty(), "export lacks contract fields: {extra:?}");
}

#[test]
fn db_id_is_stable_for_one_database_and_shared_with_the_snapshot() {
    let env = Env::new();
    let store = env.open();
    assert!(store.export_bar_state().unwrap());
    let id = read_bar(&env)["db_id"].as_str().unwrap().to_owned();
    assert!(id.starts_with("db-") && id.len() == 27, "{id}");
    assert_eq!(store.snapshot().unwrap().db_id, id);
    drop(store);
    // Reopening the same database keeps it; nothing about a life change alters it.
    let mut store = env.open();
    assert_eq!(store.snapshot().unwrap().db_id, id);
    let life = store.snapshot().unwrap().life.unwrap().life_id;
    store.select_appearance("look", &life, "dog").unwrap();
    assert_eq!(store.snapshot().unwrap().db_id, id);
    // The id never repeats the life id itself.
    assert!(!id.contains(&life[5..]));
}

#[test]
fn a_file_from_another_database_is_replaced_even_with_a_higher_version() {
    // Restoring a pre-lifecycle backup, deleting and recreating the database, or pointing
    // at another state copy leaves a bar-state.json whose counter may be far ahead. It
    // must not pin the bar to the other database's pet until this counter catches up.
    let env = Env::new();
    let store = env.open();
    assert!(store.export_bar_state().unwrap());
    let ours = read_bar(&env);
    let path = env.paths.bar_state_path();
    for foreign in [
        serde_json::json!({"schema": "codepet.bar.v1", "state_version": 1_000_000,
            "db_id": "db-000000000000000000000000", "life": {"name": "Someone else"}}),
        // A file written before db_id existed names no database: also replaced.
        serde_json::json!({"schema": "codepet.bar.v1", "state_version": 1_000_000}),
    ] {
        fs::write(&path, foreign.to_string()).unwrap();
        assert!(store.export_bar_state().unwrap(), "{foreign}");
        let v = read_bar(&env);
        assert_eq!(v["db_id"], ours["db_id"]);
        assert_eq!(v["state_version"], ours["state_version"]);
        assert_eq!(v["life"]["name"], "Ginger");
    }
}

#[test]
fn a_recreated_database_gets_a_new_db_id_and_replaces_the_old_file() {
    let env = Env::new();
    let mut store = env.open();
    for i in 0..5 {
        store.care(&format!("c{i}"), "food").unwrap();
    }
    assert!(store.export_bar_state().unwrap());
    let old = read_bar(&env);
    drop(store);
    // Delete the database (and its WAL/SHM) and start again from nothing.
    for entry in fs::read_dir(&env.paths.state_dir).unwrap() {
        let p = entry.unwrap().path();
        if p.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("codepet.sqlite3")
        {
            fs::remove_file(p).unwrap();
        }
    }
    let store = env.open();
    let fresh = store.snapshot().unwrap();
    assert_ne!(fresh.db_id, old["db_id"].as_str().unwrap());
    assert!(fresh.state_version < old["state_version"].as_i64().unwrap());
    assert!(store.export_bar_state().unwrap());
    let v = read_bar(&env);
    assert_eq!(v["db_id"], fresh.db_id.as_str());
    assert_eq!(v["state_version"], fresh.state_version);
}
