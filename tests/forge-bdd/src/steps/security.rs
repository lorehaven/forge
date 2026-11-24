//! The authorisation check of `forge-sectest`, run against a service this suite started.
//!
//! The suite already has what the check needs and a scan of a live estate never does: a gatehouse in test mode to
//! mint identities from, throwaway stores that nobody minds being written to, and (with `FORGE_BDD_DATABASE_URL`) the
//! databases the ownership recipes need. So the same check that the nightly cannot run against production runs here
//! every day, and lands in the same report.

use crate::world::ForgeWorld;
use forge_sectest::census::{self, Route};
use forge_sectest::finding::{Finding, Severity};
use forge_sectest::identity::{Bind, Spec};
use forge_sectest::scan::{self, Options};
use forge_sectest::wire::Target;
use quench_test_bdd::then;
use std::sync::OnceLock;

/// Where the reviewed spec lives, relative to the forge checkout.
const SPEC: &str = "tests/forge-sectest/authz.toml";

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("forge-bdd sits two levels below the workspace root")
        .to_path_buf()
}

/// Every route the estate declares; reading them is slow enough (`cargo metadata`) to do once.
fn routes() -> &'static [Route] {
    static ROUTES: OnceLock<Vec<Route>> = OnceLock::new();
    ROUTES.get_or_init(|| {
        let root = root();
        let extra = census::quench_route_sources(&root);
        census::scan_forge(&root, &extra)
    })
}

fn url_of<'a>(world: &'a ForgeWorld, service: &str) -> Option<&'a str> {
    Some(match service {
        "sage" => &world.sage_url,
        "switchboard" => &world.switchboard_url,
        "warehouse" => &world.warehouse_url,
        "conveyor" => &world.conveyor_url,
        "workbench" => &world.workbench_url,
        "daybook" => &world.daybook_url,
        "gauge" => &world.gauge_url,
        "mailroom" => &world.mailroom_url,
        "gatehouse" => &world.gatehouse_url,
        _ => return None,
    })
}

/// Fails on anything a person must look at: medium and above. Low and info are notes the report keeps.
#[then(expr = "the authorisation of {word} holds")]
#[allow(
    clippy::needless_pass_by_ref_mut,
    reason = "the step macro requires `&mut World`"
)]
async fn authorisation_holds(world: &mut ForgeWorld, service: String) {
    let url = url_of(world, &service)
        .unwrap_or_else(|| panic!("no such service in this suite: {service}"));
    let text = std::fs::read_to_string(root().join(SPEC)).unwrap_or_else(|e| panic!("{SPEC}: {e}"));
    let mut spec = Spec::parse(&text).unwrap_or_else(|e| panic!("{SPEC}: {e}"));
    // A recipe about a table has nothing to say on a store that has none.
    if crate::services::database_url().is_none() {
        spec.resources
            .retain(|r| r.requires.as_deref() != Some("database"));
    }

    // The recipes' users own rows in `auth.users` (the foreign key of every user-owned table), which gatehouse
    // writes in the estate and nobody has written here.
    if crate::services::database_url().is_some() {
        // The scanner's own identities too: its setup creates records as one of them.
        for user in ["sectest-setup", "sectest-generic"] {
            crate::services::ensure_user(user).await;
        }
        for resource in spec.resources.iter().filter(|r| r.service == service) {
            crate::services::ensure_user(&resource.owner).await;
            crate::services::ensure_user(&resource.other).await;
        }
    }

    let mut target = Target::parse(&service, url).unwrap_or_else(|e| panic!("{url}: {e}"));
    target.insecure = true;
    let mut options = Options::new(vec![target], routes().to_vec());
    options.rps = 500;
    options.active = true;
    options.checks = vec!["authz".to_string()];
    options.client_ip = Some("198.51.100.10".to_string());
    options.authz = Some(spec);
    options.mint = Some(world.gatehouse_url.clone());

    let (findings, requests) = scan::run(options)
        .await
        .unwrap_or_else(|e| panic!("the authorisation scan of {service} did not run: {e}"));
    // What was noted but is not a failure: a report that only ever says "passed" cannot be told from a check that did nothing.
    println!(
        "authz {service}: {requests} requests, {} notes",
        findings.len()
    );
    for note in findings.iter().filter(|f| f.severity < Severity::Medium) {
        println!("{note}");
    }
    let serious: Vec<String> = findings
        .iter()
        .filter(|f: &&Finding| f.severity >= Severity::Medium)
        .map(ToString::to_string)
        .collect();
    assert!(
        serious.is_empty(),
        "{} authorisation finding(s) for {service} in {requests} requests:\n{}",
        serious.len(),
        serious.join("\n")
    );
}

/// forge-sectest's fuzzer, from the service's own `OpenAPI` document, as a signed-in user with the service's full grants.
///
/// Fails on what a person must look at: medium and above (a 500, a leaked error, a stall, a client-settable privileged
/// field). What it merely noted (a field accepted with the wrong type, requests it could not make) is printed.
#[then(expr = "the fuzzing of {word} finds nothing wrong")]
#[allow(
    clippy::needless_pass_by_ref_mut,
    reason = "the step macro requires `&mut World`"
)]
async fn fuzzing_finds_nothing(world: &mut ForgeWorld, service: String) {
    let url = url_of(world, &service)
        .unwrap_or_else(|| panic!("no such service in this suite: {service}"));
    let text = std::fs::read_to_string(root().join(SPEC)).unwrap_or_else(|e| panic!("{SPEC}: {e}"));
    let spec = Spec::parse(&text).unwrap_or_else(|e| panic!("{SPEC}: {e}"));
    if crate::services::database_url().is_some() {
        crate::services::ensure_user("sectest-fuzz").await;
    }
    let mut target = Target::parse(&service, url).unwrap_or_else(|e| panic!("{url}: {e}"));
    target.insecure = true;
    let mut options = Options::new(vec![target], Vec::new());
    options.rps = 500;
    options.active = true;
    options.checks = vec!["fuzz".to_string()];
    options.client_ip = Some("198.51.100.20".to_string());
    options.forge_root = Some(root());
    if let Some(grants) = spec.grants_of(&service) {
        options.binds = seeds(world, &service, &grants.writer).await;
    }
    options.authz = Some(spec);
    options.mint = Some(world.gatehouse_url.clone());
    if service == "gatehouse" {
        // Where its parsing is: the administration routes. They are bound to the throwaway account above.
        options.token = Some(
            crate::world::mint_test_token(
                &world.client,
                &world.gatehouse_url,
                "sectest-fuzz",
                &["gatehouse"],
                "admin",
            )
            .await,
        );
    }

    let (findings, requests) = scan::run(options)
        .await
        .unwrap_or_else(|e| panic!("the fuzzing of {service} did not run: {e}"));
    println!(
        "fuzz {service}: {requests} requests, {} notes",
        findings.len()
    );
    for note in findings.iter().filter(|f| f.severity < Severity::Medium) {
        println!("{note}");
    }
    let serious: Vec<String> = findings
        .iter()
        .filter(|f: &&Finding| f.severity >= Severity::Medium)
        .map(ToString::to_string)
        .collect();
    assert!(
        serious.is_empty(),
        "{} fuzzing finding(s) for {service} in {requests} requests:\n{}",
        serious.len(),
        serious.join("\n")
    );
}

/// The mock vLLM instance the harness starts sage with (the same id its chat scenarios send).
const MOCK_INSTANCE: &str = "mock-1782724283792";

/// Objects the fuzzer cannot make for itself (it speaks JSON and reads no event streams), made here as the fuzz user and
/// handed over as path-parameter values, so the routes that take their ids are fuzzed against things that exist.
async fn seeds(world: &ForgeWorld, service: &str, scope: &str) -> Vec<Bind> {
    let mint = |audience: &'static str| async move {
        crate::world::mint_test_token(
            &world.client,
            &world.gatehouse_url,
            "sectest-fuzz",
            &[audience],
            scope,
        )
        .await
    };
    match service {
        "gauge" => vec![Bind::value("gauge", "name", None, "bdd-host")],
        "gatehouse" => gatehouse_seeds(world).await,
        "mailroom" => mailroom_seeds(world, &mint("mailroom").await).await,
        "warehouse" => warehouse_seeds(world, &mint("warehouse").await).await,
        "sage" => sage_seeds(world, &mint("sage").await).await,
        _ => Vec::new(),
    }
}

async fn warehouse_seeds(world: &ForgeWorld, token: &str) -> Vec<Bind> {
    // A crate, in the wire format `cargo publish` speaks: length-prefixed metadata, then the archive.
    let metadata = quench_serde::json::to_string(&quench_serde::json!({
        "name": "fuzz-seed-crate", "vers": "0.1.0", "deps": [], "features": {},
        "authors": ["sectest"], "description": "seed for the fuzzer",
    }));
    let archive = b"seed archive";
    let mut body = Vec::new();
    body.extend_from_slice(&u32::try_from(metadata.len()).unwrap_or(0).to_le_bytes());
    body.extend_from_slice(metadata.as_bytes());
    body.extend_from_slice(&u32::try_from(archive.len()).unwrap_or(0).to_le_bytes());
    body.extend_from_slice(archive);
    let _ = world
        .client
        .put(format!("{}/api/v1/crates/new", world.warehouse_url))
        .bearer_auth(token)
        .body(body)
        .send()
        .await;
    // A build in the artifact store (a repeat answers 409, which still leaves it there).
    let _ = world
        .client
        .put(format!(
            "{}/api/v1/artifacts/com.example.fuzz.mac/macos/1?format=dmg&arch=aarch64",
            world.warehouse_url
        ))
        .bearer_auth(token)
        .body(b"seed build".to_vec())
        .send()
        .await;
    vec![
        Bind::value(
            "warehouse",
            "name",
            Some("/api/v1/crates"),
            "fuzz-seed-crate",
        ),
        Bind::value(
            "warehouse",
            "version",
            Some("/api/v1/crates/{name}"),
            "0.1.0",
        ),
        Bind::value("warehouse", "program", None, "com.example.fuzz.mac"),
        Bind::value("warehouse", "platform", None, "macos"),
        Bind::value("warehouse", "version_code", None, "1"),
    ]
}

/// What a person owns in sage once they have talked to it.
#[derive(Debug, Default)]
struct SageObjects {
    turn: Option<String>,
    conversation: Option<String>,
    message: Option<String>,
    file: Option<String>,
    /// Why the file is missing, when it is.
    upload: Option<String>,
}

/// A conversation exists only once a turn has been streamed to its end, and the id is the server's: it is in the last event.
async fn sage_objects(world: &ForgeWorld, token: &str) -> SageObjects {
    let base = &world.sage_url;
    let get_id =
        |reply: Option<String>| reply.and_then(|b| forge_sectest::identity::json_at(&b, "id"));
    let text = |response: Result<
        quench_client::transport::Response,
        quench_client::transport::Error,
    >| async move {
        match response {
            Ok(r) => r.text().await.ok(),
            Err(_) => None,
        }
    };
    let proposed = get_id(
        text(
            world
                .client
                .get(format!("{base}/api/v1/conversation-id"))
                .bearer_auth(token)
                .send()
                .await,
        )
        .await,
    )
    .unwrap_or_default();
    let started = text(
        world
            .client
            .post(format!("{base}/api/v1/turns"))
            .bearer_auth(token)
            .json(&quench_serde::json!({"instance_id": MOCK_INSTANCE, "message": "seed", "conversation_id": proposed}))
            .send()
            .await,
    )
    .await;
    let mut objects = SageObjects {
        turn: get_id(started),
        ..SageObjects::default()
    };
    let Some(turn) = objects.turn.clone() else {
        return objects;
    };
    let stream = text(
        world
            .client
            .get(format!("{base}/api/v1/turns/{turn}/stream"))
            .bearer_auth(token)
            .send()
            .await,
    )
    .await
    .unwrap_or_default();
    let done = stream
        .split("event: done")
        .nth(1)
        .and_then(|rest| rest.lines().find_map(|l| l.strip_prefix("data: ")))
        .unwrap_or_default()
        .to_string();
    objects.conversation = forge_sectest::identity::json_at(&done, "conversation_id");
    objects.message = forge_sectest::identity::json_at(&done, "message_id");
    if let Some(conversation) = &objects.conversation {
        let form = quench_client::multipart::Form::new()
            .part(
                "file",
                quench_client::multipart::Part::bytes(b"seed file".to_vec())
                    .file_name("seed.txt".to_string()),
            )
            .text("conversation_id", conversation.clone());
        let sent = world
            .client
            .post(format!("{base}/api/v1/files"))
            .bearer_auth(token)
            .multipart(form)
            .send()
            .await;
        let uploaded = match sent {
            Ok(r) => {
                let status = r.status().as_u16();
                let body = r.text().await.unwrap_or_default();
                objects.upload = Some(format!("{status}: {body}"));
                (status < 300).then_some(body)
            }
            Err(e) => {
                objects.upload = Some(format!("not sent: {e}"));
                None
            }
        };
        objects.file = get_id(uploaded);
    }
    objects
}

async fn sage_seeds(world: &ForgeWorld, token: &str) -> Vec<Bind> {
    let objects = sage_objects(world, token).await;
    let mut binds = Vec::new();
    if let Some(turn) = &objects.turn {
        binds.push(Bind::value("sage", "id", Some("/api/v1/turns"), turn));
    }
    if let Some(conversation) = &objects.conversation {
        binds.push(Bind::value(
            "sage",
            "id",
            Some("/api/v1/conversations"),
            conversation,
        ));
    }
    if let Some(message) = &objects.message {
        binds.push(Bind::value("sage", "id", Some("/api/v1/messages"), message));
    }
    if let Some(file) = &objects.file {
        binds.push(Bind::value("sage", "file_id", None, file));
    }
    binds
}

/// One person's conversation, message and file, and what another person with the same grants is refused: reading them,
/// changing them, taking them away. (The generic recipes cannot make a conversation: it needs a streamed turn.)
#[then("sage keeps one person's conversations and files from another")]
#[allow(
    clippy::needless_pass_by_ref_mut,
    clippy::too_many_lines,
    reason = "the step macro requires `&mut World`; one scenario's worth of requests, in the order it reads"
)]
async fn sage_ownership(world: &mut ForgeWorld) {
    let scope = "user sage:read sage:write";
    let (alice, bob) = ("sectest-alice", "sectest-bob");
    if crate::services::database_url().is_some() {
        crate::services::ensure_user(alice).await;
        crate::services::ensure_user(bob).await;
    }
    let token = |sub: &'static str| {
        let world = &*world;
        async move {
            crate::world::mint_test_token(
                &world.client,
                &world.gatehouse_url,
                sub,
                &["sage"],
                scope,
            )
            .await
        }
    };
    let (owner, other) = (token(alice).await, token(bob).await);
    let objects = sage_objects(world, &owner).await;
    let (Some(conversation), Some(message), Some(file)) =
        (&objects.conversation, &objects.message, &objects.file)
    else {
        panic!(
            "the owner could not make a conversation, a message and a file in sage: {objects:?}"
        );
    };
    let base = &world.sage_url;
    let send =
        |method: &'static str, path: String, token: String, body: Option<quench_serde::Value>| {
            let client = world.client.clone();
            let url = format!("{base}{path}");
            async move {
                let request = match method {
                    "GET" => client.get(&url),
                    "POST" => client.post(&url),
                    "DELETE" => client.delete(&url),
                    other => panic!("method {other}"),
                }
                .bearer_auth(token);
                let request = match body {
                    Some(b) => request.json(&b),
                    None => request,
                };
                let response = request.send().await.expect("sage request");
                (
                    response.status().as_u16(),
                    response.text().await.unwrap_or_default(),
                )
            }
        };

    // The owner sees all of it.
    for path in [
        format!("/api/v1/conversations/{conversation}"),
        format!("/api/v1/files/{file}"),
    ] {
        let (status, _) = send("GET", path.clone(), owner.clone(), None).await;
        assert_eq!(
            status, 200,
            "the owner's own {path} was answered {status}: the check below would prove nothing"
        );
    }

    // Another person is refused every one of them, and learns nothing from the answer.
    let attempts: Vec<(&'static str, String, Option<quench_serde::Value>)> = vec![
        ("GET", format!("/api/v1/conversations/{conversation}"), None),
        (
            "POST",
            format!("/api/v1/conversations/{conversation}/resume"),
            Some(quench_serde::json!({})),
        ),
        (
            "POST",
            format!("/api/v1/conversations/{conversation}/switch"),
            Some(quench_serde::json!({"target_message_id": message})),
        ),
        (
            "POST",
            format!("/api/v1/messages/{message}/edit"),
            Some(quench_serde::json!({"content": "tampered"})),
        ),
        ("GET", format!("/api/v1/files/{file}"), None),
        ("GET", format!("/api/v1/files/{file}/download"), None),
        ("GET", format!("/api/v1/files/{file}/chunks"), None),
        ("POST", format!("/api/v1/files/{file}/reprocess"), None),
        ("DELETE", format!("/api/v1/files/{file}"), None),
        (
            "DELETE",
            format!("/api/v1/conversations/{conversation}"),
            None,
        ),
    ];
    let mut leaks = Vec::new();
    for (method, path, body) in attempts {
        let (status, reply) = send(method, path.clone(), other.clone(), body).await;
        if (200..400).contains(&status) || reply.contains("seed") {
            leaks.push(format!("{method} {path} -> {status}"));
        }
    }
    let (_, listed) = send(
        "GET",
        "/api/v1/conversations".to_string(),
        other.clone(),
        None,
    )
    .await;
    if listed.contains(conversation.as_str()) {
        leaks.push("GET /api/v1/conversations lists the owner's conversation".to_string());
    }
    assert!(
        leaks.is_empty(),
        "IDOR in sage: another person could reach what is not theirs:\n{}",
        leaks.join("\n")
    );

    // And what was refused did not happen.
    for path in [
        format!("/api/v1/conversations/{conversation}"),
        format!("/api/v1/files/{file}"),
    ] {
        let (status, _) = send("GET", path.clone(), owner.clone(), None).await;
        assert_eq!(
            status, 200,
            "the owner's {path} is gone or changed after another person's refused requests ({status})"
        );
    }
}

/// The one account the fuzzer may do anything to. Without it the user list's first member, `admin`, is what every
/// `{username}` route would be fuzzed against, and the whole suite signs in as `admin`.
async fn gatehouse_seeds(world: &ForgeWorld) -> Vec<Bind> {
    let admin = crate::world::mint_test_token(
        &world.client,
        &world.gatehouse_url,
        "sectest-fuzz",
        &["gatehouse"],
        "admin",
    )
    .await;
    let _ = world
        .client
        .post(format!("{}/api/v1/users", world.gatehouse_url))
        .bearer_auth(&admin)
        .json(&quench_serde::json!({"username": "sectest-victim", "password": "Victim-pass-1", "roles": ["user"]}))
        .send()
        .await;
    vec![Bind::value("gatehouse", "username", None, "sectest-victim")]
}

/// forge-sectest's other checks, against a service the suite started: no credentials, forged tokens, traversal, hardening
/// headers, CORS, hostile input, request smuggling and password-guessing throttles. (The nightly runs them against the
/// deployed estate; this catches a regression before the deploy.) Fails on high and above; the rest is printed.
#[then(expr = "the generic security checks of {word} find nothing serious")]
#[allow(
    clippy::needless_pass_by_ref_mut,
    clippy::too_many_lines,
    reason = "the step macro requires `&mut World`; one scan's options and its report"
)]
async fn generic_checks(world: &mut ForgeWorld, service: String) {
    let url = url_of(world, &service)
        .unwrap_or_else(|| panic!("no such service in this suite: {service}"));
    let text = std::fs::read_to_string(root().join(SPEC)).unwrap_or_else(|e| panic!("{SPEC}: {e}"));
    let spec = Spec::parse(&text).unwrap_or_else(|e| panic!("{SPEC}: {e}"));
    let mut target = Target::parse(&service, url).unwrap_or_else(|e| panic!("{url}: {e}"));
    target.insecure = true;
    let mut options = Options::new(vec![target], routes().to_vec());
    options.rps = 500;
    options.active = true;
    options.client_ip = Some("198.51.100.30".to_string());
    options.checks = [
        "auth",
        "tokens",
        "traversal",
        "headers",
        "cors",
        "injection",
        "protocol",
        "throttle",
    ]
    .map(str::to_string)
    .to_vec();
    if let Some(grants) = spec.grants_of(&service) {
        // Behind the auth wall too: the same checks as an ordinary signed-in user.
        options.token = Some(
            crate::world::mint_test_token(
                &world.client,
                &world.gatehouse_url,
                "sectest-generic",
                &[grants.audience.as_str()],
                &grants.writer,
            )
            .await,
        );
    }
    let baseline = std::fs::read_to_string(root().join("tests/forge-sectest/public-routes.txt"))
        .unwrap_or_default();
    options.baseline = Some(
        baseline
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_string)
            .collect(),
    );

    let (findings, requests) = scan::run(options)
        .await
        .unwrap_or_else(|e| panic!("the security scan of {service} did not run: {e}"));
    println!(
        "scan {service}: {requests} requests, {} notes",
        findings.len()
    );
    for note in findings.iter().filter(|f| f.severity < Severity::High) {
        println!("{note}");
    }
    let serious: Vec<String> = findings
        .iter()
        .filter(|f: &&Finding| f.severity >= Severity::High)
        .map(ToString::to_string)
        .collect();
    assert!(
        serious.is_empty(),
        "{} finding(s) at high or above for {service} in {requests} requests:\n{}",
        serious.len(),
        serious.join("\n")
    );
}

/// One message in the fuzz user's inbox, so the routes that take a message's id find a real one in the list.
async fn mailroom_seeds(world: &ForgeWorld, token: &str) -> Vec<Bind> {
    let base = &world.mailroom_url;
    // A mailbox is made on first use.
    let _ = world
        .client
        .get(format!("{base}/api/v1/me"))
        .bearer_auth(token)
        .send()
        .await;
    let _ = world
        .client
        .post(format!("{base}/api/v1/outgoing"))
        .bearer_auth(token)
        .json(&quench_serde::json!({"to": ["sectest-fuzz"], "subject": "seed", "text": "seed for the fuzzer"}))
        .send()
        .await;
    Vec::new()
}
