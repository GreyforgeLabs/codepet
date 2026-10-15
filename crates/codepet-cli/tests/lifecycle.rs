//! The v0.2 companion lifecycle end to end through the `codepet` binary (spec §6, §9,
//! §17.3). Attended time cannot pass inside a short test process, so a `Store` on a
//! `ManualClock` stands in for the hours between commands, settling the same
//! `presence.json` the bar leader writes. Every check of what users see goes through fresh
//! CLI processes and the published `bar-state.json`.
mod common;
use common::{error_code, Env, Timeline, HOUR, MIN};
use serde_json::{json, Value};
use std::sync::Barrier;
use std::thread;

fn need<'a>(s: &'a Value, name: &str) -> &'a Value {
    s["lifecycle"]["needs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["need"] == name)
        .unwrap()
}

fn care(env: &Env, action: &str) -> Value {
    env.ok("pet.care", json!({"action": action}))
}

#[test]
fn steady_timeline_thirst_6_6h_warning_11h_critical_17h_dead() {
    let env = Env::new();
    care(&env, "routine");
    env.arm();
    let mut t = Timeline::new(&env, "bar-timeline-steady");

    // One attended hour of satiety hold, then 12.5 points per hour: 3001 at 6.6 h - 1 ms.
    let s = t.settle_to(&env, 23_759_999);
    assert_eq!(s["meters"]["thirst"], 3001);
    assert_eq!(s["life"]["status"], "healthy");
    let s = t.settle_to(&env, 23_760_000);
    assert_eq!(s["meters"]["thirst"], 3000);
    assert_eq!(s["life"]["status"], "warning", "meter == warning_at warns");
    assert_eq!(need(&s, "thirst")["state"], "low");

    // Empty at 9 h; two hours of neglect later (11 h) it becomes critical.
    let s = t.settle_to(&env, 39_599_999);
    assert_eq!(s["meters"]["thirst"], 0);
    assert_eq!(need(&s, "thirst")["neglect_ms"], 7_199_999);
    assert_eq!(s["life"]["status"], "warning");
    assert_eq!(s["lifecycle"]["critical"], Value::Null);
    let s = t.settle_to(&env, 39_600_000);
    assert_eq!(s["life"]["status"], "critical");
    assert_eq!(s["lifecycle"]["critical"]["need"], "thirst");
    assert_eq!(s["lifecycle"]["critical"]["grace_ms"], 21_600_000);
    assert_eq!(s["lifecycle"]["critical"]["elapsed_ms"], 0);

    // What the CLI and the bar show while critical: care offered, work and retiring refused.
    let cli = env.state();
    assert_eq!(cli["life"]["status"], "critical");
    assert_eq!(cli["care"]["ready_to_work"], false);
    let bar = env.bar_state().unwrap();
    assert_eq!(bar["life"]["status"], "critical");
    assert_eq!(bar["critical"]["need"], "thirst");
    assert_eq!(bar["critical"]["action"], "water");
    assert_eq!(bar["state_version"], cli["state_version"]);
    let task = env.create("hunger", 0, "artifact");
    let r = env.call("task.preflight", json!({"task_id": task, "revision": 1}));
    assert_eq!(error_code(&r), "INVALID_STATE");
    assert!(
        r.response()["error"]["message"]
            .as_str()
            .unwrap()
            .contains("urgently needs care"),
        "{r:?}"
    );
    let life = cli["life"]["life_id"].clone();
    assert_eq!(
        env.err(
            "pet.retire",
            json!({"life_id": life, "confirm_name": "Ginger"})
        ),
        "INVALID_STATE",
        "retirement never launders a critical companion"
    );

    // Grace is six attended hours, frozen at entry: still living 1 ms before 17 h.
    let s = t.settle_to(&env, 61_199_999);
    assert_eq!(s["life"]["status"], "critical");
    assert_eq!(s["lifecycle"]["critical"]["elapsed_ms"], 21_599_999);
    assert_eq!(s["lifecycle"]["critical"]["remaining_ms"], 1);
    assert_eq!(s["memorial"]["count"], 0);

    // The last millisecond is settled by an ordinary CLI command.
    t.attend_for_cli(&env, 61_200_000);
    let s = env.state();
    assert_eq!(s["life"], Value::Null);
    assert_eq!(s["lifecycle"]["frozen_reason"], "no_life");
    let latest = &s["memorial"]["latest"];
    assert_eq!(s["memorial"]["count"], 1);
    assert_eq!(latest["name"], "Ginger");
    assert_eq!(latest["end_cause"], "neglect:thirst");
    assert_eq!(latest["attended_ms"], 61_200_000);
    assert_eq!(latest["final_level"], 1);
    assert_eq!(latest["life_xp"], 0);
    let bar = env.bar_state().unwrap();
    assert_eq!(bar["life"], Value::Null);
    assert_eq!(
        bar["memorial"],
        json!({"count": 1, "latest_name": "Ginger"})
    );
    assert_eq!(bar["presence"]["wanted"], false);
    assert_eq!(bar["state_version"], s["state_version"]);
    // Exactly one end, however often it is observed.
    assert_eq!(env.state()["memorial"]["count"], 1);
    assert_eq!(
        env.ok("pet.memorial", json!({}))["entries"],
        json!([latest.clone()])
    );
    assert_eq!(
        error_code(&env.call("pet.care", json!({"action": "water"}))),
        "NO_ACTIVE_PET"
    );
}

#[test]
fn only_the_matching_care_recovers_from_critical() {
    let env = Env::new();
    care(&env, "routine");
    env.arm();
    let mut t = Timeline::new(&env, "bar-timeline-care");
    t.settle_to(&env, 6 * HOUR);
    t.settle_to(&env, 11 * HOUR);
    assert_eq!(env.state()["life"]["status"], "critical");

    // Food fills hunger but cannot clear a thirst emergency.
    let fed = care(&env, "food");
    assert_eq!(fed["lifecycle"]["critical"]["need"], "thirst");
    let s = env.state();
    assert_eq!(s["life"]["status"], "critical");
    assert_eq!(s["meters"]["hunger"], 10_000);
    assert_eq!(env.bar_state().unwrap()["critical"]["need"], "thirst");

    // Water does.
    let watered = care(&env, "water");
    assert_eq!(watered["lifecycle"]["critical"], Value::Null);
    let s = env.state();
    assert_eq!(s["life"]["status"], "healthy");
    assert_eq!(need(&s, "thirst")["neglect_ms"], 0);
    let bar = env.bar_state().unwrap();
    assert_eq!(bar["critical"], Value::Null);
    assert_eq!(bar["life"]["status"], "healthy");
}

#[test]
fn gentle_faints_and_never_dies() {
    let env = Env::new();
    env.retire();
    let adopted = env.adopt("frog", "gentle", "playtest");
    assert_eq!(adopted["life"]["mode"], "gentle");
    assert_eq!(adopted["life"]["pace"], "playtest");
    care(&env, "routine");
    let mut t = Timeline::new(&env, "bar-timeline-gentle");
    // Playtest: 3 min hold, empty at 27 min, critical at 33 min, grace 18 min.
    let s = t.settle_to(&env, 33 * MIN);
    assert_eq!(s["life"]["status"], "critical");
    let s = t.settle_to(&env, 51 * MIN);
    assert_eq!(s["life"]["fainted"], true);
    assert_eq!(s["life"]["status"], "critical");
    let s = t.settle_to(&env, 51 * MIN + 8 * HOUR);
    assert_eq!(s["life"]["fainted"], true);
    assert_eq!(s["memorial"]["count"], 1, "only the retired companion");

    let cli = env.state();
    assert_eq!(cli["life"]["name"], "Hopper");
    assert_eq!(cli["life"]["fainted"], true);
    assert_eq!(env.bar_state().unwrap()["life"]["fainted"], true);
    let task = env.create("thirst", 0, "artifact");
    assert_eq!(
        env.err("task.preflight", json!({"task_id": task, "revision": 1})),
        "INVALID_STATE"
    );
    // Care revives the companion.
    care(&env, "routine");
    let s = env.state();
    assert_eq!(s["life"]["fainted"], false);
    assert_eq!(s["life"]["status"], "healthy");
    assert_eq!(s["memorial"]["count"], 1);
}

#[test]
fn retire_requires_the_name_and_a_free_slot_then_adoption_starts_over() {
    let env = Env::new();
    let (id, digest) = env.reviewed("hunger", "artifact");
    env.ok(
        "task.accept_artifact",
        json!({"task_id": id, "review_digest": digest, "acknowledged": true,
            "override_reason": "Reviewed the simulation"}),
    );
    let life = env.state()["life"].clone();
    assert_eq!(life["xp"], 20);
    let life_id = life["life_id"].clone();
    assert_eq!(
        env.err(
            "pet.retire",
            json!({"life_id": life_id, "confirm_name": "Biscuit"})
        ),
        "INVALID_PARAMS"
    );
    assert_eq!(
        env.err(
            "pet.retire",
            json!({"life_id": "life-000000000000000000000000", "confirm_name": "Ginger"})
        ),
        "STALE_REVISION"
    );
    let (busy, busy_digest) = env.reviewed("thirst", "artifact");
    assert_eq!(
        env.err(
            "pet.retire",
            json!({"life_id": life_id, "confirm_name": "Ginger"})
        ),
        "INVALID_STATE",
        "a task holds the slot"
    );
    env.ok(
        "task.discard",
        json!({"task_id": busy, "review_digest": busy_digest}),
    );

    let retired = env.ok_with(
        "retire-1",
        "pet.retire",
        json!({"life_id": life_id, "confirm_name": " Ginger "}),
    );
    assert_eq!(retired["changed"], true);
    assert_eq!(retired["life"], Value::Null);
    assert_eq!(retired["memorial"]["count"], 1);
    assert_eq!(retired["memorial"]["latest"]["end_cause"], "retired");
    assert_eq!(retired["memorial"]["latest"]["life_xp"], 20);
    assert_eq!(
        env.ok_with(
            "retire-1",
            "pet.retire",
            json!({"life_id": life_id, "confirm_name": " Ginger "}),
        ),
        retired,
        "a replayed retirement returns the stored response"
    );

    // No companion: care, pause, rules and work all say so, calmly.
    for (method, params) in [
        ("pet.care", json!({"action": "routine"})),
        ("pet.sleep", json!({})),
        ("pet.pause", json!({"paused": true})),
        ("pet.select", json!({"life_id": life_id, "pet_id": "dog"})),
        (
            "pet.rules",
            json!({"life_id": life_id, "mode": "gentle", "acknowledge_rules": true}),
        ),
    ] {
        assert_eq!(env.err(method, params), "NO_ACTIVE_PET", "{method}");
    }
    let task = env.create("hunger", 0, "artifact");
    assert_eq!(
        env.err("task.preflight", json!({"task_id": task, "revision": 1})),
        "NO_ACTIVE_PET"
    );
    let s = env.state();
    assert_eq!(s["life"], Value::Null);
    assert_eq!(s["progression"]["demo"]["lifetime_xp"], 20, "history stays");
    assert_eq!(s["recent"][2]["state"], "accepted_artifact");
    let bar = env.bar_state().unwrap();
    assert_eq!(bar["life"], Value::Null);
    assert_eq!(
        bar["memorial"],
        json!({"count": 1, "latest_name": "Ginger"})
    );

    // Adoption is explicit and acknowledged, and starts at level 1.
    assert_eq!(
        env.err(
            "pet.adopt",
            json!({"species": "dog", "mode": "permadeath", "acknowledge_rules": false})
        ),
        "ACKNOWLEDGEMENT_REQUIRED"
    );
    let adopted = env.ok_with(
        "adopt-1",
        "pet.adopt",
        json!({"species": "dog", "mode": "permadeath", "acknowledge_rules": true}),
    );
    let new = &adopted["life"];
    assert_eq!(new["name"], "Biscuit", "the species' default name");
    assert_eq!(new["species"], "dog");
    assert_eq!(new["appearance"], "dog");
    assert_eq!(new["ordinal"], 2);
    assert_eq!(new["origin"], "adopted");
    assert_eq!(new["armed"], true);
    assert_eq!(new["pace"], "steady");
    assert_eq!(
        (&new["level"], &new["xp"], &new["stage"]),
        (&json!(1), &json!(0), &json!(1))
    );
    assert_eq!(new["tasks_accepted"], 0);
    let s = env.state();
    assert_eq!(
        s["meters"],
        json!({"hunger": 7000, "thirst": 7000, "mental_health": 7000, "hygiene": 7000})
    );
    assert_eq!(
        s["progression"]["demo"]["lifetime_xp"], 20,
        "account totals stay"
    );
    assert_ne!(s["life"]["life_id"], life_id);
    assert_eq!(
        env.ok_with(
            "adopt-1",
            "pet.adopt",
            json!({"species": "dog", "mode": "permadeath", "acknowledge_rules": true}),
        ),
        adopted
    );
    assert_eq!(
        env.err_with(
            "adopt-1",
            "pet.adopt",
            json!({"species": "cat", "mode": "permadeath", "acknowledge_rules": true}),
        ),
        "IDEMPOTENCY_CONFLICT"
    );
    assert_eq!(
        env.err(
            "pet.adopt",
            json!({"species": "cat", "mode": "permadeath", "acknowledge_rules": true})
        ),
        "INVALID_STATE",
        "one living companion at a time"
    );
    let bar = env.bar_state().unwrap();
    assert_eq!(bar["life"]["name"], "Biscuit");
    assert_eq!(bar["life"]["armed"], true);
    assert_eq!(bar["presence"]["wanted"], true);

    // Names are normalized by core; a name that is too long is refused.
    env.retire();
    assert_eq!(
        env.err(
            "pet.adopt",
            json!({"species": "owl", "name": "x".repeat(25), "mode": "gentle",
                "acknowledge_rules": true})
        ),
        "INVALID_PARAMS"
    );
    let named = env.ok(
        "pet.adopt",
        json!({"species": "owl", "name": "  Sage\u{202e} ", "mode": "gentle",
            "acknowledge_rules": true}),
    );
    assert_eq!(named["life"]["name"], "Sage");
    assert_eq!(named["life"]["ordinal"], 3);
}

#[test]
fn parallel_adoptions_create_exactly_one_life() {
    let env = Env::new();
    env.retire();
    let species = [
        "dog", "capybara", "penguin", "robot", "iguana", "spider", "snake", "rabbit",
    ];
    let barrier = Barrier::new(species.len());
    let replies: Vec<_> = thread::scope(|scope| {
        let handles: Vec<_> = species
            .iter()
            .map(|sp| {
                let (env, barrier) = (&env, &barrier);
                scope.spawn(move || {
                    barrier.wait();
                    env.call(
                        "pet.adopt",
                        json!({"species": sp, "mode": "permadeath", "acknowledge_rules": true}),
                    )
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut adopted = Vec::new();
    for r in &replies {
        if r.response()["ok"] == true {
            adopted.push(r.response()["result"]["life"]["species"].clone());
        } else {
            assert_eq!(error_code(r), "INVALID_STATE", "{r:?}");
        }
    }
    assert_eq!(adopted.len(), 1, "{replies:?}");
    let s = env.state();
    assert_eq!(s["life"]["species"], adopted[0]);
    assert_eq!(s["life"]["ordinal"], 2);
    assert_eq!(s["memorial"]["count"], 1);
    assert_eq!(env.bar_state().unwrap()["life"]["appearance"], adopted[0]);
}

#[test]
fn select_changes_only_the_look_even_mid_task() {
    let env = Env::new();
    let (task, _digest) = env.reviewed("mental_health", "artifact");
    let before = env.state();
    let life_id = before["life"]["life_id"].clone();
    let chosen = env.ok_with(
        "look-1",
        "pet.select",
        json!({"life_id": life_id, "pet_id": "axolotl"}),
    );
    assert_eq!(chosen["changed"], true);
    assert_eq!(chosen["life"]["appearance"], "axolotl");
    assert_eq!(
        chosen["life"]["species"], "cat",
        "adopted as; never changes"
    );
    let after = env.state();
    assert_eq!(
        after["life"]["revision"],
        before["life"]["revision"].as_i64().unwrap() + 1
    );
    for key in [
        "meters",
        "care",
        "progression",
        "active_task",
        "lifecycle",
        "recent",
    ] {
        assert_eq!(after[key], before[key], "{key} unchanged by a look change");
    }
    assert_eq!(env.task(&task)["state"], "review_ready");
    assert_eq!(env.bar_state().unwrap()["life"]["appearance"], "axolotl");
    // Replays, the same look, a stale life and unknown looks.
    assert_eq!(
        env.ok_with(
            "look-1",
            "pet.select",
            json!({"life_id": life_id, "pet_id": "axolotl"})
        ),
        chosen
    );
    assert_eq!(
        env.ok(
            "pet.select",
            json!({"life_id": life_id, "pet_id": "axolotl"})
        )["changed"],
        false
    );
    assert_eq!(
        env.err(
            "pet.select",
            json!({"life_id": "life-000000000000000000000000", "pet_id": "dog"})
        ),
        "STALE_REVISION"
    );
    for pet in ["dragon", "Cat", "red-panda", ""] {
        assert_eq!(
            env.err("pet.select", json!({"life_id": life_id, "pet_id": pet})),
            "INVALID_PARAMS",
            "{pet}"
        );
    }
}

#[test]
fn rules_and_leniency_through_the_cli() {
    let env = Env::new();
    let s = env.state();
    let life_id = s["life"]["life_id"].clone();
    assert_eq!(s["life"]["armed"], false);
    assert_eq!(s["life"]["origin"], "first_run");
    assert_eq!(s["lifecycle"]["frozen_reason"], "unarmed");
    assert_eq!(env.bar_state().unwrap()["presence"]["wanted"], false);
    assert_eq!(
        env.err(
            "pet.rules",
            json!({"life_id": life_id, "mode": "permadeath", "acknowledge_rules": false})
        ),
        "ACKNOWLEDGEMENT_REQUIRED"
    );
    let armed = env.ok_with(
        "rules-1",
        "pet.rules",
        json!({"life_id": life_id, "mode": "permadeath", "acknowledge_rules": true}),
    );
    assert_eq!(armed["life"]["armed"], true);
    assert_eq!(armed["lifecycle"]["frozen_reason"], "none");
    assert_eq!(env.bar_state().unwrap()["presence"]["wanted"], true);
    assert_eq!(
        env.err_with(
            "rules-1",
            "pet.rules",
            json!({"life_id": life_id, "mode": "gentle", "acknowledge_rules": true}),
        ),
        "IDEMPOTENCY_CONFLICT"
    );
    // Permadeath → gentle at any time; back again needs the acknowledgement.
    let gentle = env.ok(
        "pet.rules",
        json!({"life_id": life_id, "mode": "gentle", "acknowledge_rules": false}),
    );
    assert_eq!(gentle["life"]["mode"], "gentle");
    assert_eq!(
        env.err(
            "pet.rules",
            json!({"life_id": life_id, "mode": "permadeath", "acknowledge_rules": false})
        ),
        "ACKNOWLEDGEMENT_REQUIRED"
    );
    assert_eq!(
        env.ok(
            "pet.rules",
            json!({"life_id": life_id, "mode": "permadeath", "acknowledge_rules": true}),
        )["life"]["mode"],
        "permadeath"
    );
    assert_eq!(env.bar_state().unwrap()["life"]["mode"], "permadeath");

    // Leniency only, against the current revision.
    let revision = env.state()["life"]["revision"].as_u64().unwrap();
    assert_eq!(
        env.err(
            "pet.configure_life",
            json!({"life_id": life_id, "expected_revision": revision + 1, "grace_hours": 8})
        ),
        "STALE_REVISION"
    );
    for stricter in [
        json!({"grace_hours": 5}),
        json!({"critical_after_hours": 1}),
    ] {
        let mut p = stricter.clone();
        p["life_id"] = life_id.clone();
        p["expected_revision"] = json!(revision);
        assert_eq!(
            env.err("pet.configure_life", p),
            "INVALID_PARAMS",
            "{stricter}"
        );
    }
    for invalid in [
        json!({}),
        json!({"grace_hours": 0}),
        json!({"grace_hours": 49}),
        json!({"decay_percent": 24}),
        json!({"decay_percent": 101}),
    ] {
        let mut p = invalid.clone();
        p["life_id"] = life_id.clone();
        p["expected_revision"] = json!(revision);
        assert_eq!(
            env.err("pet.configure_life", p),
            "INVALID_PARAMS",
            "{invalid}"
        );
    }
    let lenient = env.ok(
        "pet.configure_life",
        json!({"life_id": life_id, "expected_revision": revision,
            "grace_hours": 12, "critical_after_hours": 3, "decay_percent": 50}),
    );
    assert_eq!(lenient["changed"], true);
    let rules = &lenient["lifecycle"]["rules"];
    assert_eq!(rules["grace_ms"], 12 * HOUR);
    assert_eq!(rules["critical_after_ms"], 3 * HOUR);
    assert_eq!(rules["decay_per_hour"], json!([500, 625, 300, 250]));
    assert_eq!(env.state()["life"]["revision"], lenient["life"]["revision"]);
}

#[test]
fn lifecycle_rules_are_the_canonical_ruleset() {
    let env = Env::new();
    let rules = env.ok("lifecycle.rules", json!({}));
    assert!(!env.xdg().exists(), "static rules never open storage");
    assert_eq!(rules.to_string(), codepet_storage::lifecycle_rules_json());
    assert_eq!(rules["id"], "lifecycle-v1");
    assert_eq!(
        rules["steady"]["decay_per_hour"],
        json!([1000, 1250, 600, 500])
    );
    assert_eq!(rules["steady"]["grace_ms"], 21_600_000);
    assert_eq!(rules["playtest"]["grace_ms"], 1_080_000);
    assert_eq!(rules["steady"]["level_xp"], json!([0, 80, 200, 400, 700]));
    assert_eq!(rules["steady"]["stage_min_level"], json!([1, 2, 4]));
    assert_eq!(rules["tick_ms"], 60_000);
    assert_eq!(rules["settle_cap_ms"], 28_800_000);
    assert_eq!(
        env.err("lifecycle.rules", json!({"pace": "steady"})),
        "INVALID_PARAMS"
    );
}

/// Issue #5 wave exit flow for every companion: select → routine → nap → Start → accept,
/// then a fresh process sees one life, one free slot and the right XP. The fourth
/// acceptance evolves the companion once; seeing it is durable.
#[test]
fn roster_flow_keeps_one_life_one_slot_and_exact_xp() {
    let env = Env::new();
    let life_id = env.life_id();
    let level = |xp: u64| match xp {
        0..=79 => 1,
        80..=199 => 2,
        200..=399 => 3,
        400..=699 => 4,
        _ => 5,
    };
    for (n, pet) in codepet_core::species::SPECIES_IDS.iter().enumerate() {
        let n = n as u64 + 1;
        env.ok("pet.select", json!({"life_id": life_id, "pet_id": pet}));
        care(&env, "routine");
        env.elapsed_nap();
        let (task, digest) = env.reviewed("hunger", "artifact");
        let accepted = env.ok(
            "task.accept_artifact",
            json!({"task_id": task, "review_digest": digest, "acknowledged": true,
                "override_reason": "Reviewed the simulation"}),
        );
        let receipt = &accepted["receipt"];
        assert_eq!(receipt["life_id"], life_id);
        assert_eq!(receipt["demo_xp"], 20);
        assert_eq!(receipt["level_before"], level(20 * (n - 1)));
        assert_eq!(receipt["level_after"], level(20 * n));
        let s = env.state();
        let life = &s["life"];
        assert_eq!(life["life_id"], life_id, "{pet}: one life");
        assert_eq!(life["ordinal"], 1);
        assert_eq!(life["appearance"], *pet);
        assert_eq!(life["species"], "cat");
        assert_eq!(life["xp"], 20 * n);
        assert_eq!(life["tasks_accepted"], n);
        assert_eq!(life["level"], level(20 * n));
        assert_eq!(s["active_task"], Value::Null, "{pet}: the slot is free");
        assert_eq!(s["progression"]["demo"]["lifetime_xp"], 20 * n);
        let bar = env.bar_state().unwrap();
        assert_eq!(bar["life"]["appearance"], *pet);
        assert_eq!(bar["life"]["xp"], 20 * n);
        assert_eq!(bar["task"], Value::Null);
        if n == 4 {
            assert_eq!(receipt["evolved_to"], json!([2]));
            assert_eq!(life["stage"], 2);
            assert_eq!(life["stage_key"], "grown");
            let unseen = s["unseen_events"].as_array().unwrap();
            assert_eq!(unseen.len(), 1);
            assert_eq!(unseen[0]["kind"], "evolved");
            assert_eq!(unseen[0]["stage"], 2);
            let seq = unseen[0]["seq"].clone();
            assert_eq!(bar["unseen_events"], json!([{"seq": seq, "stage": 2}]));
            let params = json!({"life_id": life_id, "through_seq": seq});
            let seen = env.ok_with("seen-1", "pet.events_seen", params.clone());
            assert_eq!(seen["changed"], true);
            assert_eq!(
                env.ok_with("seen-1", "pet.events_seen", params.clone()),
                seen
            );
            assert_eq!(env.ok("pet.events_seen", params)["changed"], false);
            assert_eq!(
                env.state()["unseen_events"],
                json!([]),
                "seen survives restart"
            );
            assert_eq!(env.bar_state().unwrap()["unseen_events"], json!([]));
        } else {
            assert_eq!(receipt["evolved_to"], json!([]), "{pet}");
            assert_eq!(s["unseen_events"], json!([]));
        }
    }
    let s = env.state();
    assert_eq!(s["life"]["xp"], 300);
    assert_eq!(s["life"]["level"], 3);
    assert_eq!(s["life"]["stage"], 2);
    assert_eq!(
        s["life"]["next_stage"],
        json!({"stage": 3, "min_level": 4, "xp": 400})
    );
    assert_eq!(s["memorial"]["count"], 0);
}

#[test]
fn ended_lives_are_immutable_through_every_method() {
    let env = Env::new();
    let first = env.life_id();
    let retired = env.retire();
    let entry = retired["memorial"]["latest"].clone();
    env.adopt("penguin", "permadeath", "steady");
    let living = env.life_id();
    for (method, params) in [
        ("pet.select", json!({"life_id": first, "pet_id": "dog"})),
        (
            "pet.rules",
            json!({"life_id": first, "mode": "gentle", "acknowledge_rules": true}),
        ),
        (
            "pet.configure_life",
            json!({"life_id": first, "expected_revision": 1, "grace_hours": 48}),
        ),
        (
            "pet.retire",
            json!({"life_id": first, "confirm_name": "Ginger"}),
        ),
    ] {
        assert_eq!(env.err(method, params), "STALE_REVISION", "{method}");
    }
    // Seeing events of an ended life is harmless and changes nothing.
    env.ok(
        "pet.events_seen",
        json!({"life_id": first, "through_seq": 1_000}),
    );
    let page = env.ok("pet.memorial", json!({"limit": 1}));
    assert_eq!(page["entries"], json!([entry]));
    assert_eq!(page["next_before_ordinal"], Value::Null);
    assert_eq!(env.state()["memorial"]["latest"], entry);
    assert_eq!(env.life_id(), living);
    for bad in [
        json!({"limit": 0}),
        json!({"limit": 21}),
        json!({"before": 1}),
    ] {
        assert_eq!(
            env.err("pet.memorial", bad.clone()),
            "INVALID_PARAMS",
            "{bad}"
        );
    }
}
