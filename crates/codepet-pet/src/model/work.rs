//! Work snapshot rules used by the pet's automatic scout (qml/omarchy/WorkModel.js and the
//! scheduling half of WorkController.qml).

use super::js::js_string;
use serde_json::Value;

pub fn finished(task: Option<&Value>) -> bool {
    let Some(t) = task.filter(|t| t.is_object()) else {
        return false;
    };
    let status = js_string(t.get("status"));
    ["accepted", "cancelled", "failed", "discarded"].contains(&status.as_str())
        || (status == "scouted" && t.get("has_finding") != Some(&Value::Bool(true)))
}

pub fn finding(task: Option<&Value>) -> bool {
    task.is_some_and(|t| {
        t.get("status").and_then(Value::as_str) == Some("scouted")
            && t.get("has_finding") == Some(&Value::Bool(true))
    })
}

pub fn review(task: Option<&Value>) -> bool {
    task.and_then(|t| t.get("status"))
        .and_then(Value::as_str)
        .is_some_and(|s| s == "review" || s == "applying")
}

fn task_of(state: &Value) -> Option<&Value> {
    state.get("task").filter(|t| t.is_object())
}

/// WorkModel.scoutEligible.
pub fn scout_eligible(state: &Value) -> bool {
    let scout = state.get("scout");
    scout.and_then(|s| s.get("enabled")) == Some(&Value::Bool(true))
        && scout
            .and_then(|s| s.get("next_due_ms"))
            .and_then(Value::as_f64)
            .is_some_and(f64::is_finite)
        && state.get("readiness").and_then(|r| r.get("ready")) == Some(&Value::Bool(true))
        && state.get("provider").and_then(|p| p.get("available")) == Some(&Value::Bool(true))
        && (task_of(state).is_none() || finished(task_of(state)))
}

pub fn next_due_ms(state: &Value) -> Option<f64> {
    state
        .get("scout")
        .and_then(|s| s.get("next_due_ms"))
        .and_then(Value::as_f64)
}

/// WorkController.scoutKey: the snapshot facts a scout attempt is tied to.
pub fn scout_key(state: &Value) -> String {
    let scout = state
        .get("scout")
        .map(|s| serde_json::json!([s.get("enabled"), s.get("next_due_ms")]));
    let task = task_of(state)
        .filter(|t| !finished(Some(t)))
        .map(|t| serde_json::json!([t.get("id"), t.get("status")]));
    serde_json::json!([scout, state.get("readiness"), task]).to_string()
}

/// The pet shows a "!" badge for a finding or a result waiting for review.
pub fn badge(state: Option<&Value>) -> bool {
    let task = state.and_then(task_of);
    review(task) || finding(task)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn eligibility() {
        let mut s = json!({"scout": {"enabled": true, "next_due_ms": 5}, "readiness": {"ready": true},
                           "provider": {"available": true}, "task": null});
        assert!(scout_eligible(&s));
        s["task"] = json!({"id": "t", "status": "running"});
        assert!(!scout_eligible(&s));
        s["task"] = json!({"id": "t", "status": "scouted", "has_finding": false});
        assert!(scout_eligible(&s));
        assert!(!badge(Some(&s)));
        s["task"]["has_finding"] = json!(true);
        assert!(badge(Some(&s)));
        s["scout"]["enabled"] = json!(false);
        assert!(!scout_eligible(&s));
    }
}
