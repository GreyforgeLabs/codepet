//! Hook latency micro-benchmark (SPEC-v0.3 §5.9), release builds only:
//! `cargo test --release -p codepet --test hook_latency -- --nocapture`.
//!
//! Wall time from spawn to exit of what a client runs, stdin = one payload then EOF:
//! - Off: the shim with the `off` flag (budget p50 ≤ 3 ms);
//! - hook: shim + `codepet hook` with a fold already running (lock busy), so the run is
//!   tokenizer + normalization + inbox write + lock attempt (budget p50 ≤ 10 ms, p99 ≤ 20 ms);
//! - hook+spawn: the same with the lock free, so the hook also starts the detached fold
//!   (loaded-host budget p99 ≤ 40 ms, the Grok fallback threshold of §5.4);
//! - `sh -c` stored command (Claude Code, Codex, Grok add one shell): reported only.
//!
//! This measures this machine at test time, with whatever else it is running; numbers are
//! printed for the WP7 record. It is not the 200-run idle/`stress-ng` harness of WP7.
mod hookkit;

use hookkit::Hk;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const RUNS: usize = 200;
const WARMUP: usize = 20;

fn payload(i: usize) -> Vec<u8> {
    format!(
        r#"{{"session_id":"5f0c2a1e-7b3d-4c9a-8e21-0d6b9a4f3c11","prompt_id":"550e8400-e29b-41d4-a716-446655440000","transcript_path":"/home/u/.claude/projects/p/x.jsonl","cwd":"/home/u/p","permission_mode":"default","hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{{"command":"cargo test --workspace","description":"Run the tests","timeout":120000}},"tool_use_id":"toolu_01ABC{i:06}"}}"#
    )
    .into_bytes()
}

fn time_one(mut cmd: Command, stdin: &[u8]) -> Duration {
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let t = Instant::now();
    let mut child = cmd.spawn().unwrap();
    {
        let mut pipe = child.stdin.take().unwrap();
        pipe.write_all(stdin).unwrap();
    }
    let status = child.wait().unwrap();
    let took = t.elapsed();
    assert!(status.success());
    took
}

struct Stats {
    p50: f64,
    p90: f64,
    p99: f64,
    max: f64,
}
fn stats(mut v: Vec<Duration>) -> Stats {
    v.sort();
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    let at = |q: f64| ms(v[((v.len() as f64 * q).ceil() as usize).clamp(1, v.len()) - 1]);
    Stats {
        p50: at(0.50),
        p90: at(0.90),
        p99: at(0.99),
        max: ms(*v.last().unwrap()),
    }
}

fn measure(
    label: &str,
    mut make: impl FnMut(usize) -> (Command, Vec<u8>),
    mut between: impl FnMut(),
) -> Stats {
    let mut v = Vec::with_capacity(RUNS);
    for i in 0..WARMUP + RUNS {
        let (cmd, stdin) = make(i);
        let d = time_one(cmd, &stdin);
        if i >= WARMUP {
            v.push(d);
        }
        between();
    }
    let s = stats(v);
    println!(
        "{label:<34} n={RUNS} p50 {:6.2} ms  p90 {:6.2} ms  p99 {:6.2} ms  max {:6.2} ms",
        s.p50, s.p90, s.p99, s.max
    );
    s
}

/// Empties the inbox between runs (steady state) and returns how many records it held.
fn clear_inbox(hk: &Hk) -> usize {
    let names = hk.inbox_names();
    for n in &names {
        let _ = fs::remove_file(hk.state.join("inbox").join(n));
    }
    names.len()
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only latency budget: cargo test --release --test hook_latency"
)]
fn hook_latency_meets_the_budget() {
    let load = fs::read_to_string("/proc/loadavg").unwrap_or_default();
    let cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
    let load1: f64 = load
        .split_whitespace()
        .next()
        .and_then(|l| l.parse().ok())
        .unwrap_or(0.0);
    // §5.9 distinguishes an idle host from "this machine, loaded".
    let idle = load1 < cpus as f64 * 0.5;
    println!(
        "loadavg at start: {} ({cpus} CPUs: {})",
        load.trim(),
        if idle { "idle" } else { "loaded" }
    );
    let hk = Hk::with_db();
    let env = |c: &mut Command| {
        c.env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("CLAUDE_CODE_ENTRYPOINT", "cli");
    };
    let shim_cmd = |client: &str| {
        let mut c = Command::new(&hk.shim);
        c.arg(client);
        env(&mut c);
        c
    };

    fs::write(hk.state.join("off"), b"").unwrap();
    let off = measure(
        "off (shim, off flag)",
        |i| (shim_cmd("codex"), payload(i)),
        || {},
    );
    fs::remove_file(hk.state.join("off")).unwrap();

    // A fold "already running": the test holds fold.lock, so no hook spawns one.
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(hk.state.join("fold.lock"))
        .unwrap();
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive).unwrap();
    let mut written = 0;
    let hook = measure(
        "hook (shim + codepet hook, busy)",
        |i| (shim_cmd("claude-code"), payload(i)),
        || written += clear_inbox(&hk),
    );
    assert_eq!(
        written,
        WARMUP + RUNS,
        "every busy-phase run wrote its record"
    );
    let sh = measure(
        "stored command via sh -c (busy)",
        |i| {
            let mut c = Command::new("/bin/sh");
            c.arg("-c").arg(hk.stored_command("claude-code"));
            env(&mut c);
            (c, payload(i))
        },
        || {
            clear_inbox(&hk);
        },
    );
    drop(lock);

    let spawn = measure(
        "hook + fold spawn (lock free)",
        |i| (shim_cmd("claude-code"), payload(i + 10_000)),
        || {},
    );
    assert!(hk.wait_for(Duration::from_secs(10), || hk.inbox_names().is_empty()));
    assert!(hk.wait_for(Duration::from_secs(5), || hk.fold_processes() == 0));
    let folded = hk.db().int("SELECT COUNT(*) FROM agent_events");
    println!("records folded by the spawned folds: {folded}");
    let load = fs::read_to_string("/proc/loadavg").unwrap_or_default();
    println!("loadavg at end: {}", load.trim());

    let _ = sh;
    if idle {
        assert!(off.p50 <= 3.0, "off p50 {:.2} ms > 3 ms", off.p50);
        assert!(hook.p50 <= 10.0, "hook p50 {:.2} ms > 10 ms", hook.p50);
        assert!(hook.p99 <= 20.0, "hook p99 {:.2} ms > 20 ms", hook.p99);
        assert!(
            spawn.p50 <= 10.0,
            "hook+spawn p50 {:.2} ms > 10 ms",
            spawn.p50
        );
        assert!(
            spawn.p99 <= 20.0,
            "hook+spawn p99 {:.2} ms > 20 ms",
            spawn.p99
        );
    } else {
        // Loaded host: the §5.9 loaded budget (p99 ≤ 40 ms) for the hook paths. The Off
        // budget (p50 ≤ 3 ms) is an idle figure: it is reported, and only bounded loosely.
        println!(
            "note: host loaded; the idle budgets (off p50 <= 3 ms, hook p50 <= 10 / p99 <= 20 ms) are not assessed"
        );
        assert!(
            off.p50 <= 10.0,
            "off p50 {:.2} ms > 10 ms even loaded",
            off.p50
        );
        assert!(hook.p99 <= 40.0, "hook p99 {:.2} ms > 40 ms", hook.p99);
        assert!(
            spawn.p99 <= 40.0,
            "hook+spawn p99 {:.2} ms > 40 ms",
            spawn.p99
        );
    }
    assert_eq!(
        folded,
        (WARMUP + RUNS) as i64,
        "every spawned-run record folded"
    );
}
