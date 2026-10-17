//! Crash injection through `CODEPET_FAULT` (debug builds only). Each fault aborts the real
//! process; the next process must see a consistent durable state.
// Task-pipeline tests and helpers run only with `legacy-tasks` (retained for non-default compatibility).
#![cfg_attr(not(feature = "legacy-tasks"), allow(dead_code, unused_imports))]
mod common;
use codepet_storage::{Clock, SystemClock};
use common::{Env, Timeline, MIN};
use serde_json::{json, Value};

fn assert_aborted_silently(r: &common::Reply) {
    assert!(r.aborted(), "expected SIGABRT: {r:?}");
    assert!(
        r.lines.is_empty(),
        "no response after an abort before reply: {r:?}"
    );
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn abort_before_accept_commit_grants_nothing_and_retry_succeeds_once() {
    let env = Env::with_first_run_pet();
    let (id, digest) = env.reviewed("hunger", "artifact");
    let params = json!({"task_id": id, "review_digest": digest, "acknowledged": true,
        "override_reason": "mock reviewed"});
    let r = env.call_with(
        "acc-1",
        "task.accept_artifact",
        params.clone(),
        Some("accept_before_commit"),
    );
    assert_aborted_silently(&r);
    assert_eq!(env.meter("hunger"), 9000);
    assert_eq!(env.state()["progression"]["demo"]["lifetime_xp"], 0);
    let t = env.task(&id);
    assert_eq!(t["state"], "review_ready");
    assert_eq!(t["receipt"], Value::Null);

    let first = env.ok_with("acc-1", "task.accept_artifact", params.clone());
    assert_eq!(first["duplicate"], false);
    assert_eq!(first["receipt"]["amount_hundredths"], 0);
    assert_eq!(first["receipt"]["demo_xp"], 20);
    assert_eq!(first["receipt"]["real_xp"], 0);
    assert_eq!(env.state()["progression"]["demo"]["lifetime_xp"], 20);
    assert_eq!(env.meter("hunger"), 9000);
    assert_eq!(env.ok_with("acc-1", "task.accept_artifact", params), first);
    assert_eq!(env.meter("hunger"), 9000);
    assert_eq!(env.state()["progression"]["demo"]["lifetime_xp"], 20);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn abort_after_accept_commit_keeps_exactly_one_reward() {
    let env = Env::with_first_run_pet();
    let (id, digest) = env.reviewed("thirst", "artifact");
    let params = json!({"task_id": id, "review_digest": digest, "acknowledged": true,
        "override_reason": "mock reviewed"});
    let r = env.call_with(
        "acc-1",
        "task.accept_artifact",
        params.clone(),
        Some("accept_after_commit"),
    );
    assert_aborted_silently(&r);
    // Committed before the abort: reward, acceptance and state are all durable.
    assert_eq!(env.meter("thirst"), 9000);
    assert_eq!(env.task(&id)["state"], "accepted_artifact");
    assert_eq!(env.state()["progression"]["demo"]["lifetime_xp"], 20);
    assert_eq!(env.state()["progression"]["real"]["lifetime_xp"], 0);

    let retry = env.ok_with("acc-1", "task.accept_artifact", params.clone());
    assert_eq!(
        retry["duplicate"], false,
        "the recorded response is returned as stored"
    );
    assert_eq!(retry["receipt"]["request_id"], "acc-1");
    assert_eq!(retry["receipt"]["override_reason"], "mock reviewed");
    let other = env.ok_with("acc-2", "task.accept_artifact", params);
    assert_eq!(other["duplicate"], true);
    assert_eq!(other["receipt"], retry["receipt"]);
    assert_eq!(env.meter("thirst"), 9000, "acceptance never refills care");
    assert_eq!(
        env.state()["progression"]["demo"]["lifetime_xp"],
        20,
        "20 demo XP exactly once"
    );
    assert_eq!(env.task(&id)["receipt"], retry["receipt"]);
}

#[cfg(feature = "legacy-tasks")]
/// Starts with a fault armed; returns (task_id, reply).
fn start_with_fault(env: &Env, fault: &str) -> (String, common::Reply) {
    env.care_routine();
    let id = env.create("hygiene", 0, "artifact");
    let token = env.preflight(&id, 1);
    let r = env.call_with(
        &env.id(),
        "task.start",
        Env::start_params(&id, 1, &token),
        Some(fault),
    );
    assert!(r.aborted(), "{fault}: {r:?}");
    (id, r)
}

#[cfg(feature = "legacy-tasks")]
/// `task.recover` turns the dead supervisor's row into `interrupted`, never resumes it,
/// and frees the slot for a new explicit start.
fn recover_interrupts(env: &Env, id: &str, from: &str) {
    assert_eq!(env.state()["active_task"]["state"], from);
    let rec = env.ok("task.recover", json!({}));
    assert_eq!(
        rec["changed"],
        json!([{"task_id": id, "from": from, "to": "interrupted"}])
    );
    assert_eq!(rec["active_task"], Value::Null);
    // A second recovery changes nothing, and no process relaunched the task.
    assert_eq!(env.ok("task.recover", json!({}))["changed"], json!([]));
    std::thread::sleep(std::time::Duration::from_millis(100));
    let t = env.task(id);
    assert_eq!(t["state"], "interrupted");
    assert_eq!(t["review"], Value::Null);
    assert_eq!(env.states(id).last().unwrap(), "interrupted");
    assert_eq!(
        env.meter("hygiene"),
        if from == "preparing" { 10000 } else { 9000 }
    );
    assert_eq!(
        env.state()["care"]["work_sessions"],
        if from == "preparing" { 0 } else { 1 }
    );
    assert!(!env.folder().exists());
    // Slot released: a fresh task can be started explicitly.
    let (next, _) = env.reviewed("hunger", "artifact");
    assert_ne!(next, id);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn abort_after_start_commit_is_recovered_as_interrupted() {
    let env = Env::with_first_run_pet();
    let (id, r) = start_with_fault(&env, "start_after_commit");
    assert!(r.lines.is_empty(), "aborted before the response: {r:?}");
    assert_eq!(env.states(&id), ["draft", "preparing"]);
    recover_interrupts(&env, &id, "preparing");
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn abort_in_running_supervisor_is_recovered_as_interrupted() {
    let env = Env::with_first_run_pet();
    let (id, r) = start_with_fault(&env, "supervisor_running");
    // The start response was printed and flushed before supervision began.
    assert_eq!(r.response()["ok"], true);
    assert_eq!(r.response()["result"]["state"], "preparing");
    assert_eq!(env.states(&id), ["draft", "preparing", "running"]);
    recover_interrupts(&env, &id, "running");
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn abort_before_review_commit_rolls_back_the_review() {
    let env = Env::with_first_run_pet();
    let (id, r) = start_with_fault(&env, "review_before_commit");
    assert_eq!(r.response()["ok"], true);
    assert_eq!(env.task(&id)["review"], Value::Null);
    assert_eq!(
        env.states(&id),
        ["draft", "preparing", "running", "verifying"]
    );
    recover_interrupts(&env, &id, "verifying");
}

#[test]
fn abort_before_adoption_commit_adopts_nothing_and_retry_adopts_once() {
    let env = Env::with_first_run_pet();
    env.retire();
    let before = env.state();
    let params = json!({"species": "capybara", "mode": "permadeath", "acknowledge_rules": true});
    let r = env.call_with(
        "adopt-1",
        "pet.adopt",
        params.clone(),
        Some("adopt_before_commit"),
    );
    assert_aborted_silently(&r);
    let after = env.state();
    assert_eq!(after["life"], Value::Null);
    assert_eq!(after["memorial"], before["memorial"]);
    assert_eq!(after["meters"], before["meters"]);
    assert_eq!(env.bar_state().unwrap()["life"], Value::Null);

    let adopted = env.ok_with("adopt-1", "pet.adopt", params.clone());
    assert_eq!(adopted["changed"], true);
    assert_eq!(adopted["life"]["ordinal"], 2);
    assert_eq!(adopted["life"]["name"], "Yuzu");
    assert_eq!(env.ok_with("adopt-1", "pet.adopt", params), adopted);
    assert_eq!(env.state()["life"]["life_id"], adopted["life"]["life_id"]);
    assert_eq!(env.bar_state().unwrap()["life"]["name"], "Yuzu");
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn abort_before_evolution_commit_grants_no_xp_and_no_event() {
    let env = Env::with_first_run_pet();
    for n in 0..3 {
        let (id, digest) = env.reviewed("hunger", "artifact");
        env.ok(
            "task.accept_artifact",
            json!({"task_id": id, "review_digest": digest, "acknowledged": true,
                "override_reason": format!("reviewed {n}")}),
        );
    }
    env.elapsed_nap();
    let (id, digest) = env.reviewed("thirst", "artifact");
    let params = json!({"task_id": id, "review_digest": digest, "acknowledged": true,
        "override_reason": "mock reviewed"});
    let r = env.call_with(
        "acc-4",
        "task.accept_artifact",
        params.clone(),
        Some("evolve_before_commit"),
    );
    assert_aborted_silently(&r);
    let s = env.state();
    assert_eq!(s["life"]["xp"], 60);
    assert_eq!(s["life"]["stage"], 1);
    assert_eq!(s["unseen_events"], json!([]));
    assert_eq!(env.task(&id)["state"], "review_ready");

    let first = env.ok_with("acc-4", "task.accept_artifact", params.clone());
    assert_eq!(first["receipt"]["evolved_to"], json!([2]));
    assert_eq!(env.ok_with("acc-4", "task.accept_artifact", params), first);
    let s = env.state();
    assert_eq!(s["life"]["xp"], 80);
    assert_eq!(s["life"]["stage"], 2);
    assert_eq!(s["unseen_events"].as_array().unwrap().len(), 1);
    assert_eq!(
        env.bar_state().unwrap()["unseen_events"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn abort_before_presence_commit_charges_once_on_the_next_command() {
    let env = Env::with_first_run_pet();
    env.care_routine();
    env.arm();
    let life = env.life_id();
    let now = SystemClock.now_ms();
    env.write_presence("bar-crash-0001", &life, now - 10 * MIN, now, MIN);
    let r = env.call_with(
        "s-1",
        "state.get",
        json!({}),
        Some("presence_before_commit"),
    );
    assert_aborted_silently(&r);
    let attended = |s: &Value| s["lifecycle"]["attended_total_ms"].as_i64().unwrap();
    assert_eq!(
        attended(&env.state()),
        MIN,
        "applied exactly once after the abort"
    );
    assert_eq!(attended(&env.state()), MIN);
}

#[test]
fn abort_before_death_commit_keeps_the_life_and_retry_ends_it_once() {
    let env = Env::with_first_run_pet();
    env.retire();
    env.adopt("rabbit", "permadeath", "playtest");
    env.care_routine();
    let mut t = Timeline::new(&env, "bar-crash-death");
    // Playtest: critical at 33 attended minutes, grace 18 minutes.
    assert_eq!(t.settle_to(&env, 33 * MIN)["life"]["status"], "critical");
    let s = t.settle_to(&env, 51 * MIN - 1);
    assert_eq!(s["lifecycle"]["critical"]["remaining_ms"], 1);
    t.attend_for_cli(&env, 51 * MIN);
    let r = env.call_with("s-1", "state.get", json!({}), Some("death_before_commit"));
    assert_aborted_silently(&r);
    assert_eq!(
        env.bar_state().unwrap()["life"]["name"],
        "Clover",
        "the aborted end published nothing"
    );

    let s = env.state();
    assert_eq!(s["life"], Value::Null);
    assert_eq!(s["memorial"]["count"], 2);
    assert_eq!(s["memorial"]["latest"]["name"], "Clover");
    assert_eq!(s["memorial"]["latest"]["end_cause"], "neglect:thirst");
    assert_eq!(env.state()["memorial"]["count"], 2, "one end only");
    assert_eq!(env.bar_state().unwrap()["life"], Value::Null);
}
