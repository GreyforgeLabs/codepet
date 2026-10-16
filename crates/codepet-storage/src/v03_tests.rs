//! WP1 (SPEC-v0.3 §6.2, §6.4, §7): `pet.start`, `pet.rename`, the Hardcore switch through
//! `pet.rules`, the treat jar and `treats.give`, On/Off, background helpers and "Forget
//! coding history". Default build (no `legacy-tasks`).
use super::*;
use crate::treats::{self, CapCheck, Earned};
use codepet_core::lifecycle::{Mode, HOUR_MS};
use codepet_core::species::default_name;

const DAY: i64 = 24 * HOUR_MS;

fn key(tag: &str, i: u64) -> String {
    sys::sha256_hex(format!("{tag}:{i}").as_bytes())[..32].to_owned()
}

fn journal(env: &Env, life: &str) -> Vec<(String, Option<String>, String, Option<String>)> {
    let c = env.raw();
    let mut stmt = c
        .prepare(
            "SELECT kind, dedup_key, data, request_id FROM life_events WHERE life_id = ?1
             ORDER BY seq",
        )
        .unwrap();
    stmt.query_map([life], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

fn kinds(env: &Env, life: &str) -> Vec<String> {
    journal(env, life).into_iter().map(|e| e.0).collect()
}

fn start(
    life_id: Option<&str>,
    species: &str,
    name: Option<&str>,
    mode: Mode,
) -> StartRequest<'static> {
    // The tests only pass literals or leaked test strings.
    let leak = |s: &str| -> &'static str { Box::leak(s.to_owned().into_boxed_str()) };
    StartRequest {
        life_id: life_id.map(leak),
        species: leak(species),
        name: name.map(leak),
        mode,
        acknowledge_rules: true,
    }
}

/// Drops `n` treats into the living life's jar, each from its own turn, spaced so the caps
/// never apply (`at` is explicit, independent of the store clock).
fn seed(store: &Store, n: u64, from: u64) {
    let life = life_id(store);
    for i in from..from + n {
        let at = T0 + i64::try_from(i).unwrap() * 31 * 60_000;
        let got = treats::earn(
            store.conn(),
            &life,
            "claude-code",
            &key("s", i),
            &key("t", i),
            at,
        )
        .unwrap();
        assert_eq!(got, Earned::Treat, "seed {i}");
    }
}

fn jar(store: &Store) -> (u32, bool) {
    let s = store.snapshot().unwrap();
    (s.treats.jar, s.treats.full)
}

// ---- pet.start --------------------------------------------------------------------------

#[test]
fn pet_start_arms_the_owners_unarmed_goldfish_gentle_and_spends_the_welcome_routine() {
    let env = Env::with_boot();
    let mut store = env.open();
    let id = life_id(&store);
    store.select_appearance("look", &id, "goldfish").unwrap();
    let before = store.snapshot().unwrap();
    let ginger = before.life.clone().unwrap();
    assert_eq!(
        (
            ginger.mode.as_str(),
            ginger.armed,
            ginger.appearance.as_str()
        ),
        ("permadeath", false, "goldfish")
    );
    assert!(before.care.welcome_care_available);

    let req = start(Some(&id), "goldfish", Some("Ginger"), Mode::Gentle);
    let s = store.start_pet("lets-go", &req).unwrap();
    let l = s.life.clone().unwrap();
    assert_eq!(
        (
            l.life_id.as_str(),
            l.name.as_str(),
            l.appearance.as_str(),
            l.mode.as_str(),
            l.armed,
            l.ordinal
        ),
        (id.as_str(), "Ginger", "goldfish", "gentle", true, 1)
    );
    assert_eq!(
        s.meters,
        MetersView {
            hunger: 10000,
            thirst: 10000,
            mental_health: 10000,
            hygiene: 10000
        },
        "the welcome routine filled every need"
    );
    assert!(!s.care.welcome_care_available);
    assert_eq!(s.lifecycle.frozen_reason, "none");
    assert!(s.state_version > before.state_version);
    let k = kinds(&env, &id);
    assert_eq!(&k[k.len() - 2..], ["armed", "mode_changed"]);
    let journal_len = k.len();

    // A replay of the same click is the same success, never ALREADY_ARMED.
    let again = store.start_pet("lets-go", &req).unwrap();
    assert_eq!(again.life, s.life);
    assert_eq!(kinds(&env, &id).len(), journal_len);
    // A new click on an armed life: ALREADY_ARMED (the UI just opens Home).
    assert!(matches!(
        store.start_pet("lets-go-2", &req),
        Err(StorageError::AlreadyArmed)
    ));
    // The same id with other params is a conflict.
    let other = start(Some(&id), "cat", Some("Ginger"), Mode::Gentle);
    assert!(matches!(
        store.start_pet("lets-go", &other),
        Err(StorageError::IdempotencyConflict)
    ));
}

#[test]
fn pet_start_on_an_existing_life_changes_the_look_and_keeps_or_sets_the_name() {
    let env = Env::with_boot();
    let mut store = env.open();
    let id = life_id(&store);
    let s = store
        .start_pet("go", &start(None, "owl", None, Mode::Permadeath))
        .unwrap();
    let l = s.life.unwrap();
    assert_eq!(
        (
            l.name.as_str(),
            l.species.as_str(),
            l.appearance.as_str(),
            l.mode.as_str()
        ),
        ("Ginger", "cat", "owl", "permadeath"),
        "an existing life keeps its name and species; the look follows"
    );
    let k = kinds(&env, &id);
    assert!(k.contains(&"appearance_changed".to_owned()));
    assert_eq!(
        k.last().unwrap(),
        "armed",
        "no mode change: still permadeath"
    );

    let env = Env::with_boot();
    let mut store = env.open();
    let s = store
        .start_pet(
            "go",
            &start(None, "cat", Some("  Mr   Whiskers "), Mode::Gentle),
        )
        .unwrap();
    assert_eq!(s.life.unwrap().name, "Mr Whiskers");
}

#[test]
fn pet_start_adopts_when_no_life_is_living() {
    let env = Env::with_boot();
    let mut store = env.open();
    let old = life_id(&store);
    store.retire("bye", &old, "Ginger").unwrap();
    assert!(matches!(
        store.start_pet("x", &start(Some(&old), "dog", None, Mode::Gentle)),
        Err(StorageError::LifeNotFound)
    ));
    let s = store
        .start_pet("adopt", &start(None, "dog", None, Mode::Gentle))
        .unwrap();
    let l = s.life.unwrap();
    assert_eq!(
        (
            l.name.as_str(),
            l.species.as_str(),
            l.origin.as_str(),
            l.mode.as_str(),
            l.pace.as_str(),
            l.armed,
            l.ordinal,
            l.xp
        ),
        (
            default_name("dog").unwrap(),
            "dog",
            "adopted",
            "gentle",
            "steady",
            true,
            2,
            0
        )
    );
    assert_eq!(
        s.meters.hunger, 10000,
        "welcome routine spent on the new pet"
    );
    assert!(!s.care.welcome_care_available);
    assert_eq!(&kinds(&env, &l.life_id)[..2], ["born", "armed"]);
    // An adoption receipt is kept like pet.adopt's.
    assert_eq!(
        env.count("SELECT count(*) FROM request_results WHERE method = 'pet.start'"),
        1
    );
}

#[test]
fn pet_start_refusals_change_nothing() {
    let env = Env::with_boot();
    let mut store = env.open();
    let id = life_id(&store);
    let version = store.snapshot().unwrap().state_version;
    let mut no_ack = start(None, "cat", None, Mode::Gentle);
    no_ack.acknowledge_rules = false;
    assert!(matches!(
        store.start_pet("a", &no_ack),
        Err(StorageError::AcknowledgementRequired)
    ));
    assert!(matches!(
        store.start_pet(
            "b",
            &start(
                Some("life-000000000000000000000000"),
                "cat",
                None,
                Mode::Gentle
            )
        ),
        Err(StorageError::LifeNotFound)
    ));
    let long = "\u{1F431}".repeat(32);
    assert_eq!(long.len(), 128);
    assert!(matches!(
        store.start_pet("c", &start(None, "cat", Some(&long), Mode::Gentle)),
        Err(StorageError::NameInvalid(_))
    ));
    assert!(matches!(
        store.start_pet("d", &start(None, "unicorn", None, Mode::Gentle)),
        Err(StorageError::InvalidParams(_))
    ));
    // Hardcore only while healthy (pet.rules semantics).
    env.raw()
        .execute("UPDATE lives SET status = 'warning'", [])
        .unwrap();
    assert!(matches!(
        store.start_pet("e", &start(None, "cat", None, Mode::Permadeath)),
        Err(StorageError::InvalidState(_))
    ));
    env.raw()
        .execute("UPDATE lives SET status = 'healthy'", [])
        .unwrap();
    // Off: INVALID_STATE "CodePet is off".
    store.set_enabled("off", false).unwrap();
    let err = store
        .start_pet("f", &start(None, "cat", None, Mode::Gentle))
        .unwrap_err();
    assert!(matches!(err, StorageError::Off));
    assert_eq!(
        (err.code(), err.to_string()),
        ("INVALID_STATE", "CodePet is off".to_owned())
    );
    let s = store.snapshot().unwrap();
    let l = s.life.unwrap();
    assert!(!l.armed && l.name == "Ginger" && l.life_id == id);
    assert_eq!(kinds(&env, &id), ["born"]);
    assert_eq!(
        s.state_version,
        version + 1,
        "only the Off switch committed"
    );
}

#[test]
fn pet_start_while_asleep_keeps_the_welcome_routine_for_later() {
    let env = Env::with_boot();
    let mut store = env.open();
    store.sleep("nap").unwrap();
    let s = store
        .start_pet("go", &start(None, "cat", None, Mode::Gentle))
        .unwrap();
    assert!(s.life.unwrap().armed);
    assert!(s.care.sleeping);
    assert!(s.care.welcome_care_available);
    assert_eq!(s.meters.hunger, 7000);
}

// ---- pet.rename -------------------------------------------------------------------------

#[test]
fn pet_rename_validates_in_rust_and_is_cosmetic() {
    let env = Env::with_boot();
    let mut store = env.open();
    let id = life_id(&store);
    let journal_len = kinds(&env, &id).len();
    let s = store.rename("r1", &id, "  Captain \u{7}Fluff ").unwrap();
    assert_eq!(s.life.unwrap().name, "Captain Fluff");
    let max = "\u{1F431}".repeat(24);
    assert_eq!(max.len(), 96);
    assert_eq!(
        store.rename("r2", &id, &max).unwrap().life.unwrap().name,
        max
    );
    for bad in [
        "\u{1F431}".repeat(32),
        "x".repeat(25),
        " \t ".to_owned(),
        "\u{202e}\u{7}".to_owned(),
    ] {
        assert!(
            matches!(
                store.rename("bad", &id, &bad),
                Err(StorageError::NameInvalid(_))
            ),
            "{bad:?}"
        );
    }
    // The bar export follows at once; the journal has no rename kind.
    store.export_bar_state().unwrap();
    let bar: serde_json::Value =
        serde_json::from_slice(&fs::read(env.paths.bar_state_path()).unwrap()).unwrap();
    assert_eq!(bar["life"]["name"], max);
    assert_eq!(kinds(&env, &id).len(), journal_len);
    // Allowed while Off; another life id is NOT_FOUND; no life is NO_ACTIVE_PET.
    store.set_enabled("off", false).unwrap();
    assert_eq!(
        store
            .rename("r3", &id, "Ginger")
            .unwrap()
            .life
            .unwrap()
            .name,
        "Ginger"
    );
    assert!(matches!(
        store.rename("r4", "life-000000000000000000000000", "Bob"),
        Err(StorageError::LifeNotFound)
    ));
    store.set_enabled("on", true).unwrap();
    store.retire("bye", &id, "Ginger").unwrap();
    assert!(matches!(
        store.rename("r5", &id, "Bob"),
        Err(StorageError::NoActivePet)
    ));
    // A replay answers with today's snapshot.
    assert!(store.rename("r3", &id, "Ginger").unwrap().life.is_none());
}

// ---- Hardcore through pet.rules (Help) ---------------------------------------------------

#[test]
fn hardcore_switch_uses_pet_rules_semantics() {
    let env = Env::with_boot();
    let mut store = env.open();
    let id = life_id(&store);
    store
        .start_pet("go", &start(None, "cat", None, Mode::Gentle))
        .unwrap();
    // gentle -> permadeath needs the acknowledgement (the UI's typed name) ...
    assert!(matches!(
        store.set_rules("h1", &id, Mode::Permadeath, false),
        Err(StorageError::AcknowledgementRequired)
    ));
    // ... and a healthy pet: INVALID_STATE -> "Make {name} happy first."
    env.raw()
        .execute("UPDATE lives SET status = 'warning'", [])
        .unwrap();
    assert!(matches!(
        store.set_rules("h2", &id, Mode::Permadeath, true),
        Err(StorageError::InvalidState(_))
    ));
    env.raw()
        .execute("UPDATE lives SET status = 'healthy'", [])
        .unwrap();
    let on = store.set_rules("h3", &id, Mode::Permadeath, true).unwrap();
    assert_eq!(on.life.unwrap().mode, "permadeath");
    // permadeath -> gentle is one tap, any time, no acknowledgement.
    env.raw()
        .execute("UPDATE lives SET status = 'warning'", [])
        .unwrap();
    let off = store.set_rules("h4", &id, Mode::Gentle, false).unwrap();
    assert_eq!(off.life.unwrap().mode, "gentle");
    // Refused while Off.
    store.set_enabled("off", false).unwrap();
    assert!(matches!(
        store.set_rules("h5", &id, Mode::Permadeath, true),
        Err(StorageError::Off)
    ));
}

// ---- the treat jar ----------------------------------------------------------------------

#[test]
fn caps_follow_the_contract_vectors_in_the_pure_rule_and_in_storage() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/v0.3/contracts/agent_vectors.json"
    ))
    .unwrap();
    let cases = vectors["caps"]["vectors"].as_array().unwrap();
    assert!(cases.len() >= 8);
    let ms = |v: &serde_json::Value| -> Vec<i64> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_i64().unwrap())
            .collect()
    };
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let at = case["at"].as_i64().unwrap();
        let jar_n = u32::try_from(case["jar"].as_u64().unwrap()).unwrap();
        let session = ms(&case["session_earned"]);
        let all = ms(&case["all_earned"]);
        let expect = case["expect"].as_str().unwrap();
        assert_eq!(
            treats::caps_allow(at, jar_n, &session, &all).as_str(),
            expect,
            "{name}"
        );

        // The same verdict from durable rows: the session's earns under key S, the others
        // under their own sessions (all given), plus `jar` unspent forgotten rows.
        let env = Env::new();
        let store = env.open();
        let life = life_id(&store);
        let c = store.conn();
        let mut session_left = session.clone();
        for (i, t) in all.iter().enumerate() {
            let s = match session_left.iter().position(|x| x == t) {
                Some(p) => {
                    session_left.remove(p);
                    key("S", 0)
                }
                None => key("other", i as u64),
            };
            c.execute(
                "INSERT INTO activity_turns (session_key, turn_key, client, life_id,
                    earned_at_ms, given_at_ms) VALUES (?1, ?2, 'codex', ?3, ?4, 1)",
                rusqlite::params![s, key("turn", i as u64), life, t],
            )
            .unwrap();
        }
        assert!(
            session_left.is_empty(),
            "{name}: session earns are a subset"
        );
        for j in 0..jar_n {
            c.execute(
                "INSERT INTO activity_turns (session_key, turn_key, client, life_id,
                    earned_at_ms) VALUES (?1, ?2, 'codex', ?3, 0)",
                rusqlite::params![key("jar", u64::from(j)), key("jt", u64::from(j)), life],
            )
            .unwrap();
        }
        assert_eq!(
            treats::check(c, &life, &key("S", 0), at).unwrap().as_str(),
            expect,
            "{name} (storage)"
        );
    }
}

#[test]
fn earning_is_once_per_turn_capped_and_bounded_by_the_jar() {
    let env = Env::new();
    let store = env.open();
    let life = life_id(&store);
    let c = store.conn();
    let earn = |s: u64, t: u64, at: i64| {
        treats::earn(c, &life, "opencode", &key("s", s), &key("t", t), at).unwrap()
    };
    // X1: the same (session, turn) earns once.
    assert_eq!(earn(1, 1, T0), Earned::Treat);
    assert_eq!(earn(1, 1, T0 + DAY), Earned::Duplicate);
    // One per session per 30 s; another session may earn at once.
    assert_eq!(
        earn(1, 2, T0 + 29_999),
        Earned::Capped(CapCheck::CappedSession)
    );
    assert_eq!(earn(1, 2, T0 + 30_000), Earned::Treat);
    for i in 3..11 {
        assert_eq!(earn(i, i, T0 + 60_000 + i as i64), Earned::Treat, "{i}");
    }
    // The jar holds 10: full beats the caps.
    assert_eq!(jar(&store), (10, true));
    assert_eq!(earn(50, 50, T0 + 2 * HOUR_MS), Earned::JarFull);
    assert_eq!(jar(&store), (10, true));
    assert_eq!(
        env.count("SELECT count(*) FROM activity_turns WHERE given_at_ms IS NULL"),
        10
    );
}

#[test]
fn the_hour_and_day_caps_are_durable_and_shared_by_every_life() {
    let env = Env::with_boot();
    let mut store = env.open();
    let a = life_id(&store);
    let c = store.conn();
    // 12 earns in the last hour, all given so the jar never fills.
    for i in 0..12 {
        c.execute(
            "INSERT INTO activity_turns (session_key, turn_key, client, life_id, earned_at_ms,
                given_at_ms) VALUES (?1, ?2, 'grok-cli', ?3, ?4, ?4)",
            rusqlite::params![key("h", i), key("ht", i), a, T0 - 1000 * (i as i64 + 1)],
        )
        .unwrap();
    }
    store.retire("bye", &a, "Ginger").unwrap();
    let b = store
        .start_pet("new", &start(None, "frog", None, Mode::Gentle))
        .unwrap()
        .life
        .unwrap()
        .life_id;
    assert_eq!(
        jar(&store),
        (0, false),
        "a new pet starts with an empty jar"
    );
    let c = store.conn();
    assert_eq!(
        treats::earn(c, &b, "grok-cli", &key("n", 1), &key("nt", 1), T0).unwrap(),
        Earned::Capped(CapCheck::CappedHour),
        "adopting never resets the caps"
    );
    assert_eq!(
        treats::earn(c, &b, "grok-cli", &key("n", 1), &key("nt", 1), T0 + HOUR_MS).unwrap(),
        Earned::Treat
    );
    // Restart: the verdicts come from the rows.
    drop(store);
    let store = env.open();
    assert_eq!(jar(&store), (1, false));
}

#[test]
fn give_treat_turns_one_treat_into_five_xp_exactly_once() {
    let env = Env::with_boot();
    let mut store = env.open();
    let id = life_id(&store);
    store
        .start_pet("go", &start(None, "cat", None, Mode::Gentle))
        .unwrap();
    assert!(matches!(
        store.give_treat("g0", &id),
        Err(StorageError::NoTreats)
    ));
    seed(&store, 3, 0);
    assert_eq!(jar(&store), (3, false));
    let s = store.give_treat("g1", &id).unwrap();
    assert_eq!((s.treats.jar, s.life.as_ref().unwrap().xp), (2, 5));
    // The oldest treat went first.
    assert_eq!(
        env.count("SELECT id FROM activity_turns WHERE given_at_ms IS NOT NULL"),
        1
    );
    assert_eq!(
        env.count("SELECT count(*) FROM activity_xp WHERE request_id = 'g1' AND amount = 5"),
        1
    );
    // A replay gives nothing more.
    let again = store.give_treat("g1", &id).unwrap();
    assert_eq!((again.treats.jar, again.life.unwrap().xp), (2, 5));
    assert_eq!(env.count("SELECT count(*) FROM activity_xp"), 1);
    // Another life id is NOT_FOUND.
    assert!(matches!(
        store.give_treat("g2", "life-000000000000000000000000"),
        Err(StorageError::LifeNotFound)
    ));
    // The export carries the jar.
    store.export_bar_state().unwrap();
    let bar: serde_json::Value =
        serde_json::from_slice(&fs::read(env.paths.bar_state_path()).unwrap()).unwrap();
    assert_eq!(bar["treats"], serde_json::json!({"jar": 2, "full": false}));
    assert_eq!(bar["life"]["xp"], 5);
}

#[test]
fn give_treat_waits_while_the_pet_is_asleep_paused_or_fainted() {
    let env = Env::with_boot();
    let mut store = env.open();
    let id = life_id(&store);
    store
        .start_pet("go", &start(None, "cat", None, Mode::Gentle))
        .unwrap();
    seed(&store, 2, 0);
    store.sleep("nap").unwrap();
    assert!(matches!(
        store.give_treat("t1", &id),
        Err(StorageError::PetNeedsCare)
    ));
    store.wake("up").unwrap();
    store.set_paused(true).unwrap();
    assert!(matches!(
        store.give_treat("t2", &id),
        Err(StorageError::PetNeedsCare)
    ));
    store.set_paused(false).unwrap();
    env.raw()
        .execute(
            "UPDATE life_vitals SET fainted = 1, critical_need = 'thirst',
                critical_grace_ms = 900000, critical_since_ms = 1",
            [],
        )
        .unwrap();
    assert!(matches!(
        store.give_treat("t3", &id),
        Err(StorageError::PetNeedsCare)
    ));
    env.raw()
        .execute(
            "UPDATE life_vitals SET fainted = 0, critical_need = NULL,
                critical_grace_ms = NULL, critical_since_ms = NULL",
            [],
        )
        .unwrap();
    // While Off: INVALID_STATE, nothing given.
    store.set_enabled("off", false).unwrap();
    assert!(matches!(
        store.give_treat("t4", &id),
        Err(StorageError::Off)
    ));
    store.set_enabled("on", true).unwrap();
    assert_eq!(store.give_treat("t5", &id).unwrap().treats.jar, 1);
    assert_eq!(env.count("SELECT count(*) FROM activity_xp"), 1);
}

#[test]
fn sixteen_treats_grow_the_pet_and_eighty_make_it_mature() {
    let env = Env::with_boot();
    let mut store = env.open();
    let id = life_id(&store);
    store
        .start_pet("go", &start(None, "cat", None, Mode::Gentle))
        .unwrap();
    let mut grew_at = Vec::new();
    let mut unseen = 0;
    for i in 0..80u64 {
        seed(&store, 1, i);
        let rid = format!("give-{i}");
        let s = store.give_treat(&rid, &id).unwrap();
        let l = s.life.unwrap();
        assert_eq!(l.xp, 5 * (u32::try_from(i).unwrap() + 1));
        if s.unseen_events.len() > unseen {
            unseen = s.unseen_events.len();
            grew_at.push((s.unseen_events[unseen - 1].stage, i + 1));
        }
    }
    assert_eq!(
        grew_at,
        [(2, 16), (3, 80)],
        "grown after 16 treats, mature after 80"
    );
    let s = store.snapshot().unwrap();
    let l = s.life.unwrap();
    assert_eq!((l.xp, l.stage, l.level), (400, 3, 4));
    // Journaled once per stage, with the treat as the source and its request id.
    let evolved: Vec<_> = journal(&env, &id)
        .into_iter()
        .filter(|e| e.0 == "evolved")
        .collect();
    assert_eq!(evolved.len(), 2);
    assert_eq!(evolved[0].1.as_deref(), Some("stage:2"));
    assert_eq!(evolved[0].3.as_deref(), Some("give-15"));
    let data: serde_json::Value = serde_json::from_str(&evolved[0].2).unwrap();
    assert_eq!(
        data,
        serde_json::json!({"from": 1, "to": 2, "xp": 80, "level": 2, "source": "treat"})
    );
    assert_eq!(evolved[1].3.as_deref(), Some("give-79"));
    assert_eq!(
        env.count(
            "SELECT count(*) FROM life_events WHERE kind = 'evolved' AND task_id IS NOT NULL"
        ),
        0
    );
    // Unseen until the popup acknowledges them.
    assert_eq!(s.unseen_events.len(), 2);
    let last = s.unseen_events[1].seq;
    store.events_seen("seen", &id, last).unwrap();
    assert!(store.snapshot().unwrap().unseen_events.is_empty());
}

#[test]
fn a_crash_before_the_treat_commit_gives_nothing() {
    let env = Env::with_boot();
    let mut store = env.open();
    let id = life_id(&store);
    seed(&store, 1, 0);
    crate::test_fault::arm(Some("treat_before_commit"));
    let r = catch_unwind(AssertUnwindSafe(|| store.give_treat("crash", &id)));
    crate::test_fault::arm(None);
    assert!(r.is_err());
    let store = env.open();
    assert_eq!(jar(&store), (1, false));
    assert_eq!(env.count("SELECT count(*) FROM activity_xp"), 0);
    assert_eq!(
        env.count("SELECT count(*) FROM request_results WHERE request_id = 'crash'"),
        0
    );
}

// ---- On/Off -----------------------------------------------------------------------------

#[test]
fn off_freezes_decay_grace_and_death_and_on_resumes_without_catch_up() {
    let env = Env::with_boot();
    let (mut store, id) = armed(&env);
    // 6 h of attended time: thirst is low but no one is critical yet.
    env.pass(6 * HOUR_MS);
    env.write_presence("bar-session-01", &id, 6 * HOUR_MS);
    let before = store.snapshot().unwrap();
    assert!(before.lifecycle.attended_total_ms > 0);
    let off = store.set_enabled("off", false).unwrap();
    assert!(!off.enabled);
    assert_eq!(off.lifecycle.frozen_reason, "off");
    store.export_bar_state().unwrap();
    let bar: serde_json::Value =
        serde_json::from_slice(&fs::read(env.paths.bar_state_path()).unwrap()).unwrap();
    assert_eq!(
        (bar["enabled"].clone(), bar["presence"]["wanted"].clone()),
        (false.into(), false.into())
    );
    assert!(env.paths.off_flag_path().is_file());

    // A day passes while Off, with a stale sidecar claiming all of it: nothing is charged.
    env.pass(DAY);
    env.write_presence("bar-session-02", &id, DAY);
    let frozen = store.snapshot().unwrap();
    assert_eq!(frozen.meters, before.meters);
    assert_eq!(
        frozen.lifecycle.attended_total_ms,
        before.lifecycle.attended_total_ms
    );
    assert_eq!(frozen.lifecycle.needs, before.lifecycle.needs);
    assert!(frozen.life.as_ref().unwrap().status != "dead");
    let ledger: i64 =
        env.count("SELECT discarded_ms FROM presence_sessions WHERE session_id = 'bar-session-02'");
    assert_eq!(ledger, DAY, "Off time is discarded, never banked");

    // On: exactly where it left off; only time attended after On counts.
    let on = store.set_enabled("on", true).unwrap();
    assert_eq!(on.meters, before.meters);
    assert_eq!(on.lifecycle.frozen_reason, "none");
    assert!(!env.paths.off_flag_path().exists());
    env.pass(HOUR_MS);
    env.write_presence("bar-session-03", &id, HOUR_MS);
    let after = store.snapshot().unwrap();
    assert_eq!(
        after.lifecycle.attended_total_ms,
        before.lifecycle.attended_total_ms + HOUR_MS
    );
}

#[test]
fn a_pending_end_waits_while_off() {
    let env = Env::with_boot();
    let (mut store, id) = armed(&env);
    // A permadeath pet whose grace ran out while Off is only finalized after On.
    store.set_enabled("off", false).unwrap();
    env.raw()
        .execute_batch(
            "UPDATE pet_state SET thirst = 0;
             UPDATE life_vitals SET critical_need = 'thirst', critical_grace_ms = 900000,
                critical_elapsed_ms = 900000, critical_since_ms = 1, death_pending = 1;
             UPDATE lives SET status = 'critical';",
        )
        .unwrap();
    let s = store.snapshot().unwrap();
    assert_eq!(s.life.as_ref().unwrap().life_id, id, "no death while Off");
    assert_eq!(s.lifecycle.frozen_reason, "off");
    let s = store.set_enabled("on", true).unwrap();
    assert!(s.life.is_none(), "On resumes the rules where they were");
    assert_eq!(s.memorial.count, 1);
}

#[test]
fn off_freezes_a_nap_and_on_resumes_it_where_it_left_off() {
    let env = Env::with_boot();
    let mut store = env.open();
    store.sleep("nap").unwrap();
    env.pass(30_000);
    assert_eq!(store.snapshot().unwrap().care.sleep_remaining_ms, 90_000);
    store.set_enabled("off", false).unwrap();
    env.pass(HOUR_MS);
    let s = store.snapshot().unwrap();
    assert!(s.care.sleeping);
    assert_eq!(s.care.sleep_remaining_ms, 90_000);
    store.set_enabled("on", true).unwrap();
    env.pass(89_999);
    assert_eq!(store.snapshot().unwrap().care.sleep_remaining_ms, 1);
    env.pass(1);
    assert!(!store.snapshot().unwrap().care.sleeping);
}

#[test]
fn off_refuses_what_would_age_arm_grow_or_end_a_life() {
    let env = Env::with_boot();
    let (mut store, id) = armed(&env);
    seed(&store, 1, 0);
    store.set_enabled("off", false).unwrap();
    let off = |r: Result<()>| assert!(matches!(r, Err(StorageError::Off)), "{r:?}");
    off(store.care("c", "food").map(drop));
    off(store.sleep("s").map(drop));
    off(store.wake("w").map(drop));
    off(store.set_paused(true).map(drop));
    off(store.set_rules("r", &id, Mode::Gentle, true).map(drop));
    off(store
        .configure_life(
            "l",
            &id,
            1,
            &Leniency {
                grace_hours: Some(48),
                ..Leniency::default()
            },
        )
        .map(drop));
    off(store.retire("bye", &id, "Ginger").map(drop));
    off(store.give_treat("t", &id).map(drop));
    off(store
        .start_pet("go", &start(None, "cat", None, Mode::Gentle))
        .map(drop));
    // Cosmetic and bookkeeping methods still run.
    store.select_appearance("look", &id, "owl").unwrap();
    store.rename("name", &id, "Pip").unwrap();
    store.events_seen("seen", &id, 0).unwrap();
    store.set_helpers("h", true).unwrap();
    store.forget_history("f").unwrap();
    store.memorial(None, None).unwrap();
    let s = store.snapshot().unwrap();
    assert_eq!(s.life.unwrap().name, "Pip");
    assert_eq!(s.treats.jar, 1);
    // A refused request leaves no receipt: the same id works after On.
    store.set_enabled("on", true).unwrap();
    assert_eq!(store.give_treat("t", &id).unwrap().treats.jar, 0);
}

#[test]
fn adopting_while_off_is_refused_too() {
    let env = Env::with_boot();
    let mut store = env.open();
    let id = life_id(&store);
    store.retire("bye", &id, "Ginger").unwrap();
    store.set_enabled("off", false).unwrap();
    let s = store.snapshot().unwrap();
    assert!(s.life.is_none() && !s.enabled);
    assert_eq!(s.lifecycle.frozen_reason, "no_life", "no_life outranks off");
    let req = AdoptRequest {
        species: "cat",
        name: None,
        mode: Mode::Gentle,
        pace: codepet_core::lifecycle::Pace::Steady,
        acknowledge_rules: true,
    };
    assert!(matches!(store.adopt("a", &req), Err(StorageError::Off)));
}

#[test]
fn set_enabled_is_idempotent_works_without_a_life_and_survives_a_crash() {
    let env = Env::with_boot();
    let mut store = env.open();
    let id = life_id(&store);
    store.retire("bye", &id, "Ginger").unwrap();
    let a = store.set_enabled("o1", false).unwrap();
    let b = store.set_enabled("o2", false).unwrap();
    assert!(!a.enabled && !b.enabled);
    assert!(b.state_version > a.state_version);
    crate::test_fault::arm(Some("enable_before_commit"));
    let r = catch_unwind(AssertUnwindSafe(|| store.set_enabled("o3", true)));
    crate::test_fault::arm(None);
    assert!(r.is_err());
    let mut store = env.open();
    assert!(!store.snapshot().unwrap().enabled, "the crash rolled back");
    assert!(store.set_enabled("o3", true).unwrap().enabled);
    assert_eq!(env.count("SELECT enabled FROM meta"), 1);
}

#[test]
fn flags_mirror_the_database_and_heal() {
    let env = Env::with_boot();
    let mut store = env.open();
    let off = env.paths.off_flag_path();
    let helpers = env.paths.helpers_flag_path();
    store.set_enabled("off", false).unwrap();
    store.set_helpers("h", true).unwrap();
    for f in [&off, &helpers] {
        let m = fs::symlink_metadata(f).unwrap();
        assert!(m.is_file());
        assert_eq!((m.len(), mode(f)), (0, 0o600));
    }
    // Removed by hand: the next export puts them back.
    fs::remove_file(&off).unwrap();
    fs::remove_file(&helpers).unwrap();
    store.export_bar_state().unwrap();
    assert!(off.is_file() && helpers.is_file());
    // A symlink planted in their place is replaced, never followed.
    let target = env.root.join("planted-target");
    fs::remove_file(&off).unwrap();
    std::os::unix::fs::symlink(&target, &off).unwrap();
    store.sync_flags().unwrap();
    assert!(fs::symlink_metadata(&off).unwrap().is_file());
    assert!(!target.exists());
    store.set_enabled("on", true).unwrap();
    store.set_helpers("h2", false).unwrap();
    assert!(!off.exists() && !helpers.exists());
    // A stray flag disappears at the next export; a directory in its place is refused.
    fs::write(&off, b"").unwrap();
    store.export_bar_state().unwrap();
    assert!(!off.exists());
    fs::create_dir(&off).unwrap();
    assert!(matches!(
        store.sync_flags(),
        Err(StorageError::UnsafeStateDir(_))
    ));
}

#[test]
fn helpers_switch_sets_meta_and_shows_in_the_snapshot() {
    let env = Env::with_boot();
    let mut store = env.open();
    assert!(!store.snapshot().unwrap().count_helpers);
    assert!(store.set_helpers("h1", true).unwrap().count_helpers);
    assert_eq!(env.count("SELECT count_helpers FROM meta"), 1);
    assert!(!store.set_helpers("h2", false).unwrap().count_helpers);
}

// ---- Forget coding history --------------------------------------------------------------

#[test]
fn forget_history_clears_activity_but_keeps_treats_and_xp() {
    let env = Env::with_boot();
    let mut store = env.open();
    let id = life_id(&store);
    seed(&store, 3, 0);
    store.give_treat("g", &id).unwrap();
    let c = store.conn();
    c.execute(
        "INSERT INTO agent_events (dedupe_key, client, kind, at_ms, outcome, interactive)
         VALUES (?1, 'codex', 'turn_done', ?2, 'folded', 1)",
        rusqlite::params![key("d", 1), T0],
    )
    .unwrap();
    c.execute(
        "INSERT INTO agent_sessions (session_key, client, interactive, state, state_since_ms,
            last_event_ms) VALUES (?1, 'codex', 1, 'idle', ?2, ?2)",
        rusqlite::params![key("s", 1), T0],
    )
    .unwrap();
    let s = store.forget_history("forget").unwrap();
    assert_eq!((s.treats.jar, s.life.unwrap().xp), (2, 5));
    for t in ["agent_events", "agent_sessions"] {
        assert_eq!(env.count(&format!("SELECT count(*) FROM {t}")), 0, "{t}");
    }
    assert_eq!(
        env.count("SELECT count(*) FROM activity_turns WHERE earned_at_ms != 0"),
        0
    );
    assert_eq!(
        env.count(
            "SELECT count(*) FROM activity_turns WHERE given_at_ms IS NOT NULL AND given_at_ms != 0"
        ),
        0
    );
    assert_eq!(
        env.count("SELECT count(*) FROM activity_turns WHERE given_at_ms IS NULL"),
        2,
        "unspent treats stay in the jar"
    );
    // Forgotten earns no longer count toward the caps.
    let c = store.conn();
    assert_eq!(
        treats::check(c, &id, &key("s", 0), T0).unwrap(),
        CapCheck::Allow
    );
}

#[test]
fn forget_history_is_busy_while_the_fold_holds_its_lock() {
    let env = Env::with_boot();
    let mut store = env.open();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(env.paths.fold_lock_path())
        .unwrap();
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive).unwrap();
    let err = store.forget_history("f").unwrap_err();
    assert!(matches!(err, StorageError::FoldRunning));
    assert_eq!(err.code(), "BUSY");
    drop(lock);
    store.forget_history("f").unwrap();
}

#[test]
fn forget_history_creates_a_private_fold_lock_and_never_follows_a_symlink() {
    let env = Env::with_boot();
    let mut store = env.open();
    store.forget_history("f1").unwrap();
    assert_eq!(mode(&env.paths.fold_lock_path()), 0o600);
    fs::remove_file(env.paths.fold_lock_path()).unwrap();
    let target = env.root.join("lock-target");
    std::os::unix::fs::symlink(&target, env.paths.fold_lock_path()).unwrap();
    assert!(store.forget_history("f2").is_err());
    assert!(!target.exists());
}

// ---- receipts ---------------------------------------------------------------------------

#[test]
fn v03_receipts_replay_with_a_fresh_snapshot_and_frequent_ones_are_bounded() {
    let env = Env::with_boot();
    let mut store = env.open();
    let first = store.set_helpers("same", true).unwrap();
    store.rename("other", &life_id(&store), "Pip").unwrap();
    // The replay is a success with today's state, not a stale copy.
    let replay = store.set_helpers("same", true).unwrap();
    assert_eq!(replay.life.unwrap().name, "Pip");
    assert!(replay.state_version > first.state_version);
    assert!(matches!(
        store.set_helpers("same", false),
        Err(StorageError::IdempotencyConflict)
    ));
    for i in 0..(crate::MAX_LIFE_RECEIPTS + 20) {
        store.set_helpers(&format!("h-{i}"), i % 2 == 0).unwrap();
    }
    assert_eq!(
        env.count(
            "SELECT count(*) FROM request_results WHERE method IN
             ('pet.rename','treats.give','app.set_enabled','app.set_helpers','agent.forget_history')"
        ),
        crate::MAX_LIFE_RECEIPTS
    );
}

#[test]
fn pet_start_never_adopts_over_an_end_its_own_settlement_made_final() {
    let env = Env::with_boot();
    let (mut store, id) = armed(&env);
    env.raw()
        .execute_batch(
            "UPDATE pet_state SET thirst = 0;
             UPDATE life_vitals SET critical_need = 'thirst', critical_grace_ms = 900000,
                critical_elapsed_ms = 900000, critical_since_ms = 1, death_pending = 1;
             UPDATE lives SET status = 'critical';",
        )
        .unwrap();
    let err = store
        .start_pet("go", &start(None, "cat", None, Mode::Gentle))
        .unwrap_err();
    assert!(matches!(err, StorageError::PassedAway), "{err:?}");
    assert_eq!(err.code(), "NO_ACTIVE_PET");
    // The end itself was committed; the memorial comes first, then a new start adopts.
    assert_eq!(
        env.count("SELECT count(*) FROM lives WHERE status = 'dead'"),
        1
    );
    let s = store
        .start_pet("go-2", &start(None, "cat", None, Mode::Gentle))
        .unwrap();
    assert_ne!(s.life.unwrap().life_id, id);
}

#[test]
fn nothing_is_earned_while_off() {
    let env = Env::new();
    let mut store = env.open();
    let life = life_id(&store);
    store.set_enabled("off", false).unwrap();
    assert_eq!(
        treats::earn(
            store.conn(),
            &life,
            "hermes",
            &key("s", 1),
            &key("t", 1),
            T0
        )
        .unwrap(),
        Earned::Off
    );
    assert_eq!(env.count("SELECT count(*) FROM activity_turns"), 0);
}
