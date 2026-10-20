//! Bounded reader for `bar-state.json` (qml/omarchy/BarState.js `parse` and the watcher's
//! version rule). Anything that is not a valid export is rejected and the last good
//! document stays on screen.

use serde_json::{Map, Value};

const MAX_BYTES: usize = 16384;
const CLIENT_IDS: [&str; 5] = ["claude-code", "codex", "opencode", "hermes", "grok-cli"];
const METERS: [&str; 4] = ["hunger", "thirst", "mental_health", "hygiene"];
const MAX_SAFE: f64 = 9007199254740991.0;

fn is_count(v: Option<&Value>) -> bool {
    v.and_then(Value::as_f64).is_some_and(|n| n.is_finite() && n.fract() == 0.0 && (0.0..=MAX_SAFE).contains(&n))
}
fn is_ms(v: Option<&Value>) -> bool {
    v.and_then(Value::as_f64).is_some_and(|n| n.is_finite() && n.fract() == 0.0 && n.abs() <= MAX_SAFE)
}
fn is_ms_or_null(v: Option<&Value>) -> bool {
    matches!(v, Some(Value::Null)) || is_count(v)
}
fn is_token(v: Option<&Value>) -> bool {
    v.and_then(Value::as_str)
        .is_some_and(|s| (1..=32).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'))
}
fn bounded(v: Option<&Value>, max: usize) -> bool {
    v.and_then(Value::as_str).is_some_and(|s| s.len() <= max)
}
fn is_bool(v: Option<&Value>) -> bool {
    matches!(v, Some(Value::Bool(_)))
}
fn client_or_null(v: Option<&Value>) -> bool {
    matches!(v, Some(Value::Null)) || v.and_then(Value::as_str).is_some_and(|s| CLIENT_IDS.contains(&s))
}
fn absent(v: Option<&Value>) -> bool {
    v.is_none()
}

fn valid_meters(m: Option<&Value>) -> bool {
    let Some(m) = m.and_then(Value::as_object) else { return false };
    METERS.iter().all(|k| is_count(m.get(*k)) && m[*k].as_f64().unwrap_or(0.0) <= 10000.0)
}

fn valid_life(l: Option<&Value>) -> bool {
    let l = match l {
        None | Some(Value::Null) => return true,
        Some(Value::Object(l)) => l,
        _ => return false,
    };
    if !bounded(l.get("life_id"), 128) || !bounded(l.get("name"), 96) {
        return false;
    }
    let pet_ok = l.get("appearance").and_then(Value::as_str).is_some_and(|s| {
        (2..=24).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
    });
    if !pet_ok {
        return false;
    }
    if !is_count(l.get("stage")) || !(1.0..=3.0).contains(&l["stage"].as_f64().unwrap_or(0.0)) {
        return false;
    }
    if !is_count(l.get("level")) || !is_count(l.get("xp")) || !is_token(l.get("status")) {
        return false;
    }
    if (!absent(l.get("mode")) && !is_token(l.get("mode")))
        || (!absent(l.get("pace")) && !is_token(l.get("pace")))
        || (!absent(l.get("ordinal")) && !is_count(l.get("ordinal")))
    {
        return false;
    }
    ["armed", "paused", "fainted", "death_pending"].iter().all(|f| is_bool(l.get(*f)))
}

fn valid_care(c: Option<&Value>) -> bool {
    let Some(c) = c.and_then(Value::as_object) else { return false };
    if !["sleeping", "sleep_due", "bathroom_due"].iter().all(|k| is_bool(c.get(*k))) {
        return false;
    }
    if !is_ms(c.get("sleep_remaining_ms")) {
        return false;
    }
    match c.get("sleep_started_at_ms") {
        None | Some(Value::Null) => {}
        v if !is_ms(v) => return false,
        _ => {}
    }
    if !absent(c.get("ready_to_work")) && !is_bool(c.get("ready_to_work")) {
        return false;
    }
    match c.get("readiness_reasons") {
        None => true,
        Some(Value::Array(r)) => r.len() <= 8 && r.iter().all(|x| bounded(Some(x), 160)),
        _ => false,
    }
}

fn valid_task(t: Option<&Value>) -> bool {
    match t {
        None | Some(Value::Null) => true,
        Some(Value::Object(t)) => {
            is_token(t.get("state")) && matches!(t.get("category"), None | Some(Value::Null) | Some(Value::String(_)))
                && (t.get("category").is_none_or(|c| c.is_null() || is_token(Some(c))))
        }
        _ => false,
    }
}

fn valid_needs(n: Option<&Value>) -> bool {
    let n = match n {
        None => return true,
        Some(Value::Array(n)) => n,
        _ => return false,
    };
    if n.len() > METERS.len() {
        return false;
    }
    n.iter().all(|e| {
        let Some(e) = e.as_object() else { return false };
        let need_ok = e.get("need").and_then(Value::as_str).is_some_and(|s| METERS.contains(&s));
        let opt_ms = |k: &str| matches!(e.get(k), None | Some(Value::Null)) || is_ms(e.get(k));
        need_ok
            && is_token(e.get("action"))
            && is_token(e.get("state"))
            && opt_ms("until_warning_ms")
            && opt_ms("until_critical_ms")
            && opt_ms("until_end_ms")
    })
}

fn valid_events(ev: Option<&Value>) -> bool {
    let ev = match ev {
        None => return true,
        Some(Value::Array(ev)) => ev,
        _ => return false,
    };
    ev.len() <= 64
        && ev.iter().all(|e| {
            e.as_object().is_some_and(|e| {
                is_count(e.get("seq"))
                    && is_count(e.get("stage"))
                    && (1.0..=3.0).contains(&e["stage"].as_f64().unwrap_or(0.0))
            })
        })
}

fn valid_app(a: &Value) -> bool {
    a.as_object().is_some_and(|a| {
        a.get("id").and_then(Value::as_str).is_some_and(|s| CLIENT_IDS.contains(&s))
            && a.get("state").and_then(Value::as_str).is_some_and(|s| ["working", "waiting", "error"].contains(&s))
            && is_count(a.get("since_ms"))
            && is_count(a.get("stale_at_ms"))
    })
}

fn valid_test(t: Option<&Value>) -> bool {
    match t {
        Some(Value::Null) => true,
        Some(Value::Object(t)) => {
            t.get("state").and_then(Value::as_str).is_some_and(|s| ["running", "done", "failed"].contains(&s))
                && is_count(t.get("at_ms"))
                && client_or_null(t.get("client"))
        }
        _ => false,
    }
}

fn valid_agent(a: Option<&Value>) -> bool {
    let Some(a) = a.and_then(Value::as_object) else { return false };
    let state_ok = a.get("state").and_then(Value::as_str).is_some_and(|s| ["idle", "error", "working", "waiting"].contains(&s));
    let apps_ok = a.get("apps").and_then(Value::as_array).is_some_and(|x| x.len() <= 5 && x.iter().all(valid_app));
    state_ok
        && apps_ok
        && is_ms_or_null(a.get("stale_at_ms"))
        && is_count(a.get("finished_seq"))
        && is_ms_or_null(a.get("finished_at_ms"))
        && client_or_null(a.get("finished_app"))
        && is_bool(a.get("finished_treat"))
        && is_bool(a.get("finished_jar_full"))
        && is_count(a.get("hello_seq"))
        && is_ms_or_null(a.get("hello_at_ms"))
        && client_or_null(a.get("hello_app"))
        && valid_test(a.get("test"))
        && is_ms_or_null(a.get("error_at_ms"))
        && client_or_null(a.get("error_app"))
        && is_bool(a.get("helpers_busy"))
}

fn valid_v03(s: &Map<String, Value>) -> bool {
    let keys = ["enabled", "agent", "treats", "clients"];
    let present = keys.iter().filter(|k| s.contains_key(**k)).count();
    if present == 0 {
        return s.get("schema").and_then(Value::as_str) == Some("codepet.bar.v1");
    }
    if present != keys.len() {
        return false;
    }
    let treats_ok = s["treats"].as_object().is_some_and(|t| {
        is_count(t.get("jar")) && t["jar"].as_f64().unwrap_or(99.0) <= 10.0 && is_bool(t.get("full"))
    });
    let clients_ok = s["clients"]
        .as_object()
        .is_some_and(|c| c.iter().all(|(k, v)| CLIENT_IDS.contains(&k.as_str()) && v.is_object()));
    is_bool(s.get("enabled")) && valid_agent(s.get("agent")) && treats_ok && clients_ok
}

fn valid_memorial(m: Option<&Value>) -> bool {
    match m {
        None => true,
        Some(Value::Object(m)) => {
            is_count(m.get("count"))
                && (matches!(m.get("latest_name"), None | Some(Value::Null)) || bounded(m.get("latest_name"), 96))
        }
        _ => false,
    }
}

/// Bounded parse. Returns the validated document, or None for anything else.
pub fn parse(text: &[u8]) -> Option<Value> {
    if text.is_empty() || text.len() > MAX_BYTES {
        return None;
    }
    let v: Value = serde_json::from_slice(text).ok()?;
    let s = v.as_object()?;
    let schema_ok = s.get("schema").and_then(Value::as_str).is_some_and(|x| x == "codepet.bar.v1" || x == "codepet.bar.v2");
    if !schema_ok || !is_count(s.get("state_version")) {
        return None;
    }
    if let Some(db) = s.get("db_id") {
        let ok = db.as_str().is_some_and(|d| {
            d.len() == 27 && d.starts_with("db-") && d[3..].bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        });
        if !ok {
            return None;
        }
    }
    if !valid_meters(s.get("meters"))
        || !valid_life(s.get("life"))
        || !valid_care(s.get("care"))
        || !valid_task(s.get("task"))
        || !valid_needs(s.get("needs"))
        || !valid_events(s.get("unseen_events"))
        || !valid_memorial(s.get("memorial"))
    {
        return None;
    }
    if !matches!(s.get("critical"), None | Some(Value::Null) | Some(Value::Object(_))) {
        return None;
    }
    if !valid_v03(s) {
        return None;
    }
    Some(v)
}

/// The watcher's rule: accept a newer version of the same database, or any other database.
#[derive(Default)]
pub struct Watcher {
    last_version: f64,
    last_db: String,
    started: bool,
}

impl Watcher {
    pub fn accept(&mut self, doc: &Value) -> bool {
        let db = doc.get("db_id").and_then(Value::as_str).unwrap_or("").to_string();
        let version = doc.get("state_version").and_then(Value::as_f64).unwrap_or(0.0);
        if self.started && db == self.last_db && version <= self.last_version {
            return false;
        }
        self.started = true;
        self.last_db = db;
        self.last_version = version;
        true
    }
}
