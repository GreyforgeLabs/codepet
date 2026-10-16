// Task-pipeline tests and helpers run only with `legacy-tasks` (deleted in v0.3.1).
#![cfg_attr(not(feature = "legacy-tasks"), allow(dead_code, unused_imports))]

mod common;
use common::Env;

#[cfg(feature = "legacy-tasks")]
#[test]
fn no_arguments_do_not_start_a_task() {
    let env = Env::new();
    let output = env.run(&[]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("No daemon"));
    assert!(!text.contains("seq="));
    for args in [
        &["--help"][..],
        &["help"],
        &["--version"],
        &["providers"],
        &["mock-demo"],
    ] {
        assert!(env.run(args).status.success(), "{args:?}");
    }
    assert!(
        !env.xdg().exists(),
        "help, providers and mock-demo create no state"
    );
}
#[cfg(feature = "legacy-tasks")]
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
#[cfg(feature = "legacy-tasks")]
#[test]
fn providers_lists_mock_only() {
    let env = Env::new();
    let text = String::from_utf8(env.run(&["providers"]).stdout).unwrap();
    assert!(text.contains("Mock: available (simulation only)"));
    for p in ["Codex", "OpenCode", "Hermes", "Claude Code"] {
        assert!(text.contains(&format!("{p}: unsupported")), "{text}");
    }
}
#[cfg(feature = "legacy-tasks")]
#[test]
fn usage_errors_exit_2_without_output_or_state() {
    let env = Env::new();
    for args in [
        &["rpc", "extra"][..],
        &["state", "--json"],
        &["providers", "x"],
        &["--version", "--json"],
    ] {
        let output = env.run(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
    }
    assert!(!env.xdg().exists());
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
#[test]
fn version_prints_without_opening_storage() {
    let env = Env::new();
    let output = env.run(&["--version"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("codepet {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(output.stderr.is_empty());
    assert!(!env.xdg().exists());
    let help = String::from_utf8(env.run(&["--help"]).stdout).unwrap();
    assert!(!help.contains("milestone"), "{help}");
    assert!(help.contains("--version"), "{help}");
}
#[test]
fn state_command_publishes_the_bar_state() {
    use std::os::unix::fs::PermissionsExt;
    let env = Env::with_first_run_pet();
    let output = env.run(&["state"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let s: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let bar = env
        .bar_state()
        .expect("codepet state publishes bar-state.json");
    assert_eq!(bar["schema"], "codepet.bar.v1");
    assert_eq!(bar["app_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(bar["ruleset"], "lifecycle-v1");
    assert_eq!(bar["state_version"], s["state_version"]);
    assert_eq!(bar["life"]["life_id"], s["life"]["life_id"]);
    assert_eq!(bar["life"]["name"], "Ginger");
    assert_eq!(bar["meters"], s["meters"]);
    let meta = std::fs::metadata(env.bar_state_path()).unwrap();
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    assert!(meta.len() <= 16 * 1024);
}
#[test]
fn state_command_reports_a_failed_publish_on_one_line_only() {
    let env = Env::with_first_run_pet();
    assert!(env.run(&["state"]).status.success());
    std::fs::remove_file(env.bar_state_path()).unwrap();
    // A directory in the file's place cannot be replaced by the rename.
    std::fs::create_dir(env.bar_state_path()).unwrap();
    let output = env.run(&["state"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let s: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(s["life"]["name"], "Ginger");
    let err = String::from_utf8(output.stderr).unwrap();
    assert_eq!(err.lines().count(), 1, "{err}");
    assert!(
        err.starts_with("codepet: bar-state.json not updated: "),
        "{err}"
    );
    assert!(env.bar_state_path().is_dir());
    let leftovers: Vec<_> = std::fs::read_dir(env.state_dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.contains(".tmp-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

// ---- v0.3 skeleton (spec §6.3) ----

#[test]
fn v03_wp3_commands_validate_their_arguments_before_touching_state() {
    let env = Env::new();
    for (args, code) in [
        (&["clients-job", "job-1"][..], 2),
        (&["clients-job"][..], 2),
        (&["connect"][..], 2),
        (&["connect", "crush"][..], 2),
        (&["disconnect"][..], 2),
        (&["disconnect", "codex", "x"][..], 2),
        (&["doctor", "--bogus"][..], 2),
    ] {
        let output = env.run(args);
        assert_eq!(output.status.code(), Some(code), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
    }
    assert!(!env.xdg().exists(), "refused commands create no state");
    // Doctor on a fresh home reports and creates nothing.
    let output = env.run(&["doctor", "--json"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(!env.xdg().exists(), "doctor creates no state");
    // v0.3 WP2: the fold is implemented; a missing state dir fails with one plain line
    // and creates nothing (it never creates directories).
    let output = env.run(&["fold", "--state-dir", "/nonexistent"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let err = String::from_utf8(output.stderr).unwrap();
    assert!(
        err.starts_with("codepet fold: ") && err.lines().count() == 1,
        "{err}"
    );
    assert!(!std::path::Path::new("/nonexistent").exists());
    assert!(!env.xdg().exists());
}

#[test]
fn v03_hook_is_always_silent_and_exits_zero() {
    let env = Env::new();
    for args in [
        &["hook"][..],
        &["hook", "--state-dir", "/nonexistent", "claude-code"],
        &["hook", "--bogus", "x", "y", "z"],
    ] {
        let output = env.run(args);
        assert_eq!(output.status.code(), Some(0), "{args:?}");
        assert!(
            output.stdout.is_empty() && output.stderr.is_empty(),
            "{args:?}"
        );
    }
    assert!(!env.xdg().exists(), "the hook never creates a directory");
}

// ---- v0.3 WP1 commands (spec §6.3) ----

#[test]
fn help_lists_the_v03_commands_and_no_retired_ones() {
    let env = Env::new();
    let output = env.run(&[]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    for c in [
        "migrate",
        "on | off",
        "helpers on|off",
        "forget-history",
        "state",
        "rpc",
    ] {
        assert!(text.contains(c), "{c}: {text}");
    }
    assert!(text.contains("No daemon"));
    let retired = text.contains("mock-demo") || text.contains("providers");
    assert_eq!(retired, cfg!(feature = "legacy-tasks"), "{text}");
    assert!(!env.xdg().exists(), "help creates no state");
}

#[test]
fn retired_commands_are_unknown_in_the_default_build() {
    let env = Env::new();
    for c in ["providers", "mock-demo"] {
        let output = env.run(&[c]);
        let expected = if cfg!(feature = "legacy-tasks") { 0 } else { 2 };
        assert_eq!(output.status.code(), Some(expected), "{c}");
    }
    if !cfg!(feature = "legacy-tasks") {
        assert!(!env.xdg().exists());
    }
}

#[test]
fn v03_usage_errors_exit_2_without_output_or_state() {
    let env = Env::new();
    for args in [
        &["rpc", "extra"][..],
        &["state", "--json"],
        &["--version", "--json"],
        &["migrate", "now"],
        &["on", "please"],
        &["off", "--force"],
        &["helpers"],
        &["helpers", "maybe"],
        &["helpers", "on", "off"],
        &["forget-history", "--all"],
    ] {
        let output = env.run(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
    }
    assert!(!env.xdg().exists());
}

#[test]
fn v03_state_command_prints_the_snapshot() {
    let env = Env::new();
    let output = env.run(&["state"]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert_eq!(text.lines().count(), 1);
    let s: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(s["schema_version"], 5);
    assert_eq!(s["protocol_version"], 1);
    assert_eq!(s["meters"]["hunger"], 7000);
    assert_eq!(s["enabled"], true);
    assert_eq!(s["count_helpers"], false);
    assert_eq!(s["treats"], serde_json::json!({"jar": 0, "full": false}));
    assert_eq!(s["agent"]["state"], "idle");
    assert_eq!(s["clients"], serde_json::json!({}));
    // A fresh v0.3 install has no companion until Let's go! (spec §2.1).
    assert_eq!(s["life"], serde_json::Value::Null);
    assert_eq!(s["lifecycle"]["frozen_reason"], "no_life");
    for dropped in ["active_task", "recent", "providers", "settings", "decay"] {
        assert_eq!(
            s.get(dropped).is_some(),
            cfg!(feature = "legacy-tasks"),
            "{dropped}"
        );
    }
    for dropped in ["ready_to_work", "readiness_reasons"] {
        assert_eq!(
            s["care"].get(dropped).is_some(),
            cfg!(feature = "legacy-tasks"),
            "{dropped}"
        );
    }
    assert_eq!(s, env.state(), "state matches rpc state.get");
}

#[test]
fn migrate_creates_then_reports_up_to_date_and_publishes() {
    let env = Env::new();
    let first = env.run(&["migrate"]);
    assert_eq!(first.status.code(), Some(0), "{first:?}");
    assert_eq!(
        String::from_utf8(first.stdout).unwrap(),
        "CodePet data created (schema 5).\n"
    );
    assert!(first.stderr.is_empty());
    let bar = env.bar_state().expect("migrate publishes bar-state.json");
    assert_eq!(bar["enabled"], true);
    assert_eq!(bar["task"], serde_json::Value::Null);
    if !cfg!(feature = "legacy-tasks") {
        // The inert v0.3.0 compatibility values of the v1 keys.
        assert_eq!(bar["care"]["ready_to_work"], false);
        assert_eq!(bar["care"]["readiness_reasons"], serde_json::json!([]));
    }
    let again = env.run(&["migrate"]);
    assert_eq!(again.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(again.stdout).unwrap(),
        "CodePet data is up to date (schema 5).\n"
    );
    let backups = std::fs::read_dir(env.state_dir())
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".bak-")
        })
        .count();
    assert_eq!(backups, 0, "a fresh install has nothing to back up");
}

#[test]
fn on_off_helpers_and_forget_history_print_plain_words_and_keep_the_flags() {
    let env = Env::new();
    let run = |args: &[&str]| {
        let o = env.run(args);
        assert_eq!(o.status.code(), Some(0), "{args:?}: {o:?}");
        assert!(o.stderr.is_empty(), "{args:?}: {o:?}");
        String::from_utf8(o.stdout).unwrap()
    };
    let off = env.state_dir().join("off");
    let helpers = env.state_dir().join("helpers");
    assert_eq!(run(&["off"]), "CodePet is Off\n");
    assert!(off.is_file());
    {
        use std::os::unix::fs::PermissionsExt;
        let m = std::fs::metadata(&off).unwrap();
        assert_eq!((m.len(), m.permissions().mode() & 0o777), (0, 0o600));
    }
    assert_eq!(env.bar_state().unwrap()["enabled"], false);
    assert_eq!(env.bar_state().unwrap()["presence"]["wanted"], false);
    // No companion yet (fresh install): `no_life` outranks `off` (first_run_off).
    assert_eq!(env.state()["lifecycle"]["frozen_reason"], "no_life");
    assert_eq!(env.state()["enabled"], false);
    // Idempotent.
    assert_eq!(run(&["off"]), "CodePet is Off\n");
    assert_eq!(run(&["helpers", "on"]), "Background helpers are counted\n");
    assert!(helpers.is_file());
    assert_eq!(
        run(&["forget-history"]),
        "Coding history forgotten. Treats and growth are kept.\n"
    );
    assert_eq!(run(&["on"]), "CodePet is On\n");
    assert!(!off.exists());
    assert_eq!(env.bar_state().unwrap()["enabled"], true);
    assert_eq!(
        run(&["helpers", "off"]),
        "Background helpers are not counted\n"
    );
    assert!(!helpers.exists());
}

#[test]
fn a_stray_off_flag_heals_at_the_next_command() {
    let env = Env::new();
    assert!(env.run(&["state"]).status.success());
    let off = env.state_dir().join("off");
    std::fs::write(&off, b"").unwrap();
    assert!(env.run(&["state"]).status.success());
    assert!(!off.exists(), "the database is authoritative");
    // A symlink in its place is replaced by a plain file when Off, never followed.
    assert!(env.run(&["off"]).status.success());
    std::fs::remove_file(&off).unwrap();
    std::os::unix::fs::symlink("/nonexistent-codepet-target", &off).unwrap();
    assert!(env.run(&["state"]).status.success());
    let m = std::fs::symlink_metadata(&off).unwrap();
    assert!(m.is_file(), "{m:?}");
    assert!(!std::path::Path::new("/nonexistent-codepet-target").exists());
}
