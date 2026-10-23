//! Between quench-serde's `Value` and the libraries that speak serde (`minijinja`, `jsonschema`).

use std::collections::BTreeMap;

/// A quench value as a template value.
#[must_use]
pub fn to_jinja(value: &quench_serde::Value) -> minijinja::Value {
    use quench_serde::Value as V;
    match value {
        V::Null => minijinja::Value::from(()),
        V::Bool(b) => minijinja::Value::from(*b),
        V::Int(n) => minijinja::Value::from(*n),
        V::UInt(n) => minijinja::Value::from(*n),
        V::Float(f) => minijinja::Value::from(*f),
        V::String(s) => minijinja::Value::from(s.as_str()),
        V::Array(items) => items.iter().map(to_jinja).collect(),
        V::Object(map) => minijinja::Value::from(
            map.iter()
                .map(|(key, item)| (key.clone(), to_jinja(item)))
                .collect::<BTreeMap<_, _>>(),
        ),
    }
}

/// A template value as a quench value (through its JSON form).
pub fn from_jinja(value: &minijinja::Value) -> Result<quench_serde::Value, String> {
    let text = serde_json::to_string(value).map_err(|e| e.to_string())?;
    quench_serde::json::from_str(&text).map_err(|e| e.to_string())
}

/// A quench value as the `serde_json` value `jsonschema` validates against.
#[must_use]
pub fn to_json(value: &quench_serde::Value) -> serde_json::Value {
    serde_json::from_str(&value.to_string()).unwrap_or(serde_json::Value::Null)
}
