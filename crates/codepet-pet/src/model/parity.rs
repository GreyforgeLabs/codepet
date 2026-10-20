//! Differential test: the Rust face rules against the JavaScript reference they replace
//! (crates/codepet-pet/tests/parity.mjs runs qml/shared/petstate.js and OverlayFace.js
//! under Node over a corpus built from the v0.3 bar fixtures). Skipped without Node.

use super::face::{self, Celebrate, Seen};
use serde_json::{Map, Value};
use std::path::Path;
use std::process::Command;

/// Numbers compare by value (JS has one number type; serde keeps ints and floats apart).
fn canon(v: &Value) -> Value {
    match v {
        Value::Number(n) => serde_json::json!(n.as_f64().unwrap_or(f64::NAN)),
        Value::Array(a) => Value::Array(a.iter().map(canon).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), canon(v))).collect::<Map<_, _>>()),
        other => other.clone(),
    }
}

fn seen_of(v: &Value) -> Seen {
    let f = |k: &str| v.get(k).and_then(Value::as_f64).unwrap_or(0.0);
    Seen { finished_seq: f("finished_seq"), hello_seq: f("hello_seq"), test_at_ms: f("test_at_ms") }
}

fn seen_value(s: Seen) -> Value {
    serde_json::json!({"finished_seq": s.finished_seq, "hello_seq": s.hello_seq, "test_at_ms": s.test_at_ms})
}

#[test]
fn rust_face_rules_match_the_javascript_reference() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script = root.join("crates/codepet-pet/tests/parity.mjs");
    let Ok(out) = Command::new("node").arg(&script).output() else {
        eprintln!("node not found; parity test skipped");
        return;
    };
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let oracle: Value = serde_json::from_slice(&out.stdout).expect("oracle json");
    let works = oracle["works"].as_array().unwrap().clone();
    let dir = root.join("docs/v0.3/contracts/fixtures/bar-v2");
    let mut checked = 0;
    for case in oracle["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let variant = case["variant"].as_u64().unwrap();
        let mut doc: Value = serde_json::from_slice(&std::fs::read(dir.join(name)).unwrap()).unwrap();
        if variant > 0 {
            let hunger = [500, 2500, 4500, 7000][variant as usize - 1];
            if let Some(m) = doc.get_mut("meters").and_then(Value::as_object_mut) {
                m.insert("hunger".into(), serde_json::json!(hunger));
            }
        }
        let now = case["now"].as_f64().unwrap();
        let seen = seen_of(&case["seen"]);
        let ctx = format!("{name} v{variant} now={now} seen={}", case["seen"]);
        let shared = face::shared_priority(&doc, now, seen);
        assert_eq!(canon(&shared.to_value()), canon(&case["shared"]), "sharedPriority {ctx}");
        let current = face::current_priority(&doc, now, seen);
        assert_eq!(canon(&current.to_value()), canon(&case["current"]), "currentPriority {ctx}");
        let (start, grew) = face::seen_at_start(&doc);
        assert_eq!(canon(&seen_value(start)), canon(&case["start"]["seen"]), "seenAtStart {ctx}");
        assert_eq!(grew, case["start"]["grewSeq"].as_f64().unwrap(), "grewSeq {ctx}");
        let wg = face::without_growth(&doc, grew);
        let len = wg.get("unseen_events").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
        assert_eq!(len as u64, case["without_growth_len"].as_u64().unwrap(), "withoutGrowth {ctx}");
        for (i, w) in works.iter().enumerate() {
            let f = face::with_work(&current, if w.is_null() { None } else { Some(w) });
            assert_eq!(canon(&f.to_value()), canon(&case["faces"][i]), "withWork[{i}] {ctx}");
            for (j, set) in ["floor", "aquatic", "robot"].iter().enumerate() {
                let icons = face::bubble_icons(f.bubble.as_ref(), set, f.state() == "waiting");
                assert_eq!(serde_json::json!(icons), case["icons"][i][j], "bubbleIcons[{i}][{set}] {ctx}");
            }
            assert_eq!(face::observer_label(&f), case["labels"][i].as_str().unwrap(), "observerLabel[{i}] {ctx}");
        }
        let c: Option<&Celebrate> = current.celebrate.as_ref();
        assert_eq!(face::celebration_key(c), case["key"].as_str().unwrap(), "celebrationKey {ctx}");
        assert_eq!(canon(&seen_value(face::mark_seen(seen, c))), canon(&case["marked"]), "markSeen {ctx}");
        checked += 1;
    }
    assert!(checked > 1000, "corpus too small: {checked}");
}
