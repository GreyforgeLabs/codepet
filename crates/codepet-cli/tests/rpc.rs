//! `codepet rpc` end to end through the built binary, one process per request.
// Task-pipeline tests and helpers run only with `legacy-tasks` (deleted in v0.3.1).
#![cfg_attr(not(feature = "legacy-tasks"), allow(dead_code, unused_imports))]
mod common;
use common::{error_code, Env};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};

#[cfg(feature = "legacy-tasks")]
#[test]
fn full_manual_loop_rewards_exactly_once_and_survives_restart() {
    let env = Env::new();
    env.care_routine();
    let id = env.create("hunger", 0, "artifact");
    let token = env.preflight(&id, 1);
    let started = env.call("task.start", Env::start_params(&id, 1, &token));
    // Exactly one frame, even though the same process then supervised the job.
    let s = started.response();
    assert_eq!(s["result"]["state"], "preparing");
    assert_eq!(s["result"]["task_id"], id);
    assert_eq!(started.status.code(), Some(0));

    let review = env.ok("task.review", json!({"task_id": id}));
    assert_eq!(review["state"], "review_ready");
    let r = &review["review"];
    assert_eq!(r["verification"], "not_verified");
    assert_eq!(r["simulated"], true);
    let artifact = r["artifact"].as_str().unwrap();
    assert!(artifact.starts_with("SIMULATED"), "{artifact}");
    assert!(artifact.contains(&id) && artifact.contains("hunger"));
    let digest = r["digest"].as_str().unwrap().to_owned();
    assert_eq!(env.state()["active_task"]["task_id"], id);
    assert_eq!(
        env.states(&id),
        ["draft", "preparing", "running", "verifying", "review_ready"]
    );

    // Unverified results need acknowledgement and an override reason.
    let base = json!({"task_id": id, "review_digest": digest, "acknowledged": true});
    assert_eq!(
        env.err("task.accept_artifact", base.clone()),
        "OVERRIDE_REQUIRED"
    );
    let mut unacked = base.clone();
    unacked["acknowledged"] = json!(false);
    unacked["override_reason"] = json!("ok");
    assert_eq!(
        env.err("task.accept_artifact", unacked),
        "ACKNOWLEDGEMENT_REQUIRED"
    );
    let mut wrong = base.clone();
    wrong["review_digest"] = json!("0".repeat(64));
    wrong["override_reason"] = json!("ok");
    assert_eq!(env.err("task.accept_artifact", wrong), "REVIEW_CHANGED");
    assert_eq!(env.meter("hunger"), 9000, "work debits care exactly once");

    let mut accept = base.clone();
    accept["override_reason"] = json!("Read the simulated artifact");
    let first = env.ok_with("acc-1", "task.accept_artifact", accept.clone());
    assert_eq!(first["duplicate"], false);
    let receipt = &first["receipt"];
    assert_eq!(receipt["task_id"], id);
    assert_eq!(receipt["category"], "hunger");
    assert_eq!(receipt["amount_hundredths"], 0);
    assert_eq!(receipt["demo_xp"], 20);
    assert_eq!(receipt["real_xp"], 0);
    assert_eq!(receipt["progression_track"], "bugfix");
    assert_eq!(receipt["verification"], "not_verified");
    assert_eq!(receipt["override_reason"], "Read the simulated artifact");
    assert_eq!(receipt["request_id"], "acc-1");
    assert!(receipt["accepted_at_ms"].as_i64().unwrap() > 0);
    assert_eq!(env.meter("hunger"), 9000);

    // Same request id and params: the stored response, byte-for-byte the same result.
    assert_eq!(
        env.ok_with("acc-1", "task.accept_artifact", accept.clone()),
        first
    );
    // A different request id: the recorded receipt, flagged duplicate, no new reward.
    let again = env.ok_with("acc-2", "task.accept_artifact", accept.clone());
    assert_eq!(again["duplicate"], true);
    assert_eq!(again["receipt"], first["receipt"]);
    // Same id, different params: refused.
    let mut changed = accept.clone();
    changed["override_reason"] = json!("something else");
    assert_eq!(
        env.err_with("acc-1", "task.accept_artifact", changed),
        "IDEMPOTENCY_CONFLICT"
    );

    // Every call above was a fresh process; check the durable result once more.
    let s = env.state();
    assert_eq!(s["meters"]["hunger"], 9000);
    assert_eq!(
        s["meters"],
        json!({"hunger": 9000, "thirst": 9000, "mental_health": 9500, "hygiene": 9000}),
        "acceptance never refills care"
    );
    assert_eq!(s["progression"]["demo"]["lifetime_xp"], 20);
    assert_eq!(s["progression"]["demo"]["mastery"]["bugfix"], 20);
    assert_eq!(s["progression"]["real"]["lifetime_xp"], 0);
    assert_eq!(s["active_task"], Value::Null);
    let t = &s["recent"][0];
    assert_eq!(t["task_id"], id);
    assert_eq!(t["state"], "accepted_artifact");
    assert_eq!(t["receipt"], first["receipt"]);
    assert_eq!(env.states(&id).last().unwrap(), "accepted_artifact");
    assert!(!env.folder().exists(), "the mock never touched the folder");
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn replayed_tokens_and_stale_revisions_are_refused() {
    let env = Env::new();
    env.care_routine();
    let a = env.create("hunger", 0, "artifact");
    let token_a = env.preflight(&a, 1);
    env.start(&a, 1, &token_a);
    // Replaying the consumed token can never start a second run: not for the same task,
    // which is no longer a draft, and not for another draft.
    assert_eq!(
        env.err("task.start", Env::start_params(&a, 1, &token_a)),
        "INVALID_STATE"
    );
    let b = env.create("thirst", 0, "artifact");
    assert_eq!(
        env.err("task.start", Env::start_params(&b, 1, &token_a)),
        "TOKEN_INVALID"
    );
    assert_eq!(
        env.err("task.start", Env::start_params(&b, 1, &"f".repeat(64))),
        "TOKEN_INVALID"
    );
    let runs = env.states(&a).iter().filter(|s| *s == "preparing").count();
    assert_eq!(runs, 1, "exactly one run");
    assert_eq!(env.states(&b), ["draft"]);

    // Stale revision after an update.
    env.care_routine();
    let token_b = env.preflight(&b, 1);
    let mut update = env.draft("mental_health", 0, "artifact");
    update["task_id"] = json!(b);
    update["expected_revision"] = json!(1);
    let updated = env.ok("task.update", update.clone());
    assert_eq!(updated["revision"], 2);
    assert_eq!(updated["category"], "mental_health");
    assert_eq!(env.err("task.update", update), "STALE_REVISION");
    assert_eq!(
        env.err("task.preflight", json!({"task_id": b, "revision": 1})),
        "STALE_REVISION"
    );
    assert_eq!(
        env.err("task.start", Env::start_params(&b, 1, &token_b)),
        "STALE_REVISION"
    );
    // The update invalidated the old token even at the current revision.
    assert_eq!(
        env.err("task.start", Env::start_params(&b, 2, &token_b)),
        "TOKEN_INVALID"
    );

    // A second task cannot start while the first holds the slot in review_ready.
    assert_eq!(env.task(&a)["state"], "review_ready");
    let token_b2 = env.preflight(&b, 2);
    assert_eq!(
        env.err("task.start", Env::start_params(&b, 2, &token_b2)),
        "BUSY"
    );
    assert_eq!(env.task(&b)["state"], "draft");
    // Once the first review is resolved, the unconsumed token still works.
    let digest = env.task(&a)["review"]["digest"].clone();
    let discarded = env.ok(
        "task.discard",
        json!({"task_id": a, "review_digest": digest}),
    );
    assert_eq!(discarded["state"], "discarded");
    env.start(&b, 2, &token_b2);
    assert_eq!(env.task(&b)["state"], "review_ready");
    assert_eq!(env.meter("hunger"), 9000, "discard never refills care");
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn cancellation_from_another_process_stops_the_supervisor() {
    let env = Env::new();
    env.care_routine();
    // Long enough that the cancel lands while the supervisor sleeps before `running`.
    let id = env.create("mental_health", 2000, "artifact");
    let token = env.preflight(&id, 1);
    let mut child = env.command(&["rpc"], None).spawn().unwrap();
    let frame = json!({"v": 1, "id": "start-1", "method": "task.start",
        "params": Env::start_params(&id, 1, &token)});
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{frame}").unwrap();
    drop(stdin);
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let started: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(started["ok"], true, "{started}");
    assert_eq!(started["result"]["state"], "preparing");

    // The supervisor is still running in the child process and has no children.
    assert!(
        child.try_wait().unwrap().is_none(),
        "supervisor remains live"
    );
    common::assert_no_child_processes(child.id());
    let cancelled = env.ok("task.cancel", json!({"task_id": id}));
    assert_eq!(cancelled["state"], "cancelling");
    assert_eq!(env.state()["active_task"]["state"], "cancelling");

    let mut rest = String::new();
    stdout.read_to_string(&mut rest).unwrap();
    let out = common::wait_output(child);
    assert!(out.status.success(), "{out:?}");
    assert!(rest.is_empty(), "exactly one response frame: {rest:?}");
    // The supervisor published its confirmation: no task holds the bar's slot.
    let bar = env.bar_state().unwrap();
    assert_eq!(bar["task"], Value::Null);
    assert_eq!(bar["meters"]["mental_health"], 10000);

    let t = env.task(&id);
    assert_eq!(t["state"], "cancelled");
    assert_eq!(t["review"], Value::Null);
    assert_eq!(
        env.states(&id),
        ["draft", "preparing", "cancelling", "cancelled"]
    );
    assert_eq!(env.state()["active_task"], Value::Null);
    assert_eq!(
        env.meter("mental_health"),
        10000,
        "cancellation before execution refunds its debit exactly once"
    );
    // Cancel is idempotent; a cancelled task cannot be accepted.
    assert_eq!(
        env.ok("task.cancel", json!({"task_id": id}))["state"],
        "cancelled"
    );
    assert_eq!(
        env.err("task.review", json!({"task_id": id})),
        "INVALID_STATE"
    );
    assert!(!env.folder().exists());
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn mock_outcomes_failure_and_simulated_check() {
    let env = Env::new();
    env.care_routine();
    let failed = env.create("hygiene", 0, "provider_failure");
    let token = env.preflight(&failed, 1);
    env.start(&failed, 1, &token);
    let t = env.task(&failed);
    assert_eq!(t["state"], "failed");
    assert_eq!(t["review"], Value::Null);
    assert_eq!(
        env.states(&failed),
        ["draft", "preparing", "running", "failed"]
    );
    assert_eq!(
        env.err("task.review", json!({"task_id": failed})),
        "INVALID_STATE"
    );
    assert_eq!(
        env.state()["active_task"],
        Value::Null,
        "failure frees the slot"
    );

    let (checked, digest) = env.reviewed("hygiene", "check_failed");
    let r = env.ok("task.review", json!({"task_id": checked}))["review"].clone();
    assert_eq!(r["verification"], "failed");
    assert_eq!(r["simulated"], true);
    assert!(r["artifact"]
        .as_str()
        .unwrap()
        .contains("SIMULATED check result: failed"));
    let base = json!({"task_id": checked, "review_digest": digest, "acknowledged": true});
    assert_eq!(
        env.err("task.accept_artifact", base.clone()),
        "OVERRIDE_REQUIRED"
    );
    let mut accept = base;
    accept["override_reason"] = json!("Simulated failure acknowledged");
    let receipt = env.ok("task.accept_artifact", accept)["receipt"].clone();
    assert_eq!(receipt["verification"], "failed");
    assert_eq!(receipt["demo_xp"], 0, "failed checks cannot earn demo XP");
    assert_eq!(receipt["real_xp"], 0);
    assert_eq!(receipt["amount_hundredths"], 0);
    assert_eq!(env.state()["progression"]["demo"]["lifetime_xp"], 0);
    assert_eq!(env.meter("hygiene"), 9000);
    assert!(!env.folder().exists());
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn unsupported_providers_fail_closed() {
    let env = Env::new();
    let providers = env.ok("providers.inspect", json!({}))["providers"].clone();
    let list = providers.as_array().unwrap();
    let ids: Vec<_> = list.iter().map(|p| p["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["mock", "codex", "opencode", "hermes", "claude_code"]);
    for p in list {
        assert_eq!(p["available"], p["id"] == "mock", "{p}");
    }
    for provider in ["codex", "opencode", "hermes", "claude_code"] {
        let mut draft = env.draft("hunger", 0, "artifact");
        draft["provider"] = json!(provider);
        let id = env.ok("task.create", draft)["task_id"].clone();
        assert_eq!(
            env.err("task.preflight", json!({"task_id": id, "revision": 1})),
            "UNSUPPORTED_PROVIDER"
        );
    }
    let mut unknown = env.draft("hunger", 0, "artifact");
    unknown["provider"] = json!("shell");
    assert_eq!(env.err("task.create", unknown), "INVALID_PARAMS");
    assert_eq!(env.state()["active_task"], Value::Null);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn malformed_frames_get_typed_errors_without_touching_state() {
    let env = Env::new();
    let mut oversize = vec![b'a'; 65536];
    oversize.push(b'\n');
    assert_eq!(oversize.len(), 65537);
    let cases: Vec<(Vec<u8>, &str, Value)> = vec![
        (oversize, "INVALID_REQUEST", Value::Null),
        (
            b"{\"v\":1,\"id\":\"a\",\"method\":\"state.get\0\"}\n".to_vec(),
            "INVALID_REQUEST",
            Value::Null,
        ),
        (
            b"{\"v\":1,\"id\":\n".to_vec(),
            "INVALID_REQUEST",
            Value::Null,
        ),
        (b"\xff\xfe\n".to_vec(), "INVALID_REQUEST", Value::Null),
        (b"\n".to_vec(), "INVALID_REQUEST", Value::Null),
        (b"".to_vec(), "INVALID_REQUEST", Value::Null),
        (
            br#"{"v":1,"id":"a","method":"state.get"}"#.to_vec(),
            "INVALID_REQUEST",
            Value::Null,
        ),
        (
            b"{\"v\":2,\"id\":\"a\",\"method\":\"state.get\"}\n".to_vec(),
            "PROTOCOL_MISMATCH",
            json!("a"),
        ),
        (
            b"{\"v\":1,\"id\":\"a\",\"method\":\"state.get\",\"x\":1}\n".to_vec(),
            "INVALID_REQUEST",
            json!("a"),
        ),
        (
            b"{\"v\":1,\"id\":\"a\",\"method\":\"shell.exec\"}\n".to_vec(),
            "UNKNOWN_METHOD",
            json!("a"),
        ),
        (
            b"{\"v\":1,\"id\":\"a b\",\"method\":\"state.get\"}\n".to_vec(),
            "INVALID_REQUEST",
            Value::Null,
        ),
        (
            b"{\"v\":1,\"id\":\"a\",\"method\":\"task.create\",\"params\":{}}\n".to_vec(),
            "INVALID_PARAMS",
            json!("a"),
        ),
        (
            b"{\"v\":1,\"id\":\"a\",\"method\":\"state.get\",\"params\":{\"x\":1}}\n".to_vec(),
            "INVALID_PARAMS",
            json!("a"),
        ),
    ];
    for (input, code, id) in cases {
        let r = env.raw(&input, None);
        assert_eq!(
            error_code(&r),
            code,
            "{:?}",
            String::from_utf8_lossy(&input[..input.len().min(60)])
        );
        assert_eq!(r.response()["id"], id);
        assert_eq!(r.response()["v"], 1);
    }
    for method in ["task.logs", "task.apply", "task.rollback"] {
        let r = env.call_with("a", method, json!({"task_id": "x"}), None);
        assert_eq!(error_code(&r), "INVALID_STATE");
        assert_eq!(
            r.response()["error"]["message"],
            "not implemented in mock milestone"
        );
    }
    assert!(!env.xdg().exists(), "refused requests created no state");
    // Only the first frame is answered.
    let two = b"{\"v\":1,\"id\":\"one\",\"method\":\"providers.inspect\"}\n{\"v\":1,\"id\":\"two\",\"method\":\"providers.inspect\"}\n";
    let r = env.raw(two, None);
    assert_eq!(r.response()["id"], "one");
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn other_methods_round_trip() {
    let env = Env::new();
    let hello = env.ok("hello", json!({"client": "test", "versions": [1]}));
    assert_eq!(
        hello,
        json!({"protocol_version": 1, "supervisor": null, "last_seq": 0})
    );
    assert_eq!(
        env.err("hello", json!({"versions": [2]})),
        "PROTOCOL_MISMATCH"
    );

    let settings = env.ok(
        "settings.update",
        json!({"expected_revision": 1, "decay_enabled": true,
            "decay_per_hour": [10, 20, 30, 40], "timeout_seconds": 600}),
    );
    assert_eq!(
        settings,
        json!({"revision": 2, "decay_enabled": true, "decay_per_hour": [10, 20, 30, 40],
            "timeout_seconds": 600})
    );
    assert_eq!(
        env.err(
            "settings.update",
            json!({"expected_revision": 1, "decay_enabled": false,
                "decay_per_hour": [0, 0, 0, 0], "timeout_seconds": 600}),
        ),
        "STALE_REVISION"
    );
    assert_eq!(
        env.err(
            "settings.update",
            json!({"expected_revision": 2, "decay_enabled": false,
                "decay_per_hour": [0, 0, 0, 0], "timeout_seconds": 0}),
        ),
        "INVALID_PARAMS"
    );
    let paused = env.ok("pet.pause", json!({"paused": true}));
    assert_eq!(paused["paused"], true);
    assert_eq!(paused.as_object().unwrap().len(), 2, "{paused}");
    let s = env.state();
    assert_eq!(
        paused["life_id"], s["life"]["life_id"],
        "pause names the living life"
    );
    assert!(paused["life_id"]
        .as_str()
        .is_some_and(|id| id.starts_with("life-")));
    assert_eq!(s["decay"], json!({"enabled": true, "paused": true}));
    assert_eq!(s["settings"]["revision"], 2);

    let id = env.create("thirst", 0, "artifact");
    let page = env.ok("events.subscribe", json!({"after_seq": 0}));
    assert_eq!(page["last_seq"], 1);
    assert_eq!(page["events"][0]["event"], "task.state");
    assert_eq!(page["events"][0]["task_id"], id);
    assert_eq!(page["events"][0]["data"]["state"], "draft");
    assert_eq!(
        env.err("events.subscribe", json!({"after_seq": 99})),
        "RESYNC_REQUIRED"
    );
    assert_eq!(
        env.err("events.subscribe", json!({"after_seq": 0, "limit": 101})),
        "INVALID_PARAMS"
    );
    assert_eq!(
        env.ok("history.list", json!({"limit": 1}))["tasks"][0]["task_id"],
        id
    );
    assert_eq!(env.err("task.get", json!({"task_id": "nope"})), "NOT_FOUND");
    assert_eq!(
        env.task(&id)["instructions"],
        env.draft("thirst", 0, "artifact")["instructions"]
    );
    assert_eq!(
        env.err("task.review", json!({"task_id": "nope"})),
        "NOT_FOUND"
    );
    assert_eq!(
        env.ok("task.recover", json!({})),
        json!({"changed": [], "repaired": [], "active_task": null})
    );
    let mut bad = env.draft("thirst", 0, "passed");
    assert_eq!(env.err("task.create", bad.clone()), "INVALID_PARAMS");
    bad = env.draft("thirst", 2001, "artifact");
    assert_eq!(env.err("task.create", bad), "INVALID_PARAMS");
    let mut home = env.draft("thirst", 0, "artifact");
    home["folder"] = json!(env.home());
    let home_id = env.ok("task.create", home)["task_id"].clone();
    assert_eq!(
        env.err("task.preflight", json!({"task_id": home_id, "revision": 1})),
        "INVALID_PARAMS",
        "HOME is never a project folder"
    );
}

#[cfg(feature = "legacy-tasks")]
/// The largest draft storage accepts (32 KiB JSON-encoded), with JSON-hostile text.
fn max_draft(env: &Env) -> Value {
    let mut d = env.draft("hygiene", 0, "artifact");
    d["instructions"] = json!(format!("{}{}", "\"".repeat(8000), "\u{1}".repeat(1000)));
    d["scope"] = json!((0..4)
        .map(|i| format!("{i}{}", "s".repeat(1999)))
        .collect::<Vec<_>>());
    let encoded = |d: &Value| {
        serde_json::to_vec(&json!([d["folder"], d["scope"], d["instructions"]]))
            .unwrap()
            .len()
    };
    let pad = 32 * 1024 - encoded(&d);
    let folder = format!("{}/{}", env.folder().display(), "q".repeat(pad - 1));
    d["folder"] = json!(folder);
    assert_eq!(encoded(&d), 32 * 1024);
    d
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn maximal_draft_can_be_reviewed_and_discarded() {
    let env = Env::new();
    env.care_routine();
    let created = env.call("task.create", max_draft(&env));
    assert!(created.stdout_bytes <= 65536);
    let id = created.response()["result"]["task_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let token = env.preflight(&id, 1);
    env.start(&id, 1, &token);
    // Every read that used to overflow now fits one frame.
    let state = env.call("state.get", json!({}));
    assert!(state.stdout_bytes <= 65536);
    let active = state.response()["result"]["active_task"].clone();
    assert_eq!(active["state"], "review_ready");
    let digest = active["review"]["digest"].as_str().unwrap().to_owned();
    let review = env.ok("task.review", json!({"task_id": id}));
    assert_eq!(review["review"]["digest"], digest);
    assert!(!review["review"]["artifact"]
        .as_str()
        .unwrap()
        .contains("qqqq"));
    env.ok("history.list", json!({"limit": 1}));
    env.ok("task.recover", json!({}));
    // A second task is refused while the slot is held, then runs once it is released.
    env.care_routine();
    let next = env.create("thirst", 0, "artifact");
    let next_token = env.preflight(&next, 1);
    assert_eq!(
        env.err("task.start", Env::start_params(&next, 1, &next_token)),
        "BUSY"
    );
    let discarded = env.ok(
        "task.discard",
        json!({"task_id": id, "review_digest": digest}),
    );
    assert_eq!(discarded["state"], "discarded");
    let next_token = env.preflight(&next, 1);
    env.start(&next, 1, &next_token);
    assert_eq!(env.task(&next)["state"], "review_ready");
    assert_eq!(env.meter("hygiene"), 9000);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn oversized_drafts_are_refused_before_anything_is_stored() {
    let env = Env::new();
    // A 65,530-byte create frame passes framing and core limits but not the draft budget,
    // so it is refused without creating a draft (it used to commit and answer OUTPUT_LIMIT).
    let mut d = env.draft("hunger", 0, "artifact");
    d["instructions"] = json!("y".repeat(16384));
    d["scope"] = json!((0..12).map(|_| "z".repeat(4000)).collect::<Vec<_>>());
    let frame = |d: &Value| {
        json!({"v": 1, "id": "big", "method": "task.create", "params": d})
            .to_string()
            .len()
            + 1
    };
    d["folder"] = json!("/srv/p");
    d["scope"].as_array_mut().unwrap().push(json!(""));
    let pad = 65530 - frame(&d);
    assert!((1..=4096).contains(&pad), "{pad}");
    d["scope"][12] = json!("z".repeat(pad));
    assert_eq!(frame(&d), 65530);
    let r = env.call_with("big", "task.create", d.clone(), None);
    assert_eq!(error_code(&r), "INVALID_PARAMS");
    let message = r.response()["error"]["message"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(message.contains("32 KiB"), "{message}");
    assert_eq!(env.state()["recent"], json!([]));
    assert_eq!(
        env.ok("events.subscribe", json!({"after_seq": 0}))["last_seq"],
        0
    );
    // Updates are bounded the same way and leave the revision alone.
    let id = env.create("hunger", 0, "artifact");
    let mut update = d;
    update["task_id"] = json!(id);
    update["expected_revision"] = json!(1);
    update["folder"] = json!("/srv/p");
    update["scope"] = json!((0..5).map(|_| "z".repeat(4000)).collect::<Vec<_>>());
    let r = env.call("task.update", update);
    assert_eq!(error_code(&r), "INVALID_PARAMS");
    assert!(r.response()["error"]["message"]
        .as_str()
        .unwrap()
        .contains("32 KiB"));
    assert_eq!(env.task(&id)["revision"], 1);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn many_large_drafts_keep_state_get_readable() {
    let env = Env::new();
    // Ten drafts with 16 KiB instructions used to push state.get past 64 KiB.
    for _ in 0..10 {
        let mut draft = env.draft("hunger", 0, "artifact");
        draft["instructions"] = json!("x".repeat(16 * 1024));
        env.ok("task.create", draft);
    }
    let r = env.call("state.get", json!({}));
    assert!(r.stdout_bytes <= 65536);
    let recent = r.response()["result"]["recent"].as_array().unwrap().clone();
    assert_eq!(recent.len(), 10);
    assert_eq!(recent[0]["instructions_bytes"], 16 * 1024);
    // The full draft, for editing, comes from task.get.
    let id = recent[0]["task_id"].as_str().unwrap();
    let full = env.ok("task.get", json!({"task_id": id}));
    assert_eq!(full["instructions"].as_str().unwrap().len(), 16 * 1024);
    assert_eq!(full["scope"], json!(["src"]));
    let page = env.ok("history.list", json!({}));
    assert_eq!(page["tasks"].as_array().unwrap().len(), 10);
    assert_eq!(page["next"], Value::Null);
    let first = env.ok("history.list", json!({"limit": 4}));
    let next = &first["next"];
    let rest = env.ok(
        "history.list",
        json!({"before_ms": next["before_ms"], "before_task_id": next["before_task_id"]}),
    );
    assert_eq!(rest["tasks"].as_array().unwrap().len(), 6);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn over_long_override_reason_is_invalid_params_not_override_required() {
    let env = Env::new();
    let (id, digest) = env.reviewed("hunger", "artifact");
    // 200 CJK characters: 200 characters but 600 UTF-8 bytes.
    let r = env.accept("acc-cjk", &id, &digest, &"\u{4e2d}".repeat(200));
    assert_eq!(error_code(&r), "INVALID_PARAMS");
    let r = env.accept("acc-ok", &id, &digest, &"\u{4e2d}".repeat(170));
    assert_eq!(r.response()["ok"], true, "{:?}", r.response());
    assert_eq!(env.meter("hunger"), 9000);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn home_is_refused_even_when_home_is_unset() {
    let env = Env::new();
    env.care_routine();
    let uid = std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .and_then(|r| r.split_whitespace().nth(1).map(str::to_owned))
        .unwrap();
    let passwd = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
    let Some(home) = passwd.lines().find_map(|l| {
        let f: Vec<_> = l.split(':').collect();
        (f.len() == 7 && f[2] == uid && f[5].starts_with('/')).then(|| f[5].to_owned())
    }) else {
        eprintln!("no /etc/passwd entry for uid {uid}; skipped");
        return;
    };
    let mut draft = env.draft("thirst", 0, "artifact");
    draft["folder"] = json!(home);
    let r = env.call_without_home("task.create", draft);
    let id = r.response()["result"]["task_id"].clone();
    let r = env.call_without_home("task.preflight", json!({"task_id": id, "revision": 1}));
    assert_eq!(error_code(&r), "INVALID_PARAMS", "account home refused");
    // Another folder still passes, so the refusal is about the home, not the missing HOME.
    let r = env.call_without_home("task.create", env.draft("thirst", 0, "artifact"));
    let id = r.response()["result"]["task_id"].clone();
    let r = env.call_without_home("task.preflight", json!({"task_id": id, "revision": 1}));
    assert_eq!(r.response()["ok"], true, "{:?}", r.response());
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn sensitive_task_text_is_redacted_before_mock_artifact_and_receipt_persistence() {
    let env = Env::new();
    env.care_routine();
    let mut draft = env.draft("mental_health", 0, "artifact");
    draft["instructions"] =
        json!("Explain this safely.\nAPI_KEY=do-not-store-me\n\u{1b}[31mVisible text\u{1b}[0m");
    let created = env.ok("task.create", draft);
    let id = created["task_id"].as_str().unwrap();
    let instructions = created["instructions"].as_str().unwrap();
    assert!(!instructions.contains("do-not-store-me"));
    assert!(!instructions.contains('\u{1b}'));
    assert!(instructions.contains("Visible text"));
    let token = env.preflight(id, 1);
    env.start(id, 1, &token);
    let review = env.ok("task.review", json!({"task_id": id}));
    let artifact = review["review"]["artifact"].as_str().unwrap();
    assert!(!artifact.contains("do-not-store-me"));
    assert!(!artifact.contains('\u{1b}'));
    assert!(artifact.contains("SIMULATED"));
    let params = json!({"task_id": id, "review_digest": review["review"]["digest"],
        "acknowledged": true, "override_reason": "Reviewed.\nAuthorization: Bearer hide-this-token"});
    let first = env.ok_with("private-receipt", "task.accept_artifact", params.clone());
    assert!(!first.to_string().contains("hide-this-token"));
    assert_eq!(
        env.ok_with("private-receipt", "task.accept_artifact", params),
        first
    );
    let state = env.state().to_string();
    assert!(!state.contains("do-not-store-me"));
    assert!(!state.contains("hide-this-token"));
    assert_eq!(env.task(id)["receipt"], first["receipt"]);
}

#[test]
fn lifecycle_requests_are_validated_before_storage_opens() {
    let env = Env::new();
    let life = "life-0123456789abcdef01234567";
    let cases: Vec<(&str, Value)> = vec![
        ("pet.select", json!({})),
        ("pet.select", json!({"life_id": life})),
        ("pet.select", json!({"life_id": life, "pet_id": "dragon"})),
        (
            "pet.select",
            json!({"life_id": life, "pet_id": "cat", "x": 1}),
        ),
        ("pet.select", json!({"life_id": "", "pet_id": "cat"})),
        ("pet.adopt", json!({"species": "cat", "mode": "permadeath"})),
        (
            "pet.adopt",
            json!({"species": "unicorn", "mode": "permadeath", "acknowledge_rules": true}),
        ),
        (
            "pet.adopt",
            json!({"species": "cat", "mode": "hardcore", "acknowledge_rules": true}),
        ),
        (
            "pet.adopt",
            json!({"species": "cat", "mode": "gentle", "pace": "fast", "acknowledge_rules": true}),
        ),
        (
            "pet.adopt",
            json!({"species": "cat", "mode": "gentle", "name": "   ", "acknowledge_rules": true}),
        ),
        (
            "pet.adopt",
            json!({"species": "cat", "mode": "gentle", "name": "x".repeat(257),
                "acknowledge_rules": true}),
        ),
        (
            "pet.adopt",
            json!({"species": "cat", "mode": "gentle", "acknowledge_rules": "yes"}),
        ),
        ("pet.rules", json!({"life_id": life, "mode": "permadeath"})),
        (
            "pet.rules",
            json!({"life_id": life, "mode": "standard", "acknowledge_rules": true}),
        ),
        (
            "pet.configure_life",
            json!({"life_id": life, "expected_revision": 1}),
        ),
        (
            "pet.configure_life",
            json!({"life_id": life, "expected_revision": -1, "grace_hours": 8}),
        ),
        (
            "pet.configure_life",
            json!({"life_id": life, "expected_revision": 1, "grace_hours": 70000}),
        ),
        (
            "pet.configure_life",
            json!({"life_id": life, "expected_revision": 1, "decay_percent": 10}),
        ),
        ("pet.retire", json!({"life_id": life})),
        ("pet.retire", json!({"life_id": life, "confirm_name": ""})),
        (
            "pet.events_seen",
            json!({"life_id": life, "through_seq": -1}),
        ),
        ("pet.events_seen", json!({"life_id": life})),
        ("pet.memorial", json!({"limit": 0})),
        ("pet.memorial", json!({"limit": 21})),
        ("pet.memorial", json!({"before_ordinal": -1})),
        ("lifecycle.rules", json!({"x": 1})),
    ];
    for (method, params) in cases {
        assert_eq!(
            env.err(method, params.clone()),
            "INVALID_PARAMS",
            "{method} {params}"
        );
    }
    assert!(!env.xdg().exists(), "refused requests created no state");
    // The static ruleset needs no state either.
    env.ok("lifecycle.rules", json!({}));
    assert!(!env.xdg().exists());
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn every_successful_mutation_publishes_and_nothing_else_does() {
    let env = Env::new();
    let version = |env: &Env| env.bar_state().unwrap()["state_version"].as_i64().unwrap();
    // First contact: the store is created and state.get publishes version 1.
    let s = env.state();
    assert_eq!(version(&env), s["state_version"].as_i64().unwrap());
    let life = s["life"]["life_id"].clone();

    let mutations: Vec<(&str, Value)> = vec![
        ("pet.care", json!({"action": "routine"})),
        ("pet.pause", json!({"paused": true})),
        ("pet.pause", json!({"paused": false})),
        (
            "pet.rules",
            json!({"life_id": life, "mode": "permadeath", "acknowledge_rules": true}),
        ),
        ("pet.select", json!({"life_id": life, "pet_id": "owl"})),
        (
            "pet.configure_life",
            json!({"life_id": life, "expected_revision": 3, "grace_hours": 7}),
        ),
        (
            "pet.events_seen",
            json!({"life_id": life, "through_seq": 0}),
        ),
        (
            "settings.update",
            json!({"expected_revision": 1, "decay_enabled": false,
                "decay_per_hour": [0, 0, 0, 0], "timeout_seconds": 600}),
        ),
        ("task.create", env.draft("hunger", 0, "artifact")),
        ("task.recover", json!({})),
    ];
    for (method, params) in mutations {
        let before = version(&env);
        let result = env.ok(method, params);
        let after = version(&env);
        assert!(after > before, "{method} published ({before} -> {after})");
        assert_eq!(
            after,
            env.state()["state_version"].as_i64().unwrap(),
            "{method}: the file matches committed state"
        );
        if method == "pet.select" {
            assert_eq!(result["life"]["appearance"], "owl");
            assert_eq!(env.bar_state().unwrap()["life"]["appearance"], "owl");
        }
    }

    // Failures and read-only methods never write the file.
    let bytes = std::fs::read(env.bar_state_path()).unwrap();
    let failures: Vec<(&str, Value)> = vec![
        (
            "pet.select",
            json!({"life_id": "life-000000000000000000000000", "pet_id": "dog"}),
        ),
        (
            "pet.adopt",
            json!({"species": "dog", "mode": "permadeath", "acknowledge_rules": true}),
        ),
        ("task.cancel", json!({"task_id": "nope"})),
        (
            "settings.update",
            json!({"expected_revision": 1, "decay_enabled": false,
                "decay_per_hour": [0, 0, 0, 0], "timeout_seconds": 600}),
        ),
    ];
    for (method, params) in failures {
        env.err(method, params);
        assert_eq!(
            std::fs::read(env.bar_state_path()).unwrap(),
            bytes,
            "{method}"
        );
    }
    std::fs::remove_file(env.bar_state_path()).unwrap();
    for (method, params) in [
        ("hello", json!({})),
        ("providers.inspect", json!({})),
        ("events.subscribe", json!({"after_seq": 0})),
        ("history.list", json!({})),
        ("pet.memorial", json!({})),
        ("lifecycle.rules", json!({})),
    ] {
        env.ok(method, params);
        assert!(env.bar_state().is_none(), "{method} is read-only");
    }
    // The next state.get republishes the deleted file.
    let s = env.state();
    assert_eq!(version(&env), s["state_version"].as_i64().unwrap());
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn the_supervisor_publishes_every_step() {
    let env = Env::new();
    env.care_routine();
    let id = env.create("hygiene", 600, "artifact");
    let token = env.preflight(&id, 1);
    let mut child = env.command(&["rpc"], None).spawn().unwrap();
    let frame = json!({"v": 1, "id": "start-1", "method": "task.start",
        "params": Env::start_params(&id, 1, &token)});
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{frame}").unwrap();
    drop(stdin);
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    // Published before the start response was written.
    assert_eq!(env.bar_state().unwrap()["task"]["state"], "preparing");
    let mut seen = vec!["preparing".to_owned()];
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while child.try_wait().unwrap().is_none() {
        assert!(std::time::Instant::now() < deadline, "supervisor finished");
        if let Some(state) = env.bar_state().unwrap()["task"]["state"].as_str() {
            if seen.last().map(String::as_str) != Some(state) {
                seen.push(state.to_owned());
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let out = common::wait_output(child);
    assert!(out.status.success(), "{out:?}");
    let bar = env.bar_state().unwrap();
    assert_eq!(
        bar["task"],
        json!({"state": "review_ready", "category": "hygiene"})
    );
    if seen.last().map(String::as_str) != Some("review_ready") {
        seen.push("review_ready".to_owned());
    }
    assert_eq!(seen, ["preparing", "running", "verifying", "review_ready"]);
    // Bar-safe: no instructions, folder or artifact text.
    let text = std::fs::read_to_string(env.bar_state_path()).unwrap();
    assert!(!text.contains("SIMULATED") && !text.contains("project-never-created"));
    assert!(!text.contains("Explain the mock"));
}

#[test]
fn a_failed_publish_never_changes_the_response() {
    let env = Env::new();
    env.state();
    std::fs::remove_file(env.bar_state_path()).unwrap();
    std::fs::create_dir(env.bar_state_path()).unwrap();
    let r = env.call_with("care-1", "pet.care", json!({"action": "routine"}), None);
    assert_eq!(r.status.code(), Some(0), "{r:?}");
    assert_eq!(r.response()["ok"], true);
    assert_eq!(r.response()["result"]["changed"], true);
    assert_eq!(r.stderr.lines().count(), 1, "{r:?}");
    assert!(r
        .stderr
        .starts_with("codepet: bar-state.json not updated: "));
    // The change committed even though the file could not be published.
    assert_eq!(env.meter("thirst"), 10_000);
    std::fs::remove_dir(env.bar_state_path()).unwrap();
    let s = env.state();
    assert_eq!(
        env.bar_state().unwrap()["state_version"],
        s["state_version"],
        "the next command catches up"
    );
}
