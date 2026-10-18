//! Selecting an executor is durable intent, never permission to launch a job.
mod common;
use common::Env;
use serde_json::{json, Value};
use std::{fs, io::Write};
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
    serde_json::from_slice(&common::wait_output(child).stdout).unwrap()
}
#[test]
fn selection_survives_reopen_without_scouting_or_changing_pet() {
    let env = Env::with_first_run_pet();
    let first = request(&env, "get", json!({}));
    assert_eq!(first["result"]["provider"]["id"], "codex");
    let root = env.state_dir().join("work");
    let scout=br#"{"enabled":false,"next_due_ms":123456789,"attempts":{},"last_error":"saved previous check","last_error_acknowledged":true}"#;
    fs::write(root.join("scout-settings.json"), scout).unwrap();
    let before = env.state();
    for id in [
        "claude-code",
        "opencode",
        "muse",
        "grok",
        "hermes",
        "pi",
        "codex",
    ] {
        let result = request(&env, "client-select", json!({"client_id":id}));
        assert_eq!(result["ok"], true, "{result}");
        assert_eq!(result["result"]["provider"]["id"], id);
        assert_eq!(result["result"]["clients"].as_array().unwrap().len(), 7);
        assert!(result["result"]["task"].is_null());
        let reopened = request(&env, "get", json!({}));
        assert_eq!(reopened["result"]["provider"]["id"], id);
        assert_eq!(fs::read(root.join("scout-settings.json")).unwrap(), scout);
        assert!(!root.join("active").exists());
    }
    for params in [
        json!({}),
        json!({"client_id":"t3"}),
        json!({"client_id":"codex","automatic":true}),
    ] {
        assert_eq!(request(&env, "client-select", params)["ok"], false);
    }
    let after = env.state();
    assert_eq!(before["life"], after["life"]);
    assert_eq!(before["meters"], after["meters"]);
    assert_eq!(
        request(&env, "get", json!({}))["result"]["provider"]["id"],
        "codex"
    );
}
#[test]
fn changed_selection_refuses_stale_explicit_start_before_any_preparation() {
    let env = Env::with_first_run_pet();
    assert_eq!(
        request(&env, "client-select", json!({"client_id":"claude-code"}))["ok"],
        true
    );
    let result = request(
        &env,
        "start",
        json!({"client_id":"codex","request_id":"fixture","kind":"bug","project":"/not/a/project","prompt":"fixture","network":true}),
    );
    assert_eq!(result["ok"], false);
    assert!(
        result["error"]["message"]
            .as_str()
            .unwrap()
            .contains("selected client changed"),
        "{result}"
    );
    assert!(!env.state_dir().join("work/active").exists());
}
