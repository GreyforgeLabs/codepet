mod common;
use common::Env;
use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;

fn assert_missing_prerequisite(message: &str) {
    // These tests exercise cooldown persistence before provider preparation. An
    // empty PATH guarantees no Codex, but minimal CI hosts can lack containment
    // tools too; that earlier, safe refusal must have the same cooldown behavior.
    if !Path::new("/usr/bin/bwrap").is_file() || !Path::new("/usr/bin/prlimit").is_file() {
        assert_eq!(
            message,
            "Bubblewrap and prlimit are needed to keep jobs contained"
        );
    } else {
        assert!(message.contains("Install Codex"), "{message}");
    }
}

fn call(env: &Env, method: &str, params: Value) -> Value {
    let mut child = env
        .command(&["work-rpc"], None)
        .env("PATH", "")
        .env("CODEX_HOME", env.home().join("no-provider"))
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            json!({"method":method,"params":params})
                .to_string()
                .as_bytes(),
        )
        .unwrap();
    let output = common::wait_output(child);
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn control_requests_refuse_contention_and_do_not_queue_across_updates() {
    let env = Env::with_first_run_pet();
    assert_eq!(call(&env, "get", json!({}))["ok"], true);
    let lock = std::fs::File::open(env.state_dir().join("work/control.lock")).unwrap();
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::LockExclusive).unwrap();
    for method in ["get", "scout", "start"] {
        let start = std::time::Instant::now();
        let blocked = call(&env, method, json!({}));
        assert_eq!(blocked["ok"], false);
        assert!(blocked["error"]["message"]
            .as_str()
            .unwrap()
            .contains("busy or updating"));
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
    }
    assert!(!env.state_dir().join("work/scout-settings.json").exists());
    assert!(!env.state_dir().join("work/active").exists());
    drop(lock);
    let retried = call(&env, "get", json!({}));
    assert_eq!(retried["ok"], true);
    assert!(retried.get("error").is_none());
    assert!(retried["result"]["task"].is_null());
    assert!(!env.state_dir().join("work/scout-settings.json").exists());
}

#[test]
fn failed_scout_setup_consumes_cooldown_before_provider_or_source_preparation() {
    let env = Env::with_first_run_pet();
    std::fs::create_dir_all(env.home().join("Projects/example/.git")).unwrap();
    assert!(call(&env, "scout", json!({}))["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Let's go"));
    assert!(!env.state_dir().join("work/scout-settings.json").exists());
    env.arm();
    let initial = env.state();
    let first = call(&env, "scout", json!({}));
    assert_eq!(first["ok"], false);
    assert_missing_prerequisite(first["error"]["message"].as_str().unwrap());
    let path = env.state_dir().join("work/scout-settings.json");
    let bytes = std::fs::read(&path).unwrap();
    let settings: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(settings["attempts"].as_object().unwrap().len(), 1);
    assert!(settings["next_due_ms"].as_u64().unwrap() > 0);
    let second = call(&env, "scout", json!({}));
    assert!(second["error"]["message"]
        .as_str()
        .unwrap()
        .contains("waiting"));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let state = env.state();
    assert_eq!(state["life"]["xp"], initial["life"]["xp"]);
    assert_eq!(
        state["life"]["adopted_at_ms"],
        initial["life"]["adopted_at_ms"]
    );
    assert!(!env.state_dir().join("work/active").exists());
    // Expiring only the global cooldown must not bypass the per-repository limit.
    let mut expired = settings;
    expired["next_due_ms"] = json!(0);
    std::fs::write(&path, expired.to_string()).unwrap();
    let third = call(&env, "scout", json!({}));
    assert!(third["error"]["message"]
        .as_str()
        .unwrap()
        .contains("No eligible"));
    let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(saved["next_due_ms"].as_u64().unwrap() > 0);
}

#[test]
fn settings_pause_scouting_and_inputs_cannot_choose_a_hidden_project() {
    let env = Env::with_first_run_pet();
    let paused = call(&env, "scout-settings", json!({"enabled":false}));
    assert_eq!(paused["ok"], true);
    assert_eq!(paused["result"]["scout"]["enabled"], false);
    assert_eq!(call(&env, "scout", json!({}))["ok"], false);
    assert_eq!(
        call(&env, "scout", json!({"project":"/secret"}))["ok"],
        false
    );
    assert_eq!(
        call(
            &env,
            "scout-settings",
            json!({"enabled":true,"prompt":"anything"})
        )["ok"],
        false
    );
    let settings = call(&env, "get", json!({}));
    assert_eq!(settings["result"]["scout"]["enabled"], false);
    assert!(!env.state_dir().join("work/active").exists());
}

#[test]
fn scout_skips_oversized_and_linked_repos_before_one_provider_preparation() {
    let env = Env::with_first_run_pet();
    env.arm();
    let good = env.home().join("Projects/z-valid");
    let large = env.home().join("Projects/a-large");
    let linked = env.home().join("Projects/b-linked");
    for path in [&good, &large, &linked] {
        std::fs::create_dir_all(path.join(".git")).unwrap();
        std::fs::write(path.join("main.py"), "print(1)\n").unwrap();
        // Clamp future metadata to the same relevance, then use deterministic path order.
        std::fs::File::open(path)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new().set_modified(
                    std::time::SystemTime::now() + std::time::Duration::from_secs(60),
                ),
            )
            .unwrap();
    }
    let ledger = std::fs::File::create(large.join("assets.jsonl")).unwrap();
    ledger.set_len(3_449_401).unwrap();
    std::os::unix::fs::symlink("/etc/passwd", linked.join("source-link")).unwrap();
    // File creation updates mtime; restore tie after making the fixtures.
    for path in [&good, &large, &linked] {
        std::fs::File::open(path)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new().set_modified(
                    std::time::SystemTime::now() + std::time::Duration::from_secs(60),
                ),
            )
            .unwrap();
    }
    let first = call(&env, "scout", json!({}));
    assert_missing_prerequisite(first["error"]["message"].as_str().unwrap());
    let settings: Value = serde_json::from_slice(
        &std::fs::read(env.state_dir().join("work/scout-settings.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(settings["attempts"].as_object().unwrap().len(), 3);
    assert!(!env.state_dir().join("work/active").exists());
    assert_eq!(
        std::fs::metadata(large.join("assets.jsonl")).unwrap().len(),
        3_449_401
    );
    let reopened = call(&env, "get", json!({}));
    assert_missing_prerequisite(reopened["result"]["scout"]["last_error"].as_str().unwrap());
}

#[test]
fn failed_preflight_is_bounded_persistent_and_private() {
    let env = Env::with_first_run_pet();
    env.arm();
    for i in 0..12 {
        let project = env.home().join(format!("Projects/private-repo-{i:02}"));
        std::fs::create_dir_all(project.join(".git")).unwrap();
        std::fs::File::create(project.join("PRIVATE_SENTINEL.jsonl"))
            .unwrap()
            .set_len(3_449_401)
            .unwrap();
    }
    let first = call(&env, "scout", json!({}));
    assert!(first["error"]["message"]
        .as_str()
        .unwrap()
        .contains("No eligible"));
    let bytes = std::fs::read(env.state_dir().join("work/scout-settings.json")).unwrap();
    let settings: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(settings["attempts"].as_object().unwrap().len(), 10);
    let reopened = call(&env, "get", json!({}));
    assert!(reopened["result"]["scout"]["last_error"]
        .as_str()
        .unwrap()
        .contains("PRIVATE_SENTINEL.jsonl"));
    let export = std::fs::read_to_string(env.state_dir().join("work-state.json")).unwrap();
    assert!(!export.contains("PRIVATE_SENTINEL"));
    assert!(!export.contains("private-repo"));
    assert!(!export.contains("last_error"));
    assert!(export.contains("The last bug check could not start."));
    assert!(call(&env, "scout", json!({}))["error"]["message"]
        .as_str()
        .unwrap()
        .contains("waiting"));
    assert_eq!(
        std::fs::read(env.state_dir().join("work/scout-settings.json")).unwrap(),
        bytes
    );
    assert!(!env.state_dir().join("work/active").exists());
    assert_eq!(env.state()["life"]["xp"], 0);
}
