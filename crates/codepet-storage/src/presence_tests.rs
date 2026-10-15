//! Attended-presence settlement (spec §5.2): refusals, the never-pruned ledger, the
//! CLOCK_BOOTTIME cap, blockers and the nap rule. Nothing uncertain may charge time.
use super::*;
use crate::presence_io::{self, Refusal};
use codepet_core::lifecycle::{Mode, HOUR_MS, SETTLE_CAP_MS};
use codepet_core::presence::Reject;

const MIN: i64 = 60_000;

fn session(n: u32) -> String {
    format!("bar-test-{n:04}")
}

fn ledger(env: &Env) -> Vec<(String, i64, i64, i64)> {
    let c = env.raw();
    let mut stmt = c
        .prepare(
            "SELECT session_id, consumed_ms, credited_ms, discarded_ms FROM presence_sessions
             ORDER BY session_id",
        )
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

fn attended(store: &Store) -> i64 {
    store.snapshot().unwrap().lifecycle.attended_total_ms
}

#[test]
fn credited_presence_decays_after_the_satiety_hold() {
    let env = Env::with_boot();
    let (store, id) = armed(&env);
    env.pass(2 * HOUR_MS);
    env.write_presence(&session(1), &id, 2 * HOUR_MS);
    let s = store.snapshot().unwrap();
    // One hour of hold, then one hour of decay.
    assert_eq!(
        s.meters,
        MetersView {
            hunger: 9000,
            thirst: 8750,
            mental_health: 9400,
            hygiene: 9500
        }
    );
    assert_eq!(s.lifecycle.attended_total_ms, 2 * HOUR_MS);
    assert_eq!(s.lifecycle.frozen_reason, "none");
    assert_eq!(ledger(&env), [(session(1), 2 * HOUR_MS, 2 * HOUR_MS, 0)]);
    assert_eq!(env.rejections(), 0);
}

#[test]
fn a_missing_file_charges_nothing_and_counts_no_rejection() {
    let env = Env::with_boot();
    let (store, _) = armed(&env);
    env.pass(HOUR_MS);
    assert_eq!(attended(&store), 0);
    assert!(ledger(&env).is_empty());
    assert_eq!(env.rejections(), 0);
}

#[test]
fn refused_files_charge_nothing_and_count_a_rejection() {
    let env = Env::with_boot();
    let (store, id) = armed(&env);
    let now = || env.clock.now_ms();
    let doc = |f: &dyn Fn(&mut serde_json::Value)| {
        let mut v = serde_json::json!({
            "v": 1, "session": session(1), "life_id": id,
            "started_wall_ms": now() - 10 * MIN, "updated_wall_ms": now(),
            "attended_ms": 10 * MIN, "ticks": 10
        });
        f(&mut v);
        v.to_string()
    };
    let cases: Vec<(&str, String)> = vec![
        ("unknown field", doc(&|v| v["extra"] = 1.into())),
        (
            "missing field",
            doc(&|v| {
                v.as_object_mut().unwrap().remove("ticks");
            }),
        ),
        ("version", doc(&|v| v["v"] = 2.into())),
        ("session", doc(&|v| v["session"] = "BAR-1234567".into())),
        ("life id", doc(&|v| v["life_id"] = "life-XYZ".into())),
        ("attended > ticks", doc(&|v| v["ticks"] = 9.into())),
        (
            "attended > span",
            doc(&|v| v["started_wall_ms"] = (now() - 8 * MIN).into()),
        ),
        (
            "future updated",
            doc(&|v| {
                v["updated_wall_ms"] = (now() + 121_000).into();
                v["started_wall_ms"] = (now() + 1_000).into();
            }),
        ),
        (
            "negative attended",
            doc(&|v| v["attended_ms"] = (-1).into()),
        ),
        ("not json", "{not json".to_owned()),
        ("oversize", format!("{}{}", doc(&|_| {}), " ".repeat(2048))),
    ];
    env.pass(10 * MIN);
    let before = store.snapshot().unwrap();
    let mut rejections = env.rejections();
    for (what, text) in &cases {
        env.write_presence_raw(text);
        let s = store.snapshot().unwrap();
        rejections += 1;
        assert_eq!(env.rejections(), rejections, "{what}");
        assert_eq!(s.meters, before.meters, "{what}");
        assert_eq!(s.lifecycle.attended_total_ms, 0, "{what}");
        assert!(ledger(&env).is_empty(), "{what}");
    }
    // Group- or other-writable files are refused even with valid content.
    for mode in [0o620, 0o602, 0o666] {
        env.write_presence(&session(1), &id, 10 * MIN);
        fs::set_permissions(env.presence_path(), fs::Permissions::from_mode(mode)).unwrap();
        store.snapshot().unwrap();
        rejections += 1;
        assert_eq!(env.rejections(), rejections, "{mode:o}");
    }
    // A symlink to a valid file is refused, never followed.
    let target = env.root.join("elsewhere.json");
    env.write_presence(&session(1), &id, 10 * MIN);
    fs::rename(env.presence_path(), &target).unwrap();
    symlink(&target, env.presence_path()).unwrap();
    store.snapshot().unwrap();
    rejections += 1;
    assert_eq!(env.rejections(), rejections);
    fs::remove_file(env.presence_path()).unwrap();
    // A directory in its place.
    fs::create_dir(env.presence_path()).unwrap();
    store.snapshot().unwrap();
    rejections += 1;
    assert_eq!(env.rejections(), rejections);
    fs::remove_dir(env.presence_path()).unwrap();
    assert!(ledger(&env).is_empty());
    assert_eq!(attended(&store), 0);
    // Commands still succeed while the file is refused.
    env.write_presence_raw("{not json");
    let mut store = store;
    assert!(store.care("still-works", "food").is_ok());
}

#[test]
fn foreign_owner_and_size_checks_are_exact() {
    assert_eq!(presence_io::vet(1000, 1000, 0o100600, 2048), Ok(()));
    assert_eq!(presence_io::vet(1000, 1000, 0o100644, 0), Ok(()));
    assert_eq!(
        presence_io::vet(0, 1000, 0o100600, 10),
        Err(Refusal::ForeignOwner)
    );
    assert_eq!(
        presence_io::vet(1000, 1000, 0o100660, 10),
        Err(Refusal::Writable)
    );
    assert_eq!(
        presence_io::vet(1000, 1000, 0o100602, 10),
        Err(Refusal::Writable)
    );
    assert_eq!(
        presence_io::vet(1000, 1000, 0o100600, 2049),
        Err(Refusal::TooLarge)
    );
    let env = Env::new();
    env.paths.prepare().unwrap();
    env.write_presence_raw(r#"{"v":1}"#);
    assert_eq!(
        presence_io::read(&env.presence_path(), T0),
        presence_io::PresenceRead::Refused(Refusal::Malformed)
    );
    env.write_presence(&session(1), &format!("life-{}", "a".repeat(24)), MIN);
    assert!(matches!(
        presence_io::read(&env.presence_path(), T0 - 200_000),
        presence_io::PresenceRead::Refused(Refusal::Invalid(Reject::Future))
    ));
}

#[test]
fn replayed_session_is_never_recharged() {
    let env = Env::with_boot();
    let (store, id) = armed(&env);
    env.pass(30 * MIN);
    env.write_presence(&session(1), &id, 30 * MIN);
    assert_eq!(attended(&store), 30 * MIN);
    for _ in 0..3 {
        env.pass(10 * MIN);
        assert_eq!(attended(&store), 30 * MIN);
    }
    // The session grows: only the new part is charged (within the boot-time cap).
    env.pass(5 * MIN);
    env.write_presence(&session(1), &id, 35 * MIN);
    assert_eq!(attended(&store), 35 * MIN);
    assert_eq!(ledger(&env), [(session(1), 35 * MIN, 35 * MIN, 0)]);
}

#[test]
fn a_restored_old_session_is_never_recharged_and_the_ledger_is_never_pruned() {
    let env = Env::with_boot();
    let (store, id) = armed(&env);
    env.pass(20 * MIN);
    env.write_presence(&session(1), &id, 20 * MIN);
    assert_eq!(attended(&store), 20 * MIN);
    let old = fs::read_to_string(env.presence_path()).unwrap();
    env.pass(20 * MIN);
    env.write_presence(&session(2), &id, 20 * MIN);
    assert_eq!(attended(&store), 40 * MIN);
    // A backup restores the old sidecar: its session was consumed, so nothing changes.
    env.pass(20 * MIN);
    env.write_presence_raw(&old);
    assert_eq!(attended(&store), 40 * MIN);
    assert_eq!(ledger(&env).len(), 2);
    // Shrinking a consumed session is a rejection, never a refund or a charge.
    let rejections = env.rejections();
    env.write_presence(&session(2), &id, 5 * MIN);
    assert_eq!(attended(&store), 40 * MIN);
    assert_eq!(env.rejections(), rejections + 1);
    assert!(env
        .raw()
        .execute("DELETE FROM presence_sessions", [])
        .is_err());
}

#[test]
fn cap_by_boottime_same_boot() {
    let env = Env::with_boot();
    let (store, id) = armed(&env);
    store.snapshot().unwrap();
    // A 10 h claim after 30 minutes of real (boot) time credits 31 minutes.
    env.pass(30 * MIN);
    env.write_presence(&session(1), &id, 10 * HOUR_MS);
    assert_eq!(attended(&store), 31 * MIN);
    assert_eq!(
        ledger(&env),
        [(session(1), 10 * HOUR_MS, 31 * MIN, 10 * HOUR_MS - 31 * MIN)]
    );
}

#[test]
fn new_boot_caps_one_settlement_at_8h() {
    let env = Env::with_boot();
    let (store, id) = armed(&env);
    store.snapshot().unwrap();
    env.clock.set_steady("boot-b", 5_000);
    env.pass(10 * HOUR_MS);
    env.write_presence(&session(1), &id, 10 * HOUR_MS);
    let s = store.snapshot().unwrap();
    assert!(s.lifecycle.attended_total_ms <= SETTLE_CAP_MS);
    let row = &ledger(&env)[0];
    assert_eq!(row.1, 10 * HOUR_MS);
    assert_eq!(row.2 + row.3, 10 * HOUR_MS);
    assert!(row.3 >= 2 * HOUR_MS);
}

#[test]
fn no_boot_clock_freezes_time_and_consumes_nothing() {
    let env = Env::with_boot();
    let (store, id) = armed(&env);
    env.clock.clear_steady();
    env.pass(HOUR_MS);
    env.write_presence(&session(1), &id, HOUR_MS);
    assert_eq!(attended(&store), 0);
    assert!(ledger(&env).is_empty());
    assert_eq!(env.rejections(), 0);
    // When the boot clock is back (same boot), the session is charged once, capped by the
    // boot time elapsed since the last cursor (taken before the clock went missing).
    env.clock.set_steady(BOOT, 1_000_000 + HOUR_MS);
    assert_eq!(attended(&store), HOUR_MS);
}

#[test]
fn wall_clock_jumps_of_ten_days_change_nothing() {
    let env = Env::with_boot();
    let (store, id) = armed(&env);
    env.pass(10 * MIN);
    env.write_presence(&session(1), &id, 10 * MIN);
    let base = store.snapshot().unwrap();
    assert_eq!(base.lifecycle.attended_total_ms, 10 * MIN);
    let day = 24 * HOUR_MS;
    for jump in [10 * day, -10 * day] {
        let wall = env.clock.now_ms();
        env.clock.set(wall + jump);
        let s = store.snapshot().unwrap();
        assert_eq!(s.meters, base.meters, "{jump}");
        assert_eq!(s.lifecycle.attended_total_ms, 10 * MIN, "{jump}");
        assert_eq!(s.lifecycle.needs, base.lifecycle.needs, "{jump}");
        env.clock.set(wall);
    }
    // A forward-dated sidecar written during the jump is refused once the wall returns.
    env.clock.set(env.clock.now_ms() + 10 * day);
    env.write_presence(&session(2), &id, 10 * MIN);
    env.clock.set(env.clock.now_ms() - 10 * day);
    let rejections = env.rejections();
    assert_eq!(attended(&store), 10 * MIN);
    assert_eq!(env.rejections(), rejections + 1);
}

fn assert_discarded(env: &Env, store: &Store, what: &str) {
    let s = store.snapshot().unwrap();
    assert_eq!(s.lifecycle.attended_total_ms, 0, "{what}");
    let row = &ledger(env)[0];
    assert_eq!((row.1, row.2, row.3), (HOUR_MS, 0, HOUR_MS), "{what}");
}

#[test]
fn paused_discards() {
    let env = Env::with_boot();
    let (mut store, id) = armed(&env);
    store.set_paused(true).unwrap();
    env.pass(HOUR_MS);
    env.write_presence(&session(1), &id, HOUR_MS);
    assert_discarded(&env, &store, "paused");
    assert_eq!(store.snapshot().unwrap().lifecycle.frozen_reason, "paused");
    // Resuming restores nothing and charges nothing retroactively.
    store.set_paused(false).unwrap();
    assert_discarded(&env, &store, "resumed");
}

#[test]
fn unarmed_discards() {
    let env = Env::with_boot();
    let mut store = env.open();
    store.care("routine", "routine").unwrap();
    let id = life_id(&store);
    env.pass(HOUR_MS);
    env.write_presence(&session(1), &id, HOUR_MS);
    assert_discarded(&env, &store, "unarmed");
    let s = store.snapshot().unwrap();
    assert_eq!(s.lifecycle.frozen_reason, "unarmed");
    assert!(s.lifecycle.needs.iter().all(|n| n.until_end_ms.is_none()));
}

#[test]
fn slot_task_discards_in_every_slot_state() {
    for state in SLOT_STATE_LIST {
        let env = Env::with_boot();
        let (mut store, id) = armed(&env);
        task_in_state(&mut store, &env, state);
        let meters = store.snapshot().unwrap().meters;
        env.pass(HOUR_MS);
        env.write_presence(&session(1), &id, HOUR_MS);
        let s = store.snapshot().unwrap();
        assert_eq!(s.meters, meters, "{state:?}");
        assert_eq!(s.lifecycle.attended_total_ms, 0, "{state:?}");
        assert_eq!(s.lifecycle.frozen_reason, "task", "{state:?}");
        let row = &ledger(&env)[0];
        assert_eq!((row.2, row.3), (0, HOUR_MS), "{state:?}");
    }
}

#[test]
fn nap_discards_only_the_remaining_sleep() {
    let env = Env::with_boot();
    let (mut store, id) = armed(&env);
    store.sleep("nap").unwrap();
    env.pass(30 * MIN);
    env.write_presence(&session(1), &id, 30 * MIN);
    let s = store.snapshot().unwrap();
    // The 2-minute nap is discarded; the other 28 minutes count.
    assert_eq!(s.lifecycle.attended_total_ms, 28 * MIN);
    assert!(!s.care.sleeping);
    assert_eq!(ledger(&env), [(session(1), 30 * MIN, 28 * MIN, 2 * MIN)]);
}

#[test]
fn life_mismatch_discards() {
    let env = Env::with_boot();
    let (store, _) = armed(&env);
    env.pass(HOUR_MS);
    env.write_presence(&session(1), &format!("life-{}", "0".repeat(24)), HOUR_MS);
    assert_discarded(&env, &store, "other life");
}

#[test]
fn no_living_companion_discards() {
    let env = Env::with_boot();
    let (mut store, id) = armed(&env);
    store.retire("retire", &id, "Ginger").unwrap();
    env.pass(HOUR_MS);
    env.write_presence(&session(1), &id, HOUR_MS);
    store.snapshot().unwrap();
    let row = &ledger(&env)[0];
    assert_eq!((row.2, row.3), (0, HOUR_MS));
    assert_eq!(store.snapshot().unwrap().lifecycle.frozen_reason, "no_life");
}

#[test]
fn presence_fault_before_commit_leaves_the_exact_pre_state() {
    let env = Env::with_boot();
    let (store, id) = armed(&env);
    env.pass(2 * HOUR_MS);
    env.write_presence(&session(1), &id, 2 * HOUR_MS);
    let tables = [
        "pet_state",
        "life_vitals",
        "presence_sessions",
        "presence_clock",
        "meta",
    ];
    let before: Vec<String> = tables.iter().map(|t| env.dump(t)).collect();
    test_fault::arm(Some("presence_before_commit"));
    let r = catch_unwind(AssertUnwindSafe(|| store.snapshot()));
    test_fault::arm(None);
    assert!(r.is_err());
    let after: Vec<String> = tables.iter().map(|t| env.dump(t)).collect();
    assert_eq!(before, after);
    // A rerun applies exactly once.
    let store = env.open();
    assert_eq!(attended(&store), 2 * HOUR_MS);
    assert_eq!(attended(&store), 2 * HOUR_MS);
    assert_eq!(ledger(&env), [(session(1), 2 * HOUR_MS, 2 * HOUR_MS, 0)]);
}

#[test]
fn snapshots_settle_only_when_a_sidecar_exists() {
    let env = Env::with_boot();
    let (store, id) = armed(&env);
    let v0 = store.snapshot().unwrap().state_version;
    // No sidecar, nap or live execution: a plain read that changes nothing.
    assert_eq!(store.snapshot().unwrap().state_version, v0);
    env.pass(MIN);
    env.write_presence(&session(1), &id, MIN);
    let v1 = store.snapshot().unwrap().state_version;
    assert_eq!(v1, v0 + 1);
    assert!(store.snapshot().unwrap().state_version > v1);
}

#[test]
fn gentle_and_permadeath_charge_identically_until_grace_runs_out() {
    for mode in [Mode::Permadeath, Mode::Gentle] {
        let env = Env::with_boot();
        let mut store = env.open();
        store.care("routine", "routine").unwrap();
        arm(&mut store, mode);
        let id = life_id(&store);
        env.pass(3 * HOUR_MS);
        env.write_presence(&session(1), &id, 3 * HOUR_MS);
        assert_eq!(store.snapshot().unwrap().meters.thirst, 7500, "{mode:?}");
    }
}

/// Runs `f` on a thread and fails the test if it does not finish within five seconds
/// (a blocking `open(O_RDONLY)` on a FIFO would hang forever).
fn finishes_promptly<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_secs(5))
        .expect("opening a state-dir sidecar blocked")
}

fn make_fifo(path: &std::path::Path) {
    rustix::fs::mknodat(
        rustix::fs::CWD,
        path,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::from_raw_mode(0o600),
        0,
    )
    .unwrap();
}

#[test]
fn sidecar_open_never_follows_a_symlink_or_blocks_on_a_fifo() {
    // The lstat-then-open window: whatever is at the path by the time of open(2) must be
    // refused, not followed, and must never block the writer holding the DB lock.
    let env = Env::new();
    let fifo = env.root.join("swapped-fifo");
    make_fifo(&fifo);
    let f = fifo.clone();
    let opened = finishes_promptly(move || crate::sys::open_sidecar(&f).map(|h| h.metadata()));
    let meta = opened.expect("O_NONBLOCK open of a FIFO succeeds at once").unwrap();
    assert!(!meta.is_file(), "the fstat check then refuses it");
    let target = env.root.join("target.json");
    fs::write(&target, "{}").unwrap();
    let link = env.root.join("link.json");
    symlink(&target, &link).unwrap();
    let err = crate::sys::open_sidecar(&link).unwrap_err();
    assert!(crate::sys::is_symlink_refusal(&err), "{err:?}");
    assert_eq!(
        crate::sys::open_sidecar(&env.root.join("absent")).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
}

#[test]
fn a_fifo_presence_file_is_refused_without_blocking_settlement() {
    let env = Env::with_boot();
    let (store, _id) = armed(&env);
    make_fifo(&env.presence_path());
    let path = env.presence_path();
    let read = finishes_promptly(move || presence_io::read(&path, 0));
    assert!(matches!(
        read,
        presence_io::PresenceRead::Refused(Refusal::NotRegular)
    ));
    // A settling snapshot and the bar export (both under the DB lock) finish too.
    let snap = finishes_promptly(move || {
        let s = store.snapshot().map(|s| s.state_version);
        let e = store.export_bar_state();
        (s.is_ok(), e.is_ok())
    });
    assert_eq!(snap, (true, true));
}

#[test]
fn a_fifo_bar_state_file_never_blocks_the_export() {
    let env = Env::new();
    let store = env.open();
    make_fifo(&env.paths.bar_state_path());
    // existing_version sees a non-regular file (version 0) and the rename replaces it.
    let ok = finishes_promptly(move || store.export_bar_state().unwrap());
    assert!(ok);
    assert!(fs::symlink_metadata(env.paths.bar_state_path())
        .unwrap()
        .is_file());
}
