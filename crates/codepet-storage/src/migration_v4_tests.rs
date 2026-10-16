//! Schema v4 (the lifecycle migration; spec §7, renumbered because v3 is
//! `review_repairs`): old users never lose a pet, XP, receipts, history or in-flight work.
use super::*;
use codepet_core::lifecycle::HOUR_MS;

/// `PRAGMA table_info` names of every table in `c`.
fn tables(c: &Connection) -> Vec<(String, Vec<String>)> {
    let names: Vec<String> = c
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table'
             AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    names
        .into_iter()
        .map(|t| {
            let cols: Vec<String> = c
                .prepare(&format!("PRAGMA table_info({t})"))
                .unwrap()
                .query_map([], |r| r.get(1))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();
            (t, cols)
        })
        .collect()
}

/// Rows of `table` restricted to `cols`, as text.
fn dump_cols(c: &Connection, table: &str, cols: &[String]) -> String {
    let mut stmt = c
        .prepare(&format!(
            "SELECT {} FROM {table} ORDER BY rowid",
            cols.join(", ")
        ))
        .unwrap();
    let n = cols.len();
    stmt.query_map([], |r| {
        (0..n)
            .map(|i| {
                r.get::<_, rusqlite::types::Value>(i)
                    .map(|v| format!("{v:?}"))
            })
            .collect::<rusqlite::Result<Vec<_>>>()
            .map(|v| v.join("|"))
    })
    .unwrap()
    .collect::<rusqlite::Result<Vec<_>>>()
    .unwrap()
    .join("\n")
}

/// Every pre-existing table, over its pre-existing columns.
fn snapshot_all(c: &Connection, schema: &[(String, Vec<String>)]) -> Vec<(String, String)> {
    schema
        .iter()
        .map(|(t, cols)| (t.clone(), dump_cols(c, t, cols)))
        .collect()
}

/// Opens at schema `version` (a prefix of MIGRATIONS) and closes again.
fn old_db(env: &Env, version: usize) {
    drop(
        Store::open_with(
            &env.paths,
            env.clock.clone(),
            &schema::MIGRATIONS[..version],
        )
        .unwrap(),
    );
}

pub(super) const V1_ROWS: &str = "
INSERT INTO tasks (id,revision,state,category,provider,folder,instructions,mock_outcome,
    mock_step_ms,created_at_ms,updated_at_ms,review_digest,review_verification,review_simulated)
VALUES ('old-accepted',1,'accepted_artifact','hunger','mock','/project','Historical text',
    'artifact',0,1,1,'legacy-digest','not_verified',1);
INSERT INTO rewards (task_id,category,amount_hundredths,applied_hundredths,meter_before,
    meter_after,status,granted_at_ms) VALUES ('old-accepted','hunger',2000,2000,7000,9000,'granted',1);
INSERT INTO acceptances (task_id,request_id,review_digest,verification,override_reason,accepted_at_ms)
VALUES ('old-accepted','legacy-request','legacy-digest','not_verified','historical review',1);
INSERT INTO artifacts (task_id,kind,content,sha256,length,created_at_ms)
VALUES ('old-accepted','text','Keep the original artifact','hash',26,1);
INSERT INTO tasks (id,revision,state,category,provider,folder,instructions,mock_outcome,
    mock_step_ms,created_at_ms,updated_at_ms)
VALUES ('old-draft',3,'draft','thirst','mock','/project','Draft text','artifact',0,2,2),
       ('old-failed',1,'failed','hygiene','mock','/project','Failed','provider_failure',0,3,3),
       ('old-cancelled',1,'cancelled','hygiene','mock','/project','Cancelled','artifact',0,4,4),
       ('old-interrupted',1,'interrupted','hunger','mock','/project','Gone','artifact',0,5,5),
       ('old-discarded',1,'discarded','hunger','mock','/project','Discarded','artifact',0,6,6);
INSERT INTO task_events (task_id,from_state,to_state,revision,at_ms)
VALUES ('old-accepted',NULL,'draft',1,1),('old-accepted','review_ready','accepted_artifact',1,1);
INSERT INTO preflight_tokens (token_sha256,task_id,revision,settings_revision,status,
    created_at_ms,expires_at_ms) VALUES ('tok-pending','old-draft',3,1,'pending',2,9),
    ('tok-used','old-draft',2,1,'consumed',1,9);
UPDATE pet_state SET hunger = 10000, thirst = 2500, mental_health = 8000, hygiene = 10000;
";

/// v2-era demo progression (5 × 20 XP: level 2, stage 2) and a v2 acceptance receipt.
fn insert_v2_rows(c: &Connection) {
    for i in 0..5 {
        c.execute_batch(&format!(
            "INSERT INTO tasks (id,revision,state,category,provider,folder,instructions,
                mock_outcome,mock_step_ms,created_at_ms,updated_at_ms,review_digest,
                review_verification,review_simulated)
             VALUES ('demo-{i}',1,'accepted_artifact','thirst','mock','/project','Demo {i}',
                'artifact',0,{t},{t},'d{i}','not_verified',1);
             INSERT INTO acceptances VALUES ('demo-{i}','demo-req-{i}','d{i}','not_verified',
                'mock reviewed',{t});
             INSERT INTO demo_xp_rewards VALUES ('demo-{i}','quality_performance',20,{t});
             INSERT INTO task_care_usage (task_id, executed, execution_ms)
                VALUES ('demo-{i}',1,1000);",
            t = 10 + i
        ))
        .unwrap();
    }
    let receipt = serde_json::json!({"receipt": {"task_id":"demo-0","category":"thirst",
        "amount_hundredths":0,"verification":"not_verified","override_reason":"mock reviewed",
        "request_id":"demo-req-0","accepted_at_ms":10,"demo_xp":20,"real_xp":0,
        "progression_track":"quality_performance"},"duplicate":false});
    c.execute(
        "INSERT INTO request_results VALUES ('demo-req-0','task.accept_artifact',?1,?2,10)",
        rusqlite::params![
            sys::sha256_hex(
                r#"{"acknowledged":true,"override_reason":"mock reviewed","review_digest":"d0","task_id":"demo-0"}"#
                    .as_bytes()
            ),
            receipt.to_string()
        ],
    )
    .unwrap();
}

/// One in-flight slot task (the single-slot index allows only one).
fn insert_slot_task(c: &Connection, state: &str) {
    let sup = sup();
    c.execute(
        "INSERT INTO tasks (id,revision,state,category,provider,folder,instructions,mock_outcome,
            mock_step_ms,created_at_ms,updated_at_ms,supervisor_pid,supervisor_start_time,
            review_digest,review_verification,review_simulated)
         VALUES ('in-flight',1,?1,'mental_health','mock','/project','In flight','artifact',0,
            20,20,?2,?3,?4,?5,1)",
        rusqlite::params![
            state,
            sup.pid,
            i64::try_from(sup.start_time).unwrap(),
            (state != "running").then_some("f".repeat(64)),
            (state != "running").then_some("not_verified")
        ],
    )
    .unwrap();
    if state != "running" {
        c.execute(
            "INSERT INTO artifacts (task_id,kind,content,sha256,length,created_at_ms)
             VALUES ('in-flight','text','In-flight artifact','h',18,20)",
            [],
        )
        .unwrap();
    }
}

fn life1(env: &Env) -> (String, String, String, i64, String) {
    env.raw()
        .query_row(
            "SELECT life_id, origin, status, armed, name FROM lives WHERE ordinal = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap()
}

/// `tasks` / `task_events` after an upgrade to v5: identical to `before` except that the
/// slot task `in-flight` (if any) moved to its v5 closing state at the migration time and
/// gained exactly one `task_events` row.
fn assert_v5_closure_only(c: &Connection, table: &str, before: &str, after: &str) {
    use rusqlite::OptionalExtension;
    let closed: Option<(String, i64)> = c
        .query_row(
            "SELECT state, updated_at_ms FROM tasks WHERE id = 'in-flight'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .unwrap();
    let Some((state, at)) = closed else {
        assert_eq!(before, after, "{table} changed");
        return;
    };
    assert!(
        matches!(state.as_str(), "interrupted" | "discarded"),
        "{state}"
    );
    assert_eq!(at, T0, "closed at the migration time");
    let (b, a): (Vec<&str>, Vec<&str>) = (before.lines().collect(), after.lines().collect());
    if table == "task_events" {
        assert_eq!(a.len(), b.len() + 1);
        assert_eq!(&a[..b.len()], &b[..]);
        assert!(a[b.len()].contains("\"in-flight\""), "{}", a[b.len()]);
        assert!(
            a[b.len()].contains(&format!("Text(\"{state}\")")),
            "{}",
            a[b.len()]
        );
    } else {
        assert_eq!(a.len(), b.len());
        for (x, y) in b.iter().zip(&a) {
            if x.contains("Text(\"in-flight\")") {
                assert!(y.contains(&format!("Text(\"{state}\")")), "{y}");
            } else {
                assert_eq!(x, y);
            }
        }
    }
}

/// Shared assertions for any pre-v4 → v4 upgrade with `V1_ROWS` present.
fn check_upgrade(
    env: &Env,
    from: usize,
    before: &[(String, String)],
    schema: &[(String, Vec<String>)],
) {
    let c = env.raw();
    assert_eq!(env.count("PRAGMA user_version"), SCHEMA_VERSION);
    // Pre-existing tables are identical over their pre-existing columns, except that
    // pending approvals are invalidated (as every schema step has done).
    let after = snapshot_all(&c, schema);
    for ((table, b), (_, a)) in before.iter().zip(&after) {
        if table == "tasks" || table == "task_events" {
            // The v5 step closes a leftover slot task (checked by v5_closure below); every
            // other task row and every earlier event is untouched.
            assert_v5_closure_only(&c, table, b, a);
        } else if table == "preflight_tokens" {
            assert_eq!(
                b.replace("'pending'", "'invalidated'")
                    .replace("Text(\"pending\")", "Text(\"invalidated\")"),
                *a
            );
        } else {
            assert_eq!(b, a, "{table} changed (from v{from})");
        }
    }
    let (id, origin, status, armed, name) = life1(env);
    assert_eq!(
        (origin.as_str(), status.as_str(), armed, name.as_str()),
        ("migrated", "warning", 0, "Ginger")
    );
    // All historic work belongs to life #1.
    assert_eq!(
        env.count(&format!(
            "SELECT count(*) FROM tasks WHERE life_id IS NOT '{id}'"
        )),
        0
    );
    // Holds only on full meters; nothing backdated.
    assert_eq!(
        env.count(
            "SELECT hold_hunger + hold_thirst + hold_mental_health + hold_hygiene FROM life_vitals"
        ),
        2 * HOUR_MS
    );
    assert_eq!(env.count("SELECT hold_thirst FROM life_vitals"), 0);
    assert_eq!(
        env.count("SELECT neglect_hunger_ms + neglect_thirst_ms + critical_elapsed_ms + death_pending FROM life_vitals"),
        0
    );
    assert_eq!(env.count("SELECT state_version FROM meta"), 1);
    assert_eq!(
        env.count("SELECT count(*) FROM rulesets WHERE id = 'lifecycle-v1'"),
        1
    );
    let backup = env
        .paths
        .state_dir
        .join(format!("codepet.sqlite3.bak-v{from}-{T0}"));
    assert!(backup.is_file(), "{}", backup.display());
    assert_eq!(mode(&backup), 0o600);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn v1_to_v4_keeps_everything_and_creates_an_unarmed_migrated_life() {
    let env = Env::new();
    old_db(&env, 1);
    env.raw().execute_batch(V1_ROWS).unwrap();
    let schema = tables(&env.raw());
    let before = snapshot_all(&env.raw(), &schema);
    let mut store = env.open();
    check_upgrade(&env, 1, &before, &schema);
    let s = store.snapshot().unwrap();
    let life = s.life.unwrap();
    assert_eq!(
        (life.xp, life.level, life.stage, life.tasks_accepted),
        (0, 1, 1, 0)
    );
    assert_eq!(s.meters.thirst, 2500);
    // The historic receipt still replays and still shows its legacy fields.
    let replay = accept(
        &mut store,
        "old-accepted",
        "legacy-digest",
        "legacy-request",
        Some("historical review"),
    );
    // No v1 receipt row exists for this request id, so this is a duplicate acceptance.
    let out = replay.unwrap();
    assert!(out.duplicate);
    assert_eq!(out.receipt.amount_hundredths, 2000);
    assert_eq!(out.receipt.xp_withheld, None);
}

#[cfg(feature = "legacy-tasks")]
fn v2_or_v3_upgrade(from: usize, slot_state: &str) {
    let env = Env::with_boot();
    old_db(&env, from);
    let c = env.raw();
    c.execute_batch(V1_ROWS).unwrap();
    insert_v2_rows(&c);
    insert_slot_task(&c, slot_state);
    if from >= 3 {
        c.execute(
            "INSERT INTO review_repairs (task_id, previous_digest, previous_verification, digest,
                verification, verification_source, repaired_at_ms)
             VALUES ('old-discarded', NULL, NULL, ?1, 'not_verified', 'default', 7)",
            [&"e".repeat(64)],
        )
        .unwrap();
    }
    let progression_before = serde_json::to_value(crate::care::progression(&c).unwrap()).unwrap();
    let schema = tables(&c);
    let before = snapshot_all(&c, &schema);
    drop(c);
    let mut store = env.open();
    check_upgrade(&env, from, &before, &schema);
    let (id, ..) = life1(&env);
    // Life #1 owns the old demo XP: same totals, same account progression.
    assert_eq!(
        env.count(&format!(
            "SELECT count(*) FROM demo_xp_rewards WHERE life_id = '{id}'"
        )),
        5
    );
    let s = store.snapshot().unwrap();
    assert_eq!(
        serde_json::to_value(&s.progression).unwrap(),
        progression_before
    );
    let life = s.life.clone().unwrap();
    assert_eq!(
        (life.xp, life.level, life.stage, life.tasks_accepted),
        (100, 2, 2, 5)
    );
    // The reached stage is history, already seen: never celebrated.
    assert!(s.unseen_events.is_empty());
    assert_eq!(
        env.count("SELECT count(*) FROM life_events WHERE kind = 'evolved' AND dedup_key = 'stage:2' AND seen_at_ms IS NOT NULL AND task_id IS NULL"),
        1
    );
    // v5 closes in-flight work (no reward): the slot is free in every build.
    let t = store.task("in-flight").unwrap();
    let closed = schema::migration_close_state(State::parse(slot_state).unwrap()).unwrap();
    assert_eq!(t.state, closed);
    assert!(s.active_task.is_none());
    // A v2-era receipt replays (serde defaults for the new fields).
    let replay = accept(
        &mut store,
        "demo-0",
        "d0",
        "demo-req-0",
        Some("mock reviewed"),
    )
    .unwrap();
    assert_eq!(replay.receipt.demo_xp, 20);
    assert_eq!(replay.receipt.life_id, None);
    // ...while task.get rebuilds the lifecycle fields from durable rows.
    let rebuilt = store.task("demo-0").unwrap().receipt.unwrap();
    assert_eq!(rebuilt.life_id.as_deref(), Some(id.as_str()));
    assert_eq!((rebuilt.level_before, rebuilt.level_after), (1, 1));
    // The fourth reward (60 → 80 XP) is the one that reached level 2.
    let fourth = store.task("demo-3").unwrap().receipt.unwrap();
    assert_eq!((fourth.level_before, fourth.level_after), (1, 2));
    let fifth = store.task("demo-4").unwrap().receipt.unwrap();
    assert_eq!((fifth.level_before, fifth.level_after), (2, 2));
    // A large presence claim charges nothing while life #1 is unarmed.
    env.pass(8 * HOUR_MS);
    env.write_presence("bar-migrated-01", &id, 8 * HOUR_MS);
    let after = store.snapshot().unwrap();
    assert_eq!(after.meters, s.meters);
    assert_eq!(after.lifecycle.attended_total_ms, 0);
    assert_eq!(after.lifecycle.frozen_reason, "unarmed");
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn v2_to_v4_with_each_in_flight_slot_state() {
    for state in ["running", "review_ready", "recovery_required"] {
        v2_or_v3_upgrade(2, state);
    }
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn v3_to_v4_with_each_in_flight_slot_state() {
    for state in ["running", "review_ready", "recovery_required"] {
        v2_or_v3_upgrade(3, state);
    }
}

#[test]
fn fresh_install_v0_to_v4_is_first_run() {
    let env = Env::with_boot();
    let store = env.open();
    let (id, origin, status, armed, _) = life1(&env);
    assert_eq!(
        (origin.as_str(), status.as_str(), armed),
        ("first_run", "healthy", 0)
    );
    assert_eq!(
        env.count("SELECT count(*) FROM life_events WHERE kind = 'born'"),
        1
    );
    let (boot, steady, wall): (String, i64, i64) = env
        .raw()
        .query_row(
            "SELECT last_boot, last_steady_ms, last_wall_ms FROM presence_clock",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((boot.as_str(), steady, wall), (BOOT, 1_000_000, T0));
    // No backup for a fresh database.
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
    assert_eq!(backups, 0);
    assert_eq!(store.snapshot().unwrap().life.unwrap().life_id, id);
    assert_eq!(id.len(), 29);
}

#[test]
fn a_crash_inside_the_v4_migration_leaves_v3_and_its_backup() {
    fn v4_then_crash(tx: &rusqlite::Transaction<'_>, at: &care::TimeMark) -> Result<()> {
        schema::MIGRATIONS[3](tx, at)?;
        Err(StorageError::Unavailable("injected migration crash"))
    }
    let env = Env::new();
    old_db(&env, 3);
    env.raw().execute_batch(V1_ROWS).unwrap();
    let schema = tables(&env.raw());
    let before = snapshot_all(&env.raw(), &schema);
    let crashing: &[schema::Migration] = &[
        schema::MIGRATIONS[0],
        schema::MIGRATIONS[1],
        schema::MIGRATIONS[2],
        v4_then_crash,
    ];
    assert!(Store::open_with(&env.paths, env.clock.clone(), crashing).is_err());
    assert_eq!(env.count("PRAGMA user_version"), 3);
    assert_eq!(
        env.count(
            "SELECT count(*) FROM sqlite_master WHERE name IN ('lives','meta','life_events')"
        ),
        0
    );
    assert_eq!(snapshot_all(&env.raw(), &schema), before);
    let backup = env
        .paths
        .state_dir
        .join(format!("codepet.sqlite3.bak-v3-{T0}"));
    let b = Connection::open(&backup).unwrap();
    let v: i64 = b
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, 3);
    assert_eq!(snapshot_all(&b, &schema), before);
    // The next open migrates exactly once (with its own backup).
    env.clock.advance(1);
    let store = env.open();
    assert_eq!(env.count("SELECT count(*) FROM lives"), 1);
    assert!(store.snapshot().unwrap().life.is_some());
    assert!(env
        .paths
        .state_dir
        .join(format!("codepet.sqlite3.bak-v3-{}", T0 + 1))
        .is_file());
}

#[test]
fn migration_species_check_matches_the_core_roster() {
    let env = Env::new();
    env.open();
    let c = env.raw();
    for (i, species) in codepet_core::species::SPECIES_IDS.iter().enumerate() {
        c.execute(
            &format!("UPDATE lives SET appearance = '{species}' WHERE ordinal = 1"),
            [],
        )
        .unwrap_or_else(|e| panic!("{i} {species}: {e}"));
    }
    assert!(c
        .execute(
            "UPDATE lives SET appearance = 'dragon' WHERE ordinal = 1",
            []
        )
        .is_err());
}
