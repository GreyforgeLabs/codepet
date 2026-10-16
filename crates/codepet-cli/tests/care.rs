//! Care and progression through fresh CLI processes. Explicit actions never launch work.
// Task-pipeline tests and helpers run only with `legacy-tasks` (deleted in v0.3.1).
#![cfg_attr(not(feature = "legacy-tasks"), allow(dead_code, unused_imports))]
mod common;
use codepet_storage::{Clock, ManualClock, StatePaths, Store, SystemClock};
use common::{error_code, Env};
use serde_json::{json, Value};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

fn full_meters() -> Value {
    json!({"hunger": 10000, "thirst": 10000, "mental_health": 10000, "hygiene": 10000})
}
fn spent_meters() -> Value {
    json!({"hunger": 9000, "thirst": 9000, "mental_health": 9500, "hygiene": 9000})
}
#[cfg(feature = "legacy-tasks")]
fn discard(env: &Env, task: &str) {
    let digest = env.task(task)["review"]["digest"].clone();
    env.ok(
        "task.discard",
        json!({"task_id": task, "review_digest": digest}),
    );
}
#[cfg(feature = "legacy-tasks")]
fn accept(env: &Env, task: &str, digest: &str) -> Value {
    env.ok(
        "task.accept_artifact",
        json!({"task_id": task, "review_digest": digest,
        "acknowledged": true, "override_reason": "Reviewed the simulation"}),
    )
}

#[test]
fn invalid_care_and_rest_requests_never_open_storage() {
    let env = Env::new();
    for (method, params) in [
        ("pet.care", json!({})),
        ("pet.care", json!({"action": "feed"})),
        ("pet.care", json!({"action": "Food"})),
        ("pet.care", json!({"action": null})),
        ("pet.care", json!({"action": "food", "amount": 100})),
        (
            "pet.care",
            json!({"action": "food", "request_id": "override"}),
        ),
        ("pet.sleep", json!({"duration_ms": 0})),
        ("pet.wake", json!({"force": true})),
    ] {
        assert_eq!(env.err(method, params), "INVALID_PARAMS", "{method}");
    }
    let repeated =
        br#"{"v":1,"id":"care","method":"pet.care","params":{"action":"food","action":"routine"}}
"#;
    assert_eq!(error_code(&env.raw(repeated, None)), "INVALID_REQUEST");
    assert!(
        !env.xdg().exists(),
        "malformed care requests must not create state"
    );
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn explicit_care_is_idempotent_and_never_grants_xp_or_starts_work() {
    let env = Env::new();
    let initial = env.state();
    assert_eq!(
        initial["meters"],
        json!({"hunger":7000,"thirst":7000,"mental_health":7000,"hygiene":7000})
    );
    assert_eq!(initial["care"]["ready_to_work"], false);
    assert_eq!(
        initial["care"]["readiness_reasons"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    let mut meters = initial["meters"].clone();
    for (action, meter) in [
        ("food", "hunger"),
        ("water", "thirst"),
        ("play", "mental_health"),
        ("wash", "hygiene"),
    ] {
        let request_id = format!("care-{action}");
        let result = env.ok_with(&request_id, "pet.care", json!({"action": action}));
        meters[meter] = json!(10000);
        assert_eq!(result["request_id"], request_id);
        assert_eq!(result["action"], action);
        assert_eq!(result["changed"], true);
        assert_eq!(result["meters"], meters);
        assert_eq!(
            env.ok_with(&request_id, "pet.care", json!({"action": action})),
            result
        );
        let revision = env.state()["care"]["revision"].clone();
        let noop = env.ok("pet.care", json!({"action": action}));
        assert_eq!(noop["changed"], false);
        assert_eq!(env.state()["care"]["revision"], revision);
    }
    assert_eq!(
        env.err_with("care-food", "pet.care", json!({"action":"water"})),
        "IDEMPOTENCY_CONFLICT"
    );
    assert_eq!(
        env.err_with("care-food", "pet.sleep", json!({})),
        "IDEMPOTENCY_CONFLICT"
    );
    let s = env.state();
    assert_eq!(s["meters"], full_meters());
    assert_eq!(s["care"]["ready_to_work"], true);
    assert_eq!(s["care"]["readiness_reasons"], json!([]));
    assert_eq!(s["care"]["work_sessions"], 0);
    assert_eq!(s["active_task"], Value::Null);
    assert_eq!(s["recent"], json!([]));
    for lane in ["real", "demo"] {
        assert_eq!(s["progression"][lane]["lifetime_xp"], 0);
        assert_eq!(s["progression"][lane]["level"], 1);
        assert_eq!(s["progression"][lane]["next_level_xp"], 80);
    }
    assert!(!env.folder().exists());
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn readiness_is_rechecked_and_care_changes_invalidate_preflight() {
    let env = Env::new();
    let id = env.create("hunger", 0, "artifact");
    let token = env.preflight(&id, 1);
    let before = env.state();
    let denied = env.call("task.start", Env::start_params(&id, 1, &token));
    assert_eq!(error_code(&denied), "INVALID_STATE");
    assert!(denied.response()["error"]["message"]
        .as_str()
        .unwrap()
        .contains("food"));
    let after = env.state();
    assert_eq!(after["meters"], before["meters"]);
    assert_eq!(after["care"], before["care"]);
    assert_eq!(env.states(&id), ["draft"]);

    env.care_routine();
    assert_eq!(
        env.err("task.start", Env::start_params(&id, 1, &token)),
        "TOKEN_INVALID"
    );
    let token = env.preflight(&id, 1);
    let noop = env.ok("pet.care", json!({"action":"food"}));
    assert_eq!(noop["changed"], false);
    env.start(&id, 1, &token);
    assert_eq!(env.task(&id)["state"], "review_ready");
    assert_eq!(env.state()["meters"], spent_meters());
    assert_eq!(env.state()["care"]["work_sessions"], 1);
    assert_eq!(
        env.err("task.start", Env::start_params(&id, 1, &token)),
        "INVALID_STATE"
    );
    assert_eq!(
        env.state()["care"]["work_sessions"],
        1,
        "replay never consumes another session"
    );
    discard(&env, &id);
    let next = env.create("thirst", 0, "artifact");
    let token = env.preflight(&next, 1);
    assert_eq!(
        env.err("task.start", Env::start_params(&next, 1, &token)),
        "INVALID_STATE"
    );
    assert_eq!(
        env.state()["meters"],
        spent_meters(),
        "discard never refills care"
    );
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn bathroom_and_mandatory_sleep_survive_restarts_and_block_only_new_work() {
    let env = Env::new();
    for expected_sessions in 1..=2 {
        let (id, _) = env.reviewed("hunger", "artifact");
        assert_eq!(env.state()["care"]["work_sessions"], expected_sessions);
        discard(&env, &id);
    }
    let s = env.state();
    assert_eq!(s["care"]["bathroom_due"], true);
    assert_eq!(s["care"]["starts_since_bathroom"], 2);
    for action in ["food", "water", "play", "wash"] {
        env.ok("pet.care", json!({"action":action}));
    }
    assert_eq!(env.state()["meters"], full_meters());
    assert_eq!(
        env.state()["care"]["bathroom_due"],
        true,
        "washing alone cannot clear bathroom need"
    );
    let third = env.create("hygiene", 0, "artifact");
    let token = env.preflight(&third, 1);
    let blocked = env.call("task.start", Env::start_params(&third, 1, &token));
    assert_eq!(error_code(&blocked), "INVALID_STATE");
    assert!(blocked.response()["error"]["message"]
        .as_str()
        .unwrap()
        .contains("bathroom"));
    env.ok("pet.care", json!({"action":"bathroom"}));
    assert_eq!(env.state()["care"]["bathroom_due"], false);
    let token = env.preflight(&third, 1);
    env.start(&third, 1, &token);
    let s = env.state();
    assert_eq!(s["care"]["sleep_due"], true);
    assert_eq!(s["care"]["work_sessions"], 3);
    assert_eq!(
        env.task(&third)["state"],
        "review_ready",
        "new sleep need never interrupts the active job"
    );
    let digest = env.task(&third)["review"]["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    accept(&env, &third, &digest);
    env.care_routine();
    assert_eq!(
        env.state()["care"]["sleep_due"],
        true,
        "care does not replace sleep"
    );
    let next = env.create("mental_health", 0, "artifact");
    let token = env.preflight(&next, 1);
    let blocked = env.call("task.start", Env::start_params(&next, 1, &token));
    assert_eq!(error_code(&blocked), "INVALID_STATE");
    assert!(blocked.response()["error"]["message"]
        .as_str()
        .unwrap()
        .contains("nap"));
    let sleep = env.ok_with("nap", "pet.sleep", json!({}));
    assert_eq!(sleep["care"]["sleeping"], true);
    assert_eq!(
        env.err("task.start", Env::start_params(&next, 1, &token)),
        "TOKEN_INVALID",
        "explicit bedtime invalidates a pre-existing token"
    );
    let sleeping = env.state();
    assert_eq!(sleeping["care"]["sleeping"], true);
    assert_eq!(sleeping["care"]["ready_to_work"], false);
    assert_eq!(sleeping["meters"], full_meters());
    let token = env.preflight(&next, 1);
    assert_eq!(
        env.err("task.start", Env::start_params(&next, 1, &token)),
        "INVALID_STATE"
    );
    let wake = env.ok_with("early-wake", "pet.wake", json!({}));
    assert_eq!(wake["care"]["sleeping"], false);
    assert_eq!(wake["care"]["sleep_due"], true);
    assert_eq!(wake["care"]["work_sessions"], 3);
    assert_eq!(
        env.ok_with("nap", "pet.sleep", json!({})),
        sleep,
        "replay returns the original receipt"
    );
    assert_eq!(
        env.state()["care"]["sleeping"],
        false,
        "replayed sleep cannot put it back to bed"
    );
    assert_eq!(
        env.task(&next)["state"],
        "draft",
        "waking never starts work"
    );
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn early_wake_keeps_credit_and_repeated_sleep_does_not_restart_the_timer() {
    let env = Env::new();
    let initial_meters = env.state()["meters"].clone();
    let sleep = env.ok_with("sleep-1", "pet.sleep", json!({}));
    assert_eq!(sleep["care"]["sleep_remaining_ms"], 120_000);
    thread::sleep(Duration::from_millis(100));
    let wake = env.ok_with("wake-1", "pet.wake", json!({}));
    let remaining = wake["care"]["sleep_remaining_ms"].as_i64().unwrap();
    assert!(
        (1..120_000).contains(&remaining),
        "elapsed credit preserved: {wake}"
    );
    assert_eq!(
        wake["care"]["sleep_due"], true,
        "even a voluntary nap must finish before work"
    );
    assert_eq!(env.ok_with("wake-1", "pet.wake", json!({})), wake);
    let resumed = env.ok("pet.sleep", json!({}));
    assert_eq!(resumed["care"]["sleep_remaining_ms"], remaining);
    let repeated = env.ok("pet.sleep", json!({}));
    assert_eq!(
        repeated["care"]["sleep_started_at_ms"],
        resumed["care"]["sleep_started_at_ms"]
    );
    assert!(repeated["care"]["sleep_remaining_ms"].as_i64().unwrap() <= remaining);
    let s = env.state();
    assert_eq!(s["meters"], initial_meters);
    assert_eq!(s["recent"], json!([]));
    assert_eq!(s["progression"]["demo"]["lifetime_xp"], 0);
}

/// Seed an explicitly started nap whose wall-clock interval has already elapsed, then
/// let the normal CLI process settle it. No test-only clock environment reaches the CLI.
fn elapsed_nap(env: &Env) {
    let paths = StatePaths::resolve(|key| match key {
        "XDG_STATE_HOME" => Some(env.xdg().into_os_string()),
        "HOME" => Some(env.home().into_os_string()),
        _ => None,
    })
    .unwrap();
    let mut store = Store::open(
        &paths,
        Arc::new(ManualClock::new(SystemClock.now_ms() - 120_001)),
    )
    .unwrap();
    store.sleep(&env.id()).unwrap();
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn an_explicit_nap_finishes_offline_without_refilling_care_or_starting_work() {
    let env = Env::new();
    let before = env.state();
    elapsed_nap(&env);
    let after = env.state();
    assert_eq!(after["care"]["sleeping"], false);
    assert_eq!(after["care"]["sleep_due"], false);
    assert_eq!(after["care"]["sleep_started_at_ms"], Value::Null);
    assert_eq!(after["care"]["work_sessions"], 0);
    assert_eq!(after["care"]["execution_ms"], 0);
    assert_eq!(after["meters"], before["meters"]);
    assert_eq!(after["progression"], before["progression"]);
    assert_eq!(after["recent"], json!([]));
    assert_eq!(after["active_task"], Value::Null);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn concurrent_process_starts_get_one_slot_and_one_care_debit() {
    let env = Env::new();
    env.care_routine();
    let a = env.create("hunger", 0, "artifact");
    let b = env.create("thirst", 0, "artifact");
    let ta = env.preflight(&a, 1);
    let tb = env.preflight(&b, 1);
    let barrier = Barrier::new(2);
    let launch = |task: &str, token: &str| {
        barrier.wait();
        env.call("task.start", Env::start_params(task, 1, token))
    };
    let replies = thread::scope(|scope| {
        let one = scope.spawn(|| launch(&a, &ta));
        let two = scope.spawn(|| launch(&b, &tb));
        [one.join().unwrap(), two.join().unwrap()]
    });
    let mut successes = 0;
    for r in replies {
        if r.response()["ok"] == true {
            successes += 1;
        } else {
            assert_eq!(error_code(&r), "BUSY");
        }
    }
    assert_eq!(successes, 1);
    let s = env.state();
    assert_eq!(s["meters"], spent_meters());
    assert_eq!(s["care"]["work_sessions"], 1);
    assert_eq!(s["care"]["starts_since_bathroom"], 1);
    assert_eq!(s["active_task"]["state"], "review_ready");
    assert_eq!(
        env.states(&a)
            .iter()
            .chain(env.states(&b).iter())
            .filter(|state| *state == "preparing")
            .count(),
        1
    );
    assert!(!env.folder().exists());
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn four_accepted_simulations_level_only_demo_and_cannot_enable_providers() {
    let env = Env::new();
    let providers = env.ok("providers.inspect", json!({}));
    for (index, (category, mastery)) in [
        ("hunger", "bugfix"),
        ("thirst", "quality_performance"),
        ("mental_health", "learning_specification"),
        ("hygiene", "cleanup"),
    ]
    .into_iter()
    .enumerate()
    {
        if index == 3 {
            elapsed_nap(&env);
            assert_eq!(env.state()["care"]["sleep_due"], false);
        }
        let (id, digest) = env.reviewed(category, "artifact");
        let receipt = accept(&env, &id, &digest)["receipt"].clone();
        assert_eq!(receipt["demo_xp"], 20);
        assert_eq!(receipt["real_xp"], 0);
        assert_eq!(receipt["progression_track"], mastery);
        assert_eq!(receipt["amount_hundredths"], 0);
        let duplicate = accept(&env, &id, &digest);
        assert_eq!(duplicate["duplicate"], true);
        assert_eq!(duplicate["receipt"], receipt);
        assert_eq!(
            env.state()["progression"]["demo"]["lifetime_xp"],
            (index + 1) * 20
        );
        assert_eq!(env.state()["meters"], spent_meters());
    }
    let s = env.state();
    assert_eq!(s["progression"]["demo"]["level"], 2);
    assert_eq!(s["progression"]["demo"]["next_level_xp"], 200);
    assert_eq!(s["progression"]["real"]["level"], 1);
    assert_eq!(s["progression"]["real"]["lifetime_xp"], 0);
    assert_eq!(env.ok("providers.inspect", json!({})), providers);
    for mastery in [
        "bugfix",
        "quality_performance",
        "learning_specification",
        "cleanup",
    ] {
        assert_eq!(s["progression"]["demo"]["mastery"][mastery], 20);
        assert_eq!(s["progression"]["real"]["mastery"][mastery], 0);
    }
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn sleeping_keeps_completed_results_reviewable_and_acceptance_available() {
    let env = Env::new();
    let (id, digest) = env.reviewed("mental_health", "artifact");
    let nap = env.ok("pet.sleep", json!({}));
    assert_eq!(nap["care"]["sleeping"], true);
    let review = env.ok("task.review", json!({"task_id": id}));
    assert_eq!(review["review"]["digest"], digest);
    assert_eq!(
        env.err("pet.care", json!({"action":"routine"})),
        "INVALID_STATE"
    );
    let result = accept(&env, &id, &digest);
    assert_eq!(result["receipt"]["demo_xp"], 20);
    let s = env.state();
    assert_eq!(s["care"]["sleeping"], true);
    assert_eq!(s["meters"], spent_meters());
    assert_eq!(s["active_task"], Value::Null);
    assert_eq!(s["progression"]["real"]["lifetime_xp"], 0);
    env.ok(
        "settings.update",
        json!({"expected_revision": 1,
        "decay_enabled": false, "decay_per_hour": [0,0,0,0], "timeout_seconds": 600}),
    );
    assert_eq!(env.state()["settings"]["timeout_seconds"], 600);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn concurrent_acceptance_returns_one_receipt_and_one_demo_reward() {
    let env = Env::new();
    let (id, digest) = env.reviewed("hunger", "artifact");
    let barrier = Barrier::new(2);
    let request = || {
        barrier.wait();
        accept(&env, &id, &digest)
    };
    let [a, b] = thread::scope(|scope| {
        let a = scope.spawn(request);
        let b = scope.spawn(request);
        [a.join().unwrap(), b.join().unwrap()]
    });
    assert_ne!(a["duplicate"], b["duplicate"]);
    assert_eq!(a["receipt"], b["receipt"]);
    let s = env.state();
    assert_eq!(s["progression"]["demo"]["lifetime_xp"], 20);
    assert_eq!(s["progression"]["demo"]["mastery"]["bugfix"], 20);
    assert_eq!(s["progression"]["real"]["lifetime_xp"], 0);
    assert_eq!(s["meters"], spent_meters());
    assert_eq!(s["active_task"], Value::Null);
}
