//! Small helpers that reproduce the JavaScript value semantics the QML reference code
//! relies on (`isObject`, `num`, `String(x)`), so the Rust port of the overlay rules
//! gives the same answers on the same JSON documents.

use serde_json::{Map, Value};

/// `isObject(v)`: a JSON object (not null, not an array).
pub fn obj(v: Option<&Value>) -> Option<&Map<String, Value>> {
    v.and_then(Value::as_object)
}

/// `num(v)`: a finite number, else None.
pub fn num(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|n| n.is_finite())
}

/// `v === true`.
pub fn is_true(v: Option<&Value>) -> bool {
    matches!(v, Some(Value::Bool(true)))
}

/// `String(v)` for a value that may be missing (`undefined`).
pub fn js_string(v: Option<&Value>) -> String {
    match v {
        None => "undefined".into(),
        Some(Value::Null) => "null".into(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => number_string(n.as_f64().unwrap_or(0.0)),
        Some(Value::Array(a)) => a
            .iter()
            .map(|x| match x {
                Value::Null => String::new(),
                other => js_string(Some(other)),
            })
            .collect::<Vec<_>>()
            .join(","),
        Some(Value::Object(_)) => "[object Object]".into(),
    }
}

/// JavaScript's Number#toString for the values that appear in exports (integers and
/// simple decimals).
pub fn number_string(n: f64) -> String {
    if n.is_nan() {
        return "NaN".into();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if n == n.trunc() && n.abs() < 1e21 {
        return format!("{}", n as i64);
    }
    format!("{n}")
}

/// `v === undefined ? null : v`: a missing key becomes null, anything else is kept.
pub fn or_null(v: Option<&Value>) -> Value {
    v.cloned().unwrap_or(Value::Null)
}

pub fn s(v: &str) -> Value {
    Value::String(v.to_string())
}
