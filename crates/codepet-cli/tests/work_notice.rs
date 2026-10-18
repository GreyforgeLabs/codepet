//! Durable presentation acknowledgement only: no provider or project work runs.
mod common;
use common::Env;
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;

fn request(env: &Env, method: &str, params: Value) -> Value {
    let mut child = env
        .command(&["work-rpc"], None)
        .env_clear()
        .env("HOME", env.home())
        .env("XDG_STATE_HOME", env.xdg())
        .env("PATH", "/usr/bin:/bin")
        .spawn()
        .unwrap();
    write!(
        child.stdin.take().unwrap(),
        "{}",
        json!({"method":method,"params":params})
    )
    .unwrap();
    let output = common::wait_output(child);
    serde_json::from_slice(&output.stdout).unwrap()
}
fn fixture(env: &Env, status: &str, finding: bool) -> (std::path::PathBuf, Value) {
    let root = env.state_dir().join("work");
    let id = "a".repeat(64);
    fs::create_dir_all(root.join(&id)).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let task = json!({"id":id,"kind":"bug","project":"/fixture/repo","prompt":"Fix the fixture",
        "status":status,"progress":"Saved result","output":"PRESERVED_REPORT","changes":[],"review_digest":"saved-digest",
        "error":"Codex could not finish: A file exceeds this job's size limit","reward_xp":20,"reward_need":"hunger",
        "elapsed_ms":1200,"source_kind":"scout_fix","finding":if finding { json!({"title":"suspected bug","path":"a.rs","line":1,"evidence":"evidence","check":"check","verification":"provider_reported"}) } else { Value::Null },
        "source_task_id":null,"last_message":"","life_id":"fixture","baseline":{},"cancelled":false,"launched_at":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64});
    fs::write(
        root.join(&id).join("task.json"),
        serde_json::to_vec(&task).unwrap(),
    )
    .unwrap();
    fs::write(root.join(&id).join("evidence.txt"), b"original evidence").unwrap();
    fs::write(root.join("active"), &id).unwrap();
    fs::write(
        root.join("scout-settings.json"),
        r#"{"enabled":false,"next_due_ms":0,"attempts":{},"last_error":""}"#,
    )
    .unwrap();
    (root, task)
}

#[test]
fn dismiss_finished_work_survives_process_restart_without_erasing_evidence() {
    let env = Env::with_first_run_pet();
    let (root, original) = fixture(&env, "failed", false);
    let pet_before = env.state();
    let before = request(&env, "get", json!({}));
    assert_eq!(before["result"]["task"]["error"], original["error"]);
    assert_eq!(
        before["result"]["task"]["started_at_ms"],
        original["launched_at"]
    );
    let params = json!({"task_id":original["id"]});
    let answer = request(&env, "dismiss", params.clone());
    assert_eq!(answer["ok"], true, "{answer}");
    assert!(answer["result"]["task"].is_null());
    for method in ["get", "status", "dismiss"] {
        let answer = request(
            &env,
            method,
            if method == "dismiss" {
                params.clone()
            } else {
                json!({})
            },
        );
        assert_eq!(answer["ok"], true, "{answer}");
        assert!(answer["result"]["task"].is_null(), "{answer}");
        assert_eq!(answer["result"]["scout"]["enabled"], false);
    }
    let mut saved: Value =
        serde_json::from_slice(&fs::read(root.join("a".repeat(64)).join("task.json")).unwrap())
            .unwrap();
    assert_eq!(
        saved.as_object_mut().unwrap().remove("acknowledged"),
        Some(json!(true))
    );
    assert_eq!(saved, original);
    assert_eq!(
        fs::read(root.join("a".repeat(64)).join("evidence.txt")).unwrap(),
        b"original evidence"
    );
    let export: Value =
        serde_json::from_slice(&fs::read(env.state_dir().join("work-state.json")).unwrap())
            .unwrap();
    assert!(export["task"].is_null());
    let pet_after = env.state();
    assert_eq!(pet_after["life"]["xp"], pet_before["life"]["xp"]);
    assert_eq!(pet_after["meters"], pet_before["meters"]);
    // A different failure is a fresh notice; the previous acknowledgement is not global.
    let mut fresh = original.clone();
    fresh["id"] = json!("b".repeat(64));
    fs::create_dir(root.join("b".repeat(64))).unwrap();
    fs::write(
        root.join("b".repeat(64)).join("task.json"),
        serde_json::to_vec(&fresh).unwrap(),
    )
    .unwrap();
    fs::write(root.join("active"), "b".repeat(64)).unwrap();
    assert_eq!(
        request(&env, "get", json!({}))["result"]["task"]["id"],
        fresh["id"]
    );
    assert_eq!(request(&env, "dismiss", params)["ok"], false);
}

#[test]
fn dismiss_requires_the_exact_finished_notice_and_keeps_actionable_work() {
    for (status, finding, allowed) in [
        ("preparing", false, false),
        ("running", false, false),
        ("review", false, false),
        ("applying", false, false),
        ("scouted", true, false),
        ("unknown", false, false),
        ("accepted", false, true),
        ("cancelled", false, true),
        ("discarded", false, true),
        ("scouted", false, true),
    ] {
        let env = Env::with_first_run_pet();
        let (root, task) = fixture(&env, status, finding);
        let path = root.join("a".repeat(64)).join("task.json");
        let before = fs::read(&path).unwrap();
        for params in [
            json!({}),
            json!({"task_id":null}),
            json!({"task_id":"wrong"}),
            json!({"task_id":task["id"],"extra":true}),
        ] {
            assert_eq!(request(&env, "dismiss", params)["ok"], false);
            assert_eq!(fs::read(&path).unwrap(), before);
        }
        let answer = request(&env, "dismiss", json!({"task_id":task["id"]}));
        assert_eq!(answer["ok"], allowed, "{status}: {answer}");
        if !allowed {
            assert_eq!(fs::read(&path).unwrap(), before);
        }
    }
}

#[test]
fn dismiss_preparation_notice_is_bound_to_attempt_and_preserves_error_and_cooldown() {
    let env = Env::with_first_run_pet();
    let (root, _) = fixture(&env, "failed", false);
    fs::remove_file(root.join("active")).unwrap();
    let settings = json!({"enabled":false,"next_due_ms":123456,"attempts":{},"last_error":"PRIVATE saved preparation error"});
    let path = root.join("scout-settings.json");
    fs::write(&path, serde_json::to_vec(&settings).unwrap()).unwrap();
    assert_eq!(
        request(&env, "dismiss", json!({"scout_due_ms":123455}))["ok"],
        false
    );
    let answer = request(&env, "dismiss", json!({"scout_due_ms":123456}));
    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(answer["result"]["scout"]["notice"], "");
    assert_eq!(
        request(&env, "get", json!({}))["result"]["scout"]["last_error"],
        ""
    );
    let mut saved: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        saved
            .as_object_mut()
            .unwrap()
            .remove("last_error_acknowledged"),
        Some(json!(true))
    );
    assert_eq!(saved, settings);
    let mut next = settings;
    next["next_due_ms"] = json!(123457);
    fs::write(&path, serde_json::to_vec(&next).unwrap()).unwrap();
    assert_eq!(
        request(&env, "dismiss", json!({"scout_due_ms":123456}))["ok"],
        false
    );
    assert_eq!(
        request(&env, "get", json!({}))["result"]["scout"]["last_error"],
        "PRIVATE saved preparation error"
    );
}
