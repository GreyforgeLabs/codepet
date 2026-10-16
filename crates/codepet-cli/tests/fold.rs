//! The detached fold (SPEC-v0.3 §5.1, §6.4, §9.2): a hook that wins `fold.lock` starts
//! exactly one fold, which folds the inbox, exports `bar-state.json` and exits; bursts
//! keep at most one fold process and lose nothing; failures back off.
mod hookkit;

use hookkit::{assert_silent_ok, Hk, BIN};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

fn claude(event: &str, extra: &str) -> Vec<u8> {
    format!(r#"{{"hook_event_name":"{event}","session_id":"sess-1"{extra}}}"#).into_bytes()
}
const CLI: [(&str, &str); 1] = [("CLAUDE_CODE_ENTRYPOINT", "cli")];

#[test]
fn a_hook_starts_one_fold_that_folds_and_exports() {
    let hk = Hk::with_db();
    for (event, extra) in [
        ("SessionStart", r#","source":"startup""#),
        ("UserPromptSubmit", r#","prompt_id":"p1","prompt":"hi""#),
        (
            "PreToolUse",
            r#","prompt_id":"p1","tool_use_id":"tu1","tool_name":"Bash""#,
        ),
        ("Stop", r#","prompt_id":"p1""#),
    ] {
        assert_silent_ok(
            &hk.stored("claude-code", &claude(event, extra), &CLI),
            event,
        );
    }
    let done = hk.wait_for(Duration::from_secs(5), || {
        hk.inbox_names().is_empty()
            && hk
                .bar_state()
                .is_some_and(|b| b["agent"]["finished_seq"] == 1 && b["treats"]["jar"] == 1)
    });
    let bar = hk.bar_state().unwrap_or_default();
    assert!(done, "inbox {:?}, bar {bar}", hk.inbox_names());
    assert_eq!(bar["agent"]["hello_seq"], 1);
    assert_eq!(bar["agent"]["hello_app"], "claude-code");
    assert_eq!(bar["agent"]["finished_treat"], true);
    assert_eq!(bar["agent"]["state"], "idle");
    let db = hk.db();
    assert_eq!(
        db.int("SELECT COUNT(*) FROM agent_events WHERE outcome = 'folded'"),
        4
    );
    assert_eq!(
        db.int("SELECT events_total FROM client_hooks WHERE client = 'claude-code'"),
        4
    );
    assert!(db.int("SELECT last_seen_ms FROM client_hooks") > 0);
    assert!(!hk.state.join("fold.backoff").exists());
    assert!(hk.wait_for(Duration::from_secs(2), || hk.fold_processes() == 0));
}

/// §9.2 bursts: 20 concurrent writers (500 activity events): at most one fold process at
/// any sample (every 5 ms), the inbox empties within 1 s of the last event, nothing lost,
/// and (§8 rule 6) `bar-state.json` is replaced at most about 4 times a second, since
/// tools counted in a working turn are not urgent.
#[test]
fn bursts_keep_one_fold_and_lose_nothing() {
    use std::os::unix::fs::MetadataExt;
    let hk = Arc::new(Hk::with_db());
    let start = claude("UserPromptSubmit", r#","prompt_id":"p1""#);
    assert_silent_ok(&hk.stored("claude-code", &start, &CLI), "start");
    assert!(hk.wait_for(Duration::from_secs(3), || {
        hk.inbox_names().is_empty() && hk.fold_processes() == 0
    }));
    let stop = Arc::new(AtomicBool::new(false));
    let max = Arc::new(AtomicUsize::new(0));
    let renames = Arc::new(AtomicUsize::new(0));
    let bar = hk.state.join("bar-state.json");
    let sampler = {
        let (hk, stop, max, renames) = (hk.clone(), stop.clone(), max.clone(), renames.clone());
        std::thread::spawn(move || {
            let mut last = fs::metadata(&bar).map(|m| m.ino()).ok();
            while !stop.load(Ordering::SeqCst) {
                max.fetch_max(hk.fold_processes(), Ordering::SeqCst);
                let ino = fs::metadata(&bar).map(|m| m.ino()).ok();
                if ino != last {
                    renames.fetch_add(1, Ordering::SeqCst);
                    last = ino;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        })
    };
    let t0 = std::time::Instant::now();
    let writers: Vec<_> = (0..20)
        .map(|w| {
            let hk = hk.clone();
            std::thread::spawn(move || {
                for i in 0..25 {
                    let p = claude(
                        "PreToolUse",
                        &format!(r#","prompt_id":"p1","tool_use_id":"tu-{w}-{i}""#),
                    );
                    assert_silent_ok(&hk.hook("claude-code", &p, &CLI), "burst");
                }
            })
        })
        .collect();
    for w in writers {
        w.join().unwrap();
    }
    let emptied = hk.wait_for(Duration::from_secs(1), || hk.inbox_names().is_empty());
    let settled = hk.wait_for(Duration::from_secs(3), || hk.fold_processes() == 0);
    let window = t0.elapsed().as_secs_f64();
    stop.store(true, Ordering::SeqCst);
    sampler.join().unwrap();
    let renames = renames.load(Ordering::SeqCst);
    eprintln!("burst: {renames} bar-state.json replacements in {window:.2} s");
    assert!(
        renames as f64 <= 1.0 + 4.0 * window.ceil(),
        "{renames} replacements in {window:.2} s: the export rate limit is not holding"
    );
    assert!(
        emptied,
        "inbox not empty 1 s after the last event: {}",
        hk.inbox_names().len()
    );
    assert!(settled);
    eprintln!(
        "burst: max concurrent fold processes sampled = {}",
        max.load(Ordering::SeqCst)
    );
    assert!(
        max.load(Ordering::SeqCst) <= 1,
        "fold processes seen at once: {max:?}"
    );
    let db = hk.db();
    assert_eq!(
        db.int("SELECT COUNT(*) FROM agent_events"),
        501,
        "nothing lost"
    );
    assert_eq!(db.int("SELECT open_turn_tools FROM agent_sessions"), 500);
}

/// A fold that always fails (here: a schema v4 database) writes `fold.backoff` with a
/// future mtime; hooks then stop starting folds, so the spawn count stays bounded.
#[test]
fn a_failing_fold_backs_off() {
    let hk = Hk::new();
    let out = hk.cmd(["state"]).output().unwrap();
    assert!(out.status.success());
    let p = claude("Stop", r#","prompt_id":"p1""#);
    assert_silent_ok(&hk.hook("claude-code", &p, &CLI), "first");
    let backoff = hk.state.join("fold.backoff");
    assert!(hk.wait_for(Duration::from_secs(3), || backoff.exists()));
    assert!(hk.wait_for(Duration::from_secs(2), || hk.fold_processes() == 0));
    let read =
        || -> serde_json::Value { serde_json::from_slice(&fs::read(&backoff).unwrap()).unwrap() };
    assert_eq!(read()["count"], 1);
    assert_eq!(read()["class"], "schema");
    let retry = fs::metadata(&backoff).unwrap().modified().unwrap();
    assert!(retry > SystemTime::now() + Duration::from_secs(2));
    for i in 0..30 {
        let p = claude(
            "PreToolUse",
            &format!(r#","prompt_id":"p1","tool_use_id":"t{i}""#),
        );
        assert_silent_ok(&hk.hook("claude-code", &p, &CLI), "during backoff");
    }
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(read()["count"], 1, "no fold ran during the backoff");
    assert_eq!(hk.inbox_names().len(), 31, "records wait in the inbox");
    // A manual run fails the same way and doubles the delay.
    let out = hk
        .cmd(["fold", "--state-dir", hk.state.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("schema version 4"));
    assert_eq!(read()["count"], 2);
    let retry2 = fs::metadata(&backoff).unwrap().modified().unwrap();
    assert!(retry2 > SystemTime::now() + Duration::from_secs(7));
}

#[test]
fn a_manual_fold_takes_the_lock_itself_and_yields_to_a_holder() {
    let hk = Hk::with_db();
    hk.block_folds();
    let p = claude("UserPromptSubmit", r#","prompt_id":"p1""#);
    assert_silent_ok(&hk.hook("claude-code", &p, &CLI), "event");
    assert_eq!(hk.inbox_names().len(), 1);
    // Another holder: the manual fold exits 0 at once and leaves the record.
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(hk.state.join("fold.lock"))
        .unwrap();
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive).unwrap();
    let dir = hk.state.to_str().unwrap().to_owned();
    let out = hk.cmd(["fold", "--state-dir", &dir]).output().unwrap();
    assert_silent_ok(&out, "busy");
    assert_eq!(hk.inbox_names().len(), 1);
    drop(lock);
    let out = hk.cmd(["fold", "--state-dir", &dir]).output().unwrap();
    assert_silent_ok(&out, "manual fold");
    assert!(hk.inbox_names().is_empty());
    assert!(
        !hk.state.join("fold.backoff").exists(),
        "success removes the backoff"
    );
    assert_eq!(hk.bar_state().unwrap()["agent"]["state"], "working");
    // Bad arguments are a usage error (a fold is internal, but safe to run by hand).
    for args in [vec!["fold"], vec!["fold", "--state-dir", "relative"]] {
        let out = std::process::Command::new(BIN)
            .args(&args)
            .env_clear()
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}");
    }
}

#[cfg(debug_assertions)]
#[test]
fn a_fold_killed_before_commit_leaves_the_inbox_for_the_next() {
    let hk = Hk::with_db();
    hk.block_folds();
    for i in 0..3 {
        let p = claude(
            "PreToolUse",
            &format!(r#","prompt_id":"p1","tool_use_id":"t{i}""#),
        );
        assert_silent_ok(&hk.hook("claude-code", &p, &CLI), "event");
    }
    let dir = hk.state.to_str().unwrap().to_owned();
    let out = hk
        .cmd(["fold", "--state-dir", &dir])
        .env("CODEPET_FAULT", "fold_before_commit")
        .output()
        .unwrap();
    assert!(!out.status.success(), "aborted");
    assert_eq!(hk.inbox_names().len(), 3);
    assert_eq!(hk.db().int("SELECT COUNT(*) FROM agent_events"), 0);
    let out = hk.cmd(["fold", "--state-dir", &dir]).output().unwrap();
    assert_silent_ok(&out, "retry");
    assert!(hk.inbox_names().is_empty());
    assert_eq!(hk.db().int("SELECT COUNT(*) FROM agent_events"), 3);
}

/// Off: the shim stops before the binary, and anything that slipped in is dropped by the
/// fold without a treat.
#[test]
fn off_starts_nothing_and_the_fold_drops_stragglers() {
    let hk = Hk::with_db();
    fs::write(hk.state.join("off"), b"").unwrap();
    let p = claude("UserPromptSubmit", r#","prompt_id":"p1""#);
    assert_silent_ok(&hk.stored("claude-code", &p, &CLI), "off");
    assert!(hk.inbox_names().is_empty());
    assert_eq!(hk.fold_processes(), 0);
    fs::remove_file(hk.state.join("off")).unwrap();
    rusqlite::Connection::open(hk.state.join("codepet.sqlite3"))
        .unwrap()
        .execute_batch("UPDATE meta SET enabled = 0")
        .unwrap();
    assert_silent_ok(&hk.stored("claude-code", &p, &CLI), "straggler");
    assert!(hk.wait_for(Duration::from_secs(3), || hk.inbox_names().is_empty()));
    assert!(hk.wait_for(Duration::from_secs(2), || hk.fold_processes() == 0));
    let db = hk.db();
    assert_eq!(
        db.int("SELECT COUNT(*) FROM agent_events WHERE outcome = 'dropped_off'"),
        1
    );
    assert_eq!(db.int("SELECT COUNT(*) FROM agent_sessions"), 0);
}

/// §5.1 ordering invariant, `drain_once` side: `codepet state` holds fold.lock while it
/// folds its batch; a hook that writes meanwhile finds the lock busy and starts nothing.
/// After releasing the lock, `drain_once` looks again and hands the record to a fold.
#[cfg(debug_assertions)]
#[ignore = "needs WP1: `codepet state` opens only schema v4 until the v5 migration lands; un-ignore in the WP1 rebase (fold.rs unit tests cover drain_once now)"]
#[test]
fn drain_once_hands_records_written_meanwhile_to_a_fold() {
    let hk = Hk::with_db();
    hk.block_folds();
    let start = claude("SessionStart", r#","source":"startup""#);
    assert_silent_ok(&hk.stored("claude-code", &start, &CLI), "start");
    assert_eq!(hk.inbox_names().len(), 1);
    hk.unblock_folds();
    // The fault holds the lock for 1.5 s after the batch.
    let mut state = hk
        .cmd(["state"])
        .env("CODEPET_FAULT", "drain_once_slow")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(
        hk.wait_for(Duration::from_secs(3), || hk.inbox_names().is_empty()),
        "drain_once folded its batch"
    );
    let p = claude("UserPromptSubmit", r#","prompt_id":"p1""#);
    assert_silent_ok(&hk.stored("claude-code", &p, &CLI), "while held");
    assert_eq!(hk.inbox_names().len(), 1, "the lock was busy: no fold");
    assert_eq!(hk.fold_processes(), 0);
    assert!(state.wait().unwrap().success());
    assert!(
        hk.wait_for(Duration::from_secs(3), || hk.inbox_names().is_empty()),
        "stranded: {:?}",
        hk.inbox_names()
    );
    assert!(hk.wait_for(Duration::from_secs(3), || {
        hk.fold_processes() == 0
            && hk
                .bar_state()
                .is_some_and(|b| b["agent"]["state"] == "working")
    }));
}

/// §6.3 `drain_once`: with the lock held by another holder it folds nothing; otherwise it
/// folds a batch and hands the rest (more than 64 records) to a fold.
#[ignore = "needs WP1: `codepet state` opens only schema v4 until the v5 migration lands; un-ignore in the WP1 rebase (fold.rs unit tests cover drain_once now)"]
#[test]
fn drain_once_skips_a_held_lock_and_hands_off_the_rest() {
    let hk = Hk::with_db();
    hk.block_folds();
    let start = claude("UserPromptSubmit", r#","prompt_id":"p1""#);
    assert_silent_ok(&hk.hook("claude-code", &start, &CLI), "start");
    for i in 0..99 {
        let p = claude(
            "PreToolUse",
            &format!(r#","prompt_id":"p1","tool_use_id":"t{i}""#),
        );
        assert_silent_ok(&hk.hook("claude-code", &p, &CLI), "tool");
    }
    assert_eq!(hk.inbox_names().len(), 100);
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(hk.state.join("fold.lock"))
        .unwrap();
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive).unwrap();
    let out = hk.cmd(["state"]).output().unwrap();
    assert!(out.status.success());
    assert_eq!(hk.inbox_names().len(), 100, "a held lock: nothing folded");
    drop(lock);
    // The backoff is still in the future: drain_once folds anyway (§5.1), and its success
    // clears the backoff so the rest goes to a fold.
    let out = hk.cmd(["state"]).output().unwrap();
    assert!(out.status.success());
    assert!(
        hk.wait_for(Duration::from_secs(5), || hk.inbox_names().is_empty()),
        "left: {}",
        hk.inbox_names().len()
    );
    assert!(hk.wait_for(Duration::from_secs(3), || hk.fold_processes() == 0));
    assert_eq!(hk.db().int("SELECT COUNT(*) FROM agent_events"), 100);
    assert_eq!(
        hk.db().int("SELECT open_turn_tools FROM agent_sessions"),
        99
    );
}

/// §5.1: a fold that spends its run budget with records left hands them to a fresh fold
/// (its own budget) instead of leaving them for the next hook. The debug fault makes each
/// run fold one batch and spend its budget, so 200 records need several runs.
#[cfg(debug_assertions)]
#[test]
fn a_fold_past_its_budget_hands_the_rest_to_a_fresh_fold() {
    let hk = Hk::with_db();
    hk.block_folds();
    let start = claude("UserPromptSubmit", r#","prompt_id":"p1""#);
    assert_silent_ok(&hk.hook("claude-code", &start, &CLI), "start");
    for i in 0..199 {
        let p = claude(
            "PreToolUse",
            &format!(r#","prompt_id":"p1","tool_use_id":"t{i}""#),
        );
        assert_silent_ok(&hk.hook("claude-code", &p, &CLI), "tool");
    }
    hk.unblock_folds();
    let p = claude("PreToolUse", r#","prompt_id":"p1","tool_use_id":"last""#);
    let mut env = CLI.to_vec();
    env.push(("CODEPET_FAULT", "fold_short_run"));
    assert_silent_ok(&hk.hook("claude-code", &p, &env), "trigger");
    assert!(
        hk.wait_for(Duration::from_secs(10), || hk.inbox_names().is_empty()),
        "stranded after a spent budget: {}",
        hk.inbox_names().len()
    );
    assert!(hk.wait_for(Duration::from_secs(3), || hk.fold_processes() == 0));
    assert_eq!(hk.db().int("SELECT COUNT(*) FROM agent_events"), 201);
    assert!(!hk.state.join("fold.backoff").exists());
}

/// Two byte-identical permission prompts in one turn with a tool between them, all inside
/// one 3 s bucket (contract amendment, notes.md §5 item 2): the pet ends "waiting".
#[cfg(debug_assertions)]
#[test]
fn a_second_identical_permission_prompt_still_means_waiting() {
    let hk = Hk::with_db();
    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let base = now - now.rem_euclid(3000);
    let prompt = r#","notification_type":"permission_prompt","message":"Claude needs your permission to use Bash""#;
    let steps = [
        ("SessionStart", r#","source":"startup""#.to_owned()),
        ("UserPromptSubmit", r#","prompt_id":"p1""#.to_owned()),
        (
            "PreToolUse",
            r#","prompt_id":"p1","tool_use_id":"tu1","tool_name":"Bash""#.to_owned(),
        ),
        ("Notification", prompt.to_owned()),
        (
            "PreToolUse",
            r#","prompt_id":"p1","tool_use_id":"tu2","tool_name":"Bash""#.to_owned(),
        ),
        ("Notification", prompt.to_owned()),
    ];
    for (i, (event, extra)) in steps.iter().enumerate() {
        let at = (base + 1 + i as i64).to_string();
        let env = [
            ("CLAUDE_CODE_ENTRYPOINT", "cli"),
            ("CODEPET_TEST_NOW_MS", &at),
        ];
        assert_silent_ok(
            &hk.stored("claude-code", &claude(event, extra), &env),
            event,
        );
    }
    assert!(hk.wait_for(Duration::from_secs(5), || {
        hk.inbox_names().is_empty() && hk.fold_processes() == 0
    }));
    let bar = hk.bar_state().unwrap();
    assert_eq!(bar["agent"]["state"], "waiting", "{bar}");
    assert_eq!(
        hk.db()
            .int("SELECT COUNT(*) FROM agent_events WHERE kind = 'waiting' AND outcome = 'folded'"),
        2
    );
}

/// §9.2 "a fold that always fails" with a read-only database: one failure writes the
/// backoff, and hooks start no fold until it expires.
#[test]
fn a_read_only_database_backs_off() {
    let hk = Hk::with_db();
    let db = hk.state.join("codepet.sqlite3");
    fs::set_permissions(&db, fs::Permissions::from_mode(0o400)).unwrap();
    let p = claude("UserPromptSubmit", r#","prompt_id":"p1""#);
    assert_silent_ok(&hk.hook("claude-code", &p, &CLI), "first");
    let backoff = hk.state.join("fold.backoff");
    assert!(hk.wait_for(Duration::from_secs(5), || backoff.exists()));
    assert!(hk.wait_for(Duration::from_secs(3), || hk.fold_processes() == 0));
    let read =
        || -> serde_json::Value { serde_json::from_slice(&fs::read(&backoff).unwrap()).unwrap() };
    assert_eq!(read()["count"], 1);
    let class = read()["class"].as_str().unwrap().to_owned();
    eprintln!("read-only database: backoff class {class}");
    assert_ne!(
        class, "busy",
        "a read-only database is not a transient failure"
    );
    for i in 0..20 {
        let p = claude(
            "PreToolUse",
            &format!(r#","prompt_id":"p1","tool_use_id":"t{i}""#),
        );
        assert_silent_ok(&hk.hook("claude-code", &p, &CLI), "during backoff");
    }
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(hk.fold_processes(), 0);
    assert_eq!(read()["count"], 1, "no fold ran during the backoff");
    assert_eq!(hk.inbox_names().len(), 21, "records wait in the inbox");
    fs::set_permissions(&db, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(hk.db().int("SELECT COUNT(*) FROM agent_events"), 0);
}
