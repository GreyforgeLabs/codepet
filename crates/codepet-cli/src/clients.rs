//! The `clients.*` protocol methods (spec §5.2, §6.2) and the plain-word `codepet connect`
//! / `codepet disconnect` wrappers (§6.3).
//!
//! `clients.status` is file-only (no client program, no network): it detects every
//! supported client, reads CodePet's own artifacts (and Codex's trust entries), writes
//! `client_hooks` and re-exports. `clients.connect`, `clients.disconnect` and
//! `clients.test` start a detached `codepet clients-job` and return `{job_id}` at once;
//! they answer `TEST_STATE` while the bar runs with a `stateHome` test override (the bar
//! then sets `CODEPET_TEST_STATE=1`). Independently of that signal, a Connect never
//! repoints a shim that serves another existing state dir or binary
//! ([`install::shim_conflict`], `blocked`), so a test can never take over the owner's
//! real shim.
use crate::clients_job::{self, Journal};
use crate::errors::Failure;
use crate::install::{self, codex, detect, Artifact, Ctx, Env};
use codepet_core::agent::ClientId;
use codepet_protocol::ErrorCode;
use codepet_storage::{JobTarget, StatusUpdate, Store};
use serde_json::{json, Value};
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Set by the bar when it runs with a `stateHome` test override.
pub const TEST_STATE_ENV: &str = "CODEPET_TEST_STATE";

pub(crate) fn test_state() -> bool {
    std::env::var_os(TEST_STATE_ENV).is_some_and(|v| !v.is_empty())
}

/// The integrations context of this process (status, connect).
pub fn context(store: &Store) -> Result<Ctx, Failure> {
    let env = Env::from_process()
        .ok_or_else(|| Failure::new(ErrorCode::InvalidState, "HOME is not an absolute path"))?;
    let bin = clients_job::shim_bin()
        .ok_or_else(|| Failure::new(ErrorCode::Internal, "cannot find the codepet binary"))?;
    Ctx::new(env, store.paths().clone(), bin).ok_or_else(|| {
        Failure::new(
            ErrorCode::InvalidState,
            "no data directory (HOME or XDG_DATA_HOME)",
        )
    })
}

/// What the files say about one client (file-only).
pub fn detect_update(ctx: &Ctx, c: ClientId) -> StatusUpdate {
    let d = detect::detect(&ctx.env, c);
    let flagged = ctx.paths.disconnected_flag_path(c.as_str()).exists();
    let (artifact, needs_step) = if c == ClientId::Codex {
        let st = codex::status(ctx);
        (st.artifact, st.needs_step)
    } else {
        (install::status(ctx, c), Some(None))
    };
    StatusUpdate {
        client: c,
        installed: d.installed(),
        version: d.version,
        connected: Some(!flagged && artifact == Artifact::Current),
        needs_step,
    }
}

/// `clients.status`.
pub fn status(store: &mut Store) -> Result<Value, Failure> {
    clients_job::recover(store);
    let ctx = context(store)?;
    let updates: Vec<StatusUpdate> = ClientId::ALL
        .iter()
        .map(|c| detect_update(&ctx, *c))
        .collect();
    let rows = store.record_clients_status(&updates)?;
    let clients: Vec<Value> = ClientId::ALL
        .iter()
        .map(|c| {
            let r = rows
                .iter()
                .find(|r| r.client == c.as_str())
                .cloned()
                .unwrap_or_default();
            json!({
                "id": c.as_str(),
                "name": c.display_name(),
                "installed": r.installed,
                "version": r.detected_version,
                "connected": r.connected,
                "needs_step": r.needs_step,
                "last_seen_ms": r.last_seen_ms,
                "first_seen_ms": r.first_seen_ms,
                "events_7d": r.events_7d,
                "failed_reason": r.failed_reason(),
                "job": match (&r.job_id, &r.job_kind) {
                    (Some(id), Some(kind)) => json!({"id": id, "kind": kind, "state": "running"}),
                    _ => Value::Null,
                },
            })
        })
        .collect();
    Ok(json!({ "clients": clients }))
}

/// Starts a job for an RPC; returns `{job_id}`.
pub fn start(
    store: &mut Store,
    request_id: &str,
    method: &str,
    kind: &str,
    target: JobTarget,
) -> Result<Value, Failure> {
    start_with(store, request_id, method, kind, target, false)
}

/// [`start`]; `repoint` is `codepet connect --repoint` (never from the bar).
fn start_with(
    store: &mut Store,
    request_id: &str,
    method: &str,
    kind: &str,
    target: JobTarget,
    repoint: bool,
) -> Result<Value, Failure> {
    if test_state() {
        return Err(Failure::new(
            ErrorCode::TestState,
            "the bar runs with a test state home; connect from a normal session",
        ));
    }
    clients_job::recover(store);
    let canonical = match &target {
        JobTarget::Named(list) => {
            let mut ids: Vec<&str> = list.iter().map(|c| c.as_str()).collect();
            ids.sort_by_key(|id| ClientId::parse(id).map(|c| c as u8));
            json!({ "clients": ids }).to_string()
        }
        JobTarget::Connected(one) => json!({ "client": one.map(ClientId::as_str) }).to_string(),
    };
    let canonical = if repoint {
        format!("{canonical} repoint")
    } else {
        canonical
    };
    let ctx = context(store)?;
    let job_id = clients_job::new_job_id()
        .map_err(|_| Failure::new(ErrorCode::Internal, "no random job id"))?;
    // The journal exists before the database marks the job running, so a concurrent
    // recovery never clears a job that is about to start (a fresh journal without a pid
    // counts as starting).
    let guess: Vec<ClientId> = match &target {
        JobTarget::Named(list) => list.clone(),
        JobTarget::Connected(_) => Vec::new(),
    };
    clients_job::create_journal(&ctx.paths, &job_id, kind, &guess, &ctx.bin, repoint)
        .map_err(|_| Failure::new(ErrorCode::Internal, "could not write the job journal"))?;
    let drop_journal = || {
        let _ = install::remove_file(&ctx.paths.job_path(&job_id));
    };
    let refuses_off = kind != "disconnect";
    let started = store.start_clients_job(
        request_id,
        method,
        &canonical,
        refuses_off,
        kind,
        &target,
        &job_id,
    );
    let (running_id, clients, replayed) = match started {
        Ok(v) => v,
        Err(e) => {
            drop_journal();
            return Err(e.into());
        }
    };
    if replayed {
        // The receipt names an earlier job; this one never starts.
        drop_journal();
        return Ok(json!({ "job_id": running_id }));
    }
    let spawned = (if clients == guess {
        Ok(())
    } else {
        clients_job::create_journal(&ctx.paths, &job_id, kind, &clients, &ctx.bin, repoint)
    })
    .and_then(|()| clients_job::spawn(&job_id));
    if spawned.is_err() {
        let _ = store.finish_clients_job(&job_id);
        drop_journal();
        return Err(Failure::new(
            ErrorCode::Internal,
            "could not start the clients job",
        ));
    }
    Ok(json!({ "job_id": job_id }))
}

// ---- `codepet connect` / `codepet disconnect` ----------------------------------------

const USAGE_CONNECT: &str = "usage: codepet connect [--repoint] <client>... | --all   (clients: claude-code, codex, opencode, hermes, grok-cli)";
const USAGE_DISCONNECT: &str = "usage: codepet disconnect <client>   (clients: claude-code, codex, opencode, hermes, grok-cli)";
/// How long the wrappers wait for their job.
const WAIT: Duration = Duration::from_secs(120);

fn usage(line: &str) -> i32 {
    let _ = writeln!(std::io::stderr(), "{line}");
    2
}

fn say(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
}

fn waited_journal(store: &Store, job_id: &str) -> Option<Journal> {
    let started = Instant::now();
    while started.elapsed() < WAIT {
        if let Some(j) = clients_job::read_journal(store.paths(), job_id).filter(|j| j.done) {
            return Some(j);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}

/// One result line; `kind` is the job's (`connect`, `disconnect` or `test`).
fn plain(kind: &str, r: &clients_job::JournalResult) -> String {
    let c = ClientId::parse(&r.id).expect("journal ids are client ids");
    let name = c.display_name();
    let verb = match kind {
        "disconnect" => "stop watching",
        "test" => "check",
        _ => "connect",
    };
    match (r.outcome.as_str(), r.step.as_deref(), r.reason.as_deref()) {
        ("connected", ..) => format!("{name}: connected. Open a new {name} window{}.", if c == ClientId::ClaudeCode { " (or type /reload-plugins)" } else { "" }),
        ("needs_step", Some("codex_hooks_off"), _) => format!("{name}: hooks are turned off in Codex ([features] hooks = false); CodePet changed nothing"),
        ("needs_step", ..) => format!("{name}: connected, one more step: open Codex, type /hooks, trust the CodePet lines"),
        ("disconnected", ..) => match &r.detail {
            Some(d) => format!("{name}: stopped watching ({d})"),
            None => format!("{name}: stopped watching"),
        },
        (_, _, Some(reason)) => format!(
            "{name}: couldn't {verb} ({}){}",
            match reason {
                "not_installed" => "not installed",
                "too_old" => "too old",
                "settings_changed" => "its settings changed in an unexpected way",
                "ready_check" => "the ready check did not arrive",
                _ => "something blocked it",
            },
            r.detail.as_ref().map(|d| format!(": {d}")).unwrap_or_default()
        ),
        _ => format!("{name}: {}", r.outcome),
    }
}

fn run_job(method: codepet_protocol::Method, params: Value, repoint: bool) -> i32 {
    let opened = crate::rpc::open_current();
    let mut store = match opened {
        Ok(s) => s,
        Err(f) => {
            let _ = writeln!(std::io::stderr(), "{}: {}", f.code, f.message);
            return 1;
        }
    };
    let id = match clients_job::random_hex(12) {
        Ok(h) => format!("cli-{h}"),
        Err(_) => return 1,
    };
    let result = match method {
        codepet_protocol::Method::ClientsConnect => {
            let list: Vec<ClientId> = params["clients"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().and_then(ClientId::parse))
                .collect();
            start_with(
                &mut store,
                &id,
                "clients.connect",
                "connect",
                JobTarget::Named(list),
                repoint,
            )
        }
        _ => {
            let c = params["client"]
                .as_str()
                .and_then(ClientId::parse)
                .expect("validated");
            start(
                &mut store,
                &id,
                "clients.disconnect",
                "disconnect",
                JobTarget::Named(vec![c]),
            )
        }
    };
    let job_id = match result {
        Ok(v) => v["job_id"].as_str().unwrap_or_default().to_owned(),
        Err(f) if f.code == ErrorCode::InvalidState && f.message.contains("off") => {
            let _ = writeln!(
                std::io::stderr(),
                "CodePet is off. Turn it on first: codepet on"
            );
            return 1;
        }
        Err(f) => {
            let _ = writeln!(
                std::io::stderr(),
                "{}: {}",
                f.code,
                f.message.replace('\n', " ")
            );
            return 1;
        }
    };
    let Some(j) = waited_journal(&store, &job_id) else {
        say("Still working in the background; open Helpers or run codepet doctor later.");
        return 1;
    };
    let mut ok = true;
    for r in &j.results {
        say(&plain(&j.kind, r));
        ok &= r.outcome != "failed";
    }
    match j.test.as_deref() {
        Some("done") => say("Ready check: passed."),
        Some(_) => {
            say("Ready check: nothing arrived. Run codepet doctor.");
            ok = false;
        }
        None => {}
    }
    i32::from(!ok)
}

/// `codepet connect <id>... | --all`.
pub fn connect_cli(args: &[std::ffi::OsString]) -> i32 {
    let mut args: Vec<&str> = match args.iter().map(|a| a.to_str()).collect::<Option<Vec<_>>>() {
        Some(a) => a,
        None => return usage(USAGE_CONNECT),
    };
    // `--repoint` (first): let this Connect repoint a hook shim that serves another
    // CodePet state folder or binary that still exists. Never with --all.
    let repoint = args.first() == Some(&"--repoint");
    if repoint {
        args.remove(0);
        if args.is_empty() || args.contains(&"--all") {
            return usage(USAGE_CONNECT);
        }
    }
    let list: Vec<ClientId> = if args == ["--all"] {
        let mut store = match crate::rpc::open_current() {
            Ok(s) => s,
            Err(f) => {
                let _ = writeln!(std::io::stderr(), "{}: {}", f.code, f.message);
                return 1;
            }
        };
        if let Err(f) = status(&mut store) {
            let _ = writeln!(std::io::stderr(), "{}: {}", f.code, f.message);
            return 1;
        }
        let rows = store.client_rows().unwrap_or_default();
        let picked: Vec<ClientId> = rows
            .iter()
            .filter(|r| r.installed && !r.connected && r.job_id.is_none())
            .filter_map(|r| ClientId::parse(&r.client))
            .filter(|c| *c != ClientId::Codex)
            .collect();
        if picked.is_empty() {
            say("Nothing to connect: every installed app is connected (Codex only when named: codepet connect codex).");
            return 0;
        }
        picked
    } else {
        if args.is_empty() {
            return usage(USAGE_CONNECT);
        }
        let mut out = Vec::new();
        for a in &args {
            match ClientId::parse(a) {
                Some(c) if !out.contains(&c) => out.push(c),
                Some(_) => {}
                None => return usage(USAGE_CONNECT),
            }
        }
        out
    };
    let ids: Vec<&str> = list.iter().map(|c| c.as_str()).collect();
    run_job(
        codepet_protocol::Method::ClientsConnect,
        json!({ "clients": ids }),
        repoint,
    )
}

/// `codepet disconnect <id>`.
pub fn disconnect_cli(args: &[std::ffi::OsString]) -> i32 {
    let [one] = args else {
        return usage(USAGE_DISCONNECT);
    };
    let Some(c) = one.to_str().and_then(ClientId::parse) else {
        return usage(USAGE_DISCONNECT);
    };
    // Nothing of CodePet's there and not connected: nothing to stop, and nothing is
    // recorded (no flag, no row), so uninstall never reports apps CodePet never touched.
    if let Ok(store) = crate::rpc::open_current() {
        if let Ok(ctx) = context(&store) {
            let connected = store
                .client_rows()
                .unwrap_or_default()
                .iter()
                .any(|r| r.client == c.as_str() && (r.connected || r.job_id.is_some()));
            if !connected && !install::anything_installed(&ctx, c) {
                say(&format!(
                    "{}: CodePet is not connected there; nothing to stop",
                    c.display_name()
                ));
                return 0;
            }
        }
    }
    run_job(
        codepet_protocol::Method::ClientsDisconnect,
        json!({ "client": c.as_str() }),
        false,
    )
}

/// `codepet clients-refresh` (internal; the installer's step 5): refreshes connected
/// clients' wholly CodePet-owned files whose marker is present and whose bytes differ, and
/// upgrades an older shim in place (same paths). Never runs a client CLI, never touches
/// Codex's shared file. Prints one line per refreshed artifact.
pub fn refresh_cli() -> i32 {
    let store = match crate::rpc::open_current() {
        Ok(s) => s,
        Err(f) => {
            let _ = writeln!(std::io::stderr(), "{}: {}", f.code, f.message);
            return 1;
        }
    };
    let ctx = match context(&store) {
        Ok(c) => c,
        Err(f) => {
            let _ = writeln!(std::io::stderr(), "{}: {}", f.code, f.message);
            return 1;
        }
    };
    if let install::ShimState::Shim {
        state_dir,
        bin,
        current: false,
    } = install::shim_state(&ctx.shim)
    {
        let old = Ctx {
            bin: PathBuf::from(bin),
            paths: codepet_storage::StatePaths {
                state_dir: PathBuf::from(&state_dir),
                ..ctx.paths.clone()
            },
            ..ctx.clone()
        };
        if install::install_shim(&old, false).is_ok() {
            say("refreshed: the CodePet hook shim");
        }
    }
    let rows = store.client_rows().unwrap_or_default();
    let mut failed = false;
    for c in ClientId::ALL {
        let connected = rows.iter().any(|r| r.client == c.as_str() && r.connected);
        if !connected {
            continue;
        }
        match install::refresh(&ctx, c) {
            Ok(true) => say(&format!("refreshed: {}", c.display_name())),
            Ok(false) => {}
            Err(_) => failed = true,
        }
    }
    i32::from(failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clients_job::JournalResult;

    fn failed(id: &str, reason: &str) -> JournalResult {
        JournalResult {
            id: id.to_owned(),
            outcome: "failed".to_owned(),
            reason: Some(reason.to_owned()),
            ..JournalResult::default()
        }
    }

    #[test]
    fn result_lines_use_the_jobs_own_verb() {
        let r = failed("hermes", "blocked");
        assert_eq!(
            plain("connect", &r),
            "Hermes: couldn't connect (something blocked it)"
        );
        assert_eq!(
            plain("disconnect", &r),
            "Hermes: couldn't stop watching (something blocked it)"
        );
        assert_eq!(
            plain("test", &r),
            "Hermes: couldn't check (something blocked it)"
        );
        let ok = JournalResult {
            id: "grok-cli".to_owned(),
            outcome: "disconnected".to_owned(),
            ..JournalResult::default()
        };
        assert_eq!(plain("disconnect", &ok), "Grok: stopped watching");
    }
}
