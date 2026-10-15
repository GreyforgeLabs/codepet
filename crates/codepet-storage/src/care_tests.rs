use super::*;

fn full() -> MetersView {
    MetersView {
        hunger: 10000,
        thirst: 10000,
        mental_health: 10000,
        hygiene: 10000,
    }
}
fn spent() -> MetersView {
    MetersView {
        hunger: 9000,
        thirst: 9000,
        mental_health: 9500,
        hygiene: 9000,
    }
}

#[test]
fn explicit_care_is_idempotent_and_does_not_award_xp() {
    let env = Env::new();
    let mut s = env.open();
    let actions = ["food", "water", "play", "wash"];
    for action in actions {
        assert!(s.care(action, action).unwrap().changed);
    }
    let snapshot = s.snapshot().unwrap();
    assert_eq!(snapshot.meters, full());
    assert!(snapshot.care.ready_to_work);
    assert!(!snapshot.care.welcome_care_available);
    assert_eq!(snapshot.progression.demo.lifetime_xp, 0);
    assert_eq!(snapshot.progression.real.lifetime_xp, 0);
    assert!(!s.care("more-food", "food").unwrap().changed);
    let initial = s.care("routine-replay", "routine").unwrap();
    let t = s.create_task(&env.input(Category::Hunger)).unwrap();
    let token = s.preflight(&t.task_id, 1).unwrap();
    s.start(&t.task_id, 1, &token.preflight_token, sup())
        .unwrap();
    assert_eq!(s.snapshot().unwrap().meters, spent());
    assert_eq!(s.care("routine-replay", "routine").unwrap(), initial);
    assert_eq!(s.snapshot().unwrap().meters, spent());
    assert!(matches!(
        s.care("routine-replay", "food"),
        Err(StorageError::IdempotencyConflict)
    ));
    assert!(matches!(
        s.sleep("routine-replay"),
        Err(StorageError::IdempotencyConflict)
    ));
    assert!(matches!(
        s.care("unknown", "feed-every-hour"),
        Err(StorageError::InvalidParams(_))
    ));
}

#[test]
fn start_requires_every_full_bar_and_debits_only_once() {
    for column in ["hunger", "thirst", "mental_health", "hygiene"] {
        let env = Env::new();
        let mut s = env.open();
        let (id, rev, token) = ready(&mut s, &env, Category::Hunger);
        env.raw()
            .execute(&format!("UPDATE pet_state SET {column} = 9900"), [])
            .unwrap();
        assert!(matches!(
            s.start(&id, rev, &token, sup()),
            Err(StorageError::InvalidState(_))
        ));
        assert_eq!(env.count("SELECT count(*) FROM task_care_usage"), 0);
        assert_eq!(s.snapshot().unwrap().care.work_sessions, 0);
        assert_eq!(
            env.count("SELECT count(*) FROM preflight_tokens WHERE status='pending'"),
            1
        );
        s.care("fill", "routine").unwrap();
        assert!(matches!(
            s.start(&id, rev, &token, sup()),
            Err(StorageError::TokenInvalid)
        ));
        let p = s.preflight(&id, rev).unwrap();
        s.start(&id, rev, &p.preflight_token, sup()).unwrap();
        assert!(s.start(&id, rev, &p.preflight_token, sup()).is_err());
        let state = s.snapshot().unwrap();
        assert_eq!(state.meters, spent());
        assert_eq!(state.care.work_sessions, 1);
        assert_eq!(state.care.starts_since_bathroom, 1);
    }
}

#[test]
fn bathroom_due_after_two_starts_is_not_cleared_by_wash() {
    let env = Env::new();
    let mut s = env.open();
    for i in 0..2 {
        let id = started(&mut s, &env, Category::Hunger, sup());
        s.supervisor_step(&id, sup(), State::Preparing, State::Running)
            .unwrap();
        s.fail(&id, sup()).unwrap();
        assert_eq!(s.snapshot().unwrap().care.starts_since_bathroom, i + 1);
    }
    assert!(s.snapshot().unwrap().care.bathroom_due);
    s.care("wash", "wash").unwrap();
    assert!(s.snapshot().unwrap().care.bathroom_due);
    s.care("bathroom", "bathroom").unwrap();
    let state = s.snapshot().unwrap();
    assert!(!state.care.bathroom_due);
    assert_eq!(state.meters.hygiene, 10000);
    assert_eq!(state.care.work_sessions, 2);
}

#[test]
fn third_start_requires_sleep_but_never_interrupts_execution() {
    let env = Env::new();
    let mut s = env.open();
    for i in 0..3 {
        let id = started(&mut s, &env, Category::Hunger, sup());
        s.supervisor_step(&id, sup(), State::Preparing, State::Running)
            .unwrap();
        if i == 2 {
            let state = s.snapshot().unwrap();
            assert!(state.care.sleep_due);
            assert_eq!(state.active_task.unwrap().state, State::Running);
            assert!(matches!(
                s.sleep("too-early"),
                Err(StorageError::InvalidState(_))
            ));
        }
        s.fail(&id, sup()).unwrap();
    }
    s.care("care-cannot-rest", "routine").unwrap();
    assert!(s.snapshot().unwrap().care.sleep_due);
    let t = s.create_task(&env.input(Category::Hunger)).unwrap();
    let p = s.preflight(&t.task_id, 1).unwrap();
    assert!(matches!(
        s.start(&t.task_id, 1, &p.preflight_token, sup()),
        Err(StorageError::InvalidState(_))
    ));
    s.sleep("nap").unwrap();
    env.clock.advance(SLEEP_MS);
    let state = s.snapshot().unwrap();
    assert!(!state.care.sleeping && !state.care.sleep_due);
    assert_eq!(state.care.work_sessions, 0);
    assert_eq!(state.meters, full());
    assert_eq!(s.task(&t.task_id).unwrap().state, State::Draft);
    assert!(matches!(
        s.start(&t.task_id, 1, &p.preflight_token, sup()),
        Err(StorageError::TokenInvalid)
    ));
}

#[test]
fn early_wake_and_closed_app_preserve_sleep_credit() {
    let env = Env::new();
    let mut s = env.open();
    s.sleep("start").unwrap();
    env.clock.advance(30_000);
    let wake = s.wake("early").unwrap();
    assert_eq!(wake.care.sleep_remaining_ms, 90_000);
    assert!(wake.care.sleep_due && !wake.care.sleeping);
    drop(s);
    env.clock.advance(86_400_000);
    let mut s = env.open();
    assert_eq!(s.snapshot().unwrap().care.sleep_remaining_ms, 90_000);
    assert!(s.snapshot().unwrap().care.sleep_due);
    s.sleep("resume").unwrap();
    drop(s);
    env.clock.advance(89_999);
    let s = env.open();
    assert_eq!(s.snapshot().unwrap().care.sleep_remaining_ms, 1);
    env.clock.advance(1);
    assert!(!s.snapshot().unwrap().care.sleep_due);
    assert_eq!(s.snapshot().unwrap().meters.hunger, 7000);
    assert_eq!(env.count("SELECT count(*) FROM tasks"), 0);
}

#[test]
fn backward_wall_clock_does_not_grant_or_double_count_rest() {
    let env = Env::new();
    let mut s = env.open();
    s.sleep("nap").unwrap();
    env.clock.advance(30_000);
    assert_eq!(s.snapshot().unwrap().care.sleep_remaining_ms, 90_000);
    env.clock.set(T0 - 60_000);
    assert_eq!(s.snapshot().unwrap().care.sleep_remaining_ms, 90_000);
    env.clock.set(T0 + 30_000);
    assert_eq!(s.snapshot().unwrap().care.sleep_remaining_ms, 90_000);
    env.clock.set(i64::MAX);
    assert!(!s.snapshot().unwrap().care.sleep_due);
    assert_eq!(s.snapshot().unwrap().care.work_sessions, 0);
    // Huge jumps are bounded to the one explicit nap; no task or reward is invented.
    assert_eq!(s.snapshot().unwrap().progression.demo.lifetime_xp, 0);
}

#[test]
fn elapsed_execution_counts_running_not_waiting_or_review() {
    let env = Env::new();
    let mut s = env.open();
    let id = started(&mut s, &env, Category::Hunger, sup());
    env.clock.advance(60_000);
    s.supervisor_step(&id, sup(), State::Preparing, State::Running)
        .unwrap();
    env.clock.advance(15 * 60_000);
    s.supervisor_step(&id, sup(), State::Running, State::WaitingForUser)
        .unwrap();
    env.clock.advance(24 * 60 * 60_000);
    s.supervisor_step(&id, sup(), State::WaitingForUser, State::Running)
        .unwrap();
    assert_eq!(s.snapshot().unwrap().care.execution_ms, 15 * 60_000);
    env.clock.advance(30 * 60_000);
    s.supervisor_step(&id, sup(), State::Running, State::Verifying)
        .unwrap();
    assert!(s.snapshot().unwrap().care.sleep_due);
    s.seal_review(&id, sup(), ARTIFACT, Verification::NotVerified, true)
        .unwrap();
    let state = s.snapshot().unwrap();
    assert_eq!(state.care.execution_ms, EXECUTION_LIMIT_MS);
    assert_eq!(state.active_task.unwrap().state, State::ReviewReady);
    s.sleep("rest-in-review").unwrap();
    let digest = s.review(&id).unwrap().review.unwrap().digest;
    assert_eq!(
        accept(
            &mut s,
            &id,
            &digest,
            "accept-asleep",
            Some("reviewed simulation")
        )
        .unwrap()
        .receipt
        .demo_xp,
        20
    );
    assert!(s.snapshot().unwrap().care.sleeping);
}

#[test]
fn pre_execution_failure_refunds_once_but_executed_failure_does_not() {
    let env = Env::new();
    let mut s = env.open();
    let id = started(&mut s, &env, Category::Hunger, sup());
    s.fail(&id, sup()).unwrap();
    assert!(s.fail(&id, sup()).is_err());
    assert_eq!(s.snapshot().unwrap().meters, full());
    assert_eq!(s.snapshot().unwrap().care.work_sessions, 0);
    let id = started(&mut s, &env, Category::Hunger, dead_sup());
    s.recover().unwrap();
    s.recover().unwrap();
    assert_eq!(s.task(&id).unwrap().state, State::Interrupted);
    assert_eq!(s.snapshot().unwrap().meters, full());
    let id = started(&mut s, &env, Category::Hunger, sup());
    s.supervisor_step(&id, sup(), State::Preparing, State::Running)
        .unwrap();
    s.fail(&id, sup()).unwrap();
    assert_eq!(s.snapshot().unwrap().meters, spent());
    assert_eq!(s.snapshot().unwrap().care.work_sessions, 1);
}

#[test]
fn demo_level_thresholds_and_mastery_never_enable_real_providers() {
    let env = Env::new();
    let mut s = env.open();
    for i in 0..10 {
        let (id, digest) = reviewed(&mut s, &env, Category::Hunger, Verification::NotVerified);
        accept(
            &mut s,
            &id,
            &digest,
            &format!("accept-{i}"),
            Some("simulation reviewed"),
        )
        .unwrap();
    }
    let snap = s.snapshot().unwrap();
    assert_eq!(snap.progression.demo.lifetime_xp, 200);
    assert_eq!(snap.progression.demo.mastery.bugfix, 200);
    assert_eq!(snap.progression.demo.level, 3);
    assert_eq!(snap.progression.demo.next_level_xp, Some(400));
    assert_eq!(snap.progression.real.lifetime_xp, 0);
    assert_eq!(snap.progression.real.level, 1);
    assert!(snap
        .providers
        .iter()
        .all(|p| p.available == (p.id == Provider::Mock)));
    let (id, digest) = reviewed(&mut s, &env, Category::Hunger, Verification::Failed);
    let out = accept(
        &mut s,
        &id,
        &digest,
        "failed-check",
        Some("accepted with uncertainty"),
    )
    .unwrap();
    assert_eq!(out.receipt.demo_xp, 0);
    assert_eq!(out.receipt.real_xp, 0);
    assert_eq!(s.snapshot().unwrap().progression.demo.lifetime_xp, 200);
}

#[test]
fn new_sensitive_text_is_redacted_before_persistence_and_digest() {
    let env = Env::new();
    let mut s = env.open();
    let mut input = env.input(Category::MentalHealth);
    input.draft.instructions = "Explain configuration\nAPI_KEY=secret-never-store".into();
    let task = s.create_task(&input).unwrap();
    assert!(!task.instructions.contains("secret-never-store"));
    make_ready(&mut s, &env);
    let p = s.preflight(&task.task_id, 1).unwrap();
    s.start(&task.task_id, 1, &p.preflight_token, sup())
        .unwrap();
    s.supervisor_step(&task.task_id, sup(), State::Preparing, State::Running)
        .unwrap();
    s.supervisor_step(&task.task_id, sup(), State::Running, State::Verifying)
        .unwrap();
    s.seal_review(
        &task.task_id,
        sup(),
        "Useful artifact\npassword=secret-never-store",
        Verification::NotVerified,
        true,
    )
    .unwrap();
    let review = s.review(&task.task_id).unwrap().review.unwrap();
    assert!(!review.artifact.contains("secret-never-store"));
    let out = accept(
        &mut s,
        &task.task_id,
        &review.digest,
        "redacted",
        Some("Reviewed\napi_key=secret-never-store"),
    )
    .unwrap();
    assert!(!out
        .receipt
        .override_reason
        .unwrap()
        .contains("secret-never-store"));
    for (table, column) in [
        ("tasks", "instructions"),
        ("artifacts", "content"),
        ("acceptances", "override_reason"),
        ("request_results", "response"),
    ] {
        assert_eq!(
            env.count(&format!(
                "SELECT count(*) FROM {table} WHERE {column} LIKE '%secret-never-store%'"
            )),
            0
        );
    }
}

#[test]
fn v1_migration_preserves_historical_care_receipts_and_inflight_tasks() {
    let env = Env::new();
    let old = Store::open_with(&env.paths, env.clock.clone(), &schema::MIGRATIONS[..1]).unwrap();
    old.conn().execute_batch("
        INSERT INTO tasks (id,revision,state,category,provider,folder,instructions,mock_outcome,
            mock_step_ms,created_at_ms,updated_at_ms,review_digest,review_verification,review_simulated)
        VALUES ('old-accepted',1,'accepted_artifact','hunger','mock','/project','Historical text',
            'artifact',0,1,1,'legacy-digest','not_verified',1);
        INSERT INTO rewards (task_id,category,amount_hundredths,applied_hundredths,meter_before,
            meter_after,status,granted_at_ms) VALUES ('old-accepted','hunger',2000,2000,7000,9000,'granted',1);
        INSERT INTO acceptances (task_id,request_id,review_digest,verification,override_reason,accepted_at_ms)
        VALUES ('old-accepted','legacy-request','legacy-digest','not_verified','historical review',1);
        INSERT INTO tasks (id,revision,state,category,provider,folder,instructions,mock_outcome,
            mock_step_ms,created_at_ms,updated_at_ms,supervisor_pid,supervisor_start_time)
        VALUES ('old-running',1,'running','hygiene','mock','/project','Already running',
            'artifact',0,1,1,0,0);
        INSERT INTO artifacts (task_id,kind,content,sha256,length,created_at_ms)
        VALUES ('old-accepted','text','Keep the original artifact','hash',26,1);
        UPDATE pet_state SET hunger=9000,thirst=4200,mental_health=7500,hygiene=1200;
    ").unwrap();
    let legacy_response = serde_json::json!({"receipt": {"task_id":"old-accepted","category":"hunger",
        "amount_hundredths":2000,"verification":"not_verified","override_reason":"historical review",
        "request_id":"legacy-request","accepted_at_ms":1},"duplicate":false}).to_string();
    old.conn().execute("INSERT INTO request_results VALUES ('legacy-request','task.accept_artifact',?1,?2,1)",
        rusqlite::params![sys::sha256_hex(r#"{"acknowledged":true,"override_reason":"historical review","review_digest":"legacy-digest","task_id":"old-accepted"}"#.as_bytes()), legacy_response]).unwrap();
    drop(old);
    let mut s = env.open();
    assert_eq!(env.count("PRAGMA user_version"), 2);
    assert_eq!(
        s.snapshot().unwrap().meters,
        MetersView {
            hunger: 9000,
            thirst: 4200,
            mental_health: 7500,
            hygiene: 1200
        }
    );
    assert!(s.snapshot().unwrap().care.welcome_care_available);
    assert_eq!(s.task("old-running").unwrap().state, State::Running);
    assert_eq!(
        s.task("old-accepted")
            .unwrap()
            .receipt
            .unwrap()
            .amount_hundredths,
        2000
    );
    let replay = accept(
        &mut s,
        "old-accepted",
        "legacy-digest",
        "legacy-request",
        Some("historical review"),
    )
    .unwrap();
    assert_eq!(replay.receipt.amount_hundredths, 2000);
    assert_eq!(replay.receipt.demo_xp, 0);
    assert_eq!(replay.receipt.real_xp, 0);
    assert_eq!(s.snapshot().unwrap().progression.demo.lifetime_xp, 0);
    assert_eq!(env.count("SELECT count(*) FROM demo_xp_rewards"), 0);
    assert_eq!(env.count("SELECT count(*) FROM task_care_usage"), 0);
    let stored: String = env
        .raw()
        .query_row(
            "SELECT response FROM request_results WHERE request_id='legacy-request'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored, legacy_response);
    let backup = env
        .paths
        .state_dir
        .join(format!("codepet.sqlite3.bak-v1-{T0}"));
    assert!(backup.is_file());
    s.recover().unwrap();
    // A pre-migration task has no new debit to refund or execution time to invent.
    assert_eq!(s.snapshot().unwrap().care.work_sessions, 0);
    assert_eq!(s.snapshot().unwrap().meters.thirst, 4200);
}

#[test]
fn concurrent_identical_care_requests_have_one_receipt_and_revision() {
    let env = Env::new();
    let mut stores: Vec<Store> = (0..4).map(|_| env.open()).collect();
    let barrier = std::sync::Barrier::new(stores.len());
    let results = std::thread::scope(|scope| {
        let joins: Vec<_> = stores
            .iter_mut()
            .map(|s| {
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    s.care("one-routine", "routine").unwrap()
                })
            })
            .collect();
        joins
            .into_iter()
            .map(|j| j.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert!(results.windows(2).all(|w| w[0] == w[1]));
    assert_eq!(env.open().snapshot().unwrap().care.revision, 2);
    assert_eq!(
        env.count("SELECT count(*) FROM request_results WHERE request_id='one-routine'"),
        1
    );
    assert_eq!(
        env.open().snapshot().unwrap().progression.demo.lifetime_xp,
        0
    );
}

#[test]
fn same_boot_monotonic_time_wins_over_wall_clock_jumps() {
    struct SteadyClock {
        wall: ManualClock,
        steady: ManualClock,
    }
    impl Clock for SteadyClock {
        fn now_ms(&self) -> i64 {
            self.wall.now_ms()
        }
        fn steady_time(&self) -> Option<(String, i64)> {
            Some(("same-boot".into(), self.steady.now_ms()))
        }
    }
    let env = Env::new();
    let clock = Arc::new(SteadyClock {
        wall: ManualClock::new(T0),
        steady: ManualClock::new(1000),
    });
    let mut s = Store::open(&env.paths, clock.clone()).unwrap();
    s.sleep("monotonic-nap").unwrap();
    clock.wall.set(i64::MAX);
    clock.steady.advance(1000);
    assert_eq!(
        s.snapshot().unwrap().care.sleep_remaining_ms,
        SLEEP_MS - 1000
    );
    clock.wall.set(0);
    clock.steady.advance(1000);
    assert_eq!(
        s.snapshot().unwrap().care.sleep_remaining_ms,
        SLEEP_MS - 2000
    );
    drop(s);
    clock.steady.advance(SLEEP_MS - 2000);
    let s = Store::open(&env.paths, clock.clone()).unwrap();
    assert!(!s.snapshot().unwrap().care.sleeping);
    assert!(!s.snapshot().unwrap().care.sleep_due);
}

#[test]
fn explicit_snapshot_checkpoints_live_execution_once_and_recovery_ignores_dead_time() {
    let env = Env::new();
    let mut s = env.open();
    let id = started(&mut s, &env, Category::Hunger, sup());
    s.supervisor_step(&id, sup(), State::Preparing, State::Running)
        .unwrap();
    env.clock.advance(EXECUTION_LIMIT_MS);
    let state = s.snapshot().unwrap();
    assert!(state.care.sleep_due);
    assert_eq!(state.active_task.unwrap().state, State::Running);
    assert_eq!(state.care.execution_ms, EXECUTION_LIMIT_MS);
    assert_eq!(s.snapshot().unwrap().care.execution_ms, EXECUTION_LIMIT_MS);
    s.supervisor_step(&id, sup(), State::Running, State::Verifying)
        .unwrap();
    assert_eq!(s.snapshot().unwrap().care.execution_ms, EXECUTION_LIMIT_MS);

    let env = Env::new();
    let mut s = env.open();
    let id = started(&mut s, &env, Category::Hunger, dead_sup());
    s.supervisor_step(&id, dead_sup(), State::Preparing, State::Running)
        .unwrap();
    env.clock.advance(86_400_000);
    assert_eq!(s.snapshot().unwrap().care.execution_ms, 0);
    s.recover().unwrap();
    assert_eq!(s.snapshot().unwrap().care.execution_ms, 0);
    assert_eq!(s.snapshot().unwrap().care.work_sessions, 1);
    assert_eq!(s.snapshot().unwrap().meters, spent());
}

#[test]
fn sleep_completion_invalidates_tokens_issued_during_sleep_only_once() {
    let env = Env::new();
    let mut s = env.open();
    make_ready(&mut s, &env);
    s.sleep("nap").unwrap();
    let task = s.create_task(&env.input(Category::Hunger)).unwrap();
    let token = s.preflight(&task.task_id, 1).unwrap();
    let revision = s.snapshot().unwrap().care.revision;
    env.clock.advance(10_000);
    assert_eq!(s.snapshot().unwrap().care.revision, revision);
    assert_eq!(
        env.count("SELECT count(*) FROM preflight_tokens WHERE status='pending'"),
        1
    );
    assert!(matches!(
        s.start(&task.task_id, 1, &token.preflight_token, sup()),
        Err(StorageError::InvalidState(_))
    ));
    env.clock.advance(SLEEP_MS - 10_000);
    let state = s.snapshot().unwrap();
    assert!(state.care.ready_to_work);
    assert_eq!(state.care.revision, revision + 1);
    assert!(matches!(
        s.start(&task.task_id, 1, &token.preflight_token, sup()),
        Err(StorageError::TokenInvalid)
    ));
    assert_eq!(s.snapshot().unwrap().care.revision, revision + 1);
}

#[test]
fn older_transition_timestamp_cannot_rewind_newer_execution_checkpoint() {
    // Public snapshot path, using the conservative wall-clock fallback.
    {
        let env = Env::new();
        let mut s = env.open();
        let id = started(&mut s, &env, Category::Hunger, sup());
        s.supervisor_step(&id, sup(), State::Preparing, State::Running)
            .unwrap();
        env.clock.set(T0 + 60_000);
        assert_eq!(s.snapshot().unwrap().care.execution_ms, 60_000);
        env.clock.set(T0 + 30_000);
        assert_eq!(s.snapshot().unwrap().care.execution_ms, 60_000);
        env.clock.set(T0 + 60_000);
        assert_eq!(s.snapshot().unwrap().care.execution_ms, 60_000);
    }
    let env = Env::new();
    let mut s = env.open();
    let id = started(&mut s, &env, Category::Hunger, sup());
    s.supervisor_step(&id, sup(), State::Preparing, State::Running)
        .unwrap();
    let mark = |elapsed| crate::care::TimeMark {
        wall: T0 + elapsed,
        boot: Some("same-boot".into()),
        steady: Some(elapsed),
    };
    // Reproduce the former interleaving deterministically: a snapshot checkpoints t=100,
    // then a supervisor transition arrives carrying t=50 from before its lock wait.
    crate::care::transition(s.conn(), &id, State::Running, State::Running, &mark(0)).unwrap();
    crate::care::transition(s.conn(), &id, State::Running, State::Running, &mark(100)).unwrap();
    crate::care::transition(s.conn(), &id, State::Running, State::Verifying, &mark(50)).unwrap();
    assert_eq!(
        env.count("SELECT segment_steady_ms FROM task_care_usage"),
        100
    );
    assert_eq!(
        env.count("SELECT segment_wall_ms FROM task_care_usage"),
        T0 + 100
    );
    crate::care::transition(
        s.conn(),
        &id,
        State::Verifying,
        State::Verifying,
        &mark(200),
    )
    .unwrap();
    assert_eq!(env.count("SELECT execution_ms FROM pet_care"), 200);
    assert_eq!(env.count("SELECT execution_ms FROM task_care_usage"), 200);
}
