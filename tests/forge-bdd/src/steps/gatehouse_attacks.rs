//! Attacks on gatehouse's sign-in flows, each a scenario that passes when the defence holds.
//!
//! The functional scenarios show a login works and a bad one fails. These ask what an attacker would: can a session id
//! be planted before sign-in, does a stolen refresh token survive its replay, does the login tell known users from
//! unknown ones (by its answer or by how long it takes), do other spellings of a name escape the lockout, does a reset
//! end the sessions it was meant to, and is the link in a reset email built from the request's `Host`.

#![allow(
    clippy::needless_pass_by_ref_mut,
    reason = "the step macro requires `&mut World` as the first parameter"
)]

use crate::steps::gatehouse::{
    SESSION_COOKIE, cookie_value, extract_link, fresh_client_ip, no_redirect_client,
};
use crate::world::ForgeWorld;
use quench_serde::{Value, json};
use quench_test_bdd::then;
use std::time::{Duration, Instant};

/// gatehouse's own public origin in this suite (`PUBLIC_BASE_URL`, see `services.rs`).
const PUBLIC_ORIGIN: &str = "http://127.0.0.1:5443";

struct Answer {
    status: u16,
    body: String,
    took: Duration,
}

impl Answer {
    fn json(&self) -> Value {
        quench_serde::json::from_str(&self.body).unwrap_or_default()
    }

    fn text(&self, key: &str) -> Option<String> {
        self.json()
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
    }
}

async fn login(world: &ForgeWorld, username: &str, password: &str) -> Answer {
    let started = Instant::now();
    let res = world
        .client
        .post(format!("{}/api/v1/auth/login", world.gatehouse_url))
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .expect("login request failed");
    let status = res.status().as_u16();
    let body = res.text().await.unwrap_or_default();
    Answer {
        status,
        body,
        took: started.elapsed(),
    }
}

async fn refresh(world: &ForgeWorld, token: &str) -> Answer {
    let started = Instant::now();
    let res = world
        .client
        .post(format!("{}/api/v1/auth/refresh", world.gatehouse_url))
        .json(&json!({ "refresh_token": token }))
        .send()
        .await
        .expect("refresh request failed");
    let status = res.status().as_u16();
    let body = res.text().await.unwrap_or_default();
    Answer {
        status,
        body,
        took: started.elapsed(),
    }
}

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times[times.len() / 2]
}

/// A session id the client picked must not become the session: otherwise whoever planted it (a link, a sibling
/// subdomain, a shared machine) is signed in the moment the victim is.
#[then(
    expr = "gatehouse does not adopt a session id the client chose for {string} with password {string}"
)]
async fn session_fixation(world: &mut ForgeWorld, username: String, password: String) {
    let planted = "planted-by-the-attacker-0123456789";
    let res = no_redirect_client()
        .post(format!("{}/ui/login", world.gatehouse_url))
        .header(
            quench_client::transport::header::COOKIE,
            format!("{SESSION_COOKIE}={planted}"),
        )
        .form(&[
            ("username", username.as_str()),
            ("password", password.as_str()),
        ])
        .send()
        .await
        .expect("login form failed");
    let issued = cookie_value(res.headers(), SESSION_COOKIE);
    assert!(
        issued.is_some(),
        "a successful sign-in set no session cookie (status {})",
        res.status()
    );
    assert_ne!(
        issued.as_deref(),
        Some(planted),
        "SESSION FIXATION: sign-in kept the session id the client sent"
    );
    let probe = no_redirect_client()
        .get(format!("{}/api/v1/auth/userinfo", world.gatehouse_url))
        .header(
            quench_client::transport::header::COOKIE,
            format!("{SESSION_COOKIE}={planted}"),
        )
        .send()
        .await
        .expect("userinfo request failed");
    assert_eq!(
        probe.status().as_u16(),
        401,
        "SESSION FIXATION: the id the attacker chose now authenticates as {username}"
    );
}

/// A refresh token that was already spent and shows up again was copied. Gatehouse accepts one recovery inside a short
/// grace window, for a client that lost the response to its own refresh (see `quench-auth`'s `session.rs`): that
/// recovery hands back the pair the first refresh produced, and only once. What it must never do is mint something
/// new, or keep answering: a thief replaying the token gets at most what the owner already holds.
///
/// Not asserted, because the design accepts it: the owner's token keeps working after the replay.
#[then(
    expr = "a spent refresh token yields nothing new when replayed for {string} with password {string}"
)]
async fn refresh_replay(world: &mut ForgeWorld, username: String, password: String) {
    let first = login(world, &username, &password).await;
    let r1 = first
        .text("refresh_token")
        .expect("login gave no refresh token");
    let second = refresh(world, &r1).await;
    assert_eq!(
        second.status, 200,
        "the first refresh failed: {}",
        second.body
    );
    let r2 = second
        .text("refresh_token")
        .expect("refresh gave no new token");
    assert_ne!(r1, r2, "the refresh token was not rotated");

    for (answered, attempt) in (1..=4).enumerate() {
        let replay = refresh(world, &r1).await;
        if replay.status != 200 {
            assert!(
                answered <= 1,
                "REPLAY NOT LIMITED: the spent token was answered {answered} times before it was refused"
            );
            return;
        }
        assert_eq!(
            replay.text("refresh_token").as_deref(),
            Some(r2.as_str()),
            "REPLAY MINTS A NEW CREDENTIAL: replay {attempt} of a spent token got a refresh token other than the one the first refresh produced"
        );
    }
    panic!("REPLAY NOT LIMITED: a spent refresh token was still answered after 4 replays");
}

/// An unknown user and a known one with a wrong password must be indistinguishable, by what is said and by how long
/// saying it takes (an early return that skips the password hash is a stopwatch for the list of accounts).
#[then(expr = "gatehouse answers an unknown user like a wrong password for {string}")]
async fn enumeration(world: &mut ForgeWorld, username: String) {
    let unknown = format!("atk-nobody-{}", fresh_client_ip().replace('.', "-"));
    let mut known_times = Vec::new();
    let mut unknown_times = Vec::new();
    let (mut known, mut other) = (None, None);
    // Fewer than the lockout threshold (5): the account must still be open for the next scenario step.
    for _ in 0..3 {
        let a = login(world, &username, "definitely-wrong-1").await;
        let b = login(world, &unknown, "definitely-wrong-1").await;
        known_times.push(a.took);
        unknown_times.push(b.took);
        known = Some(a);
        other = Some(b);
    }
    let (known, other) = (known.unwrap(), other.unwrap());
    assert_eq!(
        known.status, other.status,
        "USER ENUMERATION: a wrong password for a real user is {} but for an unknown user {}",
        known.status, other.status
    );
    let scrub = |body: &str, name: &str| body.replace(name, "<user>");
    assert_eq!(
        scrub(&known.body, &username),
        scrub(&other.body, &unknown),
        "USER ENUMERATION: the answer's body differs between a real and an unknown user"
    );
    let (k, u) = (median(known_times), median(unknown_times));
    assert!(
        !(k > u * 3 && k > u + Duration::from_millis(20)),
        "USER ENUMERATION BY TIMING: a wrong password for a real user takes {k:?} but an unknown user {u:?}"
    );
}

/// Other spellings of a name (case, padding, an invisible character) must not be a second, uncounted way at the same
/// account: if they sign in as it, their failures must lock it.
#[then(expr = "gatehouse counts failed logins across spellings of {string} with password {string}")]
async fn spellings(world: &mut ForgeWorld, username: String, password: String) {
    let variants = [
        username.to_uppercase(),
        format!(" {username}"),
        format!("{username} "),
        format!("{username}\u{200b}"),
        format!("{username}\u{0}"),
    ];
    let mut same_account = Vec::new();
    for variant in &variants {
        let answer = login(world, variant, &password).await;
        if answer.status == 200 {
            same_account.push(variant.clone());
        }
    }
    if same_account.is_empty() {
        return;
    }
    // They are the same account. Then their wrong guesses must count towards the one lockout.
    for variant in variants.iter().cycle().take(8) {
        let _ = login(world, variant, "definitely-wrong-2").await;
    }
    let after = login(world, &username, &password).await;
    assert_ne!(
        after.status, 200,
        "LOCKOUT BYPASS: {same_account:?} sign in as {username}, and 8 wrong guesses through them did not lock it"
    );
}

async fn reset_token(
    world: &ForgeWorld,
    username: &str,
    email: &str,
    headers: &[(&str, &str)],
) -> String {
    let mut request = no_redirect_client()
        .post(format!("{}/ui/forgot-password", world.gatehouse_url))
        .header("x-real-ip", fresh_client_ip())
        .form(&[("username", username)]);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    request.send().await.expect("forgot-password failed");
    let marker = format!("email(password-reset) to={email}");
    let line = crate::services::wait_for_gatehouse_log(&marker)
        .await
        .unwrap_or_else(|| panic!("no reset email logged for {email}"));
    extract_link(&line)
}

async fn reset(world: &ForgeWorld, token: &str, password: &str) {
    no_redirect_client()
        .post(format!("{}/ui/reset-password", world.gatehouse_url))
        .form(&[("token", token), ("password", password)])
        .send()
        .await
        .expect("reset-password failed");
}

/// A reset is what a person does when the account may be in someone else's hands: it must end the sessions and
/// refresh tokens already issued, kill the old password, and be good for one use.
#[then(
    expr = "resetting the password of {string} at {string} ends what was signed in and spends the link"
)]
async fn reset_ends_sessions(world: &mut ForgeWorld, username: String, email: String) {
    let old = "Atk-reset-old-1";
    let signed_in = login(world, &username, old).await;
    assert_eq!(signed_in.status, 200, "setup: {}", signed_in.body);
    let held = signed_in.text("refresh_token").expect("no refresh token");

    let link = reset_token(world, &username, &email, &[]).await;
    let token = link
        .split_once("token=")
        .expect("reset link had no token")
        .1
        .to_string();
    reset(world, &token, "Atk-reset-new-2").await;

    assert_eq!(
        login(world, &username, "Atk-reset-new-2").await.status,
        200,
        "the reset did not set the new password"
    );
    assert_ne!(
        login(world, &username, old).await.status,
        200,
        "the old password still signs in after a reset"
    );
    assert_ne!(
        refresh(world, &held).await.status,
        200,
        "SESSION SURVIVES RESET: a refresh token issued before the password reset still refreshes"
    );
    reset(world, &token, "Atk-reset-third-3").await;
    assert_ne!(
        login(world, &username, "Atk-reset-third-3").await.status,
        200,
        "RESET LINK REUSED: the same link set a password a second time"
    );
}

/// The link in an email must name gatehouse's configured address. One built from `Host` or `X-Forwarded-Host` lets an
/// attacker who can send the request point the victim's reset link at a server of theirs.
#[then(
    expr = "the reset email for {string} at {string} names gatehouse, whatever Host the request claimed"
)]
async fn reset_link_host(world: &mut ForgeWorld, username: String, email: String) {
    let link = reset_token(
        world,
        &username,
        &email,
        &[
            ("host", "evil.example"),
            ("x-forwarded-host", "evil.example"),
            ("x-forwarded-proto", "https"),
            ("forwarded", "host=evil.example;proto=https"),
        ],
    )
    .await;
    assert!(
        link.starts_with(PUBLIC_ORIGIN) && !link.contains("evil"),
        "PASSWORD RESET POISONING: the emailed link is {link}"
    );
}

/// Password spraying is one guess at each of many names, so no account ever collects the failures a lockout counts. What
/// sees it is the address: past its failures, a client is turned away before anything is checked - the right password
/// included - and told when to come back. Another address, and signing in correctly, are not touched.
#[then(
    expr = "gatehouse turns away an address that keeps failing to sign in, and signs {string} in with {string} from another"
)]
async fn spraying_is_throttled(world: &mut ForgeWorld, username: String, password: String) {
    let spray_from = "198.51.100.200";
    let attempt = |client: quench_client::transport::Client,
                   url: String,
                   user: String,
                   password: String,
                   ip: String| async move {
        let res = client
            .post(url)
            .header("x-real-ip", ip)
            .json(&json!({ "username": user, "password": password }))
            .send()
            .await
            .expect("login request failed");
        let status = res.status().as_u16();
        let retry = res
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        (status, retry)
    };
    let url = format!("{}/api/v1/auth/login", world.gatehouse_url);

    // A guess at a different name each time, from one address, in tens: more than the suite's limit (100).
    let mut limited = None;
    for batch in 0..14 {
        let mut handles = Vec::new();
        for n in 0..10 {
            let (client, url) = (world.client.clone(), url.clone());
            let user = format!("spray-{batch}-{n}");
            handles.push(tokio::spawn(attempt(
                client,
                url,
                user,
                "Spring2026!".to_string(),
                spray_from.to_string(),
            )));
        }
        for handle in handles {
            let (status, retry) = handle.await.expect("attempt");
            if status == 429 {
                limited = limited.or(retry);
            } else {
                assert_eq!(
                    status, 401,
                    "a wrong guess is a 401 until the address is over its limit"
                );
            }
        }
    }
    let retry = limited.expect("PASSWORD SPRAYING NOT THROTTLED: 140 failed sign-ins from one address were never answered 429");
    assert!(
        retry.parse::<u64>().is_ok_and(|s| (1..=600).contains(&s)),
        "the refusal must say when to come back (Retry-After), got {retry:?}"
    );

    // Over its limit the address gets nothing, the right password included.
    let (status, _) = attempt(
        world.client.clone(),
        url.clone(),
        username.clone(),
        password.clone(),
        spray_from.to_string(),
    )
    .await;
    assert_eq!(
        status, 429,
        "a blocked address was let in with the right password"
    );

    // And one address's trouble is not another's.
    let (status, _) = attempt(
        world.client.clone(),
        url,
        username,
        password,
        "198.51.100.201".to_string(),
    )
    .await;
    assert_eq!(
        status, 200,
        "a client that was never over its limit was refused"
    );
}
