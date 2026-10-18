//! The fold through the real schema (WP1's v5 migration of the contract DDL): the session
//! and fold vectors replayed through SQLite, dedupe, savepoints, caps, Off, test records,
//! hello, retention, E1 and the export.
use super::*;
use crate::spool::{hmac_key, InboxRecord};
use crate::{test_fault, ManualClock, StatePaths, Store};
use codepet_core::agent::DAY_MS;
use serde_json::Value;
use std::panic::{catch_unwind, AssertUnwindSafe};
use tempfile::TempDir;

const T0: i64 = 1_792_000_000_000;
const VECTORS: &str = include_str!("../../../docs/v0.3/contracts/agent_vectors.json");

struct Env {
    _tmp: TempDir,
    paths: StatePaths,
    clock: Arc<ManualClock>,
    dig: std::cell::Cell<u64>,
}
impl Env {
    fn new() -> Self {
        Self::with(|_| {})
    }
    /// A v5 database holding the v0.2-shaped first-run pet; `before` runs on it first.
    fn with(before: impl FnOnce(&mut Store)) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let xdg = tmp.path().join("state");
        let paths = StatePaths::resolve(|k| match k {
            "XDG_STATE_HOME" => Some(xdg.clone().into()),
            _ => None,
        })
        .unwrap();
        let clock = Arc::new(ManualClock::new(T0));
        let mut store = Store::open_with_first_run_placeholder(&paths, clock.clone()).unwrap();
        before(&mut store);
        drop(store);
        Self {
            _tmp: tmp,
            paths,
            clock,
            dig: std::cell::Cell::new(1),
        }
    }
    fn fold(&self) -> Store {
        Store::open_for_fold(&self.paths.state_dir, self.clock.clone()).unwrap()
    }
    fn raw(&self) -> Connection {
        Connection::open(&self.paths.db_path).unwrap()
    }
    fn int(&self, sql: &str) -> i64 {
        self.raw().query_row(sql, [], |r| r.get(0)).unwrap()
    }
    fn salt(&self) -> Vec<u8> {
        self.raw()
            .query_row("SELECT hook_salt FROM meta WHERE id = 1", [], |r| r.get(0))
            .unwrap()
    }
    fn set(&self, sql: &str) {
        self.raw().execute_batch(sql).unwrap();
    }
    /// A record with a fresh digest (distinct events never dedupe).
    fn rec(&self, client: &str, kind: &str, at_ms: i64) -> InboxRecord {
        let d = self.dig.get();
        self.dig.set(d + 1);
        InboxRecord {
            v: 1,
            client: client.into(),
            kind: kind.into(),
            session: Some("s1".into()),
            turn: None,
            tool: None,
            akey: None,
            note: None,
            tools: 0,
            interactive: true,
            test: false,
            dig: format!("{d:016x}"),
            at_ms,
        }
    }
    fn session_key(&self, client: &str, session: &str) -> String {
        hmac_key(&self.salt(), DOMAIN_SESSION, client, &[session])
    }
    fn state_of(&self, client: &str, session: &str) -> Option<String> {
        self.raw()
            .query_row(
                "SELECT state FROM agent_sessions WHERE session_key = ?1",
                [self.session_key(client, session)],
                |r| r.get(0),
            )
            .optional()
            .unwrap()
    }
    fn treats(&self) -> i64 {
        self.int("SELECT COUNT(*) FROM activity_turns")
    }
    fn fold_one(&self, store: &mut Store, r: InboxRecord) -> FoldBatch {
        store.fold_batch(&[r], 0).unwrap()
    }
}

fn turn(r: InboxRecord, t: &str) -> InboxRecord {
    InboxRecord {
        turn: Some(t.into()),
        ..r
    }
}
fn tool(r: InboxRecord, t: &str) -> InboxRecord {
    InboxRecord {
        tool: Some(t.into()),
        ..r
    }
}

#[test]
fn session_vectors_replay_through_sqlite() {
    let v: Value = serde_json::from_str(VECTORS).unwrap();
    for vec in v["sessions"]["vectors"].as_array().unwrap() {
        let name = vec["name"].as_str().unwrap();
        let env = Env::new();
        let client = vec["client"].as_str().unwrap();
        let sid = vec["session"].as_str().unwrap_or("s1");
        let inter = vec["interactive"].as_bool().unwrap_or(true);
        let jar = vec["jar"].as_i64().unwrap_or(0);
        for i in 0..jar {
            // Forgotten treats (earned_at 0): they fill the jar but never count for caps.
            env.raw()
                .execute(
                    "INSERT INTO activity_turns (session_key, turn_key, client, life_id,
                       earned_at_ms) SELECT ?1, ?2, 'codex', life_id, 0 FROM lives
                       WHERE status = 'healthy'",
                    params![format!("{:032x}", i), format!("{:032x}", i + 100)],
                )
                .unwrap();
        }
        let base_treats = env.treats();
        let mut store = env.fold();
        for (i, e) in vec["events"].as_array().unwrap().iter().enumerate() {
            let at = T0 + e["at"].as_i64().unwrap();
            env.clock.set(at);
            let mut r = env.rec(client, e["kind"].as_str().unwrap(), at);
            r.interactive = inter;
            r.session = Some(sid.into());
            r.turn = e["turn"].as_str().map(str::to_owned);
            r.tool = e["tool"].as_str().map(str::to_owned);
            r.tools = u32::try_from(e["tools"].as_u64().unwrap_or(0)).unwrap();
            if let Some(akey) = e["akey"].as_str() {
                r.session = None;
                r.akey = Some(akey.into());
            }
            let before = env.treats();
            let b = env.fold_one(&mut store, r);
            assert_eq!(b.recorded, 1, "{name} event {i}");
            let earned = env.treats() > before;
            assert_eq!(earned, e["earned"].as_bool().unwrap_or(false), "{name} {i}");
            if let Some(want) = e["expect"].as_str() {
                let st = env.state_of(client, sid).unwrap_or_else(|| "idle".into());
                assert_eq!(st, want, "{name} event {i}: state");
            }
            if let Some(want) = e["expect_approval"].as_str() {
                let key = hmac_key(
                    &env.salt(),
                    DOMAIN_APPROVAL,
                    client,
                    &[e["akey"].as_str().unwrap()],
                );
                let st: String = env
                    .raw()
                    .query_row(
                        "SELECT state FROM agent_sessions WHERE session_key = ?1 AND approval = 1",
                        [key],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_eq!(st, want, "{name} event {i}: approval");
            }
        }
        let f = &vec["final"];
        assert_eq!(
            env.treats() - base_treats,
            f["treats"].as_i64().unwrap(),
            "{name}: treats"
        );
        assert_eq!(
            env.int("SELECT finished_seq FROM meta"),
            f["finished_seq"].as_i64().unwrap(),
            "{name}: finished_seq"
        );
        let salt = env.salt();
        let row: Option<(Option<String>, i64, Option<i64>)> = env
            .raw()
            .query_row(
                "SELECT open_turn_key, last_done_tools, error_at_ms FROM agent_sessions
                 WHERE session_key = ?1",
                [env.session_key(client, sid)],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .unwrap();
        let (open, done_tools, error_at) = row.unwrap_or((None, 0, None));
        let want_open = f["open_turn"]
            .as_str()
            .map(|t| hmac_key(&salt, DOMAIN_TURN, client, &[sid, t]));
        assert_eq!(open, want_open, "{name}: open turn");
        assert_eq!(done_tools, f["last_done_tools"].as_i64().unwrap(), "{name}");
        if let Some(err) = f.get("error_at_ms") {
            assert_eq!(error_at, err.as_i64().map(|e| T0 + e), "{name}: error_at");
        }
        if let Some(full) = f.get("finished_jar_full") {
            assert_eq!(
                env.int("SELECT finished_jar_full FROM meta") == 1,
                full.as_bool().unwrap(),
                "{name}"
            );
        }
    }
}

#[test]
fn fold_vectors_replay_through_sqlite() {
    let v: Value = serde_json::from_str(VECTORS).unwrap();
    let now = v["fold"]["now"].as_i64().unwrap();
    for vec in v["fold"]["vectors"].as_array().unwrap() {
        let name = vec["name"].as_str().unwrap();
        let env = if vec["life"].as_bool().unwrap() {
            Env::new()
        } else {
            Env::with(|store| {
                let life = store.snapshot().unwrap().life.unwrap();
                store.retire("bye", &life.life_id, &life.name).unwrap();
            })
        };
        assert_eq!(
            env.int(&format!(
                "SELECT COUNT(*) FROM lives WHERE status IN {LIVING}"
            )),
            i64::from(vec["life"].as_bool().unwrap())
        );
        env.clock.set(now);
        if !vec["enabled"].as_bool().unwrap() {
            env.set("UPDATE meta SET enabled = 0");
        }
        if let Some(ts) = vec["test_state"].as_str() {
            env.set(&format!(
                "UPDATE meta SET test_state = '{ts}', test_at_ms = 1"
            ));
        }
        if vec["first_seen"].as_bool().unwrap_or(false) {
            env.set("INSERT INTO client_hooks (client, first_seen_ms) VALUES ('claude-code', 5)");
        }
        let e = &vec["event"];
        let mut r = env.rec(
            "claude-code",
            e["kind"].as_str().unwrap(),
            e["at"].as_i64().unwrap(),
        );
        r.turn = e["turn"].as_str().map(str::to_owned);
        r.tool = e["tool"].as_str().map(str::to_owned);
        r.test = e["test"].as_bool().unwrap_or(false);
        let hello_before = env.int("SELECT hello_seq FROM meta");
        let ts_before: Option<String> = env
            .raw()
            .query_row("SELECT test_state FROM meta", [], |r| r.get(0))
            .unwrap();
        let mut store = env.fold();
        env.fold_one(&mut store, r);
        let x = &vec["expect"];
        let (outcome, at): (String, i64) = env
            .raw()
            .query_row("SELECT outcome, at_ms FROM agent_events", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(outcome, x["outcome"].as_str().unwrap(), "{name}");
        let events_total = env
            .raw()
            .query_row(
                "SELECT events_total FROM client_hooks WHERE client = 'claude-code'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .unwrap()
            .unwrap_or(0);
        assert_eq!(
            events_total > 0,
            x["client_record"].as_bool().unwrap(),
            "{name}"
        );
        assert_eq!(
            env.int("SELECT hello_seq FROM meta") > hello_before,
            x["hello"].as_bool().unwrap(),
            "{name}"
        );
        if let Some(want) = x.get("at") {
            assert_eq!(Some(at), want.as_i64(), "{name}");
        }
        let ts_after: Option<String> = env
            .raw()
            .query_row("SELECT test_state FROM meta", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            ts_after, ts_before,
            "{name}: the fold never changes test_state"
        );
    }
}

#[test]
fn duplicate_deliveries_dedupe_and_poison_is_counted() {
    let env = Env::new();
    let mut store = env.fold();
    let r = turn(env.rec("claude-code", "turn_start", T0), "t1");
    let b = store.fold_batch(&[r.clone(), r.clone()], 2).unwrap();
    assert_eq!((b.recorded, b.duplicates, b.dropped), (1, 1, 2));
    assert_eq!(env.int("SELECT COUNT(*) FROM agent_events"), 1);
    assert_eq!(env.int("SELECT events_dropped FROM meta"), 2);
    // A later batch with the same record is still a duplicate (the dedupe key is durable).
    let b = store.fold_batch(&[r], 0).unwrap();
    assert_eq!((b.recorded, b.duplicates, b.changed), (0, 1, false));
}

#[test]
fn a_failing_event_rolls_back_alone() {
    let env = Env::new();
    let mut store = env.fold();
    let good = turn(env.rec("claude-code", "turn_start", T0), "t1");
    let mut bad = env.rec("claude-code", "turn_done", T0 + 1);
    bad.session = None; // no session and no approval key: fold_one refuses it
    let good2 = tool(turn(env.rec("claude-code", "activity", T0 + 2), "t1"), "x");
    let b = store.fold_batch(&[good, bad, good2], 0).unwrap();
    assert_eq!((b.recorded, b.dropped), (2, 1));
    assert_eq!(env.int("SELECT COUNT(*) FROM agent_events"), 2);
    assert_eq!(env.int("SELECT events_dropped FROM meta"), 1);
    assert_eq!(
        env.state_of("claude-code", "s1").as_deref(),
        Some("working")
    );
}

fn earn_turn(env: &Env, store: &mut Store, session: &str, t: &str, at: i64) -> bool {
    let before = env.treats();
    let mut s = turn(env.rec("codex", "turn_start", at), t);
    s.session = Some(session.into());
    let mut a = tool(turn(env.rec("codex", "activity", at + 100), t), "c");
    a.session = Some(session.into());
    let mut d = turn(env.rec("codex", "turn_done", at + 200), t);
    d.session = Some(session.into());
    env.clock.set(at + 200);
    store.fold_batch(&[s, a, d], 0).unwrap();
    env.treats() > before
}

#[test]
fn caps_hold_durably_and_the_jar_holds_ten() {
    let env = Env::new();
    let mut store = env.fold();
    // 1 per session per 30 s.
    assert!(earn_turn(&env, &mut store, "a", "t1", T0));
    assert!(!earn_turn(&env, &mut store, "a", "t2", T0 + 10_000));
    assert!(earn_turn(&env, &mut store, "a", "t3", T0 + 31_000));
    // The jar holds 10 unspent treats; full turns record finished_jar_full.
    for i in 0..7 {
        assert!(earn_turn(
            &env,
            &mut store,
            &format!("s{i}"),
            "t",
            T0 + 40_000 + i * 1000
        ));
    }
    assert_eq!(env.treats(), 9);
    assert!(earn_turn(&env, &mut store, "x", "t", T0 + 60_000));
    assert!(!earn_turn(&env, &mut store, "y", "t", T0 + 61_000));
    assert_eq!(env.int("SELECT finished_jar_full FROM meta"), 1);
    // Giving treats empties the jar; then the hourly cap (12) bites, durably.
    env.set("UPDATE activity_turns SET given_at_ms = 1");
    assert!(earn_turn(&env, &mut store, "y", "t2", T0 + 62_000));
    assert!(earn_turn(&env, &mut store, "z", "t", T0 + 63_000));
    assert_eq!(env.treats(), 12);
    drop(store);
    let mut store = env.fold();
    assert!(
        !earn_turn(&env, &mut store, "w", "t", T0 + 64_000),
        "capped_hour"
    );
    assert!(earn_turn(
        &env,
        &mut store,
        "w",
        "t2",
        T0 + 3_600_000 + 1000
    ));
    // X1: a turn earns once even when replayed under a new digest.
    assert!(!earn_turn(&env, &mut store, "w", "t2", T0 + 3_700_000));
}

#[test]
fn off_drops_everything_and_grants_nothing() {
    let env = Env::new();
    env.set("UPDATE meta SET enabled = 0");
    let mut store = env.fold();
    assert!(!earn_turn(&env, &mut store, "a", "t1", T0));
    assert_eq!(
        env.int("SELECT COUNT(*) FROM agent_events WHERE outcome = 'dropped_off'"),
        3
    );
    assert_eq!(env.int("SELECT COUNT(*) FROM agent_sessions"), 0);
    assert_eq!(env.int("SELECT COUNT(*) FROM client_hooks"), 0);
    assert_eq!(env.int("SELECT finished_seq + hello_seq FROM meta"), 0);
}

#[test]
fn test_records_touch_no_client_or_session_state() {
    let env = Env::new();
    env.set("UPDATE meta SET test_state = 'running', test_at_ms = 1, test_client = 'codex'");
    let mut store = env.fold();
    let mut r = turn(env.rec("codex", "turn_start", T0), "codepet-ready-check");
    r.test = true;
    let mut d = turn(
        env.rec("codex", "turn_done", T0 + 1500),
        "codepet-ready-check",
    );
    d.test = true;
    store.fold_batch(&[r, d], 0).unwrap();
    assert_eq!(
        env.int("SELECT COUNT(*) FROM agent_events WHERE outcome = 'test' AND test = 1"),
        2
    );
    assert_eq!(env.int("SELECT COUNT(*) FROM client_hooks"), 0);
    assert_eq!(env.int("SELECT COUNT(*) FROM agent_sessions"), 0);
    assert_eq!(env.treats(), 0);
    assert_eq!(env.int("SELECT hello_seq + finished_seq FROM meta"), 0);
}

#[test]
fn hello_greets_once_per_connect_and_clears_the_step() {
    let env = Env::new();
    env.set(
        "INSERT INTO client_hooks (client, connected, needs_step) VALUES ('codex', 1, 'codex_trust')",
    );
    let mut store = env.fold();
    store
        .fold_batch(&[turn(env.rec("codex", "turn_start", T0), "1")], 0)
        .unwrap();
    store
        .fold_batch(&[turn(env.rec("codex", "turn_done", T0 + 5), "1")], 0)
        .unwrap();
    assert_eq!(env.int("SELECT hello_seq FROM meta"), 1);
    assert_eq!(
        env.int("SELECT needs_step IS NULL AND first_seen_ms = 1792000000000 AND events_total = 2 FROM client_hooks"),
        1
    );
    // A new Connect clears first_seen_ms: the next real event greets again.
    env.set("UPDATE client_hooks SET first_seen_ms = NULL");
    store
        .fold_batch(&[turn(env.rec("codex", "turn_start", T0 + 10), "2")], 0)
        .unwrap();
    assert_eq!(env.int("SELECT hello_seq FROM meta"), 2);
    assert_eq!(
        env.raw()
            .query_row("SELECT hello_app FROM meta", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "codex"
    );
}

fn checksum(env: &Env, tables: &[&str]) -> Vec<String> {
    let c = env.raw();
    tables
        .iter()
        .map(|t| {
            let mut stmt = c.prepare(&format!("SELECT * FROM {t}")).unwrap();
            let n = stmt.column_count();
            let rows: Vec<String> = stmt
                .query_map([], |r| {
                    Ok((0..n)
                        .map(|i| format!("{:?}", r.get_ref(i).unwrap()))
                        .collect::<Vec<_>>()
                        .join("|"))
                })
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();
            format!("{t}:{}", rows.join("\n"))
        })
        .collect()
}

#[test]
fn folding_never_touches_care_life_or_lifecycle_tables() {
    let env = Env::new();
    let c = env.raw();
    let protected: Vec<String> = c
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT IN
             ('agent_events','agent_sessions','activity_turns','activity_xp','client_hooks','meta')
             AND name NOT LIKE 'sqlite_%'",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert!(protected.iter().any(|t| t == "pet_care"));
    assert!(protected.iter().any(|t| t == "lives"));
    let names: Vec<&str> = protected.iter().map(String::as_str).collect();
    let before = checksum(&env, &names);
    let mut store = env.fold();
    for i in 0..5 {
        earn_turn(&env, &mut store, &format!("s{i}"), "t", T0 + i * 40_000);
    }
    let mut f = turn(env.rec("claude-code", "turn_failed", T0 + 300_000), "q");
    f.note = Some("rate_limit".into());
    store.fold_batch(&[f], 0).unwrap();
    assert!(env.treats() > 0);
    assert_eq!(checksum(&env, &names), before, "E1");
}

#[test]
fn fault_before_commit_keeps_the_batch_uncommitted() {
    let env = Env::new();
    let mut store = env.fold();
    let r = turn(env.rec("claude-code", "turn_start", T0), "t1");
    test_fault::arm(Some("fold_before_commit"));
    let res = catch_unwind(AssertUnwindSafe(|| {
        store.fold_batch(std::slice::from_ref(&r), 1)
    }));
    test_fault::arm(None);
    assert!(res.is_err());
    assert_eq!(env.int("SELECT COUNT(*) FROM agent_events"), 0);
    assert_eq!(env.int("SELECT events_dropped FROM meta"), 0);
    let mut store = env.fold();
    assert_eq!(store.fold_batch(&[r], 0).unwrap().recorded, 1);
}

#[test]
fn retention_is_hourly_limited_and_coarsens() {
    let env = Env::new();
    let mut store = env.fold();
    earn_turn(&env, &mut store, "a", "t", T0 + 1234);
    env.set(
        "INSERT INTO agent_sessions (session_key, client, interactive, state, state_since_ms,
           last_event_ms) VALUES ('00000000000000000000000000000001', 'codex', 1, 'ended', 1, 1)",
    );
    env.clock.set(T0 + 31 * 24 * 3_600_000);
    assert!(store.agent_retention().unwrap());
    assert!(!store.agent_retention().unwrap(), "at most hourly");
    assert_eq!(env.int("SELECT COUNT(*) FROM agent_events"), 0, "7 days");
    assert_eq!(env.int("SELECT COUNT(*) FROM agent_sessions"), 0, "30 days");
    assert_eq!(env.treats(), 1, "treat history is kept");
    assert_eq!(
        env.int("SELECT earned_at_ms % 3600000 FROM activity_turns"),
        0,
        "coarsened to the hour"
    );
    env.clock.advance(3_600_000);
    assert!(store.agent_retention().unwrap());
}

/// Heavy use (more than 1000 events per active hour) cannot outgrow the bounds: one pass
/// brings `agent_events` back to its row cap and drops everything older than 7 days, and
/// an over-cap table is trimmed even inside the hourly gate.
#[test]
fn retention_keeps_agent_events_within_its_bounds_under_heavy_use() {
    let env = Env::new();
    let mut store = env.fold();
    let cap = i64::from(AGENT_EVENTS_MAX_ROWS);
    let insert = |from: i64, n: i64, at: i64| {
        env.set(&format!(
            "WITH RECURSIVE k(i) AS (SELECT {from} UNION ALL SELECT i + 1 FROM k WHERE i < {})
             INSERT INTO agent_events (dedupe_key, client, kind, at_ms, outcome, interactive)
             SELECT printf('%032x', i), 'claude-code', 'activity', {at}, 'folded', 1 FROM k",
            from + n - 1
        ));
    };
    // 1700 rows older than 7 days, then cap + 1800 recent ones.
    insert(1, 1700, T0 - 8 * DAY_MS);
    insert(10_000, cap + 1800, T0);
    assert!(store.agent_retention().unwrap());
    assert_eq!(env.int("SELECT COUNT(*) FROM agent_events"), cap);
    assert_eq!(
        env.int(&format!(
            "SELECT COUNT(*) FROM agent_events WHERE at_ms < {}",
            T0 - AGENT_EVENTS_KEEP_MS
        )),
        0
    );
    // The newest rows are the ones kept.
    assert_eq!(
        env.int("SELECT MIN(id) FROM agent_events"),
        env.int("SELECT MAX(id) FROM agent_events") - cap + 1
    );
    // Within the hour: nothing to do at the cap, but an over-cap burst is trimmed at once.
    assert!(!store.agent_retention().unwrap(), "at the cap: hourly gate");
    insert(100_000, 600, T0);
    assert!(store.agent_retention().unwrap(), "over the cap: runs");
    assert_eq!(env.int("SELECT COUNT(*) FROM agent_events"), cap);
}

/// Two byte-identical permission prompts in one turn, an activity between them, all inside
/// one 3 s bucket (notes.md §6 item 2): the second "needs you" is not a duplicate. A
/// delivery of the same firing (same millisecond) still is, and activity keeps the bucket.
#[test]
fn identical_state_changes_in_one_bucket_are_not_deduped() {
    let env = Env::new();
    let mut store = env.fold();
    let t = T0 - T0.rem_euclid(3000);
    let prompt = |at| InboxRecord {
        note: Some("permission_prompt".into()),
        dig: "353401f5cd906398".into(),
        ..env.rec("claude-code", "waiting", at)
    };
    env.fold_one(
        &mut store,
        turn(env.rec("claude-code", "turn_start", t), "p1"),
    );
    env.fold_one(
        &mut store,
        tool(
            turn(env.rec("claude-code", "activity", t + 100), "p1"),
            "tu1",
        ),
    );
    assert_eq!(env.fold_one(&mut store, prompt(t + 200)).recorded, 1);
    assert_eq!(
        env.state_of("claude-code", "s1").as_deref(),
        Some("waiting")
    );
    env.fold_one(
        &mut store,
        tool(
            turn(env.rec("claude-code", "activity", t + 300), "p1"),
            "tu2",
        ),
    );
    assert_eq!(
        env.state_of("claude-code", "s1").as_deref(),
        Some("working")
    );
    let again = env.fold_one(&mut store, prompt(t + 400));
    assert_eq!(
        (again.recorded, again.duplicates),
        (1, 0),
        "a second prompt"
    );
    assert_eq!(
        env.state_of("claude-code", "s1").as_deref(),
        Some("waiting")
    );
    let dup = env.fold_one(&mut store, prompt(t + 400));
    assert_eq!((dup.recorded, dup.duplicates), (0, 1), "the same delivery");
    assert_eq!(
        env.int("SELECT COUNT(*) FROM agent_events WHERE kind = 'waiting'"),
        2
    );
    // Activity keeps the 3 s bucket: an identical repeat is a duplicate delivery.
    let a = InboxRecord {
        dig: "00000000000000aa".into(),
        ..tool(
            turn(env.rec("claude-code", "activity", t + 500), "p1"),
            "tu3",
        )
    };
    assert_eq!(env.fold_one(&mut store, a.clone()).recorded, 1);
    let b = InboxRecord {
        at_ms: t + 2900,
        ..a
    };
    assert_eq!(env.fold_one(&mut store, b).duplicates, 1);
}

#[test]
fn the_export_carries_the_agent_block_and_the_jar() {
    let env = Env::new();
    let mut store = env.fold();
    let b = store
        .fold_batch(&[turn(env.rec("claude-code", "turn_start", T0), "t1")], 0)
        .unwrap();
    assert!(b.changed && b.urgent);
    assert!(store.export_bar_state().unwrap());
    let bar: Value =
        serde_json::from_slice(&std::fs::read(env.paths.bar_state_path()).unwrap()).unwrap();
    assert_eq!(bar["agent"]["state"], "working");
    assert_eq!(bar["agent"]["apps"][0]["id"], "claude-code");
    assert_eq!(bar["agent"]["hello_app"], "claude-code");
    assert_eq!(bar["agent"]["stale_at_ms"], T0 + 600_000);
    earn_turn(&env, &mut store, "b", "t", T0 + 1000);
    assert!(store.export_bar_state().unwrap());
    let bar: Value =
        serde_json::from_slice(&std::fs::read(env.paths.bar_state_path()).unwrap()).unwrap();
    assert_eq!(bar["treats"]["jar"], 1);
    assert_eq!(bar["agent"]["finished_seq"], 1);
    assert_eq!(bar["agent"]["finished_treat"], true);
    assert_eq!(bar["agent"]["finished_app"], "codex");
    // An activity that changes nothing export-relevant is not urgent.
    let b = store
        .fold_batch(
            &[tool(
                turn(env.rec("claude-code", "activity", T0 + 2000), "t1"),
                "z",
            )],
            0,
        )
        .unwrap();
    assert!(b.changed && !b.urgent);
}

#[test]
fn the_fold_refuses_other_schemas_and_unsafe_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    let xdg = tmp.path().join("state");
    let paths = StatePaths::resolve(|k| match k {
        "XDG_STATE_HOME" => Some(xdg.clone().into()),
        _ => None,
    })
    .unwrap();
    let clock = Arc::new(ManualClock::new(T0));
    assert!(Store::open_for_fold(&paths.state_dir, clock.clone()).is_err());
    assert!(
        !paths.state_dir.exists(),
        "the fold never creates the state dir"
    );
    // A v0.2 (v4) database is refused, never migrated.
    drop(Store::open_with(&paths, clock.clone(), &crate::schema::MIGRATIONS[..4]).unwrap());
    assert!(matches!(
        Store::open_for_fold(&paths.state_dir, clock),
        Err(StorageError::SchemaUnsupported(4))
    ));
}

#[test]
fn delayed_events_across_fold_batches_preserve_turn_completion_and_approval() {
    let env = Env::new();
    env.clock.set(T0 + 10_000);
    let mut store = env.fold();
    env.fold_one(
        &mut store,
        turn(env.rec("codex", "turn_start", T0 + 2_000), "current"),
    );
    env.fold_one(
        &mut store,
        turn(env.rec("codex", "turn_start", T0 + 1_500), "old"),
    );
    env.fold_one(&mut store, env.rec("codex", "waiting", T0 + 1_700));
    assert_eq!(env.state_of("codex", "s1").as_deref(), Some("working"));
    let mut done = turn(env.rec("codex", "turn_done", T0 + 5_000), "current");
    done.tools = 1;
    env.fold_one(&mut store, done);
    assert_eq!(env.state_of("codex", "s1").as_deref(), Some("idle"));
    assert_eq!(env.int("SELECT finished_seq FROM meta"), 1);
    assert_eq!(env.treats(), 1);
    for (kind, at) in [("waiting", 2_000), ("activity", 3_000), ("waiting", 2_500)] {
        let mut rec = env.rec("hermes", kind, T0 + at);
        rec.session = None;
        rec.akey = Some("approval".into());
        env.fold_one(&mut store, rec);
    }
    let state: String = env
        .raw()
        .query_row(
            "SELECT state FROM agent_sessions WHERE approval=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(state, "idle");
}
