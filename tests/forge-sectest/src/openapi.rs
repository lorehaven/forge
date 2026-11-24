//! Reads an `OpenAPI` 3 document into the operations a fuzzer can drive, and builds valid requests from their schemas.
//!
//! The services publish their API as `docs/services/<name>/openapi.yaml`. A schema says what the service *expects*,
//! which is exactly what a fuzzer needs to know to stay one step off it: valid enough to get past the first
//! validation, wrong in one place.

use quench_serde::{Map, Value};

/// How deep `$ref`s are followed: a schema that refers to itself ends here instead of looping.
const REF_DEPTH: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    /// `path` or `query`; header and cookie parameters are not fuzzed.
    pub location: String,
    pub required: bool,
    pub schema: Value,
    /// What the document gives as an example value, on the parameter or on its schema.
    pub example: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Operation {
    pub method: String,
    /// The path template as the document gives it, relative to the service's `BASE_PATH`.
    pub path: String,
    pub id: String,
    pub params: Vec<Param>,
    /// The JSON request body's schema, `$ref`s resolved.
    pub body: Option<Value>,
    /// The document says a request body is required (of any content type): an empty request is not a valid one.
    pub body_required: bool,
    pub example: Option<Value>,
}

impl Operation {
    #[must_use]
    pub fn is_mutating(&self) -> bool {
        matches!(self.method.as_str(), "POST" | "PUT" | "PATCH" | "DELETE")
    }

    #[must_use]
    pub fn label(&self) -> String {
        format!("{} {}", self.method, self.path)
    }
}

/// # Errors
///
/// When the text is not YAML, or is not an `OpenAPI` document with `paths`.
pub fn parse(text: &str) -> Result<Vec<Operation>, String> {
    let root: Value = quench_serde::yaml::from_str(text).map_err(|e| format!("openapi: {e}"))?;
    let paths = root
        .get("paths")
        .and_then(Value::as_object)
        .ok_or_else(|| "openapi: the document has no `paths`".to_string())?;
    let mut operations = Vec::new();
    for (path, item) in paths.iter() {
        let shared = parameters(&root, item.get("parameters"));
        for method in ["get", "post", "put", "patch", "delete"] {
            let Some(op) = item.get(method) else { continue };
            let mut params = shared.clone();
            for own in parameters(&root, op.get("parameters")) {
                params.retain(|p| !(p.name == own.name && p.location == own.location));
                params.push(own);
            }
            let content = op
                .get("requestBody")
                .map(|b| resolve(&root, b, 0))
                .and_then(|b| b.get("content").cloned())
                .and_then(|c| c.get("application/json").cloned());
            operations.push(Operation {
                method: method.to_ascii_uppercase(),
                path: path.clone(),
                id: op
                    .get("operationId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                params,
                body_required: op
                    .get("requestBody")
                    .map(|b| resolve(&root, b, 0))
                    .and_then(|b| b.get("required").and_then(Value::as_bool))
                    .unwrap_or(false),
                body: content
                    .as_ref()
                    .and_then(|c| c.get("schema"))
                    .map(|s| resolve(&root, s, 0)),
                example: content.as_ref().and_then(|c| c.get("example")).cloned(),
            });
        }
    }
    Ok(operations)
}

fn parameters(root: &Value, list: Option<&Value>) -> Vec<Param> {
    list.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|p| resolve(root, p, 0))
        .filter_map(|p| {
            let location = p.get("in")?.as_str()?.to_string();
            matches!(location.as_str(), "path" | "query").then_some(())?;
            Some(Param {
                name: p.get("name")?.as_str()?.to_string(),
                required: p.get("required").and_then(Value::as_bool).unwrap_or(false),
                example: p
                    .get("example")
                    .or_else(|| p.get("schema").and_then(|s| s.get("example")))
                    .cloned(),
                schema: p.get("schema").cloned().unwrap_or_default(),
                location,
            })
        })
        .collect()
}

/// Follows every `$ref` in `value` (to `depth` levels) and folds `allOf` into one object schema.
fn resolve(root: &Value, value: &Value, depth: usize) -> Value {
    match value {
        Value::Object(map) => {
            if let Some(target) = map.get("$ref").and_then(Value::as_str) {
                let pointer = target.strip_prefix('#').unwrap_or(target);
                return match root.pointer(pointer) {
                    Some(found) if depth < REF_DEPTH => resolve(root, found, depth + 1),
                    _ => Value::Object(Map::new()),
                };
            }
            let mut out = Map::new();
            for (key, inner) in map.iter() {
                out.insert(key.clone(), resolve(root, inner, depth));
            }
            if let Some(parts) = out.get("allOf").and_then(Value::as_array).cloned() {
                let mut properties = Map::new();
                let mut required: Vec<Value> = Vec::new();
                for part in &parts {
                    if let Some(p) = part.get("properties").and_then(Value::as_object) {
                        for (k, v) in p.iter() {
                            properties.insert(k.clone(), v.clone());
                        }
                    }
                    required.extend(
                        part.get("required")
                            .and_then(Value::as_array)
                            .cloned()
                            .unwrap_or_default(),
                    );
                }
                out.insert("type", Value::from("object"));
                out.insert("properties", Value::Object(properties));
                out.insert("required", Value::Array(required));
            }
            Value::Object(out)
        }
        Value::Array(items) => {
            Value::Array(items.iter().map(|v| resolve(root, v, depth)).collect())
        }
        other => other.clone(),
    }
}

/// The schema's declared type; `oneOf`/`anyOf` are read as their first alternative.
#[must_use]
pub fn type_of(schema: &Value) -> String {
    if let Some(t) = schema.get("type").and_then(Value::as_str) {
        return t.to_string();
    }
    // OpenAPI 3.1 allows `type: [string, "null"]`.
    if let Some(t) = schema
        .get("type")
        .and_then(Value::as_array)
        .and_then(|a| a.iter().filter_map(Value::as_str).find(|t| *t != "null"))
    {
        return t.to_string();
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(first) = schema
            .get(key)
            .and_then(Value::as_array)
            .and_then(|a| a.first())
        {
            return type_of(first);
        }
    }
    if schema.get("properties").is_some() {
        return "object".to_string();
    }
    String::new()
}

/// The names a schema requires.
#[must_use]
pub fn required(schema: &Value) -> Vec<String> {
    schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

/// A value the schema accepts: its `example`, `default` or first `enum` member, else the plainest one of its type.
#[must_use]
pub fn sample(schema: &Value) -> Value {
    sample_at(schema, 0)
}

fn sample_at(schema: &Value, depth: usize) -> Value {
    for key in ["example", "default"] {
        if let Some(v) = schema.get(key).filter(|v| !v.is_null()) {
            return v.clone();
        }
    }
    if let Some(first) = schema
        .get("enum")
        .and_then(Value::as_array)
        .and_then(|e| e.iter().find(|v| !v.is_null()))
    {
        return first.clone();
    }
    match type_of(schema).as_str() {
        "string" => Value::from(sample_string(schema)),
        "integer" => Value::from(
            schema
                .get("minimum")
                .and_then(Value::as_i64)
                .unwrap_or(1)
                .max(1.min(schema.get("maximum").and_then(Value::as_i64).unwrap_or(1))),
        ),
        "number" => Value::from(
            schema
                .get("minimum")
                .and_then(Value::as_f64)
                .unwrap_or(1.0)
                .max(1.0),
        ),
        "boolean" => Value::from(false),
        "array" if depth < REF_DEPTH => {
            let items = schema.get("items").cloned().unwrap_or_default();
            Value::Array(vec![sample_at(&items, depth + 1)])
        }
        "object" if depth < REF_DEPTH => {
            let mut map = Map::new();
            let needed = required(schema);
            if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
                for (name, inner) in properties.iter() {
                    if needed.contains(name) {
                        map.insert(name.clone(), sample_at(inner, depth + 1));
                    }
                }
            }
            Value::Object(map)
        }
        _ => Value::Null,
    }
}

fn sample_string(schema: &Value) -> String {
    let format = schema
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let base = match format {
        "uuid" => "00000000-0000-4000-8000-000000000000".to_string(),
        "date-time" => "2026-01-01T00:00:00Z".to_string(),
        "date" => "2026-01-01".to_string(),
        "email" => "sectest@example.com".to_string(),
        "uri" | "url" => "https://example.com/".to_string(),
        _ => "sectest".to_string(),
    };
    let min =
        usize::try_from(schema.get("minLength").and_then(Value::as_i64).unwrap_or(0)).unwrap_or(0);
    if base.len() < min {
        base.chars().cycle().take(min).collect()
    } else {
        base
    }
}
