//! Authorisation: what a valid token still must not be able to do.
//!
//! The `auth` check proves a wall exists. This one acts as several identities that differ in exactly one way and
//! looks for the places the wall has a gap:
//!
//! * **audience** - an administrator's token issued for *another* service must be worthless on every protected route.
//! * **no grant** - a token for this service that carries no scope must be refused everywhere except the reviewed
//!   `open` routes (every authenticated caller may reach `/me`).
//! * **read-only** - a reader must not be able to change anything (`--active`, since a hole means the request runs).
//! * **ownership (IDOR)** - for each `[[resource]]` the spec describes, one user creates an object and another must be
//!   refused when reading, changing or deleting it, and the owner's object must come out untouched (`--active`).
//!
//! Tokens come from gatehouse's test mint endpoint, so this only runs against a stack started in test mode.

use crate::census::Route;
use crate::checks::auth::redirects_to_login;
use crate::finding::{Finding, Severity};
use crate::identity::{Grants, Level, Resource, Step, json_at};
use crate::scan::Ctx;
use crate::wire::{Probe, Reply, Target};
use std::collections::HashMap;

/// Mints and remembers the tokens of one service's identities.
struct Tokens<'a> {
    ctx: &'a Ctx,
    gatehouse: Target,
    grants: &'a Grants,
    cache: HashMap<(String, String), String>,
}

impl Tokens<'_> {
    async fn get(&mut self, sub: &str, level: Level) -> Result<String, String> {
        let (audience, scope) = match level {
            Level::Writer => (self.grants.audience.clone(), self.grants.writer.clone()),
            Level::Fuzzer => (self.grants.audience.clone(), self.grants.fuzzer.clone()),
            Level::Reader => (self.grants.audience.clone(), self.grants.reader.clone()),
            Level::Bare => (self.grants.audience.clone(), "sectest-no-grant".to_string()),
            Level::Foreign => ("sectest-another-service".to_string(), "admin".to_string()),
        };
        self.mint(sub, &audience, &scope).await
    }

    /// A token for this service's audience holding exactly `scope`.
    async fn scoped(&mut self, sub: &str, scope: &str) -> Result<String, String> {
        let audience = self.grants.audience.clone();
        self.mint(sub, &audience, scope).await
    }

    async fn mint(&mut self, sub: &str, audience: &str, scope: &str) -> Result<String, String> {
        let key = (sub.to_string(), format!("{audience}\u{1}{scope}"));
        if let Some(token) = self.cache.get(&key) {
            return Ok(token.clone());
        }
        let body = format!(
            r#"{{"sub":{},"aud":[{}],"scope":{}}}"#,
            quote(sub),
            quote(audience),
            quote(scope)
        );
        let probe = Probe::new("POST", "/api/v1/test/token").json(&body);
        let reply = self
            .ctx
            .send(&self.gatehouse, &probe)
            .await
            .ok_or_else(|| "gatehouse did not answer the mint request".to_string())?;
        if !(200..300).contains(&reply.status) {
            return Err(format!(
                "gatehouse refused to mint ({}): is it running with GATEHOUSE_TEST_MODE? {}",
                reply.status,
                reply.body.chars().take(120).collect::<String>()
            ));
        }
        let token = json_at(&reply.body, "access_token")
            .ok_or_else(|| "the mint response has no access_token".to_string())?;
        self.cache.insert(key, token.clone());
        Ok(token)
    }
}

fn quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// How the service answered a request that carried a token it should not honour (or should).
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Denied,
    /// 400/404/405/422: the request was turned away for its shape, which says nothing about permission.
    Unclear,
    Reached,
}

fn verdict(reply: &Reply) -> Verdict {
    match reply.status {
        401 | 403 => Verdict::Denied,
        301 | 302 | 303 | 307 | 308 if redirects_to_login(reply) => Verdict::Denied,
        // 502-504: the service could not do the work (no database, a dependency down); permission was not decided.
        400 | 404 | 405 | 422 | 502..=504 => Verdict::Unclear,
        _ => Verdict::Reached,
    }
}

fn body_for(method: &str) -> Option<&'static str> {
    matches!(method, "POST" | "PUT" | "PATCH").then_some("{}")
}

/// A token holding the service's full grants, for the checks that need to be signed in (the fuzzer).
///
/// # Errors
///
/// Why none could be minted: no spec, no grants for the service, no gatehouse URL, or gatehouse refused.
pub async fn writer_token(ctx: &Ctx, target: &Target, sub: &str) -> Result<String, String> {
    signed_in(ctx, target, sub, Level::Writer).await
}

/// The fuzzer's token: the service's `fuzz` grant when it has one, else its full user grant.
///
/// # Errors
///
/// As [`writer_token`].
pub async fn fuzz_token(ctx: &Ctx, target: &Target, sub: &str) -> Result<String, String> {
    signed_in(ctx, target, sub, Level::Fuzzer).await
}

async fn signed_in(ctx: &Ctx, target: &Target, sub: &str, level: Level) -> Result<String, String> {
    let spec = ctx.opts.authz.as_ref().ok_or("no authz spec")?;
    let grants = spec
        .grants_of(&target.name)
        .ok_or_else(|| format!("the authz spec has no grants for {}", target.name))?;
    let mint = ctx
        .opts
        .mint
        .clone()
        .or_else(|| spec.mint.clone())
        .ok_or("no gatehouse URL to mint from")?;
    let mut gatehouse = Target::parse("gatehouse", &mint)?;
    gatehouse.insecure = target.insecure;
    let mut tokens = Tokens {
        ctx,
        gatehouse,
        grants,
        cache: HashMap::new(),
    };
    tokens.get(sub, level).await
}

pub async fn run(ctx: &Ctx, target: &Target, protected: &[Route]) -> Vec<Finding> {
    let mut findings = Vec::new();
    let Some(spec) = &ctx.opts.authz else {
        return findings;
    };
    let finding = |severity, method: &str, path: &str, detail: String| {
        Finding::new(severity, "authz", &target.name, method, path, detail)
    };
    let Some(grants) = spec.grants_of(&target.name) else {
        findings.push(finding(
            Severity::Info,
            "-",
            "-",
            "the authz spec has no [[service]] grants for this service, so its authorisation was not tested".to_string(),
        ));
        return findings;
    };
    let Some(mint) = ctx.opts.mint.clone().or_else(|| spec.mint.clone()) else {
        findings.push(finding(
            Severity::High,
            "-",
            "-",
            "no gatehouse URL to mint identities from (--mint or `mint` in the spec): authorisation was not tested".to_string(),
        ));
        return findings;
    };
    let mut gatehouse = match Target::parse("gatehouse", &mint) {
        Ok(t) => t,
        Err(e) => {
            findings.push(finding(Severity::High, "-", "-", format!("--mint: {e}")));
            return findings;
        }
    };
    gatehouse.insecure = target.insecure;
    let mut tokens = Tokens {
        ctx,
        gatehouse,
        grants,
        cache: HashMap::new(),
    };

    findings.extend(identities(ctx, target, &mut tokens, protected).await);

    let resources: Vec<&Resource> = spec
        .resources
        .iter()
        .filter(|r| r.service == target.name)
        .collect();
    if !resources.is_empty() && !ctx.opts.active {
        findings.push(finding(
            Severity::Info,
            "-",
            "-",
            format!(
                "{} ownership recipes were not run; they create data, so they need --active",
                resources.len()
            ),
        ));
    } else {
        for resource in resources {
            findings.extend(ownership(ctx, target, &mut tokens, resource).await);
        }
    }
    findings
}

/// The checks that need no knowledge of the service: every protected route, asked with tokens that must not work.
async fn identities(
    ctx: &Ctx,
    target: &Target,
    tokens: &mut Tokens<'_>,
    protected: &[Route],
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let Some(spec) = &ctx.opts.authz else {
        return findings;
    };
    let grants = tokens.grants;
    let finding = |severity, method: &str, path: &str, detail: String| {
        Finding::new(severity, "authz", &target.name, method, path, detail)
    };
    let (mut unclear, mut skipped) = (0usize, 0usize);
    let mut open_reads: Vec<String> = Vec::new();
    for route in protected {
        if route.is_mutating() && !ctx.opts.active {
            skipped += 1;
            continue;
        }
        let mut cases = vec![
            (Level::Foreign, Severity::High),
            (Level::Bare, Severity::Medium),
        ];
        if route.is_mutating() && ctx.opts.active {
            cases.push((Level::Reader, Severity::High));
        }
        for (level, base) in cases {
            if level != Level::Foreign && spec.open.contains(&route.key()) {
                continue;
            }
            let token = match tokens.get("sectest-generic", level).await {
                Ok(token) => token,
                Err(e) => {
                    findings.push(finding(
                        Severity::High,
                        "-",
                        "-",
                        format!(
                            "could not mint {}: {e}; authorisation was not tested",
                            level.describe()
                        ),
                    ));
                    return findings;
                }
            };
            let mut probe = Probe::new(&route.method, &route.sample_path());
            if let Some(body) = body_for(&route.method) {
                probe = probe.json(body);
            }
            let Some(reply) = ctx.send(target, &probe.bearer(&token)).await else {
                continue;
            };
            match verdict(&reply) {
                Verdict::Denied => {}
                Verdict::Unclear => unclear += 1,
                Verdict::Reached
                    if level == Level::Bare && !route.is_mutating() && !grants.strict_reads =>
                {
                    open_reads.push(format!("{} {}", route.method, route.pattern));
                }
                Verdict::Reached => {
                    let severity = if route.is_mutating() && base == Severity::Medium {
                        Severity::High
                    } else {
                        base
                    };
                    findings.push(finding(
                        severity,
                        &route.method,
                        &route.pattern,
                        format!(
                            "{} was answered {} instead of being refused ({})",
                            level.describe(),
                            reply.status,
                            match level {
                                Level::Foreign => "audience is not enforced here",
                                Level::Bare => "list the route under `open` if every signed-in user may reach it",
                                Level::Reader => "a read grant can change data",
                                Level::Writer | Level::Fuzzer => "",
                            }
                        ),
                    ));
                }
            }
        }
    }
    findings.extend(notes(&target.name, &open_reads, unclear, skipped));
    findings
}

/// What the probes above could not decide or deliberately did not say, so a quiet report is not mistaken for a clean one.
fn notes(service: &str, open_reads: &[String], unclear: usize, skipped: usize) -> Vec<Finding> {
    let mut findings = Vec::new();
    let finding =
        |severity, detail: String| Finding::new(severity, "authz", service, "-", "-", detail);
    if !open_reads.is_empty() {
        let shown = open_reads
            .iter()
            .take(6)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        findings.push(finding(
            Severity::Low,
            format!(
                "{} read routes answer a signed-in caller who holds no grant of this service ({shown}{}): fine if reads need only a login, as documented; set `reads = \"grant\"` for this service to hold it to its grants",
                open_reads.len(),
                if open_reads.len() > 6 { ", ..." } else { "" }
            ),
        ));
    }
    if unclear > 0 {
        findings.push(finding(
            Severity::Info,
            format!("{unclear} probes were turned away as malformed or unrouted (400/404/405/422): permission was not decided for them"),
        ));
    }
    if skipped > 0 {
        findings.push(finding(
            Severity::Info,
            format!("{skipped} mutating routes were not probed with identities; pass --active to include them"),
        ));
    }
    findings
}

fn canary() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("sectest-canary-{nanos}")
}

/// What the recipe's placeholders stand for in one run.
#[derive(Debug, Default, Clone)]
struct Vars {
    id: String,
    canary: String,
    setup: String,
    setup2: String,
}

impl Vars {
    #[allow(
        clippy::literal_string_with_formatting_args,
        reason = "recipe placeholders, not format arguments"
    )]
    fn fill(&self, text: &str) -> String {
        // Short and alphanumeric, for the places (a project key) that cannot hold the whole canary.
        let nonce: String = self
            .canary
            .chars()
            .rev()
            .take(7)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        text.replace("{nonce}", &nonce)
            .replace("{id}", &self.id)
            .replace("{canary}", &self.canary)
            .replace("{setup}", &self.setup)
            .replace("{setup2}", &self.setup2)
    }
}

fn probe_of(step: &Step, vars: &Vars, token: &str) -> Probe {
    let mut probe = Probe::new(&step.method, &vars.fill(&step.path));
    if let Some(body) = &step.body {
        probe = probe.json(&vars.fill(body));
    } else if let Some(body) = body_for(&step.method) {
        probe = probe.json(body);
    }
    probe.bearer(token)
}

fn succeeded(reply: &Reply) -> bool {
    (200..400).contains(&reply.status) && verdict(reply) == Verdict::Reached
}

/// What the other user is refused: reading the object, then changing or deleting it.
async fn attempts(
    ctx: &Ctx,
    target: &Target,
    resource: &Resource,
    vars: &Vars,
    other: &str,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mark = vars.canary.as_str();
    let found = |severity, step: &Step, detail: String| {
        Finding::new(
            severity,
            "authz",
            &target.name,
            &step.method,
            &step.path,
            format!("{}: {detail}", resource.name),
        )
    };
    for step in &resource.read {
        let Some(reply) = ctx.send(target, &probe_of(step, vars, other)).await else {
            continue;
        };
        if succeeded(&reply) || reply.body.contains(mark) {
            findings.push(found(
                Severity::High,
                step,
                format!(
                    "another user was answered {} for an object they do not own (IDOR: read)",
                    reply.status
                ),
            ));
        }
    }
    for step in &resource.list {
        let Some(reply) = ctx.send(target, &probe_of(step, vars, other)).await else {
            continue;
        };
        if reply.body.contains(mark) {
            findings.push(found(
                Severity::High,
                step,
                "another user's list holds an object they do not own (IDOR: list)".to_string(),
            ));
        }
    }
    // Writes last: a hole that deletes the object would otherwise hide the reads above.
    for step in &resource.write {
        let Some(reply) = ctx.send(target, &probe_of(step, vars, other)).await else {
            continue;
        };
        if succeeded(&reply) {
            findings.push(found(
                Severity::Critical,
                step,
                format!(
                    "another user was answered {} for changing an object they do not own (IDOR: write)",
                    reply.status
                ),
            ));
        }
    }
    findings
}

/// The three identities of one recipe and what the setup made.
struct Actors {
    owner: String,
    other: String,
    setup: String,
}

fn stale(resource: &Resource, target: &Target, step: &Step, detail: &str) -> Finding {
    Finding::new(
        Severity::Medium,
        "authz",
        &target.name,
        &step.method,
        &step.path,
        format!(
            "{}: {detail}: the recipe is stale, so nothing about ownership was tested",
            resource.name
        ),
    )
}

/// Mints the identities, runs the setup and the prepare steps. `None` (with the reason in `findings`) when the
/// recipe cannot get started.
async fn actors(
    ctx: &Ctx,
    target: &Target,
    tokens: &mut Tokens<'_>,
    resource: &Resource,
    vars: &mut Vars,
    findings: &mut Vec<Finding>,
) -> Option<Actors> {
    let minted = tokens.get("sectest-setup", Level::Writer).await;
    let Ok(setup) = minted else {
        let e = minted.unwrap_err();
        findings.push(stale(
            resource,
            target,
            &resource.create,
            &format!("could not mint the setup identity ({e})"),
        ));
        return None;
    };
    for (step, slot) in [(&resource.setup, 1), (&resource.setup2, 2)] {
        let Some(step) = step else { continue };
        let reply = ctx.send(target, &probe_of(step, vars, &setup)).await?;
        let id = (200..300)
            .contains(&reply.status)
            .then(|| json_at(&reply.body, &resource.setup_at))
            .flatten();
        let Some(id) = id else {
            findings.push(stale(
                resource,
                target,
                step,
                &format!(
                    "the setup was answered {} (id at `{}`)",
                    reply.status, resource.setup_at
                ),
            ));
            return None;
        };
        if slot == 1 {
            vars.setup = id;
        } else {
            vars.setup2 = id;
        }
    }
    let mut person = async |sub: &str, scope: &Option<String>| match scope {
        Some(scope) => tokens.scoped(sub, &vars.fill(scope)).await,
        None => tokens.get(sub, Level::Writer).await,
    };
    let (owner, other) = (
        person(&resource.owner, &resource.owner_scope).await,
        person(&resource.other, &resource.other_scope).await,
    );
    let (Ok(owner), Ok(other)) = (owner, other) else {
        findings.push(stale(
            resource,
            target,
            &resource.create,
            "could not mint the two users",
        ));
        return None;
    };
    for step in &resource.prepare {
        for token in [&owner, &other] {
            let _ = ctx.send(target, &probe_of(step, vars, token)).await;
        }
    }
    Some(Actors {
        owner,
        other,
        setup,
    })
}

/// One user's object, the other user's attempts on it, and proof the owner's copy came through.
async fn ownership(
    ctx: &Ctx,
    target: &Target,
    tokens: &mut Tokens<'_>,
    resource: &Resource,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut vars = Vars {
        canary: canary(),
        ..Vars::default()
    };
    let Some(actors) = actors(ctx, target, tokens, resource, &mut vars, &mut findings).await else {
        return findings;
    };
    findings.extend(exercise(ctx, target, resource, &mut vars, &actors).await);
    if let Some(cleanup) = &resource.cleanup {
        let _ = ctx
            .send(target, &probe_of(cleanup, &vars, &actors.owner))
            .await;
    }
    for step in &resource.teardown {
        let _ = ctx
            .send(target, &probe_of(step, &vars, &actors.setup))
            .await;
    }
    findings
}

async fn exercise(
    ctx: &Ctx,
    target: &Target,
    resource: &Resource,
    vars: &mut Vars,
    actors: &Actors,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let Some(created) = ctx
        .send(target, &probe_of(&resource.create, vars, &actors.owner))
        .await
    else {
        return findings;
    };
    let id = (200..300)
        .contains(&created.status)
        .then(|| json_at(&created.body, &resource.id_at))
        .flatten();
    let Some(id) = id else {
        findings.push(stale(
            resource,
            target,
            &resource.create,
            &format!(
                "the owner could not create the object (answered {}, id at `{}`)",
                created.status, resource.id_at
            ),
        ));
        return findings;
    };
    vars.id = id;
    let mark = vars.canary.clone();

    // Without this a refusal below could simply mean the object never existed.
    // A get-by-id if the service has one, else the list the object must appear in.
    let first = resource
        .read
        .first()
        .or_else(|| resource.list.first())
        .expect("the spec has a read or a list request");
    let before = ctx
        .send(target, &probe_of(first, vars, &actors.owner))
        .await;
    // A list proves nothing about the other user's unless it shows the owner's own object first.
    let listed = resource.read.is_empty();
    let usable = before
        .as_ref()
        .is_some_and(|b| succeeded(b) && (!listed || b.body.contains(&mark)));
    if !usable {
        findings.push(stale(
            resource,
            target,
            first,
            &format!(
                "the owner could not read back what they created (answered {})",
                before.map_or(0, |b| b.status)
            ),
        ));
        return findings;
    }
    findings.extend(attempts(ctx, target, resource, vars, &actors.other).await);
    let after = ctx
        .send(target, &probe_of(first, vars, &actors.owner))
        .await;
    let intact = after.as_ref().is_some_and(|a| {
        succeeded(a)
            && (!before.as_ref().is_some_and(|b| b.body.contains(&mark)) || a.body.contains(&mark))
    });
    if !intact {
        findings.push(Finding::new(
            Severity::High,
            "authz",
            &target.name,
            &first.method,
            &first.path,
            format!(
                "{}: the owner's object changed or vanished after another user's refused requests",
                resource.name
            ),
        ));
    }
    findings
}
