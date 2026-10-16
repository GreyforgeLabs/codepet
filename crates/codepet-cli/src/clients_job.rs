//! `codepet clients-job <job_id>` (spec §5.2): the detached, journaled Connect, Disconnect
//! and ready-check job. The RPC that starts it (`clients.connect`, `clients.disconnect`,
//! `clients.test`) marks the job running in `client_hooks`, writes the journal
//! `$STATE/jobs/<job_id>.json` and returns at once, so the bar's single RPC slot is never
//! held. The job:
//! - isolates itself like the fold (`setsid`, `umask 077`, `nice 10`, stdio `/dev/null`) and
//!   takes `$STATE/jobs/.lock`, so one job (and one client CLI) runs at a time;
//! - records its pid and start time in the journal and counts its attempts;
//! - per client, in client order and idempotently: detect, shim, artifact, settings check
//!   (inside the client modules), record; then the ready check through the exact stored
//!   commands; then clears its running mark, marks the journal done and exports.
//!
//! Kill safety: every step re-checks what it changes. A journal whose process is dead is
//! resumed by the next `state.get` or `clients.*` call ([`recover`]); after
//! [`MAX_ATTEMPTS`] the job rolls back instead (the clients it had not recorded become
//! `failed: blocked`, a Connect's file-owned artifacts are removed again).
use crate::install::{self, detect, runner, Artifact, Ctx, Env, Failed, Reason};
use codepet_core::agent::{ClientId, TEST_ARRIVAL_MS, TEST_DONE_DELAY_MS};
use codepet_storage::{JobClientResult, StatePaths, Store, SystemClock};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A job that died this often is rolled back instead of resumed.
pub const MAX_ATTEMPTS: u32 = 3;
/// A journal that never recorded a pid is considered dead after this long.
const START_GRACE_MS: i64 = 30_000;
/// Finished journals older than this are deleted by [`recover`].
const KEEP_DONE_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// How long a job waits for another job's lock.
const LOCK_WAIT: Duration = Duration::from_secs(120);
/// The ready-check payloads (`commands.json` `ready_check`), the frozen contract itself.
const COMMANDS_JSON: &str = include_str!("../../../docs/v0.3/contracts/commands.json");

/// One client's result in the journal.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalResult {
    pub id: String,
    /// `connected`, `needs_step`, `failed` or `disconnected`.
    pub outcome: String,
    pub step: Option<String>,
    pub reason: Option<String>,
    /// A short plain detail (never file content).
    pub detail: Option<String>,
}

/// `$STATE/jobs/<job_id>.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Journal {
    pub v: u32,
    pub id: String,
    /// `connect`, `disconnect` or `test`.
    pub kind: String,
    pub clients: Vec<String>,
    /// The `codepet` the shim calls (`B`): the binary that started the job.
    pub bin: String,
    pub created_ms: i64,
    pub pid: Option<u32>,
    pub pid_start: Option<u64>,
    pub attempts: u32,
    /// Per client: the last step done (`recorded`).
    pub steps: BTreeMap<String, String>,
    pub results: Vec<JournalResult>,
    /// The ready check's outcome (`done` or `failed`), when it ran.
    pub test: Option<String>,
    pub done: bool,
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// `n` random bytes as lowercase hex.
pub fn random_hex(n: usize) -> std::io::Result<String> {
    let mut bytes = vec![0u8; n];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// `job-` + 24 lowercase hex (`protocol.json` `job_id_pattern`).
pub fn valid_job_id(id: &str) -> bool {
    id.len() == 28
        && id.starts_with("job-")
        && id[4..]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn new_job_id() -> std::io::Result<String> {
    Ok(format!("job-{}", random_hex(12)?))
}

fn jobs_dir(paths: &StatePaths) -> std::io::Result<PathBuf> {
    let dir = paths.jobs_dir();
    install::ensure_dir(&dir, 0o700)?;
    Ok(dir)
}

pub fn read_journal(paths: &StatePaths, id: &str) -> Option<Journal> {
    if !valid_job_id(id) {
        return None;
    }
    let p = paths.job_path(id);
    let m = fs::symlink_metadata(&p).ok()?;
    if !m.is_file() || m.len() > 64 * 1024 {
        return None;
    }
    serde_json::from_slice(&fs::read(p).ok()?).ok()
}

pub fn write_journal(paths: &StatePaths, j: &Journal) -> std::io::Result<()> {
    let dir = jobs_dir(paths)?;
    let body = serde_json::to_vec_pretty(j).map_err(std::io::Error::other)?;
    install::write_file(
        &paths.job_path(&j.id),
        &format!(".{}.tmp", j.id),
        &body,
        0o600,
    )
    .map(|_| ())?;
    let _ = dir;
    Ok(())
}

/// The process start time (`/proc/<pid>/stat` field 22), to tell a reused pid apart.
pub fn proc_start(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after = &stat[stat.rfind(')')? + 2..];
    after.split_whitespace().nth(19)?.parse().ok()
}

fn alive(j: &Journal) -> bool {
    match (j.pid, j.pid_start) {
        (Some(pid), Some(start)) => proc_start(pid) == Some(start),
        _ => now_ms() - j.created_ms < START_GRACE_MS,
    }
}

/// Starts `codepet clients-job <id>` detached (its own session, stdio `/dev/null`, cwd
/// `/`). The environment is kept: detection resolves the clients' config dirs from it.
pub fn spawn(id: &str) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(exe);
    cmd.arg("clients-job")
        .arg(id)
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = cmd.spawn()?;
    // A longer-lived caller (`codepet connect`) reaps it; the RPC exits right away anyway.
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// Resumes or rolls back jobs whose process died, clears running marks no journal backs,
/// and deletes old finished journals. Called before `state.get` and every `clients.*`.
pub fn recover(store: &mut Store) {
    let paths = store.paths().clone();
    let mut keep: Vec<String> = Vec::new();
    if let Ok(rd) = fs::read_dir(paths.jobs_dir()) {
        let mut done: Vec<(i64, PathBuf)> = Vec::new();
        for e in rd.flatten() {
            let name = e.file_name();
            let Some(id) = name.to_str().and_then(|n| n.strip_suffix(".json")) else {
                continue;
            };
            let Some(j) = read_journal(&paths, id) else {
                continue;
            };
            if j.done {
                done.push((j.created_ms, e.path()));
                continue;
            }
            if alive(&j) {
                keep.push(j.id.clone());
                continue;
            }
            // Resume: mark the restart first, so a second caller sees a starting job.
            let mut restart = j.clone();
            restart.pid = None;
            restart.pid_start = None;
            restart.created_ms = now_ms();
            if write_journal(&paths, &restart).is_ok() && spawn(&j.id).is_ok() {
                keep.push(j.id.clone());
            }
        }
        done.sort();
        let n = done.len();
        for (i, (created, p)) in done.into_iter().enumerate() {
            if now_ms() - created > KEEP_DONE_MS || i + 20 < n {
                let _ = fs::remove_file(p);
            }
        }
    }
    if store.clear_dead_jobs(&keep).is_ok_and(|d| !d.is_empty()) {
        crate::rpc::publish_bar_state(store);
    }
}

/// The `codepet` binary the shim should call: the absolute `argv[0]` when it is the same
/// file as the running executable (keeps a user-facing path such as `~/.local/bin/codepet`),
/// else `current_exe()`.
pub fn shim_bin() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe = PathBuf::from(
        exe.to_string_lossy()
            .trim_end_matches(" (deleted)")
            .to_owned(),
    );
    if let Some(arg0) = std::env::args_os().next().map(PathBuf::from) {
        if arg0.is_absolute() {
            use std::os::unix::fs::MetadataExt;
            let same = match (fs::metadata(&arg0), fs::metadata(&exe)) {
                (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
                _ => false,
            };
            if same {
                return Some(arg0);
            }
        }
    }
    Some(exe)
}

/// Writes the journal of a new job (before it is spawned).
pub fn create_journal(
    paths: &StatePaths,
    id: &str,
    kind: &str,
    clients: &[ClientId],
    bin: &Path,
) -> std::io::Result<()> {
    write_journal(
        paths,
        &Journal {
            v: 1,
            id: id.to_owned(),
            kind: kind.to_owned(),
            clients: clients.iter().map(|c| c.as_str().to_owned()).collect(),
            bin: bin.to_string_lossy().into_owned(),
            created_ms: now_ms(),
            ..Journal::default()
        },
    )
}

/// `codepet clients-job <job_id>`: runs (or resumes) one job. Exit 0 when the journal is
/// done, 1 otherwise; it prints nothing (stdio is `/dev/null`).
pub fn run(args: &[OsString]) -> i32 {
    let Some(id) = args
        .first()
        .and_then(|a| a.to_str())
        .filter(|_| args.len() == 1)
    else {
        return 2;
    };
    if !valid_job_id(id) {
        return 2;
    }
    let _ = rustix::process::setsid();
    rustix::process::umask(rustix::fs::Mode::from_raw_mode(0o077));
    let _ = rustix::process::nice(10);
    let Ok(paths) = StatePaths::from_env() else {
        return 1;
    };
    let Some(_lock) = job_lock(&paths) else {
        return 1;
    };
    let Some(mut j) = read_journal(&paths, id) else {
        return 1;
    };
    if j.done {
        return 0;
    }
    j.pid = Some(std::process::id());
    j.pid_start = proc_start(std::process::id());
    j.attempts += 1;
    if write_journal(&paths, &j).is_err() {
        return 1;
    }
    let Ok(mut store) = Store::open_current(&paths, Arc::new(SystemClock)) else {
        return 1;
    };
    let Some(env) = Env::from_process() else {
        return fail_all(&mut store, &paths, &mut j, "no HOME");
    };
    let Some(ctx) = Ctx::new(env, paths.clone(), PathBuf::from(&j.bin)) else {
        return fail_all(&mut store, &paths, &mut j, "no data dir");
    };
    if j.attempts > MAX_ATTEMPTS {
        rollback(&mut store, &ctx, &mut j);
    } else {
        match j.kind.as_str() {
            "connect" => connect(&mut store, &ctx, &mut j),
            "disconnect" => disconnect(&mut store, &ctx, &mut j),
            "test" => {
                let clients = parse_clients(&j);
                let outcome = ready_check(&mut store, &ctx, &clients);
                j.test = Some(outcome.to_owned());
            }
            _ => {}
        }
    }
    finish(&mut store, &paths, &mut j)
}

fn parse_clients(j: &Journal) -> Vec<ClientId> {
    let mut out: Vec<ClientId> = j
        .clients
        .iter()
        .filter_map(|c| ClientId::parse(c))
        .collect();
    out.sort_by_key(|c| *c as u8);
    out.dedup();
    out
}

fn finish(store: &mut Store, paths: &StatePaths, j: &mut Journal) -> i32 {
    let _ = store.finish_clients_job(&j.id);
    j.done = true;
    let _ = write_journal(paths, j);
    crate::rpc::publish_bar_state(store);
    0
}

fn fail_all(store: &mut Store, paths: &StatePaths, j: &mut Journal, why: &str) -> i32 {
    for c in parse_clients(j) {
        let _ = store.record_job_result(&JobClientResult {
            client: c,
            outcome: "failed",
            connected: false,
            needs_step: None,
            reason: Some(Reason::Blocked.as_str()),
            installed_version: None,
            reset_first_seen: false,
        });
        push_result(j, c, "failed", None, Some(Reason::Blocked), why);
    }
    finish(store, paths, j)
}

/// `$STATE/jobs/.lock`: one job at a time (waits up to [`LOCK_WAIT`]).
fn job_lock(paths: &StatePaths) -> Option<fs::File> {
    let dir = jobs_dir(paths).ok()?;
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC).bits() as i32)
        .open(dir.join(".lock"))
        .ok()?;
    let started = Instant::now();
    loop {
        match rustix::fs::flock(&f, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => return Some(f),
            Err(_) if started.elapsed() < LOCK_WAIT => {
                std::thread::sleep(Duration::from_millis(100))
            }
            Err(_) => return None,
        }
    }
}

fn push_result(
    j: &mut Journal,
    c: ClientId,
    outcome: &str,
    step: Option<&str>,
    reason: Option<Reason>,
    detail: &str,
) {
    j.results.retain(|r| r.id != c.as_str());
    j.results.push(JournalResult {
        id: c.as_str().to_owned(),
        outcome: outcome.to_owned(),
        step: step.map(str::to_owned),
        reason: reason.map(|r| r.as_str().to_owned()),
        detail: (!detail.is_empty()).then(|| detail.to_owned()),
    });
}

fn mark_step(store: &Store, j: &mut Journal, c: ClientId, step: &str) {
    j.steps.insert(c.as_str().to_owned(), step.to_owned());
    let _ = write_journal(store.paths(), j);
}

fn set_disconnected_flag(paths: &StatePaths, c: ClientId, on: bool) -> std::io::Result<()> {
    let dir = paths.clients_dir();
    match fs::symlink_metadata(&dir) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => return Err(std::io::Error::other("clients is not a directory")),
        Err(_) => fs::DirBuilder::new().mode(0o700).create(&dir)?,
    }
    let flag = paths.disconnected_flag_path(c.as_str());
    if on {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(&flag)
        {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(e),
        }
    } else {
        install::remove_file(&flag).map(|_| ())
    }
}

/// The version, from files, else (not Hermes) `<bin> --version` under the CLI rules.
fn version_for(ctx: &Ctx, c: ClientId, d: &detect::Detected) -> Option<String> {
    if let Some(v) = &d.version {
        return Some(v.clone());
    }
    if c == ClientId::Hermes {
        return None;
    }
    let bin = d.binary.as_deref()?;
    let extra: &[(&str, &str)] = if c == ClientId::ClaudeCode {
        &install::claude::CLI_ENV
    } else {
        &[]
    };
    let run = runner::run(&ctx.env, bin, &["--version"], extra).ok()?;
    run.ok()
        .then(|| detect::find_version(&run.stdout))
        .flatten()
}

/// One client's Connect (spec §5.2 steps 1-5).
fn connect_one(
    store: &mut Store,
    ctx: &Ctx,
    c: ClientId,
) -> Result<(install::Connected, Option<String>), Failed> {
    let d = detect::detect(&ctx.env, c);
    if !d.installed() {
        return Err(Failed::new(
            Reason::NotInstalled,
            "binary or config dir missing",
        ));
    }
    let version = version_for(ctx, c, &d);
    if version
        .as_deref()
        .and_then(detect::parse_version)
        .is_some_and(|v| v < detect::tested_version(c))
    {
        return Err(Failed::new(Reason::TooOld, "older than the tested version"));
    }
    // The shim drains and exits while `$S/inbox` is missing; the job creates it (0700).
    install::ensure_private_dir(&ctx.paths.inbox_dir())?;
    install::install_shim(ctx)?;
    let bin = d.binary.as_deref().expect("installed");
    let connected = match c {
        ClientId::ClaudeCode => install::claude::connect(ctx, bin)?,
        ClientId::Codex => install::codex::connect(ctx)?,
        ClientId::OpenCode => install::opencode::connect(ctx)?,
        ClientId::Hermes => install::hermes::connect(ctx, bin)?,
        ClientId::GrokCli => install::grok::connect(ctx)?,
    };
    codepet_storage::fault("connect_after_artifact");
    let _ = store;
    Ok((connected, version))
}

fn connect(store: &mut Store, ctx: &Ctx, j: &mut Journal) {
    let mut tested = Vec::new();
    for c in parse_clients(j) {
        if j.steps.get(c.as_str()).map(String::as_str) == Some("recorded") {
            if j.results
                .iter()
                .any(|r| r.id == c.as_str() && r.outcome != "failed")
            {
                tested.push(c);
            }
            continue;
        }
        let result = connect_one(store, ctx, c);
        let rec = match &result {
            Ok((conn, version)) => {
                let _ = set_disconnected_flag(&ctx.paths, c, false);
                let outcome = if conn.needs_step.is_some() {
                    "needs_step"
                } else {
                    "connected"
                };
                push_result(j, c, outcome, conn.needs_step, None, "");
                if conn.connected {
                    tested.push(c);
                }
                JobClientResult {
                    client: c,
                    outcome,
                    connected: conn.connected,
                    needs_step: conn.needs_step,
                    reason: None,
                    installed_version: version.clone(),
                    reset_first_seen: conn.connected,
                }
            }
            Err(f) => {
                let connected =
                    f.artifact_installed || install::status(ctx, c) == Artifact::Current;
                push_result(j, c, "failed", None, Some(f.reason), &f.detail);
                JobClientResult {
                    client: c,
                    outcome: "failed",
                    connected,
                    needs_step: None,
                    reason: Some(f.reason.as_str()),
                    installed_version: None,
                    reset_first_seen: false,
                }
            }
        };
        let _ = store.record_job_result(&rec);
        mark_step(store, j, c, "recorded");
        crate::rpc::publish_bar_state(store);
    }
    if !tested.is_empty() {
        let outcome = ready_check(store, ctx, &tested);
        j.test = Some(outcome.to_owned());
    }
}

fn disconnect(store: &mut Store, ctx: &Ctx, j: &mut Journal) {
    for c in parse_clients(j) {
        if j.steps.get(c.as_str()).map(String::as_str) == Some("recorded") {
            continue;
        }
        // The shim exits at once from here on.
        if let Err(e) = set_disconnected_flag(&ctx.paths, c, true) {
            let f = Failed::from(e);
            push_result(j, c, "failed", None, Some(f.reason), &f.detail);
            let _ = store.record_job_result(&JobClientResult {
                client: c,
                outcome: "failed",
                connected: true,
                needs_step: None,
                reason: Some(f.reason.as_str()),
                installed_version: None,
                reset_first_seen: false,
            });
            mark_step(store, j, c, "recorded");
            continue;
        }
        let bin = detect::find_binary(&ctx.env, c);
        let result: Result<String, Failed> = match c {
            ClientId::ClaudeCode => install::claude::disconnect(ctx, bin.as_deref()).map(|()| String::new()),
            ClientId::Codex => install::codex::disconnect(ctx).map(|r| {
                if r.tombstoned > 0 {
                    format!("{} CodePet line(s) left in place because other hooks follow them; they do nothing", r.tombstoned)
                } else {
                    String::new()
                }
            }),
            ClientId::OpenCode => install::opencode::disconnect(ctx).map(|()| String::new()),
            ClientId::Hermes => install::hermes::disconnect(ctx, bin.as_deref()).map(|()| String::new()),
            ClientId::GrokCli => install::grok::disconnect(ctx).map(|()| String::new()),
        };
        let rec = match result {
            Ok(detail) => {
                push_result(j, c, "disconnected", None, None, &detail);
                JobClientResult {
                    client: c,
                    outcome: "disconnected",
                    connected: false,
                    needs_step: None,
                    reason: None,
                    installed_version: None,
                    reset_first_seen: false,
                }
            }
            Err(f) => {
                push_result(j, c, "failed", None, Some(f.reason), &f.detail);
                JobClientResult {
                    client: c,
                    outcome: "failed",
                    connected: f.artifact_installed,
                    needs_step: None,
                    reason: Some(f.reason.as_str()),
                    installed_version: None,
                    reset_first_seen: false,
                }
            }
        };
        let _ = store.record_job_result(&rec);
        mark_step(store, j, c, "recorded");
        crate::rpc::publish_bar_state(store);
    }
}

/// After [`MAX_ATTEMPTS`]: clients without a recorded step fail (`blocked`); a Connect's
/// file-owned artifacts are removed again (a CLI-installed one stays for Stop watching).
fn rollback(store: &mut Store, ctx: &Ctx, j: &mut Journal) {
    for c in parse_clients(j) {
        if j.steps.get(c.as_str()).map(String::as_str) == Some("recorded") {
            continue;
        }
        if j.kind == "connect" {
            let _ = match c {
                ClientId::OpenCode => install::opencode::disconnect(ctx),
                ClientId::GrokCli => install::grok::disconnect(ctx),
                _ => Ok(()),
            };
        }
        let connected = install::status(ctx, c) == Artifact::Current
            && !ctx.paths.disconnected_flag_path(c.as_str()).exists();
        let _ = store.record_job_result(&JobClientResult {
            client: c,
            outcome: "failed",
            connected,
            needs_step: None,
            reason: Some(Reason::Blocked.as_str()),
            installed_version: None,
            reset_first_seen: false,
        });
        push_result(
            j,
            c,
            "failed",
            None,
            Some(Reason::Blocked),
            "the job stopped too often",
        );
        mark_step(store, j, c, "recorded");
    }
    if j.kind == "test" {
        let tested = parse_clients(j);
        let _ = store.finish_ready_check(&tested, &[]);
        let _ = install::remove_file(&ctx.paths.test_nonce_path());
        j.test = Some("failed".to_owned());
    }
}

/// The ready-check payload lines of `client` with `nonce` filled in.
pub fn ready_payloads(client: ClientId, nonce: &str) -> Vec<String> {
    let v: serde_json::Value = serde_json::from_str(COMMANDS_JSON).unwrap_or_default();
    let list = v["ready_check"]["payloads"][client.as_str()]
        .as_array()
        .cloned()
        .unwrap_or_default();
    list.into_iter()
        .map(|mut p| {
            if let Some(o) = p.as_object_mut() {
                o.insert("codepet_test".into(), nonce.into());
            }
            p.to_string()
        })
        .collect()
}

/// Runs a client's exact stored command with `line` on stdin (shell clients through
/// `/bin/sh -c`, argv clients directly), with only `PATH` and `HOME`. Bounded to 3 s.
pub fn run_stored(ctx: &Ctx, client: ClientId, line: &str) -> bool {
    let mut cmd = match client {
        ClientId::OpenCode | ClientId::Hermes => {
            let mut c = Command::new(&ctx.shim);
            c.arg(client.as_str());
            c
        }
        _ => {
            let Ok(stored) = ctx.shell_command(client) else {
                return false;
            };
            let mut c = Command::new("/bin/sh");
            c.arg("-c").arg(stored);
            c
        }
    };
    cmd.env_clear()
        .env("PATH", &ctx.env.path_var)
        .env("HOME", &ctx.env.home)
        .current_dir("/")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let Ok(mut child) = cmd.spawn() else {
        return false;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(format!("{line}\n").as_bytes());
    }
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(s)) => return s.success(),
            Ok(None) if started.elapsed() < Duration::from_secs(3) => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

/// The ready check (`protocol.json` `clients.test`): a fresh nonce, `test_state` running,
/// each tested client's stored command with its `turn_start` then (1.5 s later) its
/// `turn_done` payload, then wait for the folded `turn_done` records. Returns `done` or
/// `failed`.
pub fn ready_check(store: &mut Store, ctx: &Ctx, tested: &[ClientId]) -> &'static str {
    let nonce_path = ctx.paths.test_nonce_path();
    let _ = install::ensure_private_dir(&ctx.paths.inbox_dir());
    let Ok(nonce) = random_hex(16) else {
        let _ = store.finish_ready_check(tested, &[]);
        return "failed";
    };
    let since = match store.begin_ready_check(tested) {
        Ok(at) => at,
        Err(_) => return "failed",
    };
    if install::write_file(
        &nonce_path,
        ".test.nonce.tmp",
        format!("{nonce}\n").as_bytes(),
        0o600,
    )
    .is_err()
    {
        let _ = store.finish_ready_check(tested, &[]);
        return "failed";
    }
    crate::rpc::publish_bar_state(store);
    for c in tested {
        if let Some(first) = ready_payloads(*c, &nonce).first() {
            run_stored(ctx, *c, first);
        }
    }
    std::thread::sleep(Duration::from_millis(
        u64::try_from(TEST_DONE_DELAY_MS).unwrap_or(1500),
    ));
    for c in tested {
        if let Some(last) = ready_payloads(*c, &nonce).get(1) {
            run_stored(ctx, *c, last);
        }
    }
    let deadline = since + TEST_DONE_DELAY_MS + TEST_ARRIVAL_MS;
    let mut arrived = Vec::new();
    loop {
        if let Ok(a) = store.ready_check_arrivals(tested, since) {
            arrived = a;
        }
        if arrived.len() == tested.len() || now_ms() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = install::remove_file(&nonce_path);
    let done = store.finish_ready_check(tested, &arrived).unwrap_or(false);
    crate::rpc::publish_bar_state(store);
    if done {
        "done"
    } else {
        "failed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_ids_and_payloads() {
        let id = new_job_id().unwrap();
        assert!(valid_job_id(&id), "{id}");
        for bad in [
            "job-",
            "job-ABCDEF0123456789abcdef01",
            "jab-0123456789abcdef01234567",
            "job-0123456789abcdef012345678",
        ] {
            assert!(!valid_job_id(bad), "{bad}");
        }
        let nonce = "0123456789abcdef0123456789abcdef";
        for c in ClientId::ALL {
            let p = ready_payloads(c, nonce);
            assert_eq!(p.len(), 2, "{c:?}");
            for line in &p {
                assert!(line.contains(nonce) && !line.contains("@CODEPET_TEST_NONCE@"));
                assert!(line.len() < 512);
            }
        }
        assert!(proc_start(std::process::id()).is_some());
    }
}
