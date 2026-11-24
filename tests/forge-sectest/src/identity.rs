//! The identities the authorisation checks act as, and the reviewed spec that describes them.
//!
//! A scan with one credential can only show that the auth wall exists. Authorisation bugs live behind it: a
//! token for another service accepted here, a read-only grant that can write, a second user who can open the
//! first one's data. Finding those needs several identities that differ in exactly one way, so they are minted
//! on demand from gatehouse's test endpoint (`POST /api/v1/test/token`, only mounted when gatehouse runs with
//! `GATEHOUSE_TEST_MODE`, so never in production) from the grants the spec names for each service.

use quench_serde::Value;
use std::collections::HashSet;

/// How much a minted token is allowed to do at the service it is presented to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Level {
    /// The service's `writer` grant: everything a normal user may do.
    Writer,
    /// The service's `reader` grant only.
    Reader,
    /// What the fuzzer signs in with: the `writer` grant unless the spec gives it more.
    Fuzzer,
    /// A valid token for this service that carries no grant of its own.
    Bare,
    /// An administrator's scope, but issued for a different service: it must be worthless here.
    Foreign,
}

impl Level {
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Writer => "a full user",
            Self::Fuzzer => "the fuzzer",
            Self::Reader => "a read-only grant",
            Self::Bare => "a token with no grant",
            Self::Foreign => "an administrator's token issued for another service",
        }
    }
}

/// What a service's scope strings look like, so a token can be minted that holds exactly one level of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grants {
    pub service: String,
    /// The token audience (`aud`); the service's own name unless the spec says otherwise.
    pub audience: String,
    pub reader: String,
    pub writer: String,
    /// What the fuzzer signs in with, when it must reach routes a normal user cannot; else the `writer` grant.
    pub fuzzer: String,
    /// Whether this service holds that reading needs the `reader` grant. Most check only that the caller is signed in
    /// and the token is for them (documented per service), so a grant-less read is a note, not a finding, unless set.
    pub strict_reads: bool,
}

impl Grants {
    fn from_value(item: &Value) -> Result<Self, String> {
        let text_of = |key: &str| item.get(key).and_then(Value::as_str).map(str::to_string);
        let service = text_of("name").ok_or("authz spec: a [[service]] needs `name`")?;
        let need = |key: &str| {
            text_of(key).ok_or_else(|| format!("authz spec: service {service} needs `{key}`"))
        };
        Ok(Self {
            audience: text_of("audience").unwrap_or_else(|| service.clone()),
            reader: need("reader")?,
            writer: need("writer")?,
            fuzzer: text_of("fuzz").unwrap_or_else(|| need("writer").unwrap_or_default()),
            strict_reads: text_of("reads").as_deref() == Some("grant"),
            service: service.clone(),
        })
    }
}

/// One request of a recipe: `METHOD /path` and optionally ` :: body`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub method: String,
    pub path: String,
    pub body: Option<String>,
}

impl Step {
    /// # Errors
    ///
    /// When the text is not `METHOD /path` with an optional ` :: body`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let (head, body) = match text.split_once(" :: ") {
            Some((head, body)) => (head, Some(body.to_string())),
            None => (text, None),
        };
        let (method, path) = head.trim().split_once(' ').ok_or_else(|| {
            format!("{text:?}: expected `METHOD /path` with an optional ` :: body`")
        })?;
        if !path.trim().starts_with('/') {
            return Err(format!("{text:?}: the path must start with /"));
        }
        Ok(Self {
            method: method.trim().to_ascii_uppercase(),
            path: path.trim().to_string(),
            body,
        })
    }

    #[must_use]
    pub fn is_mutating(&self) -> bool {
        matches!(self.method.as_str(), "POST" | "PUT" | "PATCH" | "DELETE")
    }
}

/// An object one user owns and another must not reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    pub name: String,
    pub service: String,
    pub owner: String,
    pub other: String,
    /// Run as the owner. `{canary}` in its body is replaced by a marker no other data contains.
    pub create: Step,
    /// Where the new object's identifier is in the JSON the create returned (`id`, or `task.id`).
    pub id_at: String,
    /// Requests the other user must be refused. `{id}` is the object.
    pub read: Vec<Step>,
    /// Requests the other user may make and be answered, as long as the answer holds nothing of the object (a list).
    pub list: Vec<Step>,
    pub write: Vec<Step>,
    /// Run as the owner afterwards, to leave nothing behind.
    pub cleanup: Option<Step>,
    /// Run by both users first, ignoring the answer (the first call that makes a service provision an account).
    pub prepare: Vec<Step>,
    /// Made before the users act, by a third identity with the service's full grants; the id found at `setup_at`
    /// is `{setup}` (and `{setup2}` for a second one) in every later request and in the two scope strings below.
    pub setup: Option<Step>,
    pub setup2: Option<Step>,
    pub setup_at: String,
    /// Instead of the service's `writer` grant: for a service that grants per object (`...:project:{setup}:write`).
    pub owner_scope: Option<String>,
    pub other_scope: Option<String>,
    /// Run by the third identity at the end, for whatever the setup made.
    pub teardown: Vec<Step>,
    /// What the service needs for the recipe to mean anything (`database`: its tables, which an in-memory store lacks).
    /// The caller decides what it has; the scanner does not look.
    pub requires: Option<String>,
}

/// What a path parameter stands for when the fuzzer cannot work it out: a fixed `value`, or a request that makes one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bind {
    pub service: String,
    /// The parameter's name in the path template (`{profile}`).
    pub param: String,
    /// Only where the segment before it is this template prefix (`/api/v1/conversations`); anywhere when absent.
    pub under: Option<String>,
    pub value: Option<String>,
    pub create: Option<Step>,
    pub id_at: String,
}

impl Bind {
    /// A path parameter that is `value` wherever the template prefix is `under` (or anywhere, when `None`).
    #[must_use]
    pub fn value(service: &str, param: &str, under: Option<&str>, value: &str) -> Self {
        Self {
            service: service.to_string(),
            param: param.to_string(),
            under: under.map(str::to_string),
            value: Some(value.to_string()),
            create: None,
            id_at: "id".to_string(),
        }
    }

    fn from_value(item: &Value) -> Result<Self, String> {
        let text_of = |key: &str| item.get(key).and_then(Value::as_str).map(str::to_string);
        let need =
            |key: &str| text_of(key).ok_or_else(|| format!("authz spec: a [[bind]] needs `{key}`"));
        let bind = Self {
            service: need("service")?,
            param: need("param")?,
            under: text_of("under"),
            value: text_of("value"),
            create: text_of("create").map(|s| Step::parse(&s)).transpose()?,
            id_at: text_of("id_at").unwrap_or_else(|| "id".to_string()),
        };
        if bind.value.is_none() && bind.create.is_none() {
            return Err(format!(
                "authz spec: the [[bind]] for {:?} needs a `value` or a `create`",
                bind.param
            ));
        }
        Ok(bind)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Spec {
    /// What the fuzzer is told about path parameters it cannot discover.
    pub binds: Vec<Bind>,
    /// Gatehouse's base URL, for minting; `--mint` overrides it.
    pub mint: Option<String>,
    pub grants: Vec<Grants>,
    pub resources: Vec<Resource>,
    /// `service METHOD pattern` of routes any authenticated caller may reach without a grant (reviewed).
    pub open: HashSet<String>,
}

impl Spec {
    #[must_use]
    pub fn grants_of(&self, service: &str) -> Option<&Grants> {
        self.grants.iter().find(|g| g.service == service)
    }

    /// # Errors
    ///
    /// Which part of the spec is missing or malformed.
    pub fn parse(text: &str) -> Result<Self, String> {
        let root: Value =
            quench_serde::toml::from_str(text).map_err(|e| format!("authz spec: {e}"))?;
        let text_of = |value: &Value, key: &str| -> Option<String> {
            value.get(key).and_then(Value::as_str).map(str::to_string)
        };
        let need = |value: &Value, key: &str, what: &str| -> Result<String, String> {
            text_of(value, key).ok_or_else(|| format!("authz spec: {what} needs `{key}`"))
        };
        let list = |key: &str| {
            root.get(key)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        };

        let mut spec = Self {
            mint: text_of(&root, "mint"),
            ..Self::default()
        };
        for entry in root
            .get("open")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(key) = entry.as_str() {
                spec.open.insert(key.to_string());
            }
        }
        for service in list("service") {
            spec.grants.push(Grants::from_value(&service)?);
        }
        for item in list("bind") {
            spec.binds.push(Bind::from_value(&item)?);
        }
        for item in list("resource") {
            let name = need(&item, "name", "a [[resource]]")?;
            let what = format!("resource {name:?}");
            let steps = |key: &str| -> Result<Vec<Step>, String> {
                item.get(key)
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(Step::parse)
                    .collect()
            };
            let resource = Resource {
                service: need(&item, "service", &what)?,
                owner: need(&item, "owner", &what)?,
                other: need(&item, "other", &what)?,
                create: Step::parse(&need(&item, "create", &what)?)?,
                id_at: text_of(&item, "id_at").unwrap_or_else(|| "id".to_string()),
                read: steps("read")?,
                list: steps("list")?,
                write: steps("write")?,
                cleanup: text_of(&item, "cleanup")
                    .map(|s| Step::parse(&s))
                    .transpose()?,
                prepare: steps("prepare")?,
                setup: text_of(&item, "setup")
                    .map(|s| Step::parse(&s))
                    .transpose()?,
                setup2: text_of(&item, "setup2")
                    .map(|s| Step::parse(&s))
                    .transpose()?,
                setup_at: text_of(&item, "setup_at").unwrap_or_else(|| "id".to_string()),
                owner_scope: text_of(&item, "owner_scope"),
                other_scope: text_of(&item, "other_scope"),
                teardown: steps("teardown")?,
                requires: text_of(&item, "requires"),
                name,
            };
            if resource.owner == resource.other {
                return Err(format!("authz spec: {what} needs two different users"));
            }
            if resource.read.is_empty() && resource.list.is_empty() {
                return Err(format!(
                    "authz spec: {what} needs a `read` or a `list` request, or there is nothing to look at"
                ));
            }
            if spec.grants_of(&resource.service).is_none() {
                return Err(format!(
                    "authz spec: {what} names service {:?}, which has no [[service]] grants",
                    resource.service
                ));
            }
            spec.resources.push(resource);
        }
        if spec.grants.is_empty() {
            return Err(
                "authz spec: no [[service]] entries: there is nothing to mint tokens from"
                    .to_string(),
            );
        }
        Ok(spec)
    }
}

/// The value at a dotted path (`task.id`) of a JSON document, as text.
#[must_use]
pub fn json_at(body: &str, path: &str) -> Option<String> {
    let mut value: Value = quench_serde::json::from_str(body).ok()?;
    for key in path.split('.') {
        value = value.get(key)?.clone();
    }
    match value {
        Value::String(s) => Some(s),
        Value::Int(n) => Some(n.to_string()),
        Value::UInt(n) => Some(n.to_string()),
        _ => None,
    }
}
