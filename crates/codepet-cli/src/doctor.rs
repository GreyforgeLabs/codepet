//! `codepet doctor [--json]` (spec §6.3): a plain-word health report that changes nothing.
//! It reads the state dir (without creating it), `client_hooks`, the inbox, the fold's
//! backoff, the installed shim (its sha256 and whether its `S`/`B` paths are this
//! install's), each client's detection and CodePet artifact (file-only, as
//! `clients.status`, but without writing), and times Codex's login shell (`$SHELL -lc
//! true`, which Codex pays per hook even while CodePet is Off) and checks it prints nothing.
//! `--json` is stable (`codepet.doctor.v1`) and never includes config file contents.
use crate::install::{self, codex, detect, Artifact, Ctx, Env, ShimState};
use codepet_core::agent::ClientId;
use codepet_storage::{StatePaths, Store, SystemClock, FOLD_BACKOFF_FILE, INBOX_EXT};
use serde_json::{json, Value};
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};

fn now_ms() -> i64 {
    crate::clients_job::now_ms()
}

fn ago(ms: i64) -> String {
    let d = (now_ms() - ms).max(0) / 1000;
    match d {
        0..=59 => "just now".to_owned(),
        60..=3599 => format!(
            "{} minute{} ago",
            d / 60,
            if d / 60 == 1 { "" } else { "s" }
        ),
        3600..=86_399 => format!(
            "{} hour{} ago",
            d / 3600,
            if d / 3600 == 1 { "" } else { "s" }
        ),
        _ => format!(
            "{} day{} ago",
            d / 86_400,
            if d / 86_400 == 1 { "" } else { "s" }
        ),
    }
}

/// `$SHELL -lc true`: how long it takes and whether it printed anything (bounded to 5 s).
fn login_shell(env: &Env) -> Option<Value> {
    let shell = std::env::var_os("SHELL").filter(|s| Path::new(s).is_absolute())?;
    let started = Instant::now();
    let mut child = Command::new(&shell)
        .args(["-lc", "true"])
        .env("HOME", &env.home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let mut timed_out = false;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < Duration::from_secs(5) => {
                std::thread::sleep(Duration::from_millis(5));
            }
            _ => {
                timed_out = true;
                let _ = child.kill();
                break;
            }
        }
    }
    let ms = started.elapsed().as_millis();
    let out = child.wait_with_output().ok()?;
    Some(json!({
        "shell": shell.to_string_lossy(),
        "ms": u64::try_from(ms).unwrap_or(u64::MAX),
        "silent": out.stdout.is_empty() && out.stderr.is_empty(),
        "timed_out": timed_out,
    }))
}

fn inbox_counts(paths: &StatePaths) -> (u64, bool) {
    let dir = paths.inbox_dir();
    let waiting = fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| {
                    e.file_name()
                        .to_str()
                        .is_some_and(|n| !n.starts_with('.') && n.ends_with(INBOX_EXT))
                })
                .count() as u64
        })
        .unwrap_or(0);
    (waiting, paths.inbox_overflow_path().exists())
}

fn backoff(paths: &StatePaths) -> Option<Value> {
    let p = paths.state_dir.join(FOLD_BACKOFF_FILE);
    let m = fs::symlink_metadata(&p)
        .ok()
        .filter(|m| m.is_file() && m.len() <= 256)?;
    let until = m
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis();
    let until = i64::try_from(until).ok()?;
    if until <= now_ms() {
        return None;
    }
    let class = fs::read_to_string(&p)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v["class"].as_str().map(str::to_owned))
        .filter(|c| c.len() <= 32 && c.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
    Some(json!({ "until_ms": until, "class": class }))
}

fn artifact_word(a: Artifact) -> &'static str {
    match a {
        Artifact::Current => "current",
        Artifact::Stale => "stale",
        Artifact::Missing => "missing",
    }
}

/// Builds the report (the `--json` form).
pub fn report(
    paths: &StatePaths,
    env: Option<Env>,
    bin: Option<std::path::PathBuf>,
    time_shell: bool,
) -> Value {
    let state_exists = paths.db_path.exists();
    let store = if state_exists {
        Store::open_current(paths, Arc::new(SystemClock)).ok()
    } else {
        None
    };
    let meta = store.as_ref().and_then(|s| s.doctor_meta().ok());
    let rows = store
        .as_ref()
        .and_then(|s| s.client_rows().ok())
        .unwrap_or_default();
    let ctx = env
        .clone()
        .zip(bin.clone())
        .and_then(|(e, b)| Ctx::new(e, paths.clone(), b));
    let (waiting, overflow) = inbox_counts(paths);
    let shim = paths.shim_path().map(|p| {
        let state = install::shim_state(&p);
        let sha = fs::read(&p).ok().map(|b| codepet_storage::sha256_hex(&b));
        let (word, paths_match) = match &state {
            ShimState::Missing => ("missing", false),
            ShimState::Tombstone => ("tombstone", false),
            ShimState::Foreign => ("foreign", false),
            ShimState::Shim { state_dir, bin: b, current } => {
                let s_ok = Path::new(state_dir) == paths.state_dir;
                let b_ok = bin.as_deref().is_some_and(|mine| {
                    Path::new(b) == mine
                        || fs::canonicalize(b).ok() == fs::canonicalize(mine).ok()
                });
                (if *current { "current" } else { "outdated" }, s_ok && b_ok)
            }
        };
        json!({ "path": p.to_string_lossy(), "state": word, "sha256": sha, "paths_match": paths_match })
    });
    let clients: Vec<Value> = ClientId::ALL
        .iter()
        .map(|c| {
            let row = rows.iter().find(|r| r.client == c.as_str());
            let d = env.as_ref().map(|e| detect::detect(e, *c));
            let mut v = json!({
                "id": c.as_str(),
                "name": c.display_name(),
                "installed": d.as_ref().is_some_and(detect::Detected::installed),
                "version": d.as_ref().and_then(|d| d.version.clone()),
                "connected": row.is_some_and(|r| r.connected),
                "needs_step": row.and_then(|r| r.needs_step.clone()),
                "last_seen_ms": row.and_then(|r| r.last_seen_ms),
                "first_seen_ms": row.and_then(|r| r.first_seen_ms),
                "failed_reason": row.and_then(|r| r.failed_reason().map(str::to_owned)),
                "job": row.and_then(|r| r.job_kind.clone()),
                "disconnected_flag": paths.disconnected_flag_path(c.as_str()).exists(),
                "artifact": ctx.as_ref().map(|ctx| artifact_word(install::status(ctx, *c))),
            });
            if *c == ClientId::Codex {
                if let Some(ctx) = &ctx {
                    let st = codex::status(ctx);
                    v["codex"] = json!({
                        "hooks_off": st.hooks_off,
                        "lines": st.lines,
                        "trusted": st.trusted,
                        "turned_off": st.disabled,
                        "followed_by_other_hooks": st.followed,
                    });
                }
            }
            v
        })
        .collect();
    let codex_installed = clients
        .iter()
        .any(|c| c["id"] == "codex" && c["installed"] == true);
    json!({
        "schema": "codepet.doctor.v1",
        "version": env!("CARGO_PKG_VERSION"),
        "state_dir": paths.state_dir.to_string_lossy(),
        "state_exists": state_exists,
        "enabled": meta.as_ref().map(|m| m.enabled),
        "helpers_counted": meta.as_ref().map(|m| m.count_helpers),
        "shim": shim,
        "inbox": { "waiting": waiting, "dropped": meta.as_ref().map_or(0, |m| m.events_dropped), "overflow": overflow },
        "fold_backoff": backoff(paths),
        "clients": clients,
        "codex_login_shell": if time_shell && codex_installed { env.as_ref().and_then(login_shell) } else { None },
    })
}

fn line_for(c: &Value) -> String {
    let name = c["name"].as_str().unwrap_or_default();
    let installed = c["installed"] == true;
    let connected = c["connected"] == true;
    let mut s = if let Some(job) = c["job"].as_str() {
        format!("{name}: {job} running")
    } else if let Some(r) = c["failed_reason"].as_str() {
        format!("{name}: couldn't connect ({})", r.replace('_', " "))
    } else if c["needs_step"] == "codex_hooks_off" {
        format!("{name}: hooks are turned off in Codex; CodePet changes nothing there")
    } else if c["needs_step"] == "codex_trust" {
        format!(
            "{name}: connected, one more step: open Codex, type /hooks, trust the CodePet lines"
        )
    } else if connected {
        match (c["last_seen_ms"].as_i64(), c["first_seen_ms"].as_i64()) {
            (Some(t), _) | (None, Some(t)) => format!("{name}: seen {}", ago(t)),
            _ => {
                format!("{name}: connected, waiting for the first event (open a new {name} window)")
            }
        }
    } else if installed {
        format!("{name}: installed, not connected")
    } else {
        format!("{name}: not installed")
    };
    if let Some(v) = c["version"].as_str() {
        s.push_str(&format!(" [{v}]"));
    }
    if c["artifact"] == "stale" {
        s.push_str("; CodePet's files there are out of date (Connect repairs them)");
    }
    if let Some(cx) = c.get("codex") {
        if cx["lines"].as_u64().unwrap_or(0) > 0 {
            s.push_str(&format!(
                "; trusted {}/{} lines",
                cx["trusted"], cx["lines"]
            ));
        }
        let followed = cx["followed_by_other_hooks"].as_u64().unwrap_or(0);
        if followed > 0 {
            s.push_str(&format!(
                "; warning: {followed} CodePet line(s) have other hooks after them, so Stop watching leaves them in place (they do nothing)"
            ));
        }
    }
    s
}

/// The plain-word form.
pub fn plain(r: &Value) -> Vec<String> {
    let mut out = Vec::new();
    out.push(match r["enabled"].as_bool() {
        Some(true) => "CodePet is On".to_owned(),
        Some(false) => "CodePet is Off (hooks exit at once)".to_owned(),
        None if r["state_exists"] == false => "CodePet has no saved data yet".to_owned(),
        None => "CodePet data could not be read".to_owned(),
    });
    for c in r["clients"].as_array().into_iter().flatten() {
        out.push(line_for(c));
    }
    out.push(format!(
        "Inbox: {} waiting, {} dropped{}",
        r["inbox"]["waiting"],
        r["inbox"]["dropped"],
        if r["inbox"]["overflow"] == true {
            " (overflowed)"
        } else {
            ""
        }
    ));
    match r["fold_backoff"].as_object() {
        Some(b) => {
            let until = b["until_ms"].as_i64().unwrap_or(0);
            let secs = (until - now_ms()).max(0) / 1000;
            out.push(format!(
                "Fold: backing off for {secs} s ({})",
                b["class"].as_str().unwrap_or("unknown")
            ));
        }
        None => out.push("Fold: ready".to_owned()),
    }
    match r["shim"].as_object() {
        Some(s) => out.push(format!(
            "Hook shim: {} ({}){}{}",
            s["path"].as_str().unwrap_or_default(),
            s["state"].as_str().unwrap_or_default(),
            s["sha256"]
                .as_str()
                .map(|h| format!(", sha256 {h}"))
                .unwrap_or_default(),
            match s["state"].as_str() {
                Some("current" | "outdated") if s["paths_match"] == true =>
                    ", paths match this install".to_owned(),
                Some("current" | "outdated") =>
                    ", paths do NOT match this install (Connect again)".to_owned(),
                _ => String::new(),
            }
        )),
        None => out.push("Hook shim: no data directory".to_owned()),
    }
    if let Some(l) = r["codex_login_shell"].as_object() {
        out.push(format!(
            "Codex login shell: {} -lc took {} ms{}",
            l["shell"].as_str().unwrap_or_default(),
            l["ms"],
            if l["silent"] == true {
                ", prints nothing (good)".to_owned()
            } else {
                ", PRINTS OUTPUT: Codex hooks (CodePet's and others') may fail or leak it"
                    .to_owned()
            }
        ));
        out.push(
            "Off still costs Codex one login shell per event; Stop watching removes it.".to_owned(),
        );
    }
    out
}

pub fn run(args: &[OsString]) -> i32 {
    let json = match args {
        [] => false,
        [a] if a == "--json" => true,
        _ => {
            let _ = writeln!(std::io::stderr(), "usage: codepet doctor [--json]");
            return 2;
        }
    };
    let paths = match StatePaths::from_env() {
        Ok(p) => p,
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "STORAGE_UNAVAILABLE: {e}");
            return 1;
        }
    };
    let r = report(
        &paths,
        Env::from_process(),
        crate::clients_job::shim_bin(),
        true,
    );
    let mut out = std::io::stdout().lock();
    let written = if json {
        writeln!(out, "{r}")
    } else {
        plain(&r).iter().try_for_each(|l| writeln!(out, "{l}"))
    };
    i32::from(written.is_err())
}
