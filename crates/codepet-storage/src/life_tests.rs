//! Lives through the Store API: adoption, appearance selection, rules, leniency,
//! retirement, per-life XP and evolution journaling, celebration dedupe and the memorial.
use super::*;
use codepet_core::lifecycle::{Mode, Pace, HOUR_MS};

fn adopt_req(species: &str, mode: Mode, pace: Pace) -> AdoptRequest<'_> {
    AdoptRequest {
        species,
        name: None,
        mode,
        pace,
        acknowledge_rules: true,
    }
}

fn retire_ginger(store: &mut Store) {
    let id = life_id(store);
    store.retire("retire-ginger", &id, "Ginger").unwrap();
}

fn kinds(env: &Env, life: &str) -> Vec<String> {
    let c = env.raw();
    let mut stmt = c
        .prepare("SELECT kind FROM life_events WHERE life_id = ?1 ORDER BY seq")
        .unwrap();
    stmt.query_map([life], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

fn accept_n(store: &mut Store, env: &Env, n: usize) -> Vec<AcceptOutcome> {
    (0..n)
        .map(|i| {
            let (id, digest) = reviewed(
                store,
                env,
                Category::MentalHealth,
                Verification::NotVerified,
            );
            accept(store, &id, &digest, &format!("acc-{i}-{id}"), Some("ok")).unwrap()
        })
        .collect()
}

#[test]
fn first_run_life_is_ginger_unarmed_and_healthy() {
    let env = Env::new();
    let store = env.open();
    let s = store.snapshot().unwrap();
    let life = s.life.unwrap();
    assert_eq!(
        (
            life.name.as_str(),
            life.species.as_str(),
            life.appearance.as_str(),
            life.origin.as_str(),
            life.status.as_str(),
            life.mode.as_str(),
            life.pace.as_str()
        ),
        (
            "Ginger",
            "cat",
            "cat",
            "first_run",
            "healthy",
            "permadeath",
            "steady"
        )
    );
    assert!(!life.armed);
    assert_eq!(
        (life.ordinal, life.xp, life.level, life.stage),
        (1, 0, 1, 1)
    );
    assert_eq!(life.stage_key, "young");
    assert_eq!(
        life.next_stage,
        Some(NextStageView {
            stage: 2,
            min_level: 2,
            xp: 80
        })
    );
    assert_eq!(s.lifecycle.frozen_reason, "unarmed");
    assert_eq!(s.lifecycle.rules.lethal, ["hunger", "thirst"]);
    assert_eq!(s.lifecycle.needs.len(), 4);
    assert!(s.unseen_events.is_empty());
    assert_eq!(s.memorial.count, 0);
    assert_eq!(s.state_version, 1);
}

#[test]
fn rules_json_sha_matches_and_is_canonical() {
    let env = Env::new();
    env.open();
    let (json, sha): (String, String) = env
        .raw()
        .query_row(
            "SELECT rules_json, rules_sha256 FROM rulesets WHERE id = 'lifecycle-v1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(json, lifecycle_rules_json());
    assert_eq!(sha, lifecycle_rules_sha256());
    assert_eq!(sha, sys::sha256_hex(json.as_bytes()));
    assert!(json.len() <= 4096);
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        v["steady"]["decay_per_hour"],
        serde_json::json!([1000, 1250, 600, 500])
    );
    assert_eq!(v["playtest"]["grace_ms"], 1_080_000);
    assert_eq!(
        v["steady"]["lethal"],
        serde_json::json!(["hunger", "thirst"])
    );
    assert_eq!(v["tick_ms"], 60_000);
    // Key order is fixed (sorted), so the hash is stable across builds.
    assert_eq!(serde_json::to_string(&v).unwrap(), json);
}

#[test]
fn care_pause_start_and_select_need_a_living_companion() {
    let env = Env::new();
    let mut store = env.open();
    let t = store.create_task(&env.input(Category::Hunger)).unwrap();
    retire_ginger(&mut store);
    assert!(matches!(
        store.care("c", "food"),
        Err(StorageError::NoActivePet)
    ));
    assert!(matches!(store.sleep("s"), Err(StorageError::NoActivePet)));
    assert!(matches!(store.wake("w"), Err(StorageError::NoActivePet)));
    assert!(matches!(
        store.set_paused(true),
        Err(StorageError::NoActivePet)
    ));
    assert!(matches!(
        store.preflight(&t.task_id, 1),
        Err(StorageError::NoActivePet)
    ));
    assert!(matches!(
        store.select_appearance("sel", &format!("life-{}", "0".repeat(24)), "dog"),
        Err(StorageError::NoActivePet)
    ));
    let s = store.snapshot().unwrap();
    assert!(s.life.is_none());
    assert_eq!(
        s.care.readiness_reasons[0],
        "Adopt a companion to start work"
    );
    assert_eq!(StorageError::NoActivePet.code(), "NO_ACTIVE_PET");
}

#[test]
fn adopt_after_death_level1_xp0_meters7000_armed() {
    let env = Env::with_boot();
    let (mut store, old) = armed(&env);
    accept_n(&mut store, &env, 4);
    let old_xp = store.snapshot().unwrap().life.unwrap().xp;
    assert_eq!(old_xp, 80);
    // Neglect ends life #1.
    let mut n = 0;
    for ms in [8 * HOUR_MS, 3 * HOUR_MS, 6 * HOUR_MS] {
        n += 1;
        env.pass(ms);
        env.write_presence(&format!("bar-adopt-{n:04}"), &old, ms);
        store.snapshot().unwrap();
    }
    assert!(store.snapshot().unwrap().life.is_none());
    let out = store
        .adopt(
            "adopt-1",
            &adopt_req("capybara", Mode::Permadeath, Pace::Steady),
        )
        .unwrap();
    assert!(out.changed);
    let life = out.life.unwrap();
    assert_eq!(
        (
            life.name.as_str(),
            life.species.as_str(),
            life.origin.as_str()
        ),
        ("Yuzu", "capybara", "adopted")
    );
    assert_eq!(
        (
            life.ordinal,
            life.xp,
            life.level,
            life.stage,
            life.tasks_accepted
        ),
        (2, 0, 1, 1, 0)
    );
    assert!(life.armed);
    let s = store.snapshot().unwrap();
    assert_eq!(
        s.meters,
        MetersView {
            hunger: 7000,
            thirst: 7000,
            mental_health: 7000,
            hygiene: 7000
        }
    );
    assert!(s.care.welcome_care_available);
    assert_eq!((s.care.work_sessions, s.care.starts_since_bathroom), (0, 0));
    assert!(s.lifecycle.critical.is_none());
    assert_eq!(s.lifecycle.attended_total_ms, 0);
    assert_eq!(s.memorial.count, 1);
    // Account progression still counts every accepted task; the new life starts at 0.
    assert_eq!(s.progression.demo.lifetime_xp, 80);
    assert_eq!(kinds(&env, &life.life_id), ["born", "armed"]);
    // A second adoption is refused while this life lives.
    assert!(matches!(
        store.adopt("adopt-2", &adopt_req("dog", Mode::Gentle, Pace::Steady)),
        Err(StorageError::InvalidState(_))
    ));
}

#[test]
fn adoption_is_explicit_acknowledged_validated_and_idempotent() {
    let env = Env::new();
    let mut store = env.open();
    assert!(matches!(
        store.adopt("early", &adopt_req("dog", Mode::Permadeath, Pace::Steady)),
        Err(StorageError::InvalidState(_))
    ));
    retire_ginger(&mut store);
    let mut req = adopt_req("dog", Mode::Gentle, Pace::Playtest);
    req.acknowledge_rules = false;
    assert!(matches!(
        store.adopt("no-ack", &req),
        Err(StorageError::AcknowledgementRequired)
    ));
    assert!(matches!(
        store.adopt(
            "bad-species",
            &adopt_req("dragon", Mode::Gentle, Pace::Steady)
        ),
        Err(StorageError::InvalidParams(_))
    ));
    let mut named = adopt_req("dog", Mode::Gentle, Pace::Playtest);
    named.name = Some("   ");
    assert!(matches!(
        store.adopt("blank", &named),
        Err(StorageError::InvalidParams(_))
    ));
    named.name = Some("  Sir \u{202e}Wags\t ");
    let first = store.adopt("adopt-dog", &named).unwrap();
    let life = first.life.as_ref().unwrap();
    assert_eq!(life.name, "Sir Wags");
    assert_eq!(
        (life.mode.as_str(), life.pace.as_str()),
        ("gentle", "playtest")
    );
    let rules = &first.lifecycle.rules;
    assert_eq!(rules.grace_ms, 1_080_000);
    // Replays return the stored outcome; changed params conflict.
    assert_eq!(store.adopt("adopt-dog", &named).unwrap(), first);
    named.name = Some("Other");
    assert!(matches!(
        store.adopt("adopt-dog", &named),
        Err(StorageError::IdempotencyConflict)
    ));
    assert_eq!(env.count("SELECT count(*) FROM lives"), 2);
    // Adoption is refused while a task holds the slot (fixture: no living life + slot).
    let env = Env::new();
    let mut store = env.open();
    let t = store.create_task(&env.input(Category::Hunger)).unwrap();
    retire_ginger(&mut store);
    env.raw()
        .execute(
            "UPDATE tasks SET state = 'review_ready' WHERE id = ?1",
            [&t.task_id],
        )
        .unwrap();
    assert!(matches!(
        store.adopt("busy", &adopt_req("owl", Mode::Permadeath, Pace::Steady)),
        Err(StorageError::InvalidState(_))
    ));
}

#[test]
fn adopt_race_one_life() {
    let env = Env::new();
    {
        let mut store = env.open();
        retire_ginger(&mut store);
    }
    let mut stores: Vec<Store> = (0..6).map(|_| env.open()).collect();
    let barrier = std::sync::Barrier::new(stores.len());
    let results: Vec<bool> = std::thread::scope(|scope| {
        let joins: Vec<_> = stores
            .iter_mut()
            .enumerate()
            .map(|(i, s)| {
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    s.adopt(
                        &format!("race-{i}"),
                        &adopt_req("penguin", Mode::Permadeath, Pace::Steady),
                    )
                    .is_ok()
                })
            })
            .collect();
        joins.into_iter().map(|j| j.join().unwrap()).collect()
    });
    assert_eq!(results.iter().filter(|ok| **ok).count(), 1);
    assert_eq!(
        env.count("SELECT count(*) FROM lives WHERE status IN ('healthy','warning','critical')"),
        1
    );
}

#[test]
fn adopt_fault_before_commit_leaves_the_exact_pre_state() {
    let env = Env::new();
    let mut store = env.open();
    retire_ginger(&mut store);
    let tables = [
        "lives",
        "life_events",
        "life_vitals",
        "pet_state",
        "pet_care",
        "meta",
    ];
    let before: Vec<String> = tables.iter().map(|t| env.dump(t)).collect();
    test_fault::arm(Some("adopt_before_commit"));
    let r = catch_unwind(AssertUnwindSafe(|| {
        store.adopt("crash", &adopt_req("owl", Mode::Permadeath, Pace::Steady))
    }));
    test_fault::arm(None);
    assert!(r.is_err());
    drop(store);
    let after: Vec<String> = tables.iter().map(|t| env.dump(t)).collect();
    assert_eq!(before, after);
    let mut store = env.open();
    store
        .adopt("crash", &adopt_req("owl", Mode::Permadeath, Pace::Steady))
        .unwrap();
    store
        .adopt("crash", &adopt_req("owl", Mode::Permadeath, Pace::Steady))
        .unwrap();
    assert_eq!(env.count("SELECT count(*) FROM lives"), 2);
}

/// Everything selection must leave alone, as comparable text.
fn invariant_state(env: &Env, store: &Store) -> (Value, Vec<String>) {
    let mut v = serde_json::to_value(store.snapshot().unwrap()).unwrap();
    v.as_object_mut().unwrap().remove("state_version");
    let life = v["life"].as_object_mut().unwrap();
    life.remove("appearance");
    life.remove("revision");
    let tables = [
        "pet_state",
        "pet_care",
        "life_vitals",
        "tasks",
        "task_events",
        "demo_xp_rewards",
        "preflight_tokens",
        "acceptances",
    ];
    (v, tables.iter().map(|t| env.dump(t)).collect())
}

#[test]
fn select_preserves_everything() {
    type Setup = fn(&mut Store, &Env);
    let setups: [(&str, Setup); 8] = [
        ("low meters", |_, env| {
            env.raw()
                .execute("UPDATE pet_state SET thirst = 1200, hunger = 0", [])
                .unwrap();
        }),
        ("bathroom due", |_, env| {
            env.raw()
                .execute("UPDATE pet_care SET starts_since_bathroom = 2", [])
                .unwrap();
        }),
        ("partial sleep", |s, env| {
            s.sleep("nap").unwrap();
            env.clock.advance(30_000);
            s.snapshot().unwrap();
        }),
        ("sleep due", |_, env| {
            env.raw()
                .execute("UPDATE pet_care SET rest_required = 1", [])
                .unwrap();
        }),
        ("running task", |s, env| {
            task_in_state(s, env, State::Running);
        }),
        ("review", |s, env| {
            task_in_state(s, env, State::ReviewReady);
        }),
        ("recovery", |s, env| {
            task_in_state(s, env, State::RecoveryRequired);
        }),
        ("critical", |_, env| {
            let c = env.raw();
            c.execute("UPDATE pet_state SET thirst = 0", []).unwrap();
            c.execute(
                "UPDATE life_vitals SET neglect_thirst_ms = 7200000, critical_need = 'thirst',
                 critical_grace_ms = 21600000, critical_elapsed_ms = 60000,
                 critical_since_ms = 5",
                [],
            )
            .unwrap();
            c.execute("UPDATE lives SET status = 'critical'", [])
                .unwrap();
        }),
    ];
    for (what, setup) in setups {
        let env = Env::new();
        let mut store = env.open();
        setup(&mut store, &env);
        store.set_paused(true).unwrap();
        let id = life_id(&store);
        let (before, tables_before) = invariant_state(&env, &store);
        let revision = store.snapshot().unwrap().life.unwrap().revision;
        let out = store
            .select_appearance("pick-axolotl", &id, "axolotl")
            .unwrap();
        assert!(out.changed, "{what}");
        let life = out.life.unwrap();
        assert_eq!(life.appearance, "axolotl", "{what}");
        assert_eq!(life.species, "cat", "{what}");
        assert_eq!(life.revision, revision + 1, "{what}");
        let (after, tables_after) = invariant_state(&env, &store);
        assert_eq!(before, after, "{what}");
        assert_eq!(tables_before, tables_after, "{what}");
        // Same look again: unchanged; replay returns the stored outcome.
        assert!(
            !store
                .select_appearance("again", &id, "axolotl")
                .unwrap()
                .changed
        );
        assert!(
            store
                .select_appearance("pick-axolotl", &id, "axolotl")
                .unwrap()
                .changed
        );
        assert_eq!(
            kinds(&env, &id)
                .iter()
                .filter(|k| *k == "appearance_changed")
                .count(),
            1,
            "{what}"
        );
    }
}

#[test]
fn select_refused_for_stale_unknown_dead_or_pending() {
    let env = Env::new();
    let mut store = env.open();
    let id = life_id(&store);
    assert!(matches!(
        store.select_appearance("s1", &format!("life-{}", "f".repeat(24)), "dog"),
        Err(StorageError::StaleRevision)
    ));
    assert!(matches!(
        store.select_appearance("s2", &id, "dragon"),
        Err(StorageError::InvalidParams(_))
    ));
    env.raw()
        .execute(
            "UPDATE life_vitals SET critical_need = 'thirst', critical_grace_ms = 900000,
             critical_since_ms = 1, death_pending = 1",
            [],
        )
        .unwrap();
    assert!(matches!(
        store.select_appearance("s3", &id, "dog"),
        Err(StorageError::InvalidState(_))
    ));
    env.raw()
        .execute(
            "UPDATE life_vitals SET critical_need = NULL, critical_grace_ms = NULL,
             critical_since_ms = NULL, death_pending = 0",
            [],
        )
        .unwrap();
    store.retire("r", &id, "Ginger").unwrap();
    assert!(matches!(
        store.select_appearance("s4", &id, "dog"),
        Err(StorageError::NoActivePet)
    ));
    // Source check: selection never writes XP rows.
    let src = include_str!("life.rs");
    let start = src.find("pub fn select_appearance").unwrap();
    let end = src[start..].find("pub fn adopt").unwrap() + start;
    assert!(!src[start..end].contains("demo_xp_rewards"));
}

#[test]
fn retire_requires_name_free_slot_not_critical() {
    let env = Env::new();
    let mut store = env.open();
    let id = life_id(&store);
    assert!(matches!(
        store.retire("r1", &id, "ginger"),
        Err(StorageError::InvalidParams(_))
    ));
    let task = task_in_state(&mut store, &env, State::ReviewReady);
    assert!(matches!(
        store.retire("r2", &id, "Ginger"),
        Err(StorageError::InvalidState(_))
    ));
    let digest = store.review(&task).unwrap().review.unwrap().digest;
    store.discard(&task, &digest).unwrap();
    env.raw()
        .execute(
            "UPDATE life_vitals SET critical_need = 'hunger', critical_grace_ms = 900000,
             critical_since_ms = 1",
            [],
        )
        .unwrap();
    env.raw()
        .execute("UPDATE lives SET status = 'critical'", [])
        .unwrap();
    assert!(matches!(
        store.retire("r3", &id, "Ginger"),
        Err(StorageError::InvalidState(_))
    ));
    env.raw()
        .execute(
            "UPDATE life_vitals SET critical_need = NULL, critical_grace_ms = NULL,
             critical_since_ms = NULL",
            [],
        )
        .unwrap();
    env.raw()
        .execute("UPDATE lives SET status = 'healthy'", [])
        .unwrap();
    let out = store.retire("r4", &id, " Ginger ").unwrap();
    assert!(out.life.is_none());
    let m = out.memorial.latest.unwrap();
    assert_eq!(
        (m.end_cause.as_str(), m.name.as_str()),
        ("retired", "Ginger")
    );
    assert_eq!(m.stage_name_key, "cat.young");
    assert_eq!(out.memorial.count, 1);
    assert_eq!(kinds(&env, &id).last().unwrap(), "retired");
    // Retirement is terminal: a second one is refused.
    assert!(matches!(
        store.retire("r5", &id, "Ginger"),
        Err(StorageError::NoActivePet)
    ));
}

#[test]
fn arming_needs_the_acknowledgement() {
    let env = Env::new();
    let mut store = env.open();
    let id = life_id(&store);
    assert!(matches!(
        store.set_rules("a1", &id, Mode::Permadeath, false),
        Err(StorageError::AcknowledgementRequired)
    ));
    let out = store.set_rules("a2", &id, Mode::Permadeath, true).unwrap();
    assert!(out.changed && out.life.as_ref().unwrap().armed);
    assert_eq!(out.lifecycle.frozen_reason, "none");
    assert!(
        !store
            .set_rules("a3", &id, Mode::Permadeath, true)
            .unwrap()
            .changed
    );
    assert_eq!(kinds(&env, &id).iter().filter(|k| *k == "armed").count(), 1);
}

#[test]
fn permadeath_to_gentle_keeps_neglect_and_gentle_to_permadeath_requires_healthy() {
    let env = Env::with_boot();
    let (mut store, id) = armed(&env);
    let mut n = 0;
    let mut feed = |store: &Store, ms: i64| {
        n += 1;
        env.pass(ms);
        env.write_presence(&format!("bar-mode-{n:04}"), &id, ms);
        store.snapshot().unwrap()
    };
    feed(&store, 8 * HOUR_MS);
    let s = feed(&store, 2 * HOUR_MS);
    let neglect = s.lifecycle.needs[1].neglect_ms;
    assert_eq!(neglect, HOUR_MS);
    let out = store
        .set_rules("to-gentle", &id, Mode::Gentle, false)
        .unwrap();
    assert_eq!(out.life.as_ref().unwrap().mode, "gentle");
    assert_eq!(out.lifecycle.needs[1].neglect_ms, neglect);
    assert!(out.lifecycle.needs.iter().all(|n| n.until_end_ms.is_none()));
    // Back to permadeath while not healthy: refused.
    assert!(matches!(
        store.set_rules("to-perma", &id, Mode::Permadeath, true),
        Err(StorageError::InvalidState(_))
    ));
    store.care("routine", "routine").unwrap();
    assert!(matches!(
        store.set_rules("to-perma-2", &id, Mode::Permadeath, false),
        Err(StorageError::AcknowledgementRequired)
    ));
    let out = store
        .set_rules("to-perma-3", &id, Mode::Permadeath, true)
        .unwrap();
    assert_eq!(out.life.unwrap().mode, "permadeath");
    assert_eq!(
        kinds(&env, &id)
            .iter()
            .filter(|k| *k == "mode_changed")
            .count(),
        2
    );
}

#[test]
fn configure_stricter_refused_for_each_field() {
    let env = Env::new();
    let mut store = env.open();
    let id = life_id(&store);
    let rev = store.snapshot().unwrap().life.unwrap().revision;
    let cases = [
        Leniency {
            grace_hours: Some(5),
            ..Leniency::default()
        },
        Leniency {
            critical_after_hours: Some(1),
            ..Leniency::default()
        },
        Leniency {
            decay_percent: Some(101),
            ..Leniency::default()
        },
        Leniency {
            decay_percent: Some(24),
            ..Leniency::default()
        },
        Leniency::default(),
    ];
    for (i, l) in cases.iter().enumerate() {
        assert!(
            matches!(
                store.configure_life(&format!("c{i}"), &id, rev, l),
                Err(StorageError::InvalidParams(_))
            ),
            "{l:?}"
        );
    }
    assert!(matches!(
        store.configure_life(
            "stale",
            &id,
            rev + 5,
            &Leniency {
                grace_hours: Some(7),
                ..Leniency::default()
            }
        ),
        Err(StorageError::StaleRevision)
    ));
    // Raising critical_after above grace raises grace too (still leniency-only).
    let out = store
        .configure_life(
            "lenient",
            &id,
            rev,
            &Leniency {
                critical_after_hours: Some(8),
                decay_percent: Some(50),
                ..Leniency::default()
            },
        )
        .unwrap();
    let r = &out.lifecycle.rules;
    assert_eq!(
        (r.critical_after_ms, r.grace_ms, r.decay_per_hour),
        (8 * HOUR_MS, 8 * HOUR_MS, [500, 625, 300, 250])
    );
    assert_eq!(out.life.unwrap().revision, rev + 1);
    // The new rules are what decay uses: they come back from storage after a reopen.
    let s = env.open().snapshot().unwrap();
    assert_eq!(s.lifecycle.rules.decay_per_hour, [500, 625, 300, 250]);
}

#[test]
fn configure_lenient_extends_running_grace() {
    let env = Env::new();
    let mut store = env.open();
    let id = life_id(&store);
    let c = env.raw();
    c.execute("UPDATE pet_state SET thirst = 0", []).unwrap();
    c.execute(
        "UPDATE life_vitals SET neglect_thirst_ms = 7200000, critical_need = 'thirst',
         critical_grace_ms = 21600000, critical_elapsed_ms = 3600000, critical_since_ms = 5",
        [],
    )
    .unwrap();
    c.execute("UPDATE lives SET status = 'critical'", [])
        .unwrap();
    let rev = store.snapshot().unwrap().life.unwrap().revision;
    let out = store
        .configure_life(
            "grace",
            &id,
            rev,
            &Leniency {
                grace_hours: Some(10),
                ..Leniency::default()
            },
        )
        .unwrap();
    let crit = out.lifecycle.critical.unwrap();
    assert_eq!(
        (crit.grace_ms, crit.elapsed_ms, crit.remaining_ms),
        (10 * HOUR_MS, HOUR_MS, 9 * HOUR_MS)
    );
    // Pending with a free slot: the end is final before the body runs, so no leniency can
    // undo it; the refusal says the companion passed away and the end stays committed.
    c.execute("UPDATE life_vitals SET death_pending = 1", [])
        .unwrap();
    assert!(matches!(
        store.configure_life(
            "grace-2",
            &id,
            rev + 1,
            &Leniency {
                grace_hours: Some(12),
                ..Leniency::default()
            }
        ),
        Err(StorageError::PassedAway)
    ));
    assert!(store.snapshot().unwrap().life.is_none());
}

#[test]
fn pause_is_journaled_on_the_living_companion() {
    let env = Env::new();
    let mut store = env.open();
    let id = life_id(&store);
    let out = store.set_paused(true).unwrap();
    assert_eq!(out.life_id.as_deref(), Some(id.as_str()));
    store.set_paused(true).unwrap();
    store.set_paused(false).unwrap();
    assert_eq!(kinds(&env, &id), ["born", "paused", "resumed"]);
    assert_eq!(store.snapshot().unwrap().lifecycle.frozen_reason, "unarmed");
}

#[test]
fn evolution_is_journaled_once_and_celebrated_until_seen() {
    let env = Env::new();
    let mut store = env.open();
    let id = life_id(&store);
    let outs = accept_n(&mut store, &env, 4);
    assert!(outs[..3].iter().all(|o| o.receipt.evolved_to.is_empty()));
    let last = &outs[3].receipt;
    assert_eq!(last.evolved_to, [2]);
    assert_eq!((last.level_before, last.level_after), (1, 2));
    assert_eq!(last.life_id.as_deref(), Some(id.as_str()));
    let s = store.snapshot().unwrap();
    let life = s.life.unwrap();
    assert_eq!(
        (life.xp, life.level, life.stage, life.stage_key.as_str()),
        (80, 2, 2, "grown")
    );
    assert_eq!(life.tasks_accepted, 4);
    assert_eq!(s.unseen_events.len(), 1);
    let unseen = &s.unseen_events[0];
    assert_eq!((unseen.kind.as_str(), unseen.stage), ("evolved", 2));
    // A replay or a duplicate acceptance under another request id adds nothing.
    let task = &last.task_id;
    let digest = store.review(task).unwrap().review.unwrap().digest;
    let dup = accept(&mut store, task, &digest, "other-id", Some("ok")).unwrap();
    assert!(dup.duplicate);
    assert_eq!(dup.receipt, *last);
    assert_eq!(env.count("SELECT count(*) FROM demo_xp_rewards"), 4);
    assert_eq!(
        env.count("SELECT count(*) FROM life_events WHERE kind = 'evolved'"),
        1
    );
    // events_seen persists across a reopen and is idempotent.
    let seen = store.events_seen("seen-1", &id, unseen.seq).unwrap();
    assert!(seen.changed);
    assert!(
        !store
            .events_seen("seen-2", &id, unseen.seq)
            .unwrap()
            .changed
    );
    drop(store);
    let store = env.open();
    assert!(store.snapshot().unwrap().unseen_events.is_empty());
    assert!(env
        .raw()
        .execute(
            "UPDATE life_events SET seen_at_ms = 1 WHERE kind = 'evolved'",
            []
        )
        .is_err());
}

#[test]
fn evolve_fault_before_commit_leaves_no_xp_and_no_event() {
    let env = Env::new();
    let mut store = env.open();
    accept_n(&mut store, &env, 3);
    let (task, digest) = reviewed(
        &mut store,
        &env,
        Category::Hunger,
        Verification::NotVerified,
    );
    test_fault::arm(Some("evolve_before_commit"));
    let r = catch_unwind(AssertUnwindSafe(|| {
        accept(&mut store, &task, &digest, "evolve", Some("ok"))
    }));
    test_fault::arm(None);
    assert!(r.is_err());
    drop(store);
    assert_eq!(env.count("SELECT count(*) FROM demo_xp_rewards"), 3);
    assert_eq!(
        env.count("SELECT count(*) FROM life_events WHERE kind = 'evolved'"),
        0
    );
    let mut store = env.open();
    let out = accept(&mut store, &task, &digest, "evolve", Some("ok")).unwrap();
    assert_eq!(out.receipt.evolved_to, [2]);
}

#[test]
fn stages_crossed_two_at_once_journal_two_events() {
    let env = Env::new();
    let store = env.open();
    let id = life_id(&store);
    let (task, _) = {
        let mut s = env.open();
        reviewed(&mut s, &env, Category::Hunger, Verification::NotVerified)
    };
    let c = store.conn();
    assert!(life::record_evolution(c, &id, &task, 60, 400, T0).unwrap());
    assert!(!life::record_evolution(c, &id, &task, 60, 400, T0).unwrap());
    assert_eq!(life::evolved_by_task(c, &task).unwrap(), [2, 3]);
}

#[test]
fn xp_reversal_derives_lower_stage_and_keeps_the_journal() {
    let env = Env::new();
    let mut store = env.open();
    let outs = accept_n(&mut store, &env, 4);
    assert_eq!(store.snapshot().unwrap().life.unwrap().stage, 2);
    let c = env.raw();
    c.execute_batch("BEGIN").unwrap();
    c.execute(
        "DELETE FROM demo_xp_rewards WHERE task_id = ?1",
        [&outs[3].receipt.task_id],
    )
    .unwrap();
    let xp: i64 = c
        .query_row("SELECT SUM(amount) FROM demo_xp_rewards", [], |r| r.get(0))
        .unwrap();
    assert_eq!(xp, 60);
    c.execute_batch("COMMIT").unwrap();
    let life = store.snapshot().unwrap().life.unwrap();
    assert_eq!((life.xp, life.level, life.stage), (60, 1, 1));
    assert_eq!(
        env.count("SELECT count(*) FROM life_events WHERE kind = 'evolved'"),
        1
    );
}

#[test]
fn old_task_accept_after_adoption_never_credits_the_new_life() {
    let env = Env::new();
    let mut store = env.open();
    let (task, digest) = reviewed(
        &mut store,
        &env,
        Category::Hunger,
        Verification::NotVerified,
    );
    // Fixture: the performing life has ended (a restored DB), and a new life lives.
    let dead = format!("life-{}", "d".repeat(24));
    let summary = serde_json::json!({
        "life_id": dead, "name": "Old", "species": "dog", "appearance": "dog", "ordinal": 9,
        "mode": "permadeath", "pace": "steady", "final_level": 1, "final_stage": 1,
        "stage_name_key": "dog.young", "life_xp": 0, "tasks_accepted": 0, "adopted_at_ms": 0,
        "ended_at_ms": 1, "end_cause": "neglect:thirst",
        "final_meters": {"hunger": 0, "thirst": 0, "mental_health": 0, "hygiene": 0},
        "attended_ms": 0
    });
    let c = env.raw();
    c.execute(
        "INSERT INTO lives (life_id, ordinal, name, species, appearance, origin, status, mode,
            pace, ruleset, decay_hunger, decay_thirst, decay_mental_health, decay_hygiene,
            full_hold_ms, warning_at, critical_after_ms, grace_ms, adopted_at_ms, ended_at_ms,
            end_cause, final_summary)
         VALUES (?1, 9, 'Old', 'dog', 'dog', 'adopted', 'dead', 'permadeath', 'steady',
            'lifecycle-v1', 1000, 1250, 600, 500, 3600000, 3000, 7200000, 21600000, 0, 1,
            'neglect:thirst', ?2)",
        rusqlite::params![dead, summary.to_string()],
    )
    .unwrap();
    c.execute(
        "UPDATE tasks SET life_id = ?1 WHERE id = ?2",
        rusqlite::params![dead, task],
    )
    .unwrap();
    let current = life_id(&store);
    let out = accept(&mut store, &task, &digest, "late", Some("ok")).unwrap();
    assert_eq!(out.receipt.demo_xp, 0);
    assert_eq!(out.receipt.xp_withheld.as_deref(), Some("life_ended"));
    assert_eq!(out.receipt.life_id.as_deref(), Some(dead.as_str()));
    assert_eq!(store.task(&task).unwrap().receipt.unwrap(), out.receipt);
    let life = store.snapshot().unwrap().life.unwrap();
    assert_eq!((life.life_id.as_str(), life.xp), (current.as_str(), 0));
    assert_eq!(env.count("SELECT count(*) FROM demo_xp_rewards"), 0);
}

#[test]
fn xp_binds_to_the_task_life_not_the_active_one() {
    let env = Env::new();
    let mut store = env.open();
    let ginger = life_id(&store);
    accept_n(&mut store, &env, 2);
    retire_ginger(&mut store);
    store
        .adopt("adopt", &adopt_req("robot", Mode::Gentle, Pace::Steady))
        .unwrap();
    let bolt = life_id(&store);
    accept_n(&mut store, &env, 1);
    assert_eq!(life::life_xp(store.conn(), &ginger).unwrap(), (40, 2));
    assert_eq!(life::life_xp(store.conn(), &bolt).unwrap(), (20, 1));
    let s = store.snapshot().unwrap();
    assert_eq!(s.life.unwrap().xp, 20);
    assert_eq!(s.progression.demo.lifetime_xp, 60);
    assert_eq!(s.memorial.latest.unwrap().life_xp, 40);
}

#[test]
fn memorial_pages_newest_first() {
    let env = Env::new();
    let mut store = env.open();
    retire_ginger(&mut store);
    let species = ["dog", "owl", "frog", "snake", "turtle"];
    for (i, sp) in species.iter().enumerate() {
        let out = store
            .adopt(&format!("a{i}"), &adopt_req(sp, Mode::Gentle, Pace::Steady))
            .unwrap();
        let life = out.life.unwrap();
        store
            .retire(&format!("r{i}"), &life.life_id, &life.name)
            .unwrap();
    }
    let page = store.memorial(None, Some(4)).unwrap();
    let ordinals: Vec<u32> = page.entries.iter().map(|e| e.ordinal).collect();
    assert_eq!(ordinals, [6, 5, 4, 3]);
    assert_eq!(page.next_before_ordinal, Some(3));
    let rest = store.memorial(page.next_before_ordinal, Some(4)).unwrap();
    let ordinals: Vec<u32> = rest.entries.iter().map(|e| e.ordinal).collect();
    assert_eq!(ordinals, [2, 1]);
    assert_eq!(rest.next_before_ordinal, None);
    assert!(matches!(
        store.memorial(None, Some(21)),
        Err(StorageError::InvalidParams(_))
    ));
    assert!(matches!(
        store.memorial(None, Some(0)),
        Err(StorageError::InvalidParams(_))
    ));
    assert_eq!(store.memorial(None, None).unwrap().entries.len(), 6);
}

#[test]
fn v2_era_receipts_replay_with_serde_defaults() {
    let receipt = serde_json::json!({"task_id":"t","category":"hunger","amount_hundredths":0,
        "verification":"not_verified","override_reason":null,"request_id":"r",
        "accepted_at_ms":1,"demo_xp":20,"real_xp":0,"progression_track":"bugfix"});
    let accept: AcceptOutcome =
        serde_json::from_value(serde_json::json!({"receipt": receipt, "duplicate": false}))
            .unwrap();
    assert_eq!(accept.receipt.life_id, None);
    assert_eq!(
        (accept.receipt.level_before, accept.receipt.level_after),
        (0, 0)
    );
    assert!(accept.receipt.evolved_to.is_empty());
    assert_eq!(accept.receipt.xp_withheld, None);
    let care: CareOutcome = serde_json::from_value(serde_json::json!({
        "request_id":"c","action":"food","changed":true,
        "meters":{"hunger":10000,"thirst":7000,"mental_health":7000,"hygiene":7000},
        "care":{"revision":2,"bathroom_due":false,"starts_since_bathroom":0,"sleeping":false,
            "sleep_due":false,"sleep_remaining_ms":120000,"sleep_started_at_ms":null,
            "work_sessions":0,"execution_ms":0,"ready_to_work":false,
            "readiness_reasons":["Needs water"],"welcome_care_available":true}}))
    .unwrap();
    assert!(care.lifecycle.is_none());
    let pause: PauseView = serde_json::from_value(serde_json::json!({"paused": true})).unwrap();
    assert_eq!(pause.life_id, None);
    // And a stored v2-era care receipt replays through the Store.
    let env = Env::new();
    let mut store = env.open();
    let stored = serde_json::to_string(&care)
        .unwrap()
        .replace(",\"lifecycle\":null", "");
    assert!(!stored.contains("lifecycle"));
    env.raw()
        .execute(
            "INSERT INTO request_results VALUES ('c', 'pet.care', ?1, ?2, 1)",
            rusqlite::params![sys::sha256_hex(b"food"), stored],
        )
        .unwrap();
    assert_eq!(store.care("c", "food").unwrap(), care);
}

#[test]
fn xp_view_output_is_byte_identical_to_v0_1() {
    fn old(xp: u32) -> (u8, Option<u32>) {
        let thresholds = [0, 80, 200, 400, 700];
        let level = thresholds.iter().filter(|&&t| xp >= t).count() as u8;
        (level, thresholds.get(usize::from(level)).copied())
    }
    let rules = &codepet_core::lifecycle::RULES_V1.steady;
    for xp in [
        0, 1, 79, 80, 81, 199, 200, 399, 400, 401, 699, 700, 701, 10_000,
    ] {
        let level = codepet_core::evolution::level_for_xp(xp, rules);
        assert_eq!(
            (level, codepet_core::evolution::next_level_xp(level, rules)),
            old(xp),
            "{xp}"
        );
    }
    let env = Env::new();
    let mut store = env.open();
    accept_n(&mut store, &env, 5);
    let v = serde_json::to_value(store.snapshot().unwrap().progression).unwrap();
    assert_eq!(
        v["demo"],
        serde_json::json!({"lifetime_xp": 100, "level": 2, "next_level_xp": 200,
            "mastery": {"bugfix": 0, "quality_performance": 0,
                "learning_specification": 100, "cleanup": 0}})
    );
}

#[test]
fn worst_case_snapshot_and_outcomes_fit_one_frame() {
    let env = Env::new();
    let mut store = env.open();
    retire_ginger(&mut store);
    let long = "\u{1F43E}".repeat(24);
    assert_eq!(long.len(), 96);
    for i in 0..2 {
        let mut req = adopt_req("red_panda", Mode::Permadeath, Pace::Playtest);
        req.name = Some(&long);
        let out = store.adopt(&format!("long-{i}"), &req).unwrap();
        fits_frame("pet.adopt", &out);
        if i == 0 {
            let life = out.life.unwrap();
            let out = store.retire("long-retire", &life.life_id, &long).unwrap();
            fits_frame("pet.retire", &out);
        }
    }
    let reason = "\"".repeat(MAX_OVERRIDE_REASON_BYTES);
    for i in 0..RECENT_TASKS {
        let (id, digest) = max_review(&mut store, &env);
        let out = accept(
            &mut store,
            &id,
            &digest,
            &format!("{i:0>128}"),
            Some(&reason),
        )
        .unwrap();
        fits_frame("task.accept_artifact", &out);
        env.clock.advance(1);
    }
    max_review(&mut store, &env);
    let snapshot = store.snapshot().unwrap();
    assert!(snapshot.memorial.latest.is_some());
    fits_frame("state.get", &snapshot);
    fits_frame("pet.memorial", &store.memorial(None, None).unwrap());
}
