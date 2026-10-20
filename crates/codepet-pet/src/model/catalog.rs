//! One species' manifest (qml/shared/PetCatalog.js) and the presentation state machine
//! (qml/shared/PetStateMachine.js). The native pet loads only the current species.

use serde_json::Value;
use std::path::Path;

pub const TICK_MS: u64 = 200;

#[derive(Clone, Debug, PartialEq)]
pub struct StateMeta {
    pub name: String,
    pub frames: Vec<String>,
    pub frame_ms: f64,
    pub looping: bool,
    pub reduced_frame: f64,
    pub priority: Option<f64>,
    pub interrupt: String,
    pub mirror: bool,
    pub repeats: f64,
}

const STATE_NAMES: [&str; 15] = [
    "idle", "locomotion", "food", "water", "play", "wash", "bathroom", "sleep_enter", "sleep", "sleep_exit",
    "celebrate", "working", "waiting", "needs_care", "error",
];

fn default_meta(name: &str) -> StateMeta {
    let (frames, ms, looping, reduced, prio, intr): (&[&str], f64, bool, f64, f64, &str) = match name {
        "locomotion" => (&["b08", "b09", "b10", "b11"], 200.0, true, 0.0, 20.0, "any"),
        "food" => (&["b03", "c00", "c01", "c00", "c01", "b04"], 400.0, false, 1.0, 70.0, "higher"),
        "water" => (&["b03", "c02", "c03", "c02", "c03", "b04"], 400.0, false, 1.0, 70.0, "higher"),
        "play" => (&["b03", "c04", "c05", "c04", "c05", "b04"], 400.0, false, 1.0, 70.0, "higher"),
        "wash" => (&["b00", "c06", "c07", "c06", "c07", "b04"], 400.0, false, 1.0, 70.0, "higher"),
        "bathroom" => (&["b03", "c08", "c08", "b00"], 600.0, false, 1.0, 70.0, "higher"),
        "sleep_enter" => (&["c09", "b07"], 600.0, false, 1.0, 90.0, "none"),
        "sleep" => (&["b07"], 1000.0, true, 0.0, 85.0, "higher"),
        "sleep_exit" => (&["c10", "b00"], 600.0, false, 1.0, 90.0, "none"),
        "celebrate" => (&["c11", "b04", "c11", "b04"], 400.0, false, 0.0, 60.0, "higher"),
        "working" => (&["b04", "b00"], 800.0, true, 0.0, 40.0, "any"),
        "waiting" => (&["b02", "b00"], 800.0, true, 0.0, 35.0, "any"),
        "needs_care" => (&["b05"], 800.0, true, 0.0, 50.0, "any"),
        "error" => (&["b06"], 1000.0, true, 0.0, 100.0, "higher"),
        _ => (&["b00", "b00", "b01", "b00"], 600.0, true, 0.0, 0.0, "any"),
    };
    StateMeta {
        name: name.into(),
        frames: frames.iter().map(|f| f.to_string()).collect(),
        frame_ms: ms,
        looping,
        reduced_frame: reduced,
        priority: Some(prio),
        interrupt: intr.into(),
        mirror: true,
        repeats: 1.0,
    }
}

pub fn normalize_state(state: &str) -> &str {
    if state == "walk" {
        return "locomotion";
    }
    STATE_NAMES.iter().find(|s| **s == state).copied().unwrap_or("idle")
}

#[derive(Clone, Debug, Default)]
pub struct Species {
    pub id: String,
    pub manifest: Value,
}

impl Species {
    /// Reads `<pets>/<id>/manifest.json`; None for an unknown or unreadable species.
    pub fn load(pets_dir: &Path, id: &str) -> Option<Species> {
        let ok = !id.is_empty()
            && id.len() <= 32
            && id.as_bytes()[0].is_ascii_lowercase()
            && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        if !ok {
            return None;
        }
        let text = std::fs::read(pets_dir.join(id).join("manifest.json")).ok()?;
        if text.len() > 1 << 20 {
            return None;
        }
        let manifest: Value = serde_json::from_slice(&text).ok()?;
        manifest.is_object().then(|| Species { id: id.into(), manifest })
    }

    pub fn aquatic(&self) -> bool {
        self.manifest.get("habitat").and_then(Value::as_str) == Some("aquatic")
    }

    /// The care icon set: `careIconSet`, else from the habitat.
    pub fn care_set(&self) -> &str {
        match self.manifest.get("careIconSet").and_then(Value::as_str) {
            Some(s @ ("floor" | "aquatic" | "robot")) => s,
            _ if self.aquatic() => "aquatic",
            _ => "floor",
        }
    }

    pub fn state_meta(&self, state: &str) -> StateMeta {
        let name = normalize_state(state);
        let mut meta = default_meta(name);
        let states = self.manifest.get("states");
        let own = states.and_then(|s| s.get(name)).filter(|v| v.is_object());
        if own.is_none() {
            if let Some(idle) = states.and_then(|s| s.get("idle")).and_then(|i| i.get("frames")).and_then(Value::as_array) {
                meta.frames = idle.iter().map(frame_name).collect();
            }
        }
        let mut mirror_explicit_false = false;
        if let Some(own) = own.and_then(Value::as_object) {
            for (k, v) in own {
                if v.is_null() {
                    continue;
                }
                match k.as_str() {
                    "frames" => {
                        if let Some(a) = v.as_array() {
                            meta.frames = a.iter().map(frame_name).collect()
                        }
                    }
                    "frameMs" => meta.frame_ms = v.as_f64().unwrap_or(f64::NAN),
                    "loop" => meta.looping = truthy(v),
                    "reducedFrame" => meta.reduced_frame = v.as_f64().unwrap_or(f64::NAN),
                    "priority" => meta.priority = v.as_f64(),
                    "interrupt" => meta.interrupt = v.as_str().unwrap_or("").to_string(),
                    "mirror" => mirror_explicit_false = v == &Value::Bool(false),
                    "repeats" => meta.repeats = v.as_f64().unwrap_or(f64::NAN),
                    _ => {}
                }
            }
        }
        if meta.frames.is_empty() {
            meta.frames = default_meta(name).frames;
        }
        meta.mirror = !self.aquatic() && !mirror_explicit_false;
        meta
    }

    pub fn stage_count(&self) -> u32 {
        self.manifest.get("stages").and_then(Value::as_array).map(|a| a.len() as u32).filter(|n| *n > 0).unwrap_or(3)
    }

    pub fn clamp_stage(&self, stage: f64) -> u32 {
        if !stage.is_finite() || stage < 1.0 {
            return 1;
        }
        (stage.floor() as u32).min(self.stage_count())
    }

    /// Relative path (under the species dir) of a frame entry at a stage.
    pub fn frame_path(&self, entry: &str, stage: u32) -> String {
        if entry.contains('/') {
            return entry.replace("{stage}", &stage.to_string());
        }
        let has_ext = entry.rsplit_once('.').is_some_and(|(_, e)| !e.is_empty() && e.bytes().all(|b| b.is_ascii_alphanumeric()));
        format!("s{stage}/{}", if has_ext { entry.to_string() } else { format!("{entry}.webp") })
    }

    /// The frame to show: (relative path, may mirror).
    pub fn resolve(&self, stage: f64, state: &str, frame: i64, reduced: bool) -> (String, bool) {
        let meta = self.state_meta(state);
        let n = meta.frames.len() as i64;
        let r = meta.reduced_frame;
        let f = if reduced {
            if r.is_finite() && r >= 0.0 && (r as i64) < n { r.floor() as i64 } else { 0 }
        } else {
            frame.rem_euclid(n)
        };
        (self.frame_path(&meta.frames[f as usize], self.clamp_stage(stage)), meta.mirror)
    }

    fn geometry(&self) -> Option<&Value> {
        self.manifest.get("geometry")
    }

    pub fn roams(&self) -> bool {
        !self.aquatic() && self.geometry().and_then(|g| g.get("roams")) != Some(&Value::Bool(false))
    }

    /// Clickable rect (x, y, w, h) in display pixels for a pet drawn at `size`.
    pub fn hit_rect(&self, size: f64, facing_left: bool, stage: f64) -> [f64; 4] {
        let px = size.max(0.0);
        let g = self.geometry();
        let vb = g
            .and_then(|g| box4(g.get("viewBox")))
            .filter(|b| b[2] > 0.0 && b[3] > 0.0)
            .unwrap_or([0.0, 0.0, 192.0, 192.0]);
        let water = if self.aquatic() { g.and_then(|g| g.get("water")).and_then(|w| valid_box(w.get("outer"))) } else { None };
        let own = g
            .and_then(|g| g.get("stageHitBoxes"))
            .and_then(|s| s.get(self.clamp_stage(stage).to_string()))
            .and_then(|b| valid_box(Some(b)));
        let hit = g.and_then(|g| valid_box(g.get("hitBox"))).unwrap_or(vb);
        let b = own.or(water).unwrap_or(hit);
        let (sx, sy) = (px / vb[2], px / vb[3]);
        let mut x = b[0] - vb[0];
        if facing_left && !self.aquatic() {
            x = vb[2] - (x + b[2]);
        }
        let left = (x * sx).clamp(0.0, px);
        let top = ((b[1] - vb[1]) * sy).clamp(0.0, px);
        [left, top, (b[2] * sx).min(px - left).max(0.0), (b[3] * sy).min(px - top).max(0.0)]
    }

    /// (px per 200 ms tick, pause range in ticks, airborne frames).
    pub fn locomotion(&self) -> (f64, (u32, u32), Vec<i64>) {
        let l = self.manifest.get("locomotion");
        let pause = l
            .and_then(|l| l.get("pauseTicks"))
            .and_then(Value::as_array)
            .filter(|a| a.len() == 2)
            .and_then(|a| Some((a[0].as_f64()?, a[1].as_f64()?)))
            .filter(|(a, b)| *a >= 1.0 && b >= a)
            .map(|(a, b)| (a as u32, b as u32))
            .unwrap_or((40, 110));
        let frames = |v: Option<&Value>| -> Vec<i64> {
            v.and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_f64).filter(|n| *n >= 0.0 && n.fract() == 0.0).map(|n| n as i64).collect())
                .unwrap_or_default()
        };
        let mut air = frames(self.manifest.get("states").and_then(|s| s.get("locomotion")).and_then(|s| s.get("airborneFrames")));
        if air.is_empty() {
            air = frames(l.and_then(|l| l.get("airborneFrames")));
        }
        if air.is_empty() {
            air = frames(l.and_then(|l| l.get("pxFrames")));
        }
        let px = l.and_then(|l| l.get("pxPerTick")).and_then(Value::as_f64);
        let px = if self.aquatic() { 0.0 } else { px.filter(|p| p.is_finite() && *p >= 0.0).map(|p| p.min(16.0)).unwrap_or(5.0) };
        (px, pause, air)
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Null => false,
        _ => true,
    }
}

fn frame_name(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => super::js::js_string(Some(other)),
    }
}

fn box4(v: Option<&Value>) -> Option<[f64; 4]> {
    let a = v?.as_array()?;
    if a.len() != 4 {
        return None;
    }
    let mut out = [0.0; 4];
    for (i, x) in a.iter().enumerate() {
        out[i] = x.as_f64().filter(|f| f.is_finite())?;
    }
    Some(out)
}

fn valid_box(v: Option<&Value>) -> Option<[f64; 4]> {
    box4(v).filter(|b| b[2] > 0.0 && b[3] > 0.0)
}

// ---- PetStateMachine.js ----

pub fn priority_of(meta: &StateMeta) -> f64 {
    if let Some(p) = meta.priority {
        return p;
    }
    match meta.name.as_str() {
        "locomotion" => 20.0,
        "food" | "water" | "play" | "wash" | "bathroom" => 70.0,
        "sleep_enter" | "sleep_exit" => 90.0,
        "sleep" => 85.0,
        "celebrate" => 60.0,
        "needs_care" => 50.0,
        "working" => 40.0,
        "waiting" => 35.0,
        "error" => 100.0,
        _ => 0.0,
    }
}

pub fn interrupt_of(meta: &StateMeta) -> &str {
    match meta.interrupt.as_str() {
        s @ ("any" | "higher" | "none") => s,
        _ => match meta.name.as_str() {
            "food" | "water" | "play" | "wash" | "bathroom" | "sleep" | "celebrate" | "error" => "higher",
            "sleep_enter" | "sleep_exit" => "none",
            _ => "any",
        },
    }
}

pub fn can_interrupt(current: Option<&StateMeta>, next: &StateMeta) -> bool {
    let Some(cur) = current else { return true };
    match interrupt_of(cur) {
        "any" => true,
        "none" => false,
        _ => priority_of(next) > priority_of(cur),
    }
}

/// "transient" or "base": which of a running transient and the base state shows.
pub fn display_transient(base: &StateMeta, transient: Option<&StateMeta>) -> bool {
    match transient {
        None => false,
        Some(t) => !can_interrupt(Some(t), base),
    }
}

/// A new transient while another runs: replace (true) or queue (false).
pub fn admit_replace(current: Option<&StateMeta>, next: &StateMeta) -> bool {
    can_interrupt(current, next)
}

pub fn frame_count(meta: &StateMeta) -> i64 {
    (meta.frames.len() as i64).max(1)
}

pub fn frame_ms_of(meta: &StateMeta) -> u64 {
    let ms = meta.frame_ms;
    if !ms.is_finite() || ms < TICK_MS as f64 {
        return TICK_MS;
    }
    ((ms / TICK_MS as f64).round() as u64) * TICK_MS
}

pub fn cycle_ms(meta: &StateMeta) -> u64 {
    frame_count(meta) as u64 * frame_ms_of(meta)
}

/// Finite duration of a one-shot state; None for a loop.
pub fn duration_ms(meta: &StateMeta) -> Option<u64> {
    if meta.looping {
        return None;
    }
    let r = if meta.repeats.is_finite() && meta.repeats >= 1.0 { meta.repeats.floor() as u64 } else { 1 };
    Some(cycle_ms(meta) * r)
}

pub fn reduced_frame_of(meta: &StateMeta) -> i64 {
    let n = frame_count(meta);
    let r = meta.reduced_frame;
    if !r.is_finite() || r < 0.0 || r as i64 >= n {
        0
    } else {
        r.floor() as i64
    }
}

pub fn frame_at(meta: &StateMeta, elapsed_ticks: u64, reduced: bool) -> i64 {
    if reduced {
        return reduced_frame_of(meta);
    }
    let n = frame_count(meta);
    let index = (elapsed_ticks * TICK_MS / frame_ms_of(meta)) as i64;
    if meta.looping {
        return index % n;
    }
    if meta.repeats > 1.0 {
        let total = n * meta.repeats.floor() as i64;
        return if index >= total { n - 1 } else { index % n };
    }
    index.min(n - 1)
}

/// Next scheduler interval: a positive multiple of 200 ms (at most 5 updates/s).
pub fn snap_interval(ms: u64) -> u64 {
    ms.max(TICK_MS).div_ceil(TICK_MS) * TICK_MS
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cat() -> Species {
        Species {
            id: "cat".into(),
            manifest: json!({
                "habitat": "floor", "careIconSet": "floor",
                "stages": [{"stage":1},{"stage":2},{"stage":3}],
                "geometry": {"viewBox": [0,0,192,192], "hitBox": [9,12,177,172],
                             "stageHitBoxes": {"1": [26,45,146,139]}, "roams": true},
                "locomotion": {"pxPerTick": 4, "pauseTicks": [40,110]},
                "states": {"idle": {"frames": ["b00.webp","b00.webp","b01.webp","b00.webp"], "frameMs": 600, "loop": true,
                                    "reducedFrame": 0, "mirror": true, "priority": 0, "interrupt": "any", "fallback": null}}
            }),
        }
    }

    #[test]
    fn missing_states_fall_back_to_idle_pictures_with_their_own_timing() {
        let m = cat().state_meta("food");
        assert_eq!(m.frames, vec!["b00.webp", "b00.webp", "b01.webp", "b00.webp"]);
        assert_eq!(m.frame_ms, 400.0);
        assert!(!m.looping);
        assert_eq!(cat().state_meta("walk").name, "locomotion");
        assert_eq!(cat().state_meta("bogus").name, "idle");
    }

    #[test]
    fn frame_paths_and_hit_boxes() {
        let c = cat();
        assert_eq!(c.resolve(2.0, "idle", 2, false), ("s2/b01.webp".to_string(), true));
        assert_eq!(c.resolve(9.0, "idle", 6, false).0, "s3/b01.webp");
        let s = 112.0 / 192.0;
        let r = c.hit_rect(112.0, false, 3.0);
        assert!((r[0] - 9.0 * s).abs() < 1e-9 && (r[2] - 177.0 * s).abs() < 1e-9);
        let r1 = c.hit_rect(112.0, false, 1.0);
        assert!((r1[1] - 45.0 * s).abs() < 1e-9, "stage box");
        let m = c.hit_rect(112.0, true, 3.0);
        assert!((m[0] - (192.0 - 186.0) * s).abs() < 1e-9, "mirrored about the centre");
    }

    #[test]
    fn state_machine_rules() {
        let c = cat();
        let food = c.state_meta("food");
        let idle = c.state_meta("idle");
        assert!(display_transient(&idle, Some(&food)));
        assert!(!admit_replace(Some(&c.state_meta("sleep_enter")), &c.state_meta("error")));
        assert_eq!(duration_ms(&food), Some(1600), "idle pictures, food timing");
        assert_eq!(duration_ms(&idle), None);
        assert_eq!(frame_at(&food, 100, false), 3, "one-shot holds its last frame");
        assert_eq!(frame_at(&idle, 2, false), 0);
        assert_eq!(frame_at(&idle, 6, false), 2);
        assert_eq!(frame_at(&food, 0, true), 1);
        assert_eq!(snap_interval(1), 200);
        assert_eq!(snap_interval(601), 800);
    }
}
