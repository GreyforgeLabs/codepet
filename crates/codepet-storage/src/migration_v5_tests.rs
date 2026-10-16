//! Schema v5 (SPEC-v0.3 §6.6, `docs/v0.3/contracts/schema-v5.sql`): existing pets survive.
//!
//! Every v4 database here is built by the v0.2 code paths (the unchanged v1–v4 migrations,
//! plus the exact SQL v0.2's `pet.select` and the task pipeline wrote), never from the
//! owner's real database. The migration runs in the default build (no `legacy-tasks`).
use super::*;
use rusqlite::OptionalExtension;

/// A v4 database (the v0.2.0 / v0.2.1 schema) at `env`, opened and closed by the v0.2
/// migrations only.
fn v4_db(env: &Env) {
    drop(Store::open_with(&env.paths, env.clock.clone(), &schema::MIGRATIONS[..4]).unwrap());
    assert_eq!(env.count("PRAGMA user_version"), 4);
}

/// The owner's v4 state: life #1 "Ginger" (species cat, look `goldfish` through v0.2's
/// `pet.select`), stored mode `permadeath`, never armed, XP 0, the welcome routine still
/// available, no tasks. `pet.select` journals `appearance_changed` and bumps the life's
/// revision and `state_version`, exactly as v0.2's `select_appearance` + `seal` did.
fn owner_shaped_v4(env: &Env) -> String {
    v4_db(env);
    let c = env.raw();
    let life: String = c
        .query_row("SELECT life_id FROM lives WHERE ordinal = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    c.execute(
        "UPDATE lives SET appearance = 'goldfish', revision = revision + 1
         WHERE life_id = ?1 AND status IN ('healthy','warning','critical')",
        [&life],
    )
    .unwrap();
    c.execute(
        "INSERT INTO life_events (life_id, kind, dedup_key, data, request_id, at_ms)
         VALUES (?1, 'appearance_changed', NULL, '{\"from\":\"cat\",\"to\":\"goldfish\"}',
            'owner-select', ?2)",
        rusqlite::params![life, T0],
    )
    .unwrap();
    c.execute(
        "INSERT INTO request_results (request_id, method, params_sha256, response, created_at_ms)
         VALUES ('owner-select', 'pet.select', 'sha', '{}', ?1)",
        [T0],
    )
    .unwrap();
    c.execute(
        "UPDATE meta SET state_version = state_version + 1 WHERE id = 1",
        [],
    )
    .unwrap();
    life
}

/// Row counts of every v4 table (K11 "history row counts unchanged").
fn v4_counts(env: &Env) -> Vec<(String, i64)> {
    [
        "tasks",
        "task_scopes",
        "task_events",
        "preflight_tokens",
        "checks",
        "artifacts",
        "apply_transactions",
        "acceptances",
        "rewards",
        "pet_state",
        "settings",
        "request_results",
        "pet_care",
        "task_care_usage",
        "demo_xp_rewards",
        "review_repairs",
        "meta",
        "rulesets",
        "lives",
        "life_vitals",
        "presence_clock",
        "presence_sessions",
        "life_events",
    ]
    .iter()
    .map(|t| {
        (
            (*t).to_owned(),
            env.count(&format!("SELECT count(*) FROM {t}")),
        )
    })
    .collect()
}

/// Every v4 row (all tables, v4 columns), so v5 is shown to change nothing it should not.
fn v4_dump(c: &Connection) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let tables: Vec<String> = c
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
             ORDER BY name",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    for t in tables {
        let cols: Vec<String> = c
            .prepare(&format!("PRAGMA table_info({t})"))
            .unwrap()
            .query_map([], |r| r.get(1))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let n = cols.len();
        let rows: Vec<String> = c
            .prepare(&format!(
                "SELECT {} FROM {t} ORDER BY rowid",
                cols.join(", ")
            ))
            .unwrap()
            .query_map([], |r| {
                (0..n)
                    .map(|i| {
                        r.get::<_, rusqlite::types::Value>(i)
                            .map(|v| format!("{v:?}"))
                    })
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map(|v| v.join("|"))
            })
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        out.push((format!("{t}({})", cols.join(",")), rows.join("\n")));
    }
    out
}

/// The same tables and columns as `before`, read from `c` now.
fn redump(c: &Connection, before: &[(String, String)]) -> Vec<(String, String)> {
    before
        .iter()
        .map(|(key, _)| {
            let (t, cols) = key.split_once('(').unwrap();
            let cols = cols.trim_end_matches(')');
            let n = cols.split(',').count();
            let rows: Vec<String> = c
                .prepare(&format!("SELECT {cols} FROM {t} ORDER BY rowid"))
                .unwrap()
                .query_map([], |r| {
                    (0..n)
                        .map(|i| {
                            r.get::<_, rusqlite::types::Value>(i)
                                .map(|v| format!("{v:?}"))
                        })
                        .collect::<rusqlite::Result<Vec<_>>>()
                        .map(|v| v.join("|"))
                })
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();
            (key.clone(), rows.join("\n"))
        })
        .collect()
}

fn backups(env: &Env) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(&env.paths.state_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().contains(".bak-"))
        .collect();
    v.sort();
    v
}

#[test]
fn the_owners_ginger_survives_the_v5_migration() {
    let env = Env::with_boot();
    let life = owner_shaped_v4(&env);
    let counts = v4_counts(&env);
    let before = v4_dump(&env.raw());
    let state_version = env.count("SELECT state_version FROM meta");

    // Only `codepet migrate` / state.get may upgrade: every other open refuses untouched.
    let bytes = fs::read(&env.paths.db_path).unwrap();
    assert!(matches!(
        Store::open_current(&env.paths, env.clock.clone()),
        Err(StorageError::MigrationRequired(4))
    ));
    assert_eq!(fs::read(&env.paths.db_path).unwrap(), bytes);
    assert!(backups(&env).is_empty());

    env.clock.set(T0 + 7);
    let (store, report) = Store::open_migrating(&env.paths, env.clock.clone()).unwrap();
    assert_eq!((report.from, report.to), (4, 5));
    let backup = report.backup.clone().expect("a v4 backup");
    assert_eq!(
        backup,
        env.paths
            .state_dir
            .join(format!("codepet.sqlite3.bak-v4-{}", T0 + 7))
    );
    assert_eq!(backups(&env), std::slice::from_ref(&backup));
    assert_eq!(mode(&backup), 0o600);
    assert_eq!(env.count("PRAGMA user_version"), 5);

    // K11: same life id, name, look, XP, journal and history row counts; every v4 row and
    // column is byte-identical (the owner had no tasks, no pending death and no rest
    // counters, so v5's data steps are no-ops for them).
    assert_eq!(v4_counts(&env), counts);
    assert_eq!(redump(&env.raw(), &before), before);
    assert_eq!(env.count("SELECT state_version FROM meta"), state_version);
    let s = store.snapshot().unwrap();
    let l = s.life.clone().unwrap();
    assert_eq!(
        (
            l.life_id.as_str(),
            l.name.as_str(),
            l.species.as_str(),
            l.appearance.as_str(),
            l.mode.as_str(),
            l.armed,
            l.xp,
            l.stage
        ),
        (
            life.as_str(),
            "Ginger",
            "cat",
            "goldfish",
            "permadeath",
            false,
            0,
            1
        )
    );
    assert_eq!(s.schema_version, 5);
    assert!(s.care.welcome_care_available);
    assert_eq!(s.lifecycle.frozen_reason, "unarmed");

    // The v5 additions: CodePet on, helpers off, a fresh 32-byte hook salt, empty agent
    // tables and the sequence counters at zero.
    let c = env.raw();
    let (enabled, helpers, salt, seqs): (i64, i64, Vec<u8>, i64) = c
        .query_row(
            "SELECT enabled, count_helpers, hook_salt,
                events_dropped + finished_seq + hello_seq FROM meta",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!((enabled, helpers, salt.len(), seqs), (1, 0, 32, 0));
    assert!(salt.iter().any(|&b| b != 0));
    for t in [
        "agent_events",
        "agent_sessions",
        "activity_turns",
        "activity_xp",
        "client_hooks",
    ] {
        assert_eq!(env.count(&format!("SELECT count(*) FROM {t}")), 0, "{t}");
    }

    // The backup is the untouched v4 database.
    let b = Connection::open(&backup).unwrap();
    let (v, look): (i64, String) = b
        .query_row(
            "SELECT (SELECT user_version FROM pragma_user_version), appearance FROM lives
             WHERE ordinal = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((v, look.as_str()), (4, "goldfish"));
    drop(b);

    // A v0.2 binary (migrations v1–v4 only) refuses the v5 file without touching it.
    drop(store);
    let bytes = fs::read(&env.paths.db_path).unwrap();
    assert!(matches!(
        Store::open_with(&env.paths, env.clock.clone(), &schema::MIGRATIONS[..4]),
        Err(StorageError::SchemaUnsupported(5))
    ));
    assert_eq!(fs::read(&env.paths.db_path).unwrap(), bytes);

    // Reopening (any mode) neither migrates nor backs up again.
    let (_store, again) = Store::open_migrating(&env.paths, env.clock.clone()).unwrap();
    assert_eq!((again.from, again.backup), (5, None));
    drop(Store::open_current(&env.paths, env.clock.clone()).unwrap());
    assert_eq!(backups(&env).len(), 1);
}

/// One v4 database with a leftover slot task in `state`, a pending and a consumed preflight
/// token, an armed permadeath life whose grace ran out (`death_pending`), demo XP, used rest
/// counters and a historic accepted task. Built with the raw SQL v0.2's pipeline produced.
fn busy_v4(env: &Env, state: &str) -> String {
    v4_db(env);
    let c = env.raw();
    let life: String = c
        .query_row("SELECT life_id FROM lives WHERE ordinal = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    c.execute_batch(&format!(
        "INSERT INTO tasks (id,revision,state,category,provider,folder,instructions,mock_outcome,
            mock_step_ms,created_at_ms,updated_at_ms,review_digest,review_verification,
            review_simulated,life_id)
         VALUES ('done-1',1,'accepted_artifact','hunger','mock','/p','Done','artifact',0,1,1,
            'd1','not_verified',1,'{life}');
         INSERT INTO demo_xp_rewards (task_id,track,amount,granted_at_ms,life_id)
         VALUES ('done-1','bugfix',20,1,'{life}');
         INSERT INTO tasks (id,revision,state,category,provider,folder,instructions,mock_outcome,
            mock_step_ms,created_at_ms,updated_at_ms,supervisor_pid,supervisor_start_time,
            life_id)
         VALUES ('slot-1',3,'{state}','thirst','mock','/p','In flight','artifact',0,2,2,1,1,
            '{life}');
         INSERT INTO task_events (task_id,from_state,to_state,revision,at_ms)
         VALUES ('slot-1',NULL,'draft',3,2);
         INSERT INTO task_care_usage (task_id, executed, execution_ms) VALUES ('slot-1',1,500);
         INSERT INTO preflight_tokens (token_sha256,task_id,revision,settings_revision,status,
            created_at_ms,expires_at_ms)
         VALUES ('t-pending','slot-1',3,1,'pending',2,9),('t-used','done-1',1,1,'consumed',1,9);
         UPDATE pet_care SET work_sessions = 2, execution_ms = 1000, rest_required = 1,
            starts_since_bathroom = 2;
         UPDATE lives SET armed = 1, armed_at_ms = 1, status = 'critical' WHERE life_id = '{life}';
         UPDATE life_vitals SET critical_need = 'thirst', critical_grace_ms = 900000,
            critical_elapsed_ms = 900000, critical_since_ms = 1, death_pending = 1;
         UPDATE pet_state SET thirst = 0;"
    ))
    .unwrap();
    life
}

#[test]
fn v5_closes_leftover_slot_tasks_without_reward_and_clears_a_pending_end() {
    for state in SLOT_STATE_LIST {
        let env = Env::with_boot();
        let life = busy_v4(&env, state.as_str());
        let rewards = env.count("SELECT count(*) FROM rewards");
        let meters = env.count("SELECT hunger * 100000 + thirst FROM pet_state");
        let journal = env.count("SELECT count(*) FROM life_events");
        env.clock.set(T0 + 99);
        let store = Store::open_migrating(&env.paths, env.clock.clone())
            .unwrap()
            .0;
        let s = state.as_str();

        // The slot task is closed by close_task_for_migration, with one event and no reward.
        let expected = schema::migration_close_state(state).unwrap();
        let (now_state, updated): (String, i64) = env
            .raw()
            .query_row(
                "SELECT state, updated_at_ms FROM tasks WHERE id = 'slot-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            (now_state.as_str(), updated),
            (expected.as_str(), T0 + 99),
            "{s}"
        );
        let last: (String, String, i64, i64) = env
            .raw()
            .query_row(
                "SELECT from_state, to_state, revision, at_ms FROM task_events
                 WHERE task_id = 'slot-1' ORDER BY seq DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            last,
            (s.to_owned(), expected.as_str().to_owned(), 3, T0 + 99),
            "{s}"
        );
        assert_eq!(
            env.count(&format!(
                "SELECT count(*) FROM tasks WHERE state IN {}",
                schema::SLOT_STATES
            )),
            0,
            "{s}"
        );
        assert_eq!(env.count("SELECT count(*) FROM rewards"), rewards, "{s}");
        assert_eq!(env.count("SELECT count(*) FROM demo_xp_rewards"), 1, "{s}");
        assert_eq!(
            env.count("SELECT hunger * 100000 + thirst FROM pet_state"),
            meters,
            "{s}: no refund"
        );
        // Pending approvals are invalidated; consumed ones stay.
        assert_eq!(
            env.count("SELECT count(*) FROM preflight_tokens WHERE status = 'pending'"),
            0
        );
        assert_eq!(
            env.count("SELECT count(*) FROM preflight_tokens WHERE status = 'consumed'"),
            1
        );
        // The pending end is cleared with one journal note; the life is still living and
        // keeps its mode, armed state and XP.
        assert_eq!(env.count("SELECT death_pending FROM life_vitals"), 0, "{s}");
        assert_eq!(env.count("SELECT count(*) FROM life_events"), journal + 1);
        let note: (String, String, String) = env
            .raw()
            .query_row(
                "SELECT life_id, dedup_key, data FROM life_events ORDER BY seq DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            note,
            (
                life.clone(),
                "v5_migration".to_owned(),
                r#"{"reason":"v5_migration"}"#.to_owned()
            )
        );
        let l = store.snapshot().unwrap().life.unwrap();
        assert_eq!(
            (l.life_id.as_str(), l.mode.as_str(), l.armed, l.xp),
            (life.as_str(), "permadeath", true, 20)
        );
        // The rest counters are zeroed.
        assert_eq!(
            env.count(
                "SELECT work_sessions + execution_ms + rest_required + starts_since_bathroom
                 FROM pet_care"
            ),
            0
        );
        let c = store.snapshot().unwrap().care;
        assert!(!c.sleep_due && !c.bathroom_due);
    }
}

#[test]
fn close_task_for_migration_refuses_a_task_without_the_slot() {
    let env = Env::new();
    v4_db(&env);
    env.raw()
        .execute_batch(super::migration_v4_cases::V1_ROWS)
        .unwrap();
    let store = env.open();
    for id in ["old-accepted", "old-draft", "old-failed", "old-discarded"] {
        assert!(matches!(
            schema::close_task_for_migration(store.conn(), id, T0),
            Err(StorageError::InvalidState(_))
        ));
    }
    assert!(matches!(
        schema::close_task_for_migration(store.conn(), "missing", T0),
        Err(StorageError::NotFound)
    ));
    // Every slot state has a closing state; no other state has one.
    for state in State::ALL {
        assert_eq!(
            schema::migration_close_state(state).is_some(),
            state.occupies_slot(),
            "{state:?}"
        );
    }
}

#[test]
fn a_failed_v5_migration_rolls_back_to_v4_and_keeps_its_backup() {
    fn v5_then_crash(tx: &rusqlite::Transaction<'_>, at: &care::TimeMark) -> Result<()> {
        schema::MIGRATIONS[4](tx, at)?;
        Err(StorageError::Unavailable("injected migration crash"))
    }
    let env = Env::with_boot();
    busy_v4(&env, "running");
    let before = v4_dump(&env.raw());
    let crashing: &[schema::Migration] = &[
        schema::MIGRATIONS[0],
        schema::MIGRATIONS[1],
        schema::MIGRATIONS[2],
        schema::MIGRATIONS[3],
        v5_then_crash,
    ];
    assert!(Store::open_with(&env.paths, env.clock.clone(), crashing).is_err());
    assert_eq!(env.count("PRAGMA user_version"), 4);
    assert_eq!(redump(&env.raw(), &before), before);
    assert_eq!(
        env.count("SELECT count(*) FROM sqlite_master WHERE name = 'agent_events'"),
        0
    );
    assert_eq!(backups(&env).len(), 1);
    // The real migration then succeeds (with its own backup).
    env.clock.advance(1);
    drop(env.open());
    assert_eq!(env.count("PRAGMA user_version"), 5);
}

#[test]
fn a_fresh_install_is_created_at_v5_by_any_open() {
    let env = Env::with_boot();
    let store = Store::open_current(&env.paths, env.clock.clone()).unwrap();
    assert_eq!(env.count("PRAGMA user_version"), 5);
    assert!(backups(&env).is_empty());
    let salt: Vec<u8> = env
        .raw()
        .query_row("SELECT hook_salt FROM meta", [], |r| r.get(0))
        .unwrap();
    assert_eq!(salt.len(), 32);
    let s = store.snapshot().unwrap();
    assert_eq!(s.life.unwrap().origin, "first_run");
    // Two installs get different salts.
    let other = Env::with_boot();
    drop(other.open());
    let salt2: Vec<u8> = other
        .raw()
        .query_row("SELECT hook_salt FROM meta", [], |r| r.get(0))
        .unwrap();
    assert_ne!(salt, salt2);
    let (_s, report) = Store::open_migrating(&other.paths, other.clock.clone()).unwrap();
    assert_eq!((report.from, report.to, report.backup), (5, 5, None));
}

#[test]
fn the_v5_ddl_is_the_contract_and_its_triggers_hold() {
    let env = Env::with_boot();
    let store = env.open();
    let c = store.conn();
    let life = life_id(&store);
    // activity rows are history: never deleted, given once, identity immutable.
    c.execute(
        "INSERT INTO activity_turns (session_key, turn_key, client, life_id, earned_at_ms)
         VALUES (?1, ?2, 'codex', ?3, 5)",
        rusqlite::params!["a".repeat(32), "b".repeat(32), life],
    )
    .unwrap();
    assert!(c.execute("DELETE FROM activity_turns", []).is_err());
    c.execute("UPDATE activity_turns SET given_at_ms = 9", [])
        .unwrap();
    assert!(c
        .execute("UPDATE activity_turns SET given_at_ms = NULL", [])
        .is_err());
    assert!(c
        .execute("UPDATE activity_turns SET client = 'hermes'", [])
        .is_err());
    // X1: one treat per (session, turn).
    assert!(c
        .execute(
            "INSERT INTO activity_turns (session_key, turn_key, client, life_id, earned_at_ms)
             VALUES (?1, ?2, 'codex', ?3, 6)",
            rusqlite::params!["a".repeat(32), "b".repeat(32), life],
        )
        .is_err());
    c.execute(
        "INSERT INTO activity_xp (life_id, turn_row_id, amount, granted_at_ms)
         VALUES (?1, 1, 5, 9)",
        [&life],
    )
    .unwrap();
    assert!(c
        .execute(
            "INSERT INTO activity_xp (life_id, turn_row_id, amount, granted_at_ms)
             VALUES (?1, 1, 5, 9)",
            [&life],
        )
        .is_err());
    assert!(c.execute("UPDATE activity_xp SET amount = 5", []).is_err());
    assert!(c.execute("DELETE FROM activity_xp", []).is_err());
    // Job ids and enums are enforced by the DDL.
    assert!(c
        .execute(
            "INSERT INTO client_hooks (client, job_id, job_kind) VALUES ('codex', 'job-1', 'test')",
            [],
        )
        .is_err());
    c.execute(
        "INSERT INTO client_hooks (client, job_id, job_kind)
         VALUES ('codex', 'job-0123456789abcdef01234567', 'test')",
        [],
    )
    .unwrap();
    assert!(c
        .execute("INSERT INTO client_hooks (client) VALUES ('muse-code')", [])
        .is_err());
    assert!(c.execute("UPDATE meta SET enabled = 2", []).is_err());
    assert!(c.execute("UPDATE meta SET hook_salt = x'00'", []).is_err());
    let stored: Option<String> = c
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'activity_turns'",
            [],
            |r| r.get(0),
        )
        .optional()
        .unwrap();
    assert!(stored.unwrap().contains("UNIQUE (session_key, turn_key)"));
}
