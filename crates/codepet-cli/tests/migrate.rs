//! Migration ownership through the binary (SPEC-v0.3 §6.2, §6.6): a schema v4 database
//! (built by the unchanged v0.2 migrations, never from the owner's real file) is upgraded
//! only by `codepet migrate` and `codepet rpc` `state.get`. Every other command refuses it
//! with `SCHEMA_UNSUPPORTED` and leaves every file of the state directory byte-identical.
mod common;
use codepet_storage::{testing, SystemClock};
use common::{error_code, Env};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

/// A state directory holding a v0.2-shaped schema v4 database (unarmed life #1 "Ginger").
fn v4_env() -> Env {
    let env = Env::new();
    testing::create_v4_database(&env.paths(), Arc::new(SystemClock)).unwrap();
    assert_eq!(testing::user_version(&env.paths().db_path).unwrap(), 4);
    env
}

/// Every file of the state directory and its exact bytes.
fn files(env: &Env) -> BTreeMap<String, Vec<u8>> {
    std::fs::read_dir(env.state_dir())
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (
                e.file_name().into_string().unwrap(),
                std::fs::read(e.path()).unwrap(),
            )
        })
        .collect()
}

fn backups(env: &Env) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(env.state_dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().contains(".bak-"))
        .collect();
    v.sort();
    v
}

#[test]
fn only_migrate_upgrades_and_every_other_command_refuses_a_v4_database_untouched() {
    let env = v4_env();
    let before = files(&env);
    assert!(before.contains_key("codepet.sqlite3"));
    for (method, params) in [
        ("pet.care", json!({"action": "food"})),
        ("hello", json!({})),
        ("app.set_enabled", json!({"enabled": false})),
        (
            "pet.start",
            json!({"species": "cat", "mode": "gentle", "acknowledge_rules": true}),
        ),
    ] {
        let r = env.call(method, params);
        assert_eq!(error_code(&r), "SCHEMA_UNSUPPORTED", "{method}: {r:?}");
        let message = r.response()["error"]["message"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(message.contains("codepet migrate"), "{method}: {message}");
        assert!(
            files(&env) == before,
            "{method} changed the state directory"
        );
    }
    for args in [
        &["on"][..],
        &["off"],
        &["helpers", "on"],
        &["forget-history"],
        // `codepet state` never migrates (§6.2); it says what to run instead.
        &["state"],
    ] {
        let out = env.run(args);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {out:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
        let err = String::from_utf8(out.stderr).unwrap();
        assert!(err.starts_with("SCHEMA_UNSUPPORTED: "), "{args:?}: {err}");
        assert!(err.contains("codepet migrate"), "{args:?}: {err}");
        assert_eq!(err.lines().count(), 1, "{args:?}: {err}");
        assert!(
            files(&env) == before,
            "{args:?} changed the state directory"
        );
    }
    assert_eq!(
        files(&env),
        before,
        "refusals leave the database byte-identical, take no backup and publish nothing"
    );

    // The installer's `codepet migrate`.
    let out = env.run(&["migrate"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(out.stderr.is_empty(), "{out:?}");
    let taken = backups(&env);
    assert_eq!(taken.len(), 1, "{taken:?}");
    let backup = &taken[0];
    let name = backup.file_name().unwrap().to_string_lossy().into_owned();
    let millis = name
        .strip_prefix("codepet.sqlite3.bak-v4-")
        .unwrap_or_else(|| panic!("{name}"));
    assert!(
        !millis.is_empty() && millis.bytes().all(|b| b.is_ascii_digit()),
        "{name}"
    );
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        format!(
            "CodePet data upgraded from schema 4 to 5. Backup: {}\n",
            backup.display()
        )
    );
    let mode = std::fs::metadata(backup).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    assert_eq!(testing::user_version(backup).unwrap(), 4);
    assert_eq!(testing::user_version(&env.paths().db_path).unwrap(), 5);
    assert_eq!(
        env.bar_state().expect("migrate publishes")["life"]["name"],
        "Ginger"
    );

    // Now every command works, and the pet is the one v0.2 had.
    let state = env.run(&["state"]);
    assert_eq!(state.status.code(), Some(0), "{state:?}");
    let s: Value = serde_json::from_slice(&state.stdout).unwrap();
    assert_eq!(s["schema_version"], 5);
    assert_eq!(s["life"]["name"], "Ginger");
    assert_eq!(s["life"]["armed"], false);
    let again = env.run(&["migrate"]);
    assert_eq!(
        String::from_utf8(again.stdout).unwrap(),
        "CodePet data is up to date (schema 5).\n"
    );
    assert_eq!(
        backups(&env),
        taken,
        "an up-to-date database is not backed up"
    );
}

#[test]
fn rpc_state_get_also_upgrades_a_v4_database() {
    let env = v4_env();
    let s = env.state();
    assert_eq!(s["schema_version"], 5);
    assert_eq!(s["life"]["name"], "Ginger");
    assert_eq!(s["life"]["origin"], "first_run");
    let taken = backups(&env);
    assert_eq!(taken.len(), 1, "{taken:?}");
    assert_eq!(testing::user_version(&taken[0]).unwrap(), 4);
    assert_eq!(testing::user_version(&env.paths().db_path).unwrap(), 5);
}

/// The upgrade sequence (§6.6): the reloaded bar's `state.get` can race the installer's
/// `codepet migrate`. Both must succeed and exactly one v4 backup must exist.
#[test]
fn migrate_racing_state_get_both_succeed_with_one_backup() {
    for round in 0..10 {
        let env = v4_env();
        let mut migrate = env.command(&["migrate"], None);
        migrate.stdin(Stdio::null());
        let mut state = env.command(&["rpc"], None);
        let a = migrate.spawn().unwrap();
        let mut b = state.spawn().unwrap();
        let frame = format!(
            "{}\n",
            json!({"v": 1, "id": format!("race-{round}"), "method": "state.get", "params": {}})
        );
        let mut stdin = b.stdin.take().unwrap();
        stdin.write_all(frame.as_bytes()).unwrap();
        drop(stdin);
        let a = a.wait_with_output().unwrap();
        let b = b.wait_with_output().unwrap();
        assert_eq!(a.status.code(), Some(0), "round {round}: {a:?}");
        assert_eq!(b.status.code(), Some(0), "round {round}: {b:?}");
        let out = String::from_utf8(a.stdout).unwrap();
        assert!(
            out.starts_with("CodePet data upgraded from schema 4 to 5. Backup: ")
                || out == "CodePet data is up to date (schema 5).\n",
            "round {round}: {out}"
        );
        let r: Value = serde_json::from_slice(&b.stdout).unwrap();
        assert_eq!(r["ok"], true, "round {round}: {r}");
        assert_eq!(r["result"]["schema_version"], 5);
        let taken = backups(&env);
        assert_eq!(taken.len(), 1, "round {round}: {taken:?}");
        assert_eq!(testing::user_version(&taken[0]).unwrap(), 4);
    }
}
