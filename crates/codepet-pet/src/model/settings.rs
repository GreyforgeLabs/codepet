//! The bar entry's settings in `~/.config/omarchy/shell.json` (qml/omarchy/OverlayFlow.js):
//! "Pet on my desktop" (`overlayOnLoad`), Calm mode (`calm`) and the saved placement.
//! Read-only here; placement is written through the Omarchy shell's own mutator.

use serde_json::{Map, Value};

pub const PLUGIN_ID: &str = "greyforge.codepet";
const MAX_SETTINGS_BYTES: usize = 1048576;
pub const SETTLE_MS: u64 = 1500;
pub const DEFAULT_OMARCHY_PATH: &str = "/usr/share/omarchy";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Placement {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub pinned: bool,
    pub screen: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Settings {
    /// None: no usable shell.json (unknown, keep running).
    pub desktop_pet: Option<bool>,
    /// None: no usable file or no CodePet cell.
    pub calm: Option<bool>,
    /// None: no usable file.
    pub placement: Option<Placement>,
}

fn parse(text: &[u8]) -> Option<Map<String, Value>> {
    if text.is_empty() || text.len() > MAX_SETTINGS_BYTES {
        return None;
    }
    let v: Value = serde_json::from_slice(text).ok()?;
    let m = v.as_object()?;
    if m.get("version").and_then(Value::as_f64) != Some(1.0) {
        return None;
    }
    Some(m.clone())
}

fn entry_of(doc: &Map<String, Value>) -> Option<&Map<String, Value>> {
    let bar = doc.get("bar")?.as_object()?;
    for holder in [bar.get("layout").and_then(Value::as_object), Some(bar)].into_iter().flatten() {
        for section in ["left", "center", "right"] {
            if let Some(list) = holder.get(section).and_then(Value::as_array) {
                for e in list {
                    if let Some(e) = e.as_object() {
                        if e.get("id").and_then(Value::as_str) == Some(PLUGIN_ID) {
                            return Some(e);
                        }
                    }
                }
            }
        }
    }
    None
}

fn truthy_string(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(true)) => true,
        Some(Value::String(s)) => s == "true",
        _ => false,
    }
}

pub fn read(text: &[u8]) -> Settings {
    let Some(doc) = parse(text) else { return Settings::default() };
    let entry = entry_of(&doc);
    let desktop_pet = Some(entry.is_some_and(|e| truthy_string(e.get("overlayOnLoad"))));
    let calm = entry.map(|e| truthy_string(e.get("calm")));
    let p = entry.and_then(|e| e.get("desktopPlacement")).and_then(Value::as_object);
    let coord = |k: &str| p.and_then(|p| p.get(k)).and_then(Value::as_f64).filter(|v| v.is_finite() && (0.0..=1.0).contains(v));
    let (x, y) = (coord("x"), coord("y"));
    let valid = x.is_some() && y.is_some();
    let placement = Placement {
        x: if valid { x } else { None },
        y: if valid { y } else { None },
        pinned: p.and_then(|p| p.get("pinned")) == Some(&Value::Bool(true)),
        screen: p
            .and_then(|p| p.get("screen"))
            .and_then(Value::as_str)
            .filter(|s| s.len() <= 256)
            .unwrap_or("")
            .to_string(),
    };
    Settings { desktop_pet, calm, placement: Some(placement) }
}

/// One explicit placement write through the Omarchy shell's config mutator.
pub fn placement_argv(omarchy_path: &str, x: f64, y: f64, pinned: bool, screen: &str) -> Vec<String> {
    let base = absolute(omarchy_path).unwrap_or_else(|| DEFAULT_OMARCHY_PATH.to_string());
    let pos = serde_json::json!({"x": x, "y": y, "pinned": pinned, "screen": screen});
    vec![
        "qs".into(),
        "ipc".into(),
        "-p".into(),
        format!("{base}/shell"),
        "call".into(),
        "--".into(),
        "shell".into(),
        "setBarWidget".into(),
        PLUGIN_ID.into(),
        "desktopPlacement".into(),
        pos.to_string(),
        "{}".into(),
    ]
}

fn absolute(v: &str) -> Option<String> {
    (v.len() > 1 && v.starts_with('/') && !v.contains('\n') && !v.contains('\0') && !v.split('/').any(|p| p == ".."))
        .then(|| v.trim_end_matches('/').to_string())
}

fn shell_argv(omarchy_path: &str, call: &[&str]) -> Vec<String> {
    let mut argv: Vec<String> = match absolute(omarchy_path) {
        Some(base) => vec![format!("{base}/bin/omarchy-shell")],
        None => vec!["/usr/bin/env".into(), format!("OMARCHY_PATH={DEFAULT_OMARCHY_PATH}"), "omarchy-shell".into()],
    };
    argv.extend(call.iter().map(|s| s.to_string()));
    argv
}

/// Opens the bar popup at Home.
pub fn open_home_argv(omarchy_path: &str) -> Vec<String> {
    shell_argv(omarchy_path, &["-q", "shell", "summon", PLUGIN_ID])
}

pub fn open_updates_argv(omarchy_path: &str) -> Vec<String> {
    shell_argv(omarchy_path, &["-q", PLUGIN_ID, "openUpdates"])
}

/// The suggested show/hide hotkey (README); bind it to `codepet-overlay toggle`.
pub const TOGGLE_KEY_HINT: &str = "Super + Alt + P";

pub fn hidden_toast_argv() -> Vec<String> {
    vec![
        "notify-send".into(),
        "--app-name=CodePet".into(),
        "--expire-time=5000".into(),
        "Your pet is hiding".into(),
        format!("Press {TOGGLE_KEY_HINT} to bring it back."),
    ]
}

/// Calm: an explicit override wins, then the bar setting, then CODEPET_REDUCED_MOTION=1.
pub fn reduced_motion(over: Option<bool>, setting: Option<bool>, env: Option<&str>) -> bool {
    over.or(setting).unwrap_or(env == Some("1"))
}

/// Why the pet must exit now: "off", "desktop_pet_off" or "".
pub fn exit_reason(bar: Option<&Value>, desktop_pet: Option<bool>) -> &'static str {
    if bar.and_then(|b| b.get("enabled")) == Some(&Value::Bool(false)) {
        return "off";
    }
    if desktop_pet == Some(false) {
        return "desktop_pet_off";
    }
    ""
}

pub fn exits_at_once(reason: &str, saw_on: bool) -> bool {
    reason == "off" || (reason == "desktop_pet_off" && saw_on)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell(entry: &str) -> Vec<u8> {
        format!(r#"{{"version":1,"bar":{{"layout":{{"right":[{entry}]}}}}}}"#).into_bytes()
    }

    #[test]
    fn reads_the_codepet_entry() {
        let s = read(&shell(r#"{"id":"greyforge.codepet","overlayOnLoad":true,"calm":"true","desktopPlacement":{"x":0.5,"y":0.25,"pinned":true,"screen":"DP-2"}}"#));
        assert_eq!(s.desktop_pet, Some(true));
        assert_eq!(s.calm, Some(true));
        assert_eq!(s.placement, Some(Placement { x: Some(0.5), y: Some(0.25), pinned: true, screen: "DP-2".into() }));
    }

    #[test]
    fn missing_cell_means_off_and_bad_file_means_unknown() {
        assert_eq!(read(&shell(r#"{"id":"omarchy.clock"}"#)).desktop_pet, Some(false));
        assert_eq!(read(b"not json").desktop_pet, None);
        assert_eq!(read(br#"{"version":2}"#).desktop_pet, None);
        let s = read(&shell(r#"{"id":"greyforge.codepet","desktopPlacement":{"x":2,"y":0.1}}"#));
        assert_eq!(s.placement.unwrap().x, None, "out-of-range placement falls back to the floor");
    }

    #[test]
    fn exit_rules() {
        let off = serde_json::json!({"enabled": false});
        assert_eq!(exit_reason(Some(&off), Some(true)), "off");
        assert_eq!(exit_reason(None, Some(false)), "desktop_pet_off");
        assert_eq!(exit_reason(None, None), "");
        assert!(exits_at_once("off", false));
        assert!(!exits_at_once("desktop_pet_off", false));
        assert!(exits_at_once("desktop_pet_off", true));
        assert!(reduced_motion(None, None, Some("1")));
        assert!(!reduced_motion(Some(false), Some(true), Some("1")));
    }

    #[test]
    fn argv_never_uses_a_shell() {
        assert_eq!(open_home_argv("/usr/share/omarchy")[0], "/usr/share/omarchy/bin/omarchy-shell");
        assert_eq!(open_home_argv("")[0], "/usr/bin/env");
        assert_eq!(open_home_argv("relative/x")[1], "OMARCHY_PATH=/usr/share/omarchy");
        let p = placement_argv("", 0.5, 0.5, false, "DP-2");
        assert_eq!(p[3], "/usr/share/omarchy/shell");
        assert!(p[10].contains("\"pinned\":false"));
        assert_eq!(hidden_toast_argv().last().unwrap(), "Press Super + Alt + P to bring it back.");
    }
}
