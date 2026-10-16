//! Spec §6.6 / §9.2: during the upgrade window a still-loaded v0.2 bar or overlay reads the
//! v0.3 `bar-state.json`. The v0.2.1 readers (`CodePetBar.js` and `BarState.js`, frozen
//! verbatim from `release/v0.2.1` in `tests/fixtures/v021-readers/`) and the readers in the
//! tree must parse and render real exports written by this binary, On and Off, without
//! errors. Needs `node` (as the QML suite's JS tests do).
mod common;
use common::Env;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Copies the published `bar-state.json` byte for byte to `dir/name` and returns it parsed.
fn keep(env: &Env, dir: &Path, name: &str) -> Value {
    let to = dir.join(name);
    std::fs::copy(env.bar_state_path(), &to).unwrap();
    serde_json::from_slice(&std::fs::read(&to).unwrap()).unwrap()
}

/// The inert v0.3.0 compatibility values of the v1 keys (§6.6), whatever the pet's state.
fn assert_compat_keys(doc: &Value, what: &str) {
    if cfg!(feature = "legacy-tasks") {
        return;
    }
    assert_eq!(doc["task"], Value::Null, "{what}");
    assert_eq!(doc["care"]["ready_to_work"], false, "{what}");
    assert_eq!(doc["care"]["readiness_reasons"], json!([]), "{what}");
}

fn run_readers(bar: &Path, overlay: &Path, files: &[PathBuf]) -> Vec<Value> {
    let script = manifest().join("tests/fixtures/v021-readers/check.mjs");
    let out = Command::new("node")
        .arg(&script)
        .arg(bar)
        .arg(overlay)
        .args(files)
        .output()
        .unwrap_or_else(|e| panic!("this test needs node on PATH: {e}"));
    let lines: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(
        out.status.success(),
        "{}: {lines:?} {}",
        bar.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(lines.len(), files.len());
    lines
}

#[test]
fn v021_and_current_readers_render_the_v03_export_on_and_off() {
    let dir = tempfile::tempdir().unwrap();

    // The owner's upgraded Ginger before Let's go! (unarmed, welcome routine available).
    let env = Env::with_first_run_pet();
    assert_eq!(env.state()["life"]["armed"], false);
    let unarmed = keep(&env, dir.path(), "unarmed.json");
    assert_compat_keys(&unarmed, "unarmed");

    // After Let's go!: armed, healthy, every need full. v0.2 would have said "ready to
    // work"; the v0.3.0 export keeps the v1 keys inert.
    let s = env.ok(
        "pet.start",
        json!({"species": "goldfish", "mode": "gentle", "acknowledge_rules": true}),
    );
    for meter in ["hunger", "thirst", "mental_health", "hygiene"] {
        assert_eq!(s["meters"][meter], 10_000, "{meter}");
    }
    assert_eq!(s["life"]["status"], "healthy");
    let on = keep(&env, dir.path(), "on.json");
    assert_eq!(on["enabled"], true);
    assert_eq!(on["life"]["armed"], true);
    assert_compat_keys(&on, "on");

    let off = env.run(&["off"]);
    assert_eq!(off.status.code(), Some(0), "{off:?}");
    let off = keep(&env, dir.path(), "off.json");
    assert_eq!(off["enabled"], false);
    assert_eq!(off["presence"]["wanted"], false);
    assert_compat_keys(&off, "off");

    let files: Vec<PathBuf> = ["unarmed.json", "on.json", "off.json"]
        .iter()
        .map(|n| dir.path().join(n))
        .collect();
    let root = manifest().join("../..");
    for (bar, overlay) in [
        (
            manifest().join("tests/fixtures/v021-readers/CodePetBar.js"),
            manifest().join("tests/fixtures/v021-readers/BarState.js"),
        ),
        (
            root.join("omarchy-plugin/CodePetBar.js"),
            root.join("qml/omarchy/BarState.js"),
        ),
    ] {
        for line in run_readers(&bar, &overlay, &files) {
            assert_eq!(line["ok"], true, "{}: {line}", bar.display());
            assert_eq!(line["merged_life"], "Ginger", "{}: {line}", bar.display());
            assert!(
                line["label"].as_str().unwrap().contains("Ginger"),
                "{}: {line}",
                bar.display()
            );
        }
    }
}
