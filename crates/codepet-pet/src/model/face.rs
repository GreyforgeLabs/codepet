//! The shared face priority (petstate.js, generated from docs/v0.3/contracts) and the
//! desktop-pet presentation rules (OverlayFace.js), ported to Rust for the native pet.
//!
//! Pure: no I/O, no clock. Parity with the JavaScript reference is checked by the
//! contract vectors (tests/priority_vectors.rs) and by a differential test against Node.

use super::js::{is_true, js_string, num, obj, or_null, s};
use serde_json::{json, Map, Value};

pub const CLIENTS: [&str; 5] = ["claude-code", "codex", "opencode", "hermes", "grok-cli"];
pub const NEED_PRIORITY: [&str; 4] = ["food", "water", "play", "wash"];
const CELEBRATE_WINDOW_MS: f64 = 30000.0;
const CELEBRATE_GRACE_MS: f64 = 2000.0;
const ERROR_RECENT_MS: f64 = 120000.0;
const ERROR_FRAME_MS: f64 = 3000.0;
const TEST_DONE_DELAY_MS: f64 = 1500.0;
const TEST_ARRIVAL_MS: f64 = 5000.0;

pub fn client_name(id: &str) -> Option<&'static str> {
    Some(match id {
        "claude-code" => "Claude Code",
        "codex" => "Codex",
        "opencode" => "OpenCode",
        "hermes" => "Hermes",
        "grok-cli" => "Grok",
        _ => return None,
    })
}

fn client_rank(id: &str) -> usize {
    CLIENTS.iter().position(|c| *c == id).unwrap_or(CLIENTS.len())
}

pub fn need_level(state: &str) -> &'static str {
    match state {
        "ok" => "happy",
        "low" => "soon",
        "empty" => "now",
        "critical" => "critical",
        _ => "",
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Celebrate {
    pub kind: String,
    pub seq: Option<f64>,
    pub at_ms: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    pub state: Option<String>,
    pub bar: Option<String>,
    pub line: Option<String>,
    pub vars: Map<String, Value>,
    pub overlay: String,
    pub pulse: bool,
    pub treats_visible: bool,
    pub bubble: Option<Value>,
    pub wake_at_ms: Option<f64>,
    pub celebrate: Option<Celebrate>,
    pub need_levels: Option<Map<String, Value>>,
}

impl Default for Face {
    fn default() -> Self {
        Face {
            state: None,
            bar: None,
            line: None,
            vars: Map::new(),
            overlay: "none".into(),
            pulse: false,
            treats_visible: false,
            bubble: None,
            wake_at_ms: None,
            celebrate: None,
            need_levels: None,
        }
    }
}

impl Face {
    pub fn state(&self) -> &str {
        self.state.as_deref().unwrap_or("")
    }

    /// The face as the JavaScript object it mirrors (tests and the status verb).
    pub fn to_value(&self) -> Value {
        let mut m = Map::new();
        let opt = |v: &Option<String>| v.as_ref().map(|x| s(x)).unwrap_or(Value::Null);
        m.insert("state".into(), opt(&self.state));
        m.insert("bar".into(), opt(&self.bar));
        m.insert("line".into(), opt(&self.line));
        m.insert("vars".into(), Value::Object(self.vars.clone()));
        m.insert("overlay".into(), s(&self.overlay));
        m.insert("pulse".into(), Value::Bool(self.pulse));
        m.insert("treats_visible".into(), Value::Bool(self.treats_visible));
        m.insert("bubble".into(), self.bubble.clone().unwrap_or(Value::Null));
        m.insert("wake_at_ms".into(), self.wake_at_ms.map(|w| json!(w)).unwrap_or(Value::Null));
        m.insert(
            "celebrate".into(),
            self.celebrate
                .as_ref()
                .map(|c| {
                    json!({"kind": c.kind, "seq": c.seq.map(|x| json!(x)).unwrap_or(Value::Null),
                           "at_ms": c.at_ms.map(|x| json!(x)).unwrap_or(Value::Null)})
                })
                .unwrap_or(Value::Null),
        );
        if let Some(l) = &self.need_levels {
            m.insert("need_levels".into(), Value::Object(l.clone()));
        }
        Value::Object(m)
    }

    fn set(&mut self, state: &str, bar: &str, line: &str, vars: Map<String, Value>, overlay: &str, bubble: Option<Value>) {
        self.state = Some(state.into());
        self.bar = Some(bar.into());
        self.line = Some(line.into());
        self.vars = vars;
        self.overlay = overlay.into();
        self.bubble = bubble;
    }
}

/// What a UI already celebrated (petstate.js `seen`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Seen {
    pub finished_seq: f64,
    pub hello_seq: f64,
    pub test_at_ms: f64,
}

fn vars(pairs: &[(&str, Value)]) -> Map<String, Value> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

/// petstate.js `sharedPriority(doc, now, seen)`.
pub fn shared_priority(doc: &Value, now: f64, seen: Seen) -> Face {
    let mut res = Face::default();
    let Some(doc) = doc.as_object() else { return res };
    let life = obj(doc.get("life"));
    if doc.get("enabled") == Some(&Value::Bool(false)) {
        res.state = Some("off".into());
        res.bar = Some("off".into());
        if let Some(life) = life {
            res.line = Some("off.line".into());
            res.vars = vars(&[("name", s(&js_string(life.get("name"))))]);
        }
        return res;
    }
    let Some(life) = life else {
        res.state = Some("no_pet".into());
        res.bar = Some("egg".into());
        return res;
    };
    let name = s(&js_string(life.get("name")));
    if life.get("armed") == Some(&Value::Bool(false)) {
        res.state = Some("welcome".into());
        res.bar = Some("hi".into());
        res.line = Some("picker.welcome".into());
        res.vars = vars(&[("name", name)]);
        res.overlay = "wander".into();
        return res;
    }
    let empty = Map::new();
    let ag = obj(doc.get("agent")).unwrap_or(&empty);
    let apps: Vec<&Value> = ag.get("apps").and_then(Value::as_array).map(|a| a.iter().collect()).unwrap_or_default();
    let counting: Vec<&Map<String, Value>> = apps
        .iter()
        .filter_map(|a| a.as_object())
        .filter(|a| {
            num(a.get("stale_at_ms")).is_some_and(|st| st > now) && num(a.get("since_ms")).is_some()
        })
        .collect();
    let mut working: Vec<String> = Vec::new();
    let mut waiting: Vec<&Map<String, Value>> = Vec::new();
    for a in &counting {
        match a.get("state").and_then(Value::as_str) {
            Some("working") => working.push(js_string(a.get("id"))),
            Some("waiting") => waiting.push(a),
            _ => {}
        }
    }
    waiting.sort_by(|x, y| {
        let (xs, ys) = (num(x.get("since_ms")).unwrap_or(0.0), num(y.get("since_ms")).unwrap_or(0.0));
        if xs != ys {
            return ys.partial_cmp(&xs).unwrap_or(std::cmp::Ordering::Equal);
        }
        client_rank(&js_string(x.get("id"))).cmp(&client_rank(&js_string(y.get("id"))))
    });
    let treats = obj(doc.get("treats")).unwrap_or(&empty);
    let jar = num(treats.get("jar")).unwrap_or(0.0);
    let care = obj(doc.get("care")).unwrap_or(&empty);
    let sleeping = is_true(care.get("sleeping"));
    let fainted = is_true(life.get("fainted"));
    let paused = is_true(life.get("paused"));
    let tv = jar > 0.0 && !fainted && !paused && !sleeping;
    res.treats_visible = tv;
    let mut wakes: Vec<f64> = counting.iter().filter_map(|a| num(a.get("stale_at_ms"))).collect();
    let t = obj(ag.get("test"));
    let mut testing = false;
    if let Some(t) = t {
        if t.get("state").and_then(Value::as_str) == Some("running") {
            if let Some(at) = num(t.get("at_ms")) {
                let end = at + TEST_DONE_DELAY_MS + TEST_ARRIVAL_MS;
                if now < end {
                    testing = true;
                    wakes.push(end);
                }
            }
        }
    }
    struct Cand {
        kind: &'static str,
        line: &'static str,
        vars: Map<String, Value>,
        seq: Option<f64>,
        at_ms: Option<f64>,
    }
    let mut cands: Vec<Cand> = Vec::new();
    if let Some(unseen) = doc.get("unseen_events").and_then(Value::as_array) {
        if !unseen.is_empty() {
            let mut top = 0.0;
            for u in unseen {
                if let Some(q) = obj(Some(u)).and_then(|u| num(u.get("seq"))) {
                    if q > top {
                        top = q;
                    }
                }
            }
            cands.push(Cand { kind: "grew", line: "grew", vars: vars(&[("name", name.clone())]), seq: Some(top), at_ms: None });
        }
    }
    if let Some(t) = t {
        if t.get("state").and_then(Value::as_str) == Some("done") {
            if let Some(at) = num(t.get("at_ms")) {
                if at > seen.test_at_ms && now < at + CELEBRATE_WINDOW_MS {
                    wakes.push(at + CELEBRATE_WINDOW_MS);
                    cands.push(Cand { kind: "test", line: "act.ready", vars: vars(&[("name", name.clone())]), seq: None, at_ms: Some(at) });
                }
            }
        }
    }
    let hello_seq = num(ag.get("hello_seq")).unwrap_or(0.0);
    let hello_at = num(ag.get("hello_at_ms"));
    if let Some(ha) = hello_at {
        if hello_seq > seen.hello_seq && now < ha + CELEBRATE_WINDOW_MS {
            wakes.push(ha + CELEBRATE_WINDOW_MS);
            cands.push(Cand {
                kind: "hello",
                line: "act.hello",
                vars: vars(&[("name", name.clone()), ("app", or_null(ag.get("hello_app")))]),
                seq: Some(hello_seq),
                at_ms: Some(ha),
            });
        }
    }
    let finished_seq = num(ag.get("finished_seq")).unwrap_or(0.0);
    if let Some(fa) = num(ag.get("finished_at_ms")) {
        if finished_seq > seen.finished_seq && now < fa + CELEBRATE_WINDOW_MS {
            if now < fa + CELEBRATE_GRACE_MS {
                wakes.push(fa + CELEBRATE_GRACE_MS);
            } else {
                wakes.push(fa + CELEBRATE_WINDOW_MS);
                if !working.contains(&js_string(ag.get("finished_app"))) {
                    let (line, v) = if is_true(ag.get("finished_jar_full")) {
                        ("act.done_full", Map::new())
                    } else if is_true(ag.get("finished_treat")) {
                        ("act.done_treat", vars(&[("name", name.clone())]))
                    } else {
                        ("act.done", Map::new())
                    };
                    cands.push(Cand { kind: "finished", line, vars: v, seq: Some(finished_seq), at_ms: Some(fa) });
                }
            }
        }
    }
    let ea = num(ag.get("error_at_ms"));
    let oops = ea.is_some_and(|ea| now < ea + ERROR_RECENT_MS);
    if let Some(ea) = ea {
        for end in [ea + ERROR_FRAME_MS, ea + ERROR_RECENT_MS] {
            if end > now {
                wakes.push(end);
            }
        }
    }
    res.wake_at_ms = wakes.iter().copied().reduce(f64::min);
    let mut now_needs: Vec<(String, &'static str)> = Vec::new();
    let mut soon_needs: Vec<(String, &'static str)> = Vec::new();
    if let Some(needs) = doc.get("needs").and_then(Value::as_array) {
        for n in needs {
            let Some(n) = n.as_object() else { continue };
            let action = js_string(n.get("action"));
            if !NEED_PRIORITY.contains(&action.as_str()) {
                continue;
            }
            let lv = need_level(&js_string(n.get("state")));
            if lv == "now" || lv == "critical" {
                now_needs.push((action, lv));
            } else if lv == "soon" {
                soon_needs.push((action, lv));
            }
        }
    }
    let prio = |n: &str| NEED_PRIORITY.iter().position(|x| *x == n).unwrap_or(99);
    now_needs.sort_by_key(|(n, _)| prio(n));
    soon_needs.sort_by_key(|(n, _)| prio(n));
    let star = if tv { "star" } else { "none" };
    let named = || vars(&[("name", name.clone())]);
    if fainted {
        res.set("fainted", "sleep_bandage", "mood.fainted", named(), "sleep", Some(json!({"bandage": true})));
    } else if paused {
        res.set("paused", "sleep", "mood.asleep", named(), "sleep", None);
    } else if sleeping {
        res.set("asleep", "sleep", "mood.asleep", named(), "sleep", None);
    } else if let Some(w) = waiting.first() {
        let wid = js_string(w.get("id"));
        res.set("waiting", "hand", "act.waiting", vars(&[("app", s(&wid))]), "waiting", Some(json!({"apps": [wid]})));
        res.pulse = true;
    } else if let Some((need, level)) = now_needs.first() {
        res.set(
            "need_now",
            &format!("need_{need}"),
            &format!("mood.{need}.{level}"),
            named(),
            "needs_care",
            Some(json!({"need": need})),
        );
    } else if !working.is_empty() {
        let (line, v) = match working.len() {
            1 => ("act.working.one", vars(&[("name", name.clone()), ("app", s(&working[0]))])),
            2 => ("act.working.two", vars(&[("app", s(&working[0])), ("app2", s(&working[1]))])),
            _ => ("act.working.many", named()),
        };
        let apps: Vec<Value> = working.iter().take(3).map(|a| s(a)).collect();
        res.set("working", "working", line, v, "working", Some(json!({"apps": apps})));
    } else if testing {
        let tc = or_null(t.and_then(|t| t.get("client")));
        res.set(
            "testing",
            "working",
            "act.working.one",
            vars(&[("name", name.clone()), ("app", tc.clone())]),
            "working",
            Some(json!({"apps": [tc]})),
        );
    } else if let Some(c) = cands.first() {
        let bubble = if c.kind == "grew" { json!({"star": true, "confetti": true}) } else { json!({"star": true}) };
        res.set("celebrate", "star", c.line, c.vars.clone(), "celebrate", Some(bubble));
        res.celebrate = Some(Celebrate { kind: c.kind.into(), seq: c.seq, at_ms: c.at_ms });
    } else if oops {
        let ea = ea.unwrap_or(0.0);
        let overlay = if now < ea + ERROR_FRAME_MS { "error" } else { "idle" };
        res.set("oops", star, "act.oops", vars(&[("app", or_null(ag.get("error_app")))]), overlay, None);
    } else if let Some((need, _)) = soon_needs.first() {
        res.set("need_soon", star, &format!("mood.{need}.soon"), named(), "needs_care_frame", Some(json!({"need": need})));
    } else {
        let bubble = if is_true(ag.get("helpers_busy")) { Some(json!({"helpers": true})) } else { None };
        res.set("idle", star, "mood.happy", named(), "wander", bubble);
    }
    res
}

/// petstate.js `currentPriority`: care and growth stay legible while a coding app is active.
pub fn current_priority(doc: &Value, now: f64, seen: Seen) -> Face {
    let mut activity = shared_priority(doc, now, seen);
    let Some(map) = doc.as_object() else { return activity };
    if ["off", "no_pet", "welcome", "fainted", "paused", "asleep"].contains(&activity.state()) {
        return activity;
    }
    let mut care_doc = map.clone();
    care_doc.insert("agent".into(), Value::Object(Map::new()));
    let mut needs: Vec<Value> = map.get("needs").and_then(Value::as_array).cloned().unwrap_or_default();
    let empty = Map::new();
    let meters = obj(map.get("meters")).unwrap_or(&empty);
    let meter_of = |need: &str| match need {
        "food" => "hunger",
        "water" => "thirst",
        "play" => "mental_health",
        _ => "hygiene",
    };
    for need in NEED_PRIORITY {
        let Some(value) = num(meters.get(meter_of(need))) else { continue };
        if !(0.0..=6000.0).contains(&value) {
            continue;
        }
        let recorded = needs.iter().any(|n| {
            n.as_object().is_some_and(|n| {
                n.get("action").and_then(Value::as_str) == Some(need) && {
                    let lv = need_level(&js_string(n.get("state")));
                    lv == "now" || lv == "critical" || (value > 3000.0 && lv == "soon")
                }
            })
        });
        if !recorded {
            needs.push(json!({"action": need, "state": if value <= 3000.0 { "empty" } else { "low" }}));
        }
    }
    let mut levels = Map::new();
    for n in &needs {
        if let Some(n) = n.as_object() {
            levels.insert(js_string(n.get("action")), s(need_level(&js_string(n.get("state")))));
        }
    }
    care_doc.insert("needs".into(), Value::Array(needs));
    let mut care = shared_priority(&Value::Object(care_doc), now, seen);
    care.need_levels = Some(levels.clone());
    activity.need_levels = Some(levels);
    let grew = care.celebrate.as_ref().is_some_and(|c| c.kind == "grew");
    if care.state() == "need_now" || care.state() == "need_soon" || grew {
        care.wake_at_ms = activity.wake_at_ms;
        if care.state() == "need_soon" {
            let need = care.bubble.as_ref().and_then(|b| b.get("need")).map(|n| js_string(Some(n))).unwrap_or_default();
            care.bar = Some(format!("need_{need}"));
        }
        return care;
    }
    activity
}

// ---- OverlayFace.js ----

/// An overlay value's animation: (semantic state, may roam, bobs).
pub fn animation(overlay: &str) -> (&'static str, bool, bool) {
    match overlay {
        "none" => ("", false, false),
        "sleep" => ("sleep", false, false),
        "waiting" => ("waiting", false, true),
        "needs_care" => ("needs_care", false, false),
        "needs_care_frame" => ("needs_care", true, false),
        "working" => ("working", true, false),
        "celebrate" => ("idle", false, false),
        "error" => ("error", false, false),
        _ => ("idle", true, false),
    }
}

/// Owned jobs have a care icon; observed coding clients keep their app icon.
pub fn with_work(face: &Face, work: Option<&Value>) -> Face {
    if [
        "off", "asleep", "fainted", "paused", "no_pet", "welcome", "need_now", "need_soon", "celebrate",
    ]
    .contains(&face.state())
    {
        return face.clone();
    }
    let Some(task) = work.and_then(|w| w.get("task")).filter(|t| !t.is_null() && *t != &Value::Bool(false)) else {
        return face.clone();
    };
    let status = js_string(task.get("status"));
    if !["preparing", "running", "review", "applying", "scouted"].contains(&status.as_str()) {
        return face.clone();
    }
    let has_finding = task.get("has_finding").is_some_and(|f| !matches!(f, Value::Null | Value::Bool(false)));
    if status == "scouted" && !has_finding {
        return face.clone();
    }
    let mut next = face.clone();
    let state = match status.as_str() {
        "scouted" => "idle",
        "review" | "applying" => "waiting",
        _ => "working",
    };
    next.state = Some(state.into());
    next.overlay = state.into();
    let source = js_string(task.get("source_kind"));
    next.bubble = Some(if source == "scout" || source == "scout_fix" {
        json!({})
    } else {
        let kind = js_string(task.get("kind"));
        json!({"need": match kind.as_str() { "bug" => "food", "research" => "water", _ => "play" }})
    });
    next.celebrate = None;
    next
}

pub fn app_tile(id: &str) -> Option<&'static str> {
    Some(match id {
        "claude-code" => "apps/app_claude_code.webp",
        "codex" => "apps/app_codex.webp",
        "opencode" => "apps/app_opencode.webp",
        "hermes" => "apps/app_hermes.webp",
        "grok-cli" => "apps/app_grok.webp",
        _ => return None,
    })
}

/// "Codex working", "Codex +1 needs you", or "" (care bubbles name no client).
pub fn observer_label(face: &Face) -> String {
    if !["working", "waiting", "testing"].contains(&face.state()) {
        return String::new();
    }
    let Some(apps) = face.bubble.as_ref().and_then(|b| b.get("apps")).and_then(Value::as_array) else {
        return String::new();
    };
    let apps: Vec<&str> = apps
        .iter()
        .filter_map(Value::as_str)
        .filter(|id| app_tile(id).is_some() && client_name(id).is_some())
        .collect();
    let Some(first) = apps.first() else { return String::new() };
    let mut out = client_name(first).unwrap_or_default().to_string();
    if apps.len() > 1 {
        out.push_str(&format!(" +{}", apps.len() - 1));
    }
    out.push_str(match face.state() {
        "waiting" => " needs you",
        "testing" => " checking",
        _ => " working",
    });
    out
}

/// Icon paths (relative to assets/icons/) for a face bubble, at most four.
pub fn bubble_icons(bubble: Option<&Value>, care_set: &str, waiting: bool) -> Vec<String> {
    let mut out = Vec::new();
    let Some(b) = bubble.and_then(Value::as_object) else { return out };
    if let Some(apps) = b.get("apps").and_then(Value::as_array) {
        if waiting {
            out.push("status/bubble_wait.webp".to_string());
        }
        for a in apps.iter().take(3) {
            if let Some(tile) = a.as_str().and_then(app_tile) {
                out.push(tile.to_string());
            }
        }
    }
    if let Some(need) = b.get("need").and_then(Value::as_str) {
        if NEED_PRIORITY.contains(&need) {
            let set = if ["floor", "aquatic", "robot"].contains(&care_set) { care_set } else { "floor" };
            out.push(format!("care-{set}/{need}.webp"));
        }
    }
    for (key, icon) in [
        ("bandage", "status/bandage.webp"),
        ("star", "care-floor/star.webp"),
        ("confetti", "status/confetti.webp"),
        ("helpers", "status/robot_helpers.webp"),
    ] {
        if is_true(b.get(key)) {
            out.push(icon.to_string());
        }
    }
    out.truncate(4);
    out
}

/// A key that changes exactly when a new celebration should play once.
pub fn celebration_key(c: Option<&Celebrate>) -> String {
    let Some(c) = c else { return String::new() };
    let f = |v: Option<f64>| v.map(super::js::number_string).unwrap_or_else(|| "null".into());
    format!("{}:{}:{}", c.kind, f(c.seq), f(c.at_ms))
}

/// `seen` after a celebration was shown.
pub fn mark_seen(seen: Seen, c: Option<&Celebrate>) -> Seen {
    let mut out = seen;
    let Some(c) = c else { return out };
    match c.kind.as_str() {
        "finished" => {
            if let Some(q) = c.seq {
                out.finished_seq = out.finished_seq.max(q)
            }
        }
        "hello" => {
            if let Some(q) = c.seq {
                out.hello_seq = out.hello_seq.max(q)
            }
        }
        "test" => {
            if let Some(a) = c.at_ms {
                out.test_at_ms = out.test_at_ms.max(a)
            }
        }
        _ => {}
    }
    out
}

/// What a new pet process counts as already celebrated: everything in the first export.
/// Returns (seen, grew_seq) with grew_seq -1 when no growth waits.
pub fn seen_at_start(doc: &Value) -> (Seen, f64) {
    let mut seen = Seen::default();
    let mut grew = -1.0;
    if let Some(a) = obj(doc.get("agent")) {
        if let Some(v) = num(a.get("finished_seq")) {
            seen.finished_seq = v.max(0.0);
        }
        if let Some(v) = num(a.get("hello_seq")) {
            seen.hello_seq = v.max(0.0);
        }
        if let Some(t) = obj(a.get("test")) {
            if t.get("state").and_then(Value::as_str) == Some("done") {
                if let Some(at) = num(t.get("at_ms")) {
                    seen.test_at_ms = at.max(0.0);
                }
            }
        }
    }
    if let Some(ev) = doc.get("unseen_events").and_then(Value::as_array) {
        for e in ev {
            if let Some(q) = num(e.get("seq")) {
                if q > grew {
                    grew = q;
                }
            }
        }
    }
    (seen, grew)
}

/// The document without its unseen growth once growth was celebrated here.
pub fn without_growth(doc: &Value, grew_seq: f64) -> Value {
    let Some(ev) = doc.get("unseen_events").and_then(Value::as_array) else { return doc.clone() };
    if ev.is_empty() {
        return doc.clone();
    }
    let top = ev.iter().filter_map(|e| num(e.get("seq"))).fold(0.0, f64::max);
    if grew_seq < top {
        return doc.clone();
    }
    let mut copy = doc.clone();
    if let Some(m) = copy.as_object_mut() {
        m.insert("unseen_events".into(), Value::Array(Vec::new()));
    }
    copy
}
