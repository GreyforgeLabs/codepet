//! The lifecycle through storage: the steady timeline at exact boundaries, recovery,
//! gentle fainting, pending deaths during every slot state, crash points, concurrency,
//! and the guarantee that an ending touches only life tables.
use super::*;
use codepet_core::lifecycle::{Mode, HOUR_MS};

/// Feeds `ms` of attended time through a fresh sidecar session and settles it.
struct Feeder {
    n: u32,
}
impl Feeder {
    fn new() -> Self {
        Self { n: 0 }
    }
    fn feed(&mut self, env: &Env, store: &Store, life: &str, ms: i64) -> Snapshot {
        self.n += 1;
        env.pass(ms);
        env.write_presence(&format!("bar-feed-{:04}", self.n), life, ms);
        store.snapshot().unwrap()
    }
}

fn events(env: &Env, life: &str) -> Vec<String> {
    let c = env.raw();
    let mut stmt = c
        .prepare("SELECT kind FROM life_events WHERE life_id = ?1 ORDER BY seq")
        .unwrap();
    stmt.query_map([life], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

fn status(env: &Env, life: &str) -> String {
    env.raw()
        .query_row("SELECT status FROM lives WHERE life_id = ?1", [life], |r| {
            r.get(0)
        })
        .unwrap()
}

/// Puts the living companion into a running critical period for `need`, optionally
/// pending death, directly through the test connection.
fn inject_critical(env: &Env, need: &str, pending: bool) {
    let c = env.raw();
    let col = format!("neglect_{need}_ms");
    c.execute(&format!("UPDATE pet_state SET {need} = 0"), [])
        .unwrap();
    c.execute(
        &format!(
            "UPDATE life_vitals SET {col} = 7200000, hold_{need} = 0, critical_need = ?1,
             critical_grace_ms = 21600000, critical_elapsed_ms = ?2, critical_since_ms = 1,
             death_pending = ?3"
        ),
        rusqlite::params![need, if pending { 21_600_000 } else { 0 }, pending],
    )
    .unwrap();
    c.execute(
        "UPDATE lives SET status = 'critical' WHERE status IN ('healthy','warning')",
        [],
    )
    .unwrap();
}

#[test]
fn steady_timeline_thirst_6_6h_warning_11h_critical_17h_dead() {
    let env = Env::with_boot();
    let (store, id) = armed(&env);
    let mut f = Feeder::new();
    // 6.6 h minus 1 ms: still healthy (thirst 30.01).
    let s = f.feed(&env, &store, &id, 23_760_000 - 1);
    assert_eq!(s.life.as_ref().unwrap().status, "healthy");
    assert_eq!(s.meters.thirst, 3001);
    // Exactly 6.6 h: thirst 30.00 is a warning (≤).
    let s = f.feed(&env, &store, &id, 1);
    assert_eq!(s.meters.thirst, 3000);
    assert_eq!(s.life.as_ref().unwrap().status, "warning");
    let thirst = &s.lifecycle.needs[1];
    assert_eq!(
        (thirst.need.as_str(), thirst.state.as_str()),
        ("thirst", "low")
    );
    assert_eq!(thirst.until_critical_ms, Some(11 * HOUR_MS - 23_760_000));
    assert_eq!(thirst.until_end_ms, Some(17 * HOUR_MS - 23_760_000));
    // 9 h: empty. 11 h minus 1 ms: still a warning.
    let s = f.feed(&env, &store, &id, 9 * HOUR_MS - 23_760_000);
    assert_eq!(s.meters.thirst, 0);
    let s = f.feed(&env, &store, &id, 2 * HOUR_MS - 1);
    assert_eq!(s.life.as_ref().unwrap().status, "warning");
    assert!(s.lifecycle.critical.is_none());
    // Exactly 11 h: critical, grace frozen at 6 h, visible before any grace runs.
    let s = f.feed(&env, &store, &id, 1);
    let life = s.life.as_ref().unwrap();
    assert_eq!(life.status, "critical");
    let crit = s.lifecycle.critical.as_ref().unwrap();
    assert_eq!(
        (
            crit.need.as_str(),
            crit.grace_ms,
            crit.elapsed_ms,
            crit.remaining_ms
        ),
        ("thirst", 6 * HOUR_MS, 0, 6 * HOUR_MS)
    );
    assert!(!s.care.ready_to_work);
    assert!(s.care.readiness_reasons[0].contains("urgently needs water"));
    // 17 h minus 1 ms: alive.
    let s = f.feed(&env, &store, &id, 6 * HOUR_MS - 1);
    assert_eq!(s.lifecycle.critical.as_ref().unwrap().remaining_ms, 1);
    assert!(s.life.is_some());
    // Exactly 17 h: the life ends, in the same settling state.get.
    let s = f.feed(&env, &store, &id, 1);
    assert!(s.life.is_none());
    assert_eq!(s.lifecycle.frozen_reason, "no_life");
    assert_eq!(s.memorial.count, 1);
    let m = s.memorial.latest.unwrap();
    assert_eq!(
        (
            m.end_cause.as_str(),
            m.name.as_str(),
            m.ordinal,
            m.attended_ms
        ),
        ("neglect:thirst", "Ginger", 1, 17 * HOUR_MS)
    );
    assert_eq!(status(&env, &id), "dead");
    let kinds = events(&env, &id);
    assert_eq!(
        kinds.iter().filter(|k| *k == "died").count(),
        1,
        "{kinds:?}"
    );
    assert!(kinds.contains(&"critical".to_owned()));
    assert!(kinds.contains(&"death_pending".to_owned()));
    assert_eq!(
        env.count("SELECT count(*) FROM life_vitals WHERE life_id IS NULL AND death_pending = 0"),
        1
    );
}

#[test]
fn entering_critical_discards_the_rest_of_the_interval() {
    let env = Env::with_boot();
    let (store, id) = armed(&env);
    let mut f = Feeder::new();
    f.feed(&env, &store, &id, 8 * HOUR_MS);
    // One settlement spanning 3 h past critical entry stops at entry.
    let s = f.feed(&env, &store, &id, 6 * HOUR_MS);
    let crit = s.lifecycle.critical.unwrap();
    assert_eq!(crit.elapsed_ms, 0);
    assert_eq!(s.lifecycle.attended_total_ms, 11 * HOUR_MS);
    assert_eq!(status(&env, &id), "critical");
}

#[test]
fn water_recovers_from_critical() {
    let env = Env::with_boot();
    let (mut store, id) = armed(&env);
    let mut f = Feeder::new();
    f.feed(&env, &store, &id, 8 * HOUR_MS);
    f.feed(&env, &store, &id, 3 * HOUR_MS);
    f.feed(&env, &store, &id, HOUR_MS);
    assert_eq!(status(&env, &id), "critical");
    // Hunger reached zero at 11 h too, so feeding is also needed to leave warning.
    let out = store.care("water", "water").unwrap();
    assert!(out.changed);
    let lc = out.lifecycle.unwrap();
    assert!(lc.critical.is_none());
    assert_eq!(status(&env, &id), "warning");
    store.care("food", "food").unwrap();
    assert!(events(&env, &id).contains(&"recovered".to_owned()));
    let s = store.snapshot().unwrap();
    assert_eq!(s.lifecycle.needs[1].neglect_ms, 0);
    assert_eq!(s.lifecycle.needs[1].hold_ms, HOUR_MS);
}

#[test]
fn food_does_not_clear_thirst_critical() {
    let env = Env::with_boot();
    let (mut store, id) = armed(&env);
    let mut f = Feeder::new();
    f.feed(&env, &store, &id, 8 * HOUR_MS);
    f.feed(&env, &store, &id, 3 * HOUR_MS);
    f.feed(&env, &store, &id, 2 * HOUR_MS);
    let before = store.snapshot().unwrap().lifecycle.critical.unwrap();
    for action in ["food", "play", "wash"] {
        store.care(action, action).unwrap();
    }
    let after = store.snapshot().unwrap().lifecycle.critical.unwrap();
    assert_eq!(after, before);
    assert_eq!(status(&env, &id), "critical");
    assert!(!events(&env, &id).contains(&"recovered".to_owned()));
}

#[test]
fn caring_for_one_of_two_critical_needs_keeps_the_frozen_grace() {
    let env = Env::with_boot();
    let (mut store, id) = armed(&env);
    let mut f = Feeder::new();
    f.feed(&env, &store, &id, 8 * HOUR_MS);
    f.feed(&env, &store, &id, 3 * HOUR_MS);
    // Thirst critical at 11 h; by 14 h hunger (empty at 11 h) is past 2 h too.
    let s = f.feed(&env, &store, &id, 3 * HOUR_MS);
    let crit = s.lifecycle.critical.unwrap();
    assert_eq!(
        (crit.need.as_str(), crit.elapsed_ms),
        ("thirst", 3 * HOUR_MS)
    );
    store.care("water", "water").unwrap();
    let crit = store.snapshot().unwrap().lifecycle.critical.unwrap();
    assert_eq!(
        (crit.need.as_str(), crit.elapsed_ms, crit.grace_ms),
        ("hunger", 3 * HOUR_MS, 6 * HOUR_MS)
    );
    store.care("food", "food").unwrap();
    assert!(store.snapshot().unwrap().lifecycle.critical.is_none());
    // Mood (22.00 after 13 h of decay) is low but never lethal: a warning remains.
    assert_eq!(status(&env, &id), "warning");
    store.care("routine", "routine").unwrap();
    assert_eq!(status(&env, &id), "healthy");
}

#[test]
fn gentle_faints_never_dies() {
    let env = Env::with_boot();
    let mut store = env.open();
    store.care("routine", "routine").unwrap();
    arm(&mut store, Mode::Gentle);
    let id = life_id(&store);
    let mut f = Feeder::new();
    for ms in [
        8 * HOUR_MS,
        3 * HOUR_MS,
        7 * HOUR_MS,
        8 * HOUR_MS,
        8 * HOUR_MS,
    ] {
        f.feed(&env, &store, &id, ms);
    }
    let s = store.snapshot().unwrap();
    let life = s.life.unwrap();
    assert!(life.fainted);
    assert_eq!(life.status, "critical");
    assert!(s.lifecycle.needs.iter().all(|n| n.until_end_ms.is_none()));
    assert!(events(&env, &id).contains(&"fainted".to_owned()));
    assert!(!events(&env, &id).contains(&"died".to_owned()));
    // Start stays blocked; care revives.
    let t = store.create_task(&env.input(Category::Hunger)).unwrap();
    assert!(matches!(
        store.preflight(&t.task_id, 1),
        Err(StorageError::InvalidState(_))
    ));
    store.care("revive", "routine").unwrap();
    let life = store.snapshot().unwrap().life.unwrap();
    assert!(!life.fainted);
    assert_eq!(life.status, "healthy");
}

#[test]
fn unarmed_life_never_decays_even_with_presence() {
    let env = Env::with_boot();
    let mut store = env.open();
    let id = life_id(&store);
    let before = store.snapshot().unwrap().meters;
    let mut f = Feeder::new();
    for _ in 0..4 {
        f.feed(&env, &store, &id, 8 * HOUR_MS);
    }
    assert_eq!(store.snapshot().unwrap().meters, before);
    // Arming starts the clock from now; earlier presence was consumed and discarded.
    arm(&mut store, Mode::Permadeath);
    store.care("routine", "routine").unwrap();
    let s = f.feed(&env, &store, &id, 2 * HOUR_MS);
    assert_eq!(s.meters.thirst, 8750);
}

#[test]
fn death_pending_in_every_slot_state_preserves_the_job_and_blocks_start() {
    for state in SLOT_STATE_LIST {
        let env = Env::with_boot();
        let (mut store, id) = armed(&env);
        let task = task_in_state(&mut store, &env, state);
        inject_critical(&env, "thirst", true);
        let tables = ["tasks", "task_events", "artifacts", "checks", "acceptances"];
        let before: Vec<String> = tables.iter().map(|t| env.dump(t)).collect();
        let s = store.snapshot().unwrap();
        // The slot is busy, so the end waits; the job and its output are untouched.
        assert!(s.life.as_ref().unwrap().death_pending, "{state:?}");
        assert_eq!(s.lifecycle.frozen_reason, "pending");
        assert_eq!(status(&env, &id), "critical");
        let after: Vec<String> = tables.iter().map(|t| env.dump(t)).collect();
        assert_eq!(before, after, "{state:?}");
        assert!(matches!(
            store.care("care", "water"),
            Err(StorageError::InvalidState(_))
        ));
        assert!(matches!(
            store.set_paused(true),
            Err(StorageError::InvalidState(_))
        ));
        assert!(matches!(
            store.select_appearance("sel", &id, "dog"),
            Err(StorageError::InvalidState(_))
        ));
        let draft = store.create_task(&env.input(Category::Hunger)).unwrap();
        assert!(store.preflight(&draft.task_id, 1).is_err(), "{state:?}");
        assert_eq!(store.task(&task).unwrap().state, state);
    }
}

#[test]
fn pending_death_copy_never_promises_that_care_can_still_help() {
    // Care is refused while the end is pending and the life ends when the slot frees, so the
    // readiness reason and the Start refusal must not suggest that caring afterwards saves it.
    let env = Env::with_boot();
    let (mut store, _id) = armed(&env);
    let _task = task_in_state(&mut store, &env, State::Running);
    inject_critical(&env, "thirst", true);
    let s = store.snapshot().unwrap();
    let name = s.life.as_ref().unwrap().name.clone();
    assert_eq!(
        s.care.readiness_reasons[0],
        format!(
            "New work waits until the current task is closed; {name}'s grace time has run out."
        )
    );
    for reason in &s.care.readiness_reasons {
        assert!(!reason.contains("needs care"), "{reason}");
    }
    let draft = store.create_task(&env.input(Category::Hunger)).unwrap();
    let text = store.preflight(&draft.task_id, 1).unwrap_err().to_string();
    assert!(text.contains("grace time has run out"), "{text}");
    assert!(!text.contains("needs care"), "{text}");
}

#[test]
fn accept_then_finalize_in_the_same_transaction() {
    let env = Env::with_boot();
    let (mut store, id) = armed(&env);
    let task = task_in_state(&mut store, &env, State::ReviewReady);
    inject_critical(&env, "thirst", true);
    let digest = store.review(&task).unwrap().review.unwrap().digest;
    let out = accept(&mut store, &task, &digest, "late-accept", Some("ok")).unwrap();
    assert_eq!(out.receipt.demo_xp, DEMO_XP);
    assert_eq!(out.receipt.life_id.as_deref(), Some(id.as_str()));
    assert_eq!(
        env.count(&format!(
            "SELECT count(*) FROM demo_xp_rewards WHERE life_id = '{id}'"
        )),
        1
    );
    // The XP landed before the end: the memorial counts it.
    let s = store.snapshot().unwrap();
    assert!(s.life.is_none());
    let m = s.memorial.latest.unwrap();
    assert_eq!((m.life_xp, m.tasks_accepted), (20, 1));
    assert_eq!(status(&env, &id), "dead");
    let kinds = events(&env, &id);
    assert_eq!(kinds.last().unwrap(), "died");
}

#[test]
fn discard_cancel_confirm_and_recover_finalize_a_pending_death() {
    // Discard.
    let env = Env::with_boot();
    let (mut store, id) = armed(&env);
    let task = task_in_state(&mut store, &env, State::ReviewReady);
    inject_critical(&env, "hunger", true);
    let digest = store.review(&task).unwrap().review.unwrap().digest;
    store.discard(&task, &digest).unwrap();
    assert_eq!(status(&env, &id), "dead");
    let m = store.snapshot().unwrap().memorial.latest.unwrap();
    assert_eq!(m.end_cause, "neglect:hunger");
    // Cancel confirmation.
    let env = Env::with_boot();
    let (mut store, id) = armed(&env);
    let task = task_in_state(&mut store, &env, State::Cancelling);
    inject_critical(&env, "thirst", true);
    store.confirm_cancelled(&task, sup()).unwrap();
    assert_eq!(status(&env, &id), "dead");
    // Recovery interrupting a dead supervisor.
    let env = Env::with_boot();
    let (mut store, id) = armed(&env);
    let task = started(&mut store, &env, Category::Hunger, dead_sup());
    inject_critical(&env, "thirst", true);
    store.recover().unwrap();
    assert_eq!(store.task(&task).unwrap().state, State::Interrupted);
    assert_eq!(status(&env, &id), "dead");
}

#[test]
fn death_touches_only_life_tables() {
    let env = Env::with_boot();
    let (mut store, id) = armed(&env);
    let (task, digest) = reviewed(
        &mut store,
        &env,
        Category::Thirst,
        Verification::NotVerified,
    );
    accept(&mut store, &task, &digest, "accept-1", Some("ok")).unwrap();
    store.care("routine", "routine").unwrap();
    let folder = env.project.join("keep.txt");
    fs::write(&folder, "project bytes").unwrap();
    let untouched = [
        "tasks",
        "task_scopes",
        "task_events",
        "artifacts",
        "checks",
        "acceptances",
        "rewards",
        "demo_xp_rewards",
        "settings",
        "apply_transactions",
        "review_repairs",
        "task_care_usage",
        "rulesets",
    ];
    let before: Vec<String> = untouched.iter().map(|t| env.dump(t)).collect();
    let results_before = env.dump("request_results");
    let mut f = Feeder::new();
    f.feed(&env, &store, &id, 8 * HOUR_MS);
    f.feed(&env, &store, &id, 3 * HOUR_MS);
    f.feed(&env, &store, &id, 6 * HOUR_MS);
    assert_eq!(status(&env, &id), "dead");
    let after: Vec<String> = untouched.iter().map(|t| env.dump(t)).collect();
    for ((table, b), a) in untouched.iter().zip(&before).zip(&after) {
        assert_eq!(b, a, "{table} changed on death");
    }
    // Settling snapshots record no receipts.
    assert_eq!(env.dump("request_results"), results_before);
    assert_eq!(fs::read_to_string(&folder).unwrap(), "project bytes");
    // Retirement of a new life touches the same set only.
    store
        .adopt(
            "adopt-2",
            &AdoptRequest {
                species: "dog",
                name: None,
                mode: Mode::Permadeath,
                pace: codepet_core::lifecycle::Pace::Steady,
                acknowledge_rules: true,
            },
        )
        .unwrap();
    let before: Vec<String> = untouched.iter().map(|t| env.dump(t)).collect();
    let new_id = life_id(&store);
    store.retire("retire-2", &new_id, "Biscuit").unwrap();
    let after: Vec<String> = untouched.iter().map(|t| env.dump(t)).collect();
    assert_eq!(before, after);
}

#[test]
fn terminal_lives_and_journal_are_immutable() {
    let env = Env::with_boot();
    let (mut store, id) = armed(&env);
    store.retire("retire", &id, "Ginger").unwrap();
    let c = env.raw();
    for sql in [
        "UPDATE lives SET status = 'healthy', ended_at_ms = NULL, end_cause = NULL,
            final_summary = NULL",
        "UPDATE lives SET name = 'Other'",
        "DELETE FROM lives",
        "DELETE FROM life_events",
        "UPDATE life_events SET data = '{}'",
        "UPDATE life_events SET kind = 'born'",
    ] {
        let e = c.execute(sql, []).unwrap_err().to_string();
        assert!(
            e.contains("immutable") || e.contains("history") || e.contains("only seen_at_ms"),
            "{sql}: {e}"
        );
    }
    assert_eq!(status(&env, &id), "retired");
}

#[test]
fn pace_ruleset_and_species_never_change_after_insert() {
    let env = Env::new();
    let store = env.open();
    let id = life_id(&store);
    let c = env.raw();
    for sql in [
        "UPDATE lives SET pace = 'playtest'",
        "UPDATE lives SET species = 'dog'",
        "UPDATE lives SET ordinal = 7",
        "UPDATE lives SET origin = 'adopted'",
    ] {
        assert!(c.execute(sql, []).is_err(), "{sql}");
    }
    // An unknown ruleset id cannot even be installed.
    assert!(c
        .execute(
            "INSERT INTO rulesets VALUES ('lifecycle-v9', '{}', ?1, 0)",
            [&"0".repeat(64)],
        )
        .is_err());
    assert_eq!(status(&env, &id), "healthy");
    // Source check: no storage code updates pace or ruleset.
    for (name, src) in sources() {
        for line in src.lines() {
            let l = line.to_ascii_lowercase();
            assert!(
                !(l.contains("set pace") || l.contains("set ruleset") || l.contains(", pace =")),
                "{name}: {line}"
            );
        }
    }
}

fn sources() -> [(&'static str, &'static str); 4] {
    [
        ("life.rs", include_str!("life.rs")),
        ("lifecycle_store.rs", include_str!("lifecycle_store.rs")),
        ("presence_io.rs", include_str!("presence_io.rs")),
        ("bar_export.rs", include_str!("bar_export.rs")),
    ]
}

#[test]
fn lifecycle_modules_never_delete_rows_spawn_threads_or_sleep() {
    for (name, src) in sources() {
        let upper = src.to_ascii_uppercase();
        assert!(!upper.contains("DELETE FROM"), "{name}");
        assert!(!src.contains("thread::spawn"), "{name}");
        assert!(!src.contains("thread::sleep"), "{name}");
        assert!(!src.contains("Command::new"), "{name}");
    }
}

#[test]
fn death_fault_before_commit_leaves_the_exact_pre_state() {
    let env = Env::with_boot();
    let (store, id) = armed(&env);
    let mut f = Feeder::new();
    f.feed(&env, &store, &id, 8 * HOUR_MS);
    f.feed(&env, &store, &id, 3 * HOUR_MS);
    env.pass(6 * HOUR_MS);
    env.write_presence("bar-feed-final", &id, 6 * HOUR_MS);
    let tables = ["lives", "life_events", "life_vitals", "pet_state", "meta"];
    let before: Vec<String> = tables.iter().map(|t| env.dump(t)).collect();
    test_fault::arm(Some("death_before_commit"));
    let r = catch_unwind(AssertUnwindSafe(|| store.snapshot()));
    test_fault::arm(None);
    assert!(r.is_err());
    let after: Vec<String> = tables.iter().map(|t| env.dump(t)).collect();
    assert_eq!(before, after);
    let store = env.open();
    assert!(store.snapshot().unwrap().life.is_none());
    assert_eq!(
        env.count("SELECT count(*) FROM life_events WHERE kind = 'died'"),
        1
    );
    store.snapshot().unwrap();
    assert_eq!(
        env.count("SELECT count(*) FROM life_events WHERE kind = 'died'"),
        1
    );
}

#[test]
fn accept_versus_settlement_on_two_handles_gives_one_terminal_event() {
    let env = Env::with_boot();
    let (mut store, id) = armed(&env);
    let task = task_in_state(&mut store, &env, State::ReviewReady);
    inject_critical(&env, "thirst", true);
    let digest = store.review(&task).unwrap().review.unwrap().digest;
    let mut a = env.open();
    let b = env.open();
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let (barrier, task, digest) = (&barrier, &task, &digest);
        let ha = scope.spawn(move || {
            barrier.wait();
            accept(&mut a, task, digest, "race-accept", Some("ok")).map(|o| o.receipt.demo_xp)
        });
        let hb = scope.spawn(move || {
            barrier.wait();
            b.snapshot().map(|s| s.life.is_some())
        });
        assert_eq!(ha.join().unwrap().unwrap(), DEMO_XP);
        hb.join().unwrap().unwrap();
    });
    assert_eq!(status(&env, &id), "dead");
    assert_eq!(
        env.count("SELECT count(*) FROM life_events WHERE kind = 'died'"),
        1
    );
    assert_eq!(env.count("SELECT count(*) FROM demo_xp_rewards"), 1);
}

#[test]
fn care_versus_death_ordering_on_two_handles() {
    for _ in 0..4 {
        let env = Env::with_boot();
        let (store, id) = armed(&env);
        let mut f = Feeder::new();
        f.feed(&env, &store, &id, 8 * HOUR_MS);
        f.feed(&env, &store, &id, 3 * HOUR_MS);
        env.pass(6 * HOUR_MS);
        env.write_presence("bar-feed-race", &id, 6 * HOUR_MS);
        let mut a = env.open();
        let b = env.open();
        let barrier = std::sync::Barrier::new(2);
        let care = std::thread::scope(|scope| {
            let barrier = &barrier;
            let ha = scope.spawn(move || {
                barrier.wait();
                a.care("race-water", "water").map(|o| o.changed)
            });
            let hb = scope.spawn(move || {
                barrier.wait();
                b.snapshot().map(|_| ())
            });
            hb.join().unwrap().unwrap();
            ha.join().unwrap()
        });
        // Whoever locks first settles the due death; the other sees the result. Care can
        // never revive a life that ended, nor run on a pending one.
        assert!(matches!(
            care,
            Err(StorageError::NoActivePet | StorageError::PassedAway)
        ));
        assert_eq!(status(&env, &id), "dead");
        assert_eq!(
            env.count("SELECT count(*) FROM life_events WHERE kind = 'died'"),
            1
        );
    }
}

/// Small deterministic generator (no new dependency).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self, n: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) % n
    }
}

#[test]
fn death_never_commits_while_a_task_holds_the_slot() {
    let slot = format!(
        "SELECT EXISTS(SELECT 1 FROM tasks WHERE state IN {})",
        crate::schema::SLOT_STATES
    );
    for seed in 1..=6u64 {
        let env = Env::with_boot();
        let mut store = env.open();
        store.care("routine", "routine").unwrap();
        store
            .set_rules("arm", &life_id(&store), Mode::Permadeath, true)
            .unwrap();
        let mut life = life_id(&store);
        let mut rng = Lcg(seed);
        let mut session = 0;
        let mut active: Option<String> = None;
        for step in 0..60 {
            let died_before = env.count("SELECT count(*) FROM life_events WHERE kind = 'died'");
            match rng.next(6) {
                0 | 1 => {
                    session += 1;
                    let ms = (1 + rng.next(8) as i64) * HOUR_MS;
                    env.pass(ms);
                    env.write_presence(&format!("bar-prop-{session:04}"), &life, ms);
                    let _ = store.snapshot();
                }
                2 => {
                    let _ = store.care(&format!("c-{seed}-{step}"), "routine");
                }
                3 if active.is_none() => {
                    let ready = store.snapshot().unwrap();
                    if ready.care.ready_to_work {
                        let t = store.create_task(&env.input(Category::Hunger)).unwrap();
                        let p = store.preflight(&t.task_id, 1).unwrap();
                        store
                            .start(&t.task_id, 1, &p.preflight_token, sup())
                            .unwrap();
                        active = Some(t.task_id);
                    }
                }
                4 => {
                    if let Some(t) = active.take() {
                        store.fail(&t, sup()).unwrap();
                    }
                }
                _ => {
                    if store.snapshot().unwrap().life.is_none() {
                        store
                            .adopt(
                                &format!("adopt-{seed}-{step}"),
                                &AdoptRequest {
                                    species: "frog",
                                    name: None,
                                    mode: Mode::Permadeath,
                                    pace: codepet_core::lifecycle::Pace::Playtest,
                                    acknowledge_rules: true,
                                },
                            )
                            .unwrap();
                        life = life_id(&store);
                    }
                }
            }
            let died_after = env.count("SELECT count(*) FROM life_events WHERE kind = 'died'");
            if died_after > died_before {
                assert_eq!(env.count(&slot), 0, "seed {seed} step {step}");
            }
            // A pending end never survives a committed state with a free slot.
            if env.count("SELECT death_pending FROM life_vitals") == 1 {
                assert_eq!(env.count(&slot), 1, "seed {seed} step {step}");
            }
            assert!(
                env.count(
                    "SELECT count(*) FROM lives WHERE status IN ('healthy','warning','critical')"
                ) <= 1
            );
        }
    }
}

/// Review finding: a grace that runs out during a write's own settlement used to be rolled
/// back when the command body then refused, and the refusal named a task that did not
/// exist. Now the end commits first and every command refuses with `PassedAway`.
#[test]
fn an_end_that_settles_due_inside_a_refused_write_is_committed() {
    type Command = fn(&mut Store, &str) -> Result<()>;
    let commands: [(&str, Command); 6] = [
        ("care", |s, _| s.care("late-water", "water").map(drop)),
        ("sleep", |s, _| s.sleep("late-sleep").map(drop)),
        ("pause", |s, _| s.set_paused(true).map(drop)),
        ("rules", |s, id| {
            s.set_rules("late-rules", id, Mode::Gentle, true).map(drop)
        }),
        ("select", |s, id| {
            s.select_appearance("late-select", id, "dog").map(drop)
        }),
        ("adopt", |s, _| {
            let req = AdoptRequest {
                species: "cat",
                name: None,
                mode: Mode::Permadeath,
                pace: codepet_core::lifecycle::Pace::Steady,
                acknowledge_rules: true,
            };
            s.adopt("late-adopt", &req).map(drop)
        }),
    ];
    for (what, command) in commands {
        let env = Env::with_boot();
        let (mut store, id) = armed(&env);
        let mut f = Feeder::new();
        f.feed(&env, &store, &id, 8 * HOUR_MS);
        let s = f.feed(&env, &store, &id, 3 * HOUR_MS);
        assert_eq!(s.life.as_ref().unwrap().status, "critical", "{what}");
        // The whole grace is attended, but nothing has settled it yet.
        env.pass(6 * HOUR_MS);
        env.write_presence("bar-late-0001", &id, 6 * HOUR_MS);
        let err = command(&mut store, &id).expect_err(what);
        assert!(matches!(err, StorageError::PassedAway), "{what}: {err:?}");
        assert_eq!(err.code(), "NO_ACTIVE_PET", "{what}");
        assert!(err.to_string().contains("passed away"), "{what}: {err}");
        // The end is committed, not rolled back with the refused body.
        assert_eq!(status(&env, &id), "dead", "{what}");
        assert_eq!(
            env.count(
                "SELECT count(*) FROM life_vitals WHERE life_id IS NULL AND death_pending = 0"
            ),
            1,
            "{what}"
        );
        let s = store.snapshot().unwrap();
        assert!(s.life.is_none(), "{what}");
        assert_eq!(s.memorial.count, 1, "{what}");
        let m = s.memorial.latest.unwrap();
        assert_eq!(
            (m.end_cause.as_str(), m.attended_ms),
            ("neglect:thirst", 17 * HOUR_MS),
            "{what}: the claim is charged exactly once"
        );
        assert_eq!(
            events(&env, &id).iter().filter(|k| *k == "died").count(),
            1,
            "{what}"
        );
        // Afterwards the ordinary answer applies.
        assert!(matches!(
            store.care("later-water", "water"),
            Err(StorageError::NoActivePet)
        ));
    }
}
