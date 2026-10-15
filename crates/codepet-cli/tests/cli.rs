mod common;
use common::Env;

#[test]
fn no_arguments_do_not_start_a_task() {
    let env = Env::new();
    let output = env.run(&[]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("No daemon"));
    assert!(!text.contains("seq="));
    for args in [&["--help"][..], &["help"], &["providers"], &["mock-demo"]] {
        assert!(env.run(args).status.success(), "{args:?}");
    }
    assert!(
        !env.xdg().exists(),
        "help, providers and mock-demo create no state"
    );
}
#[test]
fn demo_stops_at_unverified_review_without_reward() {
    let env = Env::new();
    let output = env.run(&["mock-demo"]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("state=ReviewReady"));
    assert!(text.contains("Mental health=70/100"));
    assert!(!text.contains("state=Accepted"));
}
#[test]
fn real_provider_command_is_rejected() {
    let env = Env::new();
    let output = env.run(&["codex"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}
#[test]
fn providers_lists_mock_only() {
    let env = Env::new();
    let text = String::from_utf8(env.run(&["providers"]).stdout).unwrap();
    assert!(text.contains("Mock: available (simulation only)"));
    for p in ["Codex", "OpenCode", "Hermes", "Claude Code"] {
        assert!(text.contains(&format!("{p}: unsupported")), "{text}");
    }
}
#[test]
fn usage_errors_exit_2_without_output_or_state() {
    let env = Env::new();
    for args in [
        &["rpc", "extra"][..],
        &["state", "--json"],
        &["providers", "x"],
    ] {
        let output = env.run(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
    }
    assert!(!env.xdg().exists());
}
#[test]
fn state_command_prints_the_snapshot() {
    let env = Env::new();
    let output = env.run(&["state"]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert_eq!(text.lines().count(), 1);
    let s: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(s["schema_version"], 2);
    assert_eq!(s["protocol_version"], 1);
    assert_eq!(s["meters"]["hunger"], 7000);
    assert_eq!(s["active_task"], serde_json::Value::Null);
    assert_eq!(s["care"]["ready_to_work"], false);
    assert_eq!(s["care"]["sleeping"], false);
    assert_eq!(s["care"]["work_sessions"], 0);
    assert_eq!(s["progression"]["real"]["lifetime_xp"], 0);
    assert_eq!(s["progression"]["demo"]["lifetime_xp"], 0);
    assert_eq!(s["providers"].as_array().unwrap().len(), 5);
    assert_eq!(s, env.state(), "state matches rpc state.get");
}
#[test]
fn state_command_fails_closed_on_relative_xdg() {
    let env = Env::new();
    let output = env
        .command(&["state"], None)
        .env("XDG_STATE_HOME", "relative/state")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let err = String::from_utf8(output.stderr).unwrap();
    assert!(err.starts_with("STORAGE_UNAVAILABLE"), "{err}");
}
