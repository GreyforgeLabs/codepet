//! WP1 methods end to end through `codepet rpc` (SPEC-v0.3 §6.2, §7.3), each under its own
//! throwaway `XDG_STATE_HOME` and `HOME`: `pet.start`, `pet.rename`, `treats.give`,
//! `app.set_enabled`, `app.set_helpers`, `agent.forget_history`, the Off behaviour of every
//! method from the frozen `protocol.json`, and the retired task methods.
mod common;
use common::{error_code, Env};
use serde_json::{json, Value};

fn protocol() -> Value {
    serde_json::from_str(include_str!("../../../docs/v0.3/contracts/protocol.json")).unwrap()
}

/// The keys every v0.3 method's result has: the `state.get` snapshot.
fn assert_snapshot(v: &Value) {
    for k in [
        "schema_version",
        "enabled",
        "count_helpers",
        "agent",
        "clients",
        "treats",
        "lifecycle",
        "life",
        "memorial",
    ] {
        assert!(v.get(k).is_some(), "{k} missing in {v}");
    }
    assert_eq!(v["schema_version"], 6);
}

#[test]
fn pet_start_arms_ginger_gentle_and_returns_the_snapshot() {
    let env = Env::with_first_run_pet();
    let life = env.life_id();
    let s = env.ok(
        "pet.start",
        json!({"life_id": life, "species": "goldfish", "name": "Ginger", "mode": "gentle",
            "acknowledge_rules": true}),
    );
    assert_snapshot(&s);
    assert_eq!(s["life"]["life_id"], life.as_str());
    assert_eq!(s["life"]["appearance"], "goldfish");
    assert_eq!(s["life"]["mode"], "gentle");
    assert_eq!(s["life"]["armed"], true);
    assert_eq!(s["care"]["welcome_care_available"], false);
    assert_eq!(s["meters"]["hunger"], 10000);
    // The bar file follows before the answer.
    let bar = env.bar_state().unwrap();
    assert_eq!(bar["life"]["armed"], true);
    assert_eq!(bar["state_version"], s["state_version"]);
    // A replay succeeds; a new click on the armed life is ALREADY_ARMED.
    let r = env.call_with(
        "same-id",
        "pet.start",
        json!({"species": "cat", "mode": "gentle", "acknowledge_rules": true}),
        None,
    );
    assert_eq!(error_code(&r), "ALREADY_ARMED");
    assert_eq!(
        env.err(
            "pet.start",
            json!({"species": "cat", "mode": "gentle", "acknowledge_rules": false})
        ),
        "ALREADY_ARMED"
    );
}

/// Spec §2.1 / K7 through the binary: a fresh install has no pet, the bar shows none, and
/// Let's go! with another portrait adopts that species under its default name.
#[test]
fn a_fresh_install_starts_without_a_pet_and_lets_go_adopts_the_picked_one() {
    let env = Env::new();
    let s = env.state();
    assert_eq!(s["life"], Value::Null);
    assert_eq!(s["lifecycle"]["frozen_reason"], "no_life");
    assert_eq!(env.bar_state().unwrap()["life"], Value::Null);
    let s = env.ok(
        "pet.start",
        json!({"species": "dog", "mode": "gentle", "acknowledge_rules": true}),
    );
    assert_snapshot(&s);
    let dog = codepet_core::species::default_name("dog").unwrap();
    assert_eq!(s["life"]["name"], dog);
    assert_eq!(s["life"]["species"], "dog");
    assert_eq!(s["life"]["appearance"], "dog");
    assert_eq!(s["life"]["origin"], "adopted");
    assert_eq!(s["life"]["armed"], true);
    assert_eq!(s["life"]["mode"], "gentle");
    assert_eq!(env.bar_state().unwrap()["life"]["name"], dog);

    let env = Env::new();
    let s = env.ok(
        "pet.start",
        json!({"species": "frog", "name": "Hopper", "mode": "gentle",
            "acknowledge_rules": true}),
    );
    assert_eq!(s["life"]["name"], "Hopper");
    assert_eq!(s["life"]["species"], "frog");
}

#[test]
fn pet_start_errors_are_typed() {
    let env = Env::new();
    assert_eq!(
        env.err(
            "pet.start",
            json!({"species": "cat", "mode": "gentle", "acknowledge_rules": false})
        ),
        "ACKNOWLEDGEMENT_REQUIRED"
    );
    assert_eq!(
        env.err(
            "pet.start",
            json!({"species": "cat", "mode": "gentle", "acknowledge_rules": true,
                "name": "\u{1F431}".repeat(32)})
        ),
        "NAME_INVALID"
    );
    assert_eq!(
        env.err(
            "pet.start",
            json!({"life_id": "life-000000000000000000000000", "species": "cat",
                "mode": "gentle", "acknowledge_rules": true})
        ),
        "NOT_FOUND"
    );
    for bad in protocol()["added"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["method"] == "pet.start")
        .unwrap()["examples"]["invalid"]
        .as_array()
        .unwrap()
    {
        assert_eq!(env.err("pet.start", bad.clone()), "INVALID_PARAMS", "{bad}");
    }
}

#[test]
fn rename_treats_helpers_and_forget_round_trip() {
    let env = Env::with_first_run_pet();
    let life = env.life_id();
    let s = env.ok("pet.rename", json!({"life_id": life, "name": " Pip "}));
    assert_snapshot(&s);
    assert_eq!(s["life"]["name"], "Pip");
    assert_eq!(env.bar_state().unwrap()["life"]["name"], "Pip");
    assert_eq!(
        env.err(
            "pet.rename",
            json!({"life_id": life, "name": "\u{1F431}".repeat(32)})
        ),
        "NAME_INVALID"
    );
    assert_eq!(
        env.err("treats.give", json!({"life_id": life})),
        "NO_TREATS"
    );
    let s = env.ok("app.set_helpers", json!({"enabled": true}));
    assert_eq!(s["count_helpers"], true);
    assert!(env.state_dir().join("helpers").is_file());
    let s = env.ok("agent.forget_history", json!({}));
    assert_snapshot(&s);
    assert_eq!(s["treats"], json!({"jar": 0, "full": false}));
}

#[test]
fn off_is_total_and_on_resumes() {
    let env = Env::with_first_run_pet();
    let life = env.life_id();
    env.ok(
        "pet.start",
        json!({"species": "cat", "mode": "gentle", "acknowledge_rules": true}),
    );
    let s = env.ok("app.set_enabled", json!({"enabled": false}));
    assert_eq!(s["enabled"], false);
    assert_eq!(s["lifecycle"]["frozen_reason"], "off");
    let bar = env.bar_state().unwrap();
    assert_eq!(bar["enabled"], false);
    assert_eq!(bar["presence"]["wanted"], false);
    assert!(env.state_dir().join("off").is_file());

    // protocol.json `off_behaviour`: every refused method answers INVALID_STATE
    // "CodePet is off" (the clients methods too, before any client is detected).
    let p = protocol();
    let off = &p["off_behaviour"];
    let params = |m: &str| -> Value {
        match m {
            "pet.start" => {
                json!({"species": "cat", "mode": "gentle", "acknowledge_rules": true})
            }
            "pet.adopt" => {
                json!({"species": "cat", "mode": "gentle", "acknowledge_rules": true})
            }
            "pet.care" => json!({"action": "food"}),
            "pet.sleep" | "pet.wake" => json!({}),
            "pet.pause" => json!({"paused": true}),
            "pet.rules" => {
                json!({"life_id": life, "mode": "permadeath", "acknowledge_rules": true})
            }
            "pet.configure_life" => {
                json!({"life_id": life, "expected_revision": 1, "grace_hours": 48})
            }
            "pet.retire" => json!({"life_id": life, "confirm_name": "Ginger"}),
            "treats.give" => json!({"life_id": life}),
            "clients.connect" => json!({"clients": ["claude-code"]}),
            "clients.test" => json!({}),
            other => panic!("no params for {other}"),
        }
    };
    let refused = off["refused"].as_array().unwrap();
    assert_eq!(refused.len(), 12);
    for m in refused {
        let m = m.as_str().unwrap();
        let r = env.call(m, params(m));
        let resp = r.response();
        assert_eq!(resp["ok"], false, "{m}");
        assert_eq!(resp["error"]["code"], "INVALID_STATE", "{m}: {resp}");
        assert_eq!(resp["error"]["message"], "CodePet is off", "{m}");
    }
    // Allowed methods run while Off.
    let allowed = off["allowed"].as_array().unwrap();
    assert!(allowed.iter().any(|m| m == "pet.rename"));
    env.ok("hello", json!({}));
    env.ok("state.get", json!({}));
    env.ok("pet.select", json!({"life_id": life, "pet_id": "owl"}));
    env.ok("pet.rename", json!({"life_id": life, "name": "Pip"}));
    env.ok(
        "pet.events_seen",
        json!({"life_id": life, "through_seq": 0}),
    );
    env.ok("pet.memorial", json!({}));
    env.ok("lifecycle.rules", json!({}));
    env.ok("app.set_helpers", json!({"enabled": true}));
    env.ok("agent.forget_history", json!({}));
    assert_eq!(env.state()["life"]["name"], "Pip");

    let s = env.ok("app.set_enabled", json!({"enabled": true}));
    assert_eq!(s["enabled"], true);
    assert_eq!(s["lifecycle"]["frozen_reason"], "none");
    assert!(!env.state_dir().join("off").exists());
    env.ok("pet.care", json!({"action": "food"}));
}

#[test]
fn the_retired_task_methods_are_unknown_in_the_default_build() {
    let env = Env::new();
    for m in protocol()["removed"].as_array().unwrap() {
        let m = m.as_str().unwrap();
        let r = env.call(m, json!({}));
        let code = r.response()["error"]["code"].clone();
        if cfg!(feature = "legacy-tasks") {
            assert_ne!(code, "UNKNOWN_METHOD", "{m}");
        } else {
            assert_eq!(code, "UNKNOWN_METHOD", "{m}");
            assert_eq!(r.status.code(), Some(1));
        }
    }
    if !cfg!(feature = "legacy-tasks") {
        assert!(!env.xdg().exists(), "an unknown method opens no state");
    }
    // hello no longer carries the retired supervisor handshake.
    let h = env.ok("hello", json!({}));
    assert_eq!(
        h.get("supervisor").is_some(),
        cfg!(feature = "legacy-tasks")
    );
}

#[test]
fn care_tempo_round_trips_through_rpc_and_published_bar_state() {
    let env = Env::new();
    let s = env.ok(
        "pet.start",
        json!({"species":"penguin", "mode":"gentle", "acknowledge_rules":true}),
    );
    assert_eq!(s["life"]["care_tempo"], "lively");
    let params = json!({"life_id":s["life"]["life_id"], "expected_revision":s["life"]["revision"], "tempo":"steady"});
    let response = env
        .call_with("tempo-click", "pet.care_tempo", params.clone(), None)
        .response()
        .clone();
    assert_eq!(response["ok"], true, "{response}");
    let first = response["result"].clone();
    assert_eq!(first["changed"], true);
    assert_eq!(first["life"]["care_tempo"], "steady");
    assert_eq!(
        first["lifecycle"]["rules"]["decay_per_hour"],
        json!([1000, 1250, 600, 500])
    );
    let bar = env.bar_state().unwrap();
    assert_eq!(bar["life"]["care_tempo"], "steady");
    assert_eq!(bar["life"]["revision"], first["life"]["revision"]);
    assert_eq!(bar["care_rules"], first["lifecycle"]["rules"]);
    assert_eq!(
        env.call_with("tempo-click", "pet.care_tempo", params.clone(), None)
            .response()["result"],
        first
    );
    assert_eq!(env.err("pet.care_tempo", params), "STALE_REVISION");
    assert_eq!(
        env.err(
            "pet.care_tempo",
            json!({"life_id":s["life"]["life_id"], "expected_revision":0,"tempo":"fast"})
        ),
        "INVALID_PARAMS"
    );
}
