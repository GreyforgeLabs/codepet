//! The v0.3 plain-word commands of WP1 (spec §6.3): `migrate`, `on`, `off`,
//! `helpers on|off` and `forget-history`. Each wrapper runs the same code path as its
//! protocol method (`codepet rpc`), with a fresh request id, and prints one plain line.
//! Failures print `CODE: message` on one stderr line and exit 1.
use crate::errors::Failure;
use crate::rpc;
use codepet_protocol::{Method, Request};
use codepet_storage::{StatePaths, Store, SystemClock, SCHEMA_VERSION};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::sync::Arc;

/// `cli-` + 24 lowercase hex: a fresh envelope request id for a wrapper command.
fn request_id() -> Result<String, Failure> {
    let mut bytes = [0u8; 12];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|e| {
            Failure::new(
                codepet_protocol::ErrorCode::Internal,
                format!("no random id: {e}"),
            )
        })?;
    Ok(format!(
        "cli-{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}

fn fail(f: &Failure) -> i32 {
    let mut err = std::io::stderr().lock();
    let line = f.message.replace(['\n', '\r'], " ");
    let _ = writeln!(err, "{}: {line}", f.code).and_then(|()| err.flush());
    1
}

fn say(line: &str) -> i32 {
    let mut out = std::io::stdout().lock();
    match writeln!(out, "{line}").and_then(|()| out.flush()) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

/// Runs one protocol method exactly like `codepet rpc` (receipts, Off policy, publishing
/// `bar-state.json`) and returns its result.
fn call(method: Method, params: Value) -> Result<Value, Failure> {
    let request = Request::new(request_id()?, method, params);
    let frame = request.to_frame().map_err(Failure::from)?;
    let (response, _job) = rpc::handle(frame.trim_end_matches('\n'));
    match (response.result, response.error) {
        (Some(result), None) if response.ok => Ok(result),
        (_, Some(e)) => Err(Failure {
            code: e.code,
            message: e.message,
            retryable: e.retryable,
        }),
        _ => Err(Failure::new(
            codepet_protocol::ErrorCode::Internal,
            "no result",
        )),
    }
}

/// `codepet migrate` (run by the installer): opens the database, creating it at the current
/// schema or upgrading an older one after a `VACUUM INTO …bak-v<old>-<ms>` backup, then
/// republishes `bar-state.json` and the `off`/`helpers` flags. A newer or corrupt database
/// is refused untouched.
pub fn migrate() -> i32 {
    let opened = StatePaths::from_env()
        .map_err(Failure::from)
        .and_then(|paths| {
            Store::open_migrating(&paths, Arc::new(SystemClock)).map_err(Failure::from)
        });
    let (store, report) = match opened {
        Ok(v) => v,
        Err(f) => return fail(&f),
    };
    rpc::publish_bar_state(&store);
    let line = match (report.from, &report.backup) {
        (0, _) => format!("CodePet data created (schema {SCHEMA_VERSION})."),
        (from, Some(backup)) if from < report.to => format!(
            "CodePet data upgraded from schema {from} to {}. Backup: {}",
            report.to,
            backup.display()
        ),
        _ => format!("CodePet data is up to date (schema {SCHEMA_VERSION})."),
    };
    say(&line)
}

/// `codepet on` / `codepet off` (`app.set_enabled`).
pub fn set_enabled(on: bool) -> i32 {
    match call(Method::AppSetEnabled, json!({ "enabled": on })) {
        Ok(_) => say(if on {
            "CodePet is On"
        } else {
            "CodePet is Off"
        }),
        Err(f) => fail(&f),
    }
}

/// `codepet helpers on|off` (`app.set_helpers`).
pub fn set_helpers(on: bool) -> i32 {
    match call(Method::AppSetHelpers, json!({ "enabled": on })) {
        Ok(_) => say(if on {
            "Background helpers are counted"
        } else {
            "Background helpers are not counted"
        }),
        Err(f) => fail(&f),
    }
}

/// `codepet forget-history` (`agent.forget_history`).
pub fn forget_history() -> i32 {
    match call(Method::AgentForgetHistory, json!({})) {
        Ok(_) => say("Coding history forgotten. Treats and growth are kept."),
        Err(f) => fail(&f),
    }
}
