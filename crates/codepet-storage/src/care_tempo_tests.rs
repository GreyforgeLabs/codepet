//! Schema-v6 care tempo through the real transaction and presence paths.
use super::*;
use codepet_core::lifecycle::{CareTempo, Mode, Pace, HOUR_MS};

fn gentle(env: &Env) -> (Store, String) {
    let mut store = env.open();
    let id = life_id(&store);
    store
        .set_rules("arm", &id, Mode::Gentle, true, None)
        .unwrap();
    (store, id)
}
fn tempo(store: &mut Store, id: &str, request: &str, value: CareTempo) -> LifeOutcome {
    let rev = store.snapshot().unwrap().life.unwrap().revision;
    store.set_care_tempo(request, id, rev, value).unwrap()
}

#[test]
fn care_tempo_exact_attended_decay_and_bar_export() {
    for (minutes, expected) in [
        (30, [7500, 6667, 8334, 8750]),
        (60, [4500, 2667, 6334, 7250]),
    ] {
        let env = Env::with_boot();
        let (mut store, id) = gentle(&env);
        store.care("full", "routine").unwrap();
        let old = store.snapshot().unwrap().life.unwrap();
        let out = tempo(&mut store, &id, "lively", CareTempo::Lively);
        assert!(out.changed);
        assert_eq!(out.life.as_ref().unwrap().adopted_at_ms, old.adopted_at_ms);
        assert_eq!(out.life.as_ref().unwrap().xp, old.xp);
        env.pass(minutes * 60_000);
        env.write_presence("bar-tempo-0001", &id, minutes * 60_000);
        let s = store.snapshot().unwrap();
        assert_eq!(
            [
                s.meters.hunger,
                s.meters.thirst,
                s.meters.mental_health,
                s.meters.hygiene
            ],
            expected
        );
        store.export_bar_state().unwrap();
        let bar: Value =
            serde_json::from_slice(&fs::read(env.paths.bar_state_path()).unwrap()).unwrap();
        assert_eq!(bar["life"]["care_tempo"], "lively");
        assert_eq!(bar["life"]["revision"], s.life.unwrap().revision);
        assert_eq!(
            bar["care_rules"],
            serde_json::to_value(s.lifecycle.rules).unwrap()
        );
    }
}

#[test]
fn care_tempo_preserves_carries_holds_critical_clocks_and_receipts() {
    let env = Env::with_boot();
    let (mut store, id) = gentle(&env);
    store.care("full", "routine").unwrap();
    env.raw()
        .execute_batch(
            "UPDATE life_vitals SET carry_hunger=12345, carry_thirst=23456,
        carry_mental_health=34567, carry_hygiene=45678, hold_hunger=123456,
        hold_thirst=987654, hold_mental_health=65432, hold_hygiene=321000;",
        )
        .unwrap();
    let before = lifecycle_store::load(&env.raw()).unwrap();
    let rev = store.snapshot().unwrap().life.unwrap().revision;
    let first = store
        .set_care_tempo("fast", &id, rev, CareTempo::Lively)
        .unwrap();
    let mut expected = before.clone();
    expected.vitals.hold_ms = [123456, 300000, 65432, 300000];
    assert_eq!(lifecycle_store::load(&env.raw()).unwrap(), expected);
    assert_eq!(
        store
            .set_care_tempo("fast", &id, rev, CareTempo::Lively)
            .unwrap(),
        first
    );
    assert!(!tempo(&mut store, &id, "same", CareTempo::Lively).changed);
    tempo(&mut store, &id, "slow", CareTempo::Steady);
    assert_eq!(
        lifecycle_store::load(&env.raw()).unwrap(),
        expected,
        "no hold or fractional carry is refreshed"
    );
    tempo(&mut store, &id, "fast2", CareTempo::Lively);
    assert_eq!(lifecycle_store::load(&env.raw()).unwrap(), expected);
    assert!(matches!(
        store.set_care_tempo("fast", &id, rev, CareTempo::Steady),
        Err(StorageError::IdempotencyConflict)
    ));
    assert!(matches!(
        store.set_care_tempo("stale", &id, rev, CareTempo::Steady),
        Err(StorageError::StaleRevision)
    ));
    assert!(matches!(
        store.set_care_tempo("other", "life-wrong", rev, CareTempo::Steady),
        Err(StorageError::StaleRevision)
    ));
    env.raw()
        .execute_batch(
            "UPDATE pet_state SET thirst=0;
        UPDATE life_vitals SET hold_thirst=0, neglect_thirst_ms=7200000,
        critical_need='thirst', critical_grace_ms=21600000, critical_elapsed_ms=1234,
        critical_since_ms=1;
        UPDATE lives SET status='critical';",
        )
        .unwrap();
    let critical = lifecycle_store::load(&env.raw()).unwrap();
    tempo(&mut store, &id, "slow-critical", CareTempo::Steady);
    tempo(&mut store, &id, "fast-critical", CareTempo::Lively);
    assert_eq!(lifecycle_store::load(&env.raw()).unwrap(), critical);
}

#[test]
fn care_tempo_settles_old_rate_then_preserves_custom_base_leniency() {
    let env = Env::with_boot();
    let (mut store, id) = gentle(&env);
    let rev = store.snapshot().unwrap().life.unwrap().revision;
    store
        .configure_life(
            "lenient",
            &id,
            rev,
            &Leniency {
                decay_percent: Some(50),
                ..Leniency::default()
            },
        )
        .unwrap();
    // No holds at the initial 70%; this pending half hour must use the old rates.
    let rev = store.snapshot().unwrap().life.unwrap().revision;
    env.pass(HOUR_MS / 2);
    env.write_presence("bar-tempo-0002", &id, HOUR_MS / 2);
    store
        .set_care_tempo("fast", &id, rev, CareTempo::Lively)
        .unwrap();
    let s = store.snapshot().unwrap();
    assert_eq!(
        [
            s.meters.hunger,
            s.meters.thirst,
            s.meters.mental_health,
            s.meters.hygiene
        ],
        [6750, 6688, 6850, 6875]
    );
    assert_eq!(s.lifecycle.rules.decay_per_hour, [3000, 4000, 2000, 1500]);
    let rev = s.life.unwrap().revision;
    let next = store
        .configure_life(
            "more-lenient",
            &id,
            rev,
            &Leniency {
                decay_percent: Some(50),
                ..Leniency::default()
            },
        )
        .unwrap();
    assert_eq!(next.lifecycle.rules.decay_per_hour, [1500, 1996, 1000, 750]);
    let slow = tempo(&mut store, &id, "slow", CareTempo::Steady);
    assert_eq!(slow.lifecycle.rules.decay_per_hour, [250, 312, 150, 125]);
    assert_eq!(slow.lifecycle.rules.full_hold_ms, HOUR_MS);
}

#[test]
fn care_tempo_rejects_hardcore_playtest_unarmed_off_and_busy() {
    let env = Env::with_boot();
    let mut store = env.open();
    let id = life_id(&store);
    let rev = store.snapshot().unwrap().life.unwrap().revision;
    assert!(matches!(
        store.set_care_tempo("unarmed", &id, rev, CareTempo::Lively),
        Err(StorageError::InvalidState(_))
    ));
    store
        .set_rules("arm", &id, Mode::Permadeath, true, None)
        .unwrap();
    let rev = store.snapshot().unwrap().life.unwrap().revision;
    assert!(matches!(
        store.set_care_tempo("hardcore", &id, rev, CareTempo::Lively),
        Err(StorageError::InvalidParams(_))
    ));
    store
        .set_rules("gentle", &id, Mode::Gentle, true, None)
        .unwrap();
    tempo(&mut store, &id, "fast", CareTempo::Lively);
    assert!(matches!(
        store.set_rules("hardcore2", &id, Mode::Permadeath, true, Some("Ginger")),
        Err(StorageError::InvalidState(_))
    ));
    tempo(&mut store, &id, "slow", CareTempo::Steady);
    store
        .set_rules("hardcore3", &id, Mode::Permadeath, true, Some("Ginger"))
        .unwrap();
    store
        .set_rules("gentle2", &id, Mode::Gentle, true, None)
        .unwrap();
    let rev = store.snapshot().unwrap().life.unwrap().revision;
    store.set_enabled("off", false).unwrap();
    assert!(matches!(
        store.set_care_tempo("off-tempo", &id, rev, CareTempo::Lively),
        Err(StorageError::Off)
    ));
    store.set_enabled("on", true).unwrap();
    env.raw()
        .execute(
            "INSERT INTO request_results VALUES ('active', 'work.active', 'test', '{}', 1)",
            [],
        )
        .unwrap();
    assert!(matches!(
        store.set_care_tempo("busy", &id, rev, CareTempo::Lively),
        Err(StorageError::InvalidState(_))
    ));
    env.raw()
        .execute("DELETE FROM request_results WHERE request_id='active'", [])
        .unwrap();
    store.retire("retire", &id, "Ginger").unwrap();
    let out = store
        .adopt(
            "playtest",
            &AdoptRequest {
                species: "cat",
                name: None,
                mode: Mode::Gentle,
                pace: Pace::Playtest,
                acknowledge_rules: true,
            },
        )
        .unwrap();
    let l = out.life.unwrap();
    assert_eq!(l.care_tempo, "steady");
    assert!(matches!(
        store.set_care_tempo("bad-pace", &l.life_id, l.revision, CareTempo::Lively),
        Err(StorageError::InvalidParams(_))
    ));
}

#[test]
fn care_tempo_has_no_offline_catchup_or_presence_replay_charge() {
    let env = Env::with_boot();
    let (mut store, id) = gentle(&env);
    tempo(&mut store, &id, "fast", CareTempo::Lively);
    let before = store.snapshot().unwrap().meters;
    env.pass(24 * HOUR_MS);
    assert_eq!(store.snapshot().unwrap().meters, before);
    env.pass(60_000);
    env.write_presence("bar-tempo-0003", &id, 60_000);
    let after = store.snapshot().unwrap().meters;
    assert_eq!(after.hunger, before.hunger - 100);
    assert_eq!(store.snapshot().unwrap().meters, after);
    store.set_enabled("off", false).unwrap();
    env.pass(HOUR_MS);
    env.write_presence("bar-tempo-0003", &id, HOUR_MS + 60_000);
    store.set_enabled("on", true).unwrap();
    assert_eq!(store.snapshot().unwrap().meters, after);
}

/// Every old table and column, in a deterministic order; v6 may only add one column.
fn dump(c: &Connection, layout: &[(String, Vec<String>)]) -> Vec<Vec<Vec<String>>> {
    layout
        .iter()
        .map(|(table, cols)| {
            c.prepare(&format!(
                "SELECT {} FROM {table} ORDER BY rowid",
                cols.join(",")
            ))
            .unwrap()
            .query_map([], |r| {
                (0..cols.len())
                    .map(|i| {
                        r.get::<_, rusqlite::types::Value>(i)
                            .map(|v| format!("{v:?}"))
                    })
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
        })
        .collect()
}

#[test]
fn care_tempo_v5_migration_preserves_all_old_rows_and_refuses_downgrade() {
    let env = Env::with_boot();
    drop(Store::open_with(&env.paths, env.clock.clone(), &schema::MIGRATIONS[..5]).unwrap());
    let c = env.raw();
    c.execute_batch("UPDATE lives SET mode='gentle', armed=1, armed_at_ms=1, decay_hunger=333;
        INSERT INTO activity_turns (session_key,turn_key,client,life_id,earned_at_ms,given_at_ms)
        SELECT '00000000000000000000000000000001','00000000000000000000000000000002','codex',life_id,1,2 FROM lives;
        INSERT INTO activity_xp (life_id,turn_row_id,amount,granted_at_ms,request_id)
        SELECT life_id,1,5,2,'treat-before-v6' FROM lives;").unwrap();
    let tables: Vec<String> = c.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap().query_map([], |r| r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    let layout: Vec<(String, Vec<String>)> = tables
        .into_iter()
        .map(|t| {
            let cols = c
                .prepare(&format!("PRAGMA table_info({t})"))
                .unwrap()
                .query_map([], |r| r.get(1))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();
            (t, cols)
        })
        .collect();
    let before = dump(&c, &layout);
    drop(c);
    let (store, report) = Store::open_migrating(&env.paths, env.clock.clone()).unwrap();
    assert_eq!(report.from, 5);
    assert_eq!(report.to, 6);
    assert!(report.backup.is_some());
    assert_eq!(dump(&env.raw(), &layout), before);
    let s = store.snapshot().unwrap();
    assert_eq!(s.life.as_ref().unwrap().care_tempo, "steady");
    assert_eq!(s.life.as_ref().unwrap().xp, 5);
    assert_eq!(s.lifecycle.rules.decay_per_hour[0], 333);
    drop(store);
    // A snapshot may perform ordinary retention; downgrade must preserve the state
    // as it stands immediately before that attempted open.
    let before_downgrade = dump(&env.raw(), &layout);
    assert!(matches!(
        Store::open_with(&env.paths, env.clock.clone(), &schema::MIGRATIONS[..5]),
        Err(StorageError::SchemaUnsupported(6))
    ));
    assert_eq!(dump(&env.raw(), &layout), before_downgrade);
    assert!(Store::open_for_fold(&env.paths.state_dir, env.clock.clone()).is_ok());
}

#[test]
fn care_tempo_old_lifecycle_receipts_replay_as_steady_without_a_reset() {
    let env = Env::with_boot();
    let (mut store, id) = gentle(&env);
    let old = store
        .select_appearance("old-click", &id, "penguin")
        .unwrap();
    let mut receipt = serde_json::to_value(&old).unwrap();
    receipt["life"]
        .as_object_mut()
        .unwrap()
        .remove("care_tempo");
    env.raw()
        .execute(
            "UPDATE request_results SET response=?1 WHERE request_id='old-click'",
            [receipt.to_string()],
        )
        .unwrap();
    tempo(&mut store, &id, "new-click", CareTempo::Lively);
    let replay = store
        .select_appearance("old-click", &id, "penguin")
        .unwrap();
    assert_eq!(replay.life.unwrap().care_tempo, "steady");
    assert_eq!(store.snapshot().unwrap().life.unwrap().care_tempo, "lively");
}
