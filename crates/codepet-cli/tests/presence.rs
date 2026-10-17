//! Attended presence end to end: the bar leader's `presence.json` is settled by ordinary
//! CLI commands (`rpc` and `codepet state`) under the real system clocks. Nothing in the CLI
//! records presence itself, and a refused or uncertain file never fails a command.
// Task-pipeline tests and helpers run only with `legacy-tasks` (retained for non-default compatibility).
#![cfg_attr(not(feature = "legacy-tasks"), allow(dead_code, unused_imports))]
mod common;
use codepet_storage::{Clock, SystemClock};
use common::{error_code, Env, HOUR, MIN};
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use std::time::Instant;

fn attended(s: &Value) -> i64 {
    s["lifecycle"]["attended_total_ms"].as_i64().unwrap()
}

/// A routine-filled, armed companion; returns its id.
fn armed(env: &Env) -> String {
    env.care_routine();
    env.arm();
    env.life_id()
}

#[test]
fn cli_commands_settle_presence_capped_by_boot_time_exactly_once() {
    let env = Env::with_first_run_pet();
    env.care_routine();
    // The backend measures the cap from the arming command's own settle stamp, which lies
    // inside `env.arm()`; start the bound before it so the bound is never the smaller one.
    let before_arming = Instant::now();
    env.arm();
    let life = env.life_id();
    let now = SystemClock.now_ms();
    // A claim of 2 h while only seconds of boot time have passed since the last settling
    // command (the arming): credit is capped at that boot time plus one tick. The slack
    // covers clock granularity; the test only needs 2 h to be capped near one minute.
    env.write_presence("bar-cli-0001", &life, now - 3 * HOUR, now, 2 * HOUR);
    let s = env.state();
    let bound = 60_000 + i64::try_from(before_arming.elapsed().as_millis()).unwrap() + 250;
    let credited = attended(&s);
    assert!(
        (MIN..=bound).contains(&credited),
        "credited {credited} ms, expected 60000..={bound}"
    );
    assert_eq!(s["lifecycle"]["frozen_reason"], "none");
    assert_eq!(s["life"]["status"], "healthy", "within the satiety hold");
    // The claim is consumed: neither a repeat nor another command charges it again.
    assert_eq!(attended(&env.state()), credited);
    env.ok("pet.pause", json!({"paused": false}));
    assert_eq!(attended(&env.state()), credited);
    // The bar asked for exactly this life's presence.
    let bar = env.bar_state().expect("published");
    assert_eq!(bar["presence"]["wanted"], true);
    assert_eq!(bar["presence"]["life_id"], life.as_str());
    assert_eq!(bar["presence"]["tick_ms"], MIN);
    assert_eq!(bar["presence"]["idle_timeout_s"], 180);
}

#[test]
fn the_state_command_settles_presence_and_publishes() {
    let env = Env::with_first_run_pet();
    let life = armed(&env);
    let now = SystemClock.now_ms();
    env.write_presence("bar-cli-0002", &life, now - 10 * MIN, now, MIN);
    let out = env.run(&["state"]);
    assert!(out.status.success(), "{out:?}");
    assert!(out.stderr.is_empty(), "{out:?}");
    let s: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(attended(&s), MIN);
    let bar = env.bar_state().expect("codepet state publishes");
    assert_eq!(bar["state_version"], s["state_version"]);
    assert_eq!(attended(&env.state()), MIN, "consumed once");
}

#[test]
fn an_unarmed_life_never_charges_even_after_arming() {
    let env = Env::with_first_run_pet();
    env.care_routine();
    let life = env.life_id();
    let now = SystemClock.now_ms();
    env.write_presence("bar-cli-0003", &life, now - HOUR, now, 30 * MIN);
    let s = env.state();
    assert_eq!(s["life"]["armed"], false);
    assert_eq!(s["lifecycle"]["frozen_reason"], "unarmed");
    assert_eq!(attended(&s), 0);
    assert_eq!(env.bar_state().unwrap()["presence"]["wanted"], false);
    // Arming later never charges what was recorded while unarmed.
    env.arm();
    assert_eq!(attended(&env.state()), 0);
    assert_eq!(env.bar_state().unwrap()["presence"]["wanted"], true);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn pause_and_a_held_slot_discard_presence() {
    let env = Env::with_first_run_pet();
    let life = armed(&env);
    env.ok("pet.pause", json!({"paused": true}));
    let now = SystemClock.now_ms();
    env.write_presence("bar-cli-0004", &life, now - HOUR, now, MIN);
    let s = env.state();
    assert_eq!(s["lifecycle"]["frozen_reason"], "paused");
    assert_eq!(attended(&s), 0);
    assert_eq!(env.bar_state().unwrap()["presence"]["wanted"], false);
    env.ok("pet.pause", json!({"paused": false}));

    // A task in review holds the slot: time is frozen and discarded.
    let (_id, _digest) = env.reviewed("hunger", "artifact");
    let now = SystemClock.now_ms();
    env.write_presence("bar-cli-0005", &life, now - HOUR, now, MIN);
    let s = env.state();
    assert_eq!(s["lifecycle"]["frozen_reason"], "task");
    assert_eq!(attended(&s), 0);
    let bar = env.bar_state().unwrap();
    assert_eq!(bar["presence"]["wanted"], false);
    assert_eq!(bar["task"]["state"], "review_ready");
}

#[test]
fn refused_presence_files_never_fail_a_command_or_charge() {
    let env = Env::with_first_run_pet();
    let life = armed(&env);
    let now = SystemClock.now_ms();
    let good = json!({
        "v": 1, "session": "bar-cli-0006", "life_id": life,
        "started_wall_ms": now - HOUR, "updated_wall_ms": now,
        "attended_ms": MIN, "ticks": 1,
    });
    let with = |key: &str, value: Value| {
        let mut doc = good.clone();
        doc[key] = value;
        doc.to_string()
    };
    let cases = [
        with("extra", json!(1)),
        with("v", json!(2)),
        with("session", json!("not-a-bar-session")),
        with("attended_ms", json!(2 * MIN)),
        with("updated_wall_ms", json!(now + HOUR)),
        "{not json".to_owned(),
        " ".repeat(4096),
    ];
    for text in &cases {
        env.write_presence_raw(text);
        let s = env.state();
        assert_eq!(attended(&s), 0, "{text:.80}");
    }
    // Group-writable, a symlink, and a directory are refused too.
    env.write_presence_raw(&good.to_string());
    std::fs::set_permissions(env.presence_path(), std::fs::Permissions::from_mode(0o620)).unwrap();
    assert_eq!(attended(&env.state()), 0);
    let target = env.state_dir().join("elsewhere.json");
    std::fs::rename(env.presence_path(), &target).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::os::unix::fs::symlink(&target, env.presence_path()).unwrap();
    assert_eq!(attended(&env.state()), 0);
    std::fs::remove_file(env.presence_path()).unwrap();
    std::fs::create_dir(env.presence_path()).unwrap();
    assert_eq!(attended(&env.state()), 0);
    std::fs::remove_dir(env.presence_path()).unwrap();
    // Another life's session is discarded, not charged to this one.
    env.write_presence(
        "bar-cli-0007",
        "life-000000000000000000000000",
        now - HOUR,
        now,
        MIN,
    );
    assert_eq!(attended(&env.state()), 0);
    // The valid file still settles afterwards.
    env.write_presence_raw(&good.to_string());
    assert_eq!(attended(&env.state()), MIN);
}

#[test]
fn no_living_companion_freezes_presence() {
    let env = Env::with_first_run_pet();
    let life = armed(&env);
    env.retire();
    let now = SystemClock.now_ms();
    env.write_presence("bar-cli-0008", &life, now - HOUR, now, MIN);
    let s = env.state();
    assert_eq!(s["life"], Value::Null);
    assert_eq!(s["lifecycle"]["frozen_reason"], "no_life");
    assert_eq!(attended(&s), 0);
    let bar = env.bar_state().unwrap();
    assert_eq!(bar["presence"]["wanted"], false);
    assert_eq!(bar["presence"]["life_id"], Value::Null);
    assert_eq!(
        error_code(&env.call("pet.care", json!({"action": "water"}))),
        "NO_ACTIVE_PET"
    );
}
