//! Receiving deliveries - the one unauthenticated endpoint, so order matters:
//! find the repo (per-repo secret), verify raw-byte signature, then act.

use crate::config::ConveyorConfig;
use crate::providers::{self, Providers, TriggerEvent};
use crate::routers::api::{ApiError, json_error};
use crate::scheduler::queue::{self, NewRun};
use crate::scheduler::repos;
use crate::secrets::SecretKey;
use crate::workspace::checkout;
use quench_db::prelude::Db;
use quench_http::prelude::{Bytes, Inject, Path, Request, Response, http::StatusCode, post};
use serde_json::json;

#[post("/api/v1/webhooks/{provider}")]
pub async fn receive(
    Path(provider_name): Path<String>,
    request: RawHeaders,
    Bytes(body): Bytes,
    Inject(db): Inject<Db>,
    Inject(providers): Inject<Providers>,
    Inject(config): Inject<ConveyorConfig>,
) -> Response {
    let Some((provider_kind, provider)) = providers.by_name(&provider_name) else {
        return json_error(
            StatusCode::NOT_FOUND,
            &format!("unknown provider '{provider_name}'"),
        );
    };

    let event = match provider.parse(&request.0, &body) {
        Ok(Some(event)) => event,
        // An event conveyor has no use for: a ping, a branch deletion, a pull
        // request being labelled. Accepted so the provider does not retry it.
        Ok(None) => return ignored("nothing to build for this event"),
        Err(error) => return json_error(StatusCode::BAD_REQUEST, &error.to_string()),
    };

    if let Err(error) = validate(&event) {
        tracing::warn!("rejected a {} delivery: {error}", provider.name());
        return json_error(StatusCode::BAD_REQUEST, &error);
    }

    let repo = match repos::find_by_slug(&db, provider_kind, &event.owner, &event.name).await {
        Ok(Some(repo)) => repo,
        // Registration is explicit - a delivery for an unregistered repo isn't an invitation to build it.
        Ok(None) => {
            return json_error(
                StatusCode::NOT_FOUND,
                &format!(
                    "{}/{} is not registered with conveyor",
                    event.owner, event.name
                ),
            );
        }
        Err(error) => return ApiError::from(error).into_response(),
    };

    // The repo's own secret if it has one, otherwise the estate's - without either, deliveries go unverified.
    let key = SecretKey::from_env().ok().flatten();
    let Some(secret) = providers::webhook_secret_for(&db, key.as_ref(), &repo).await else {
        tracing::error!(
            "a {} delivery arrived for {} but no webhook secret is configured; refusing to accept unverified deliveries",
            provider.name(),
            repo.slug()
        );
        return json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "conveyor is not configured to accept webhooks for this repository",
        );
    };

    if !provider.verify(&request.0, &body, secret.as_bytes()) {
        // Deliberately terse - specifics would help an attacker guess the secret.
        tracing::warn!("rejected a {} delivery: bad signature", provider.name());
        return json_error(StatusCode::UNAUTHORIZED, "bad signature");
    }

    if !repo.enabled {
        return ignored("this repository is disabled");
    }

    if event.from_fork && !config.allow_fork_pr {
        // A fork's pipeline is outsider-written and would run with this service's own privileges.
        tracing::info!(
            "not building a fork's pull request for {}; set CONVEYOR_ALLOW_FORK_PR to allow it, and only under an isolating executor",
            repo.slug()
        );
        return ignored("pull requests from forks are not built");
    }

    let new = NewRun {
        repo_id: repo.id.clone(),
        trigger: event.trigger,
        git_ref: event.git_ref,
        sha: event.sha,
        message: event.message,
        delivery_id: Some(event.delivery_id),
        resumed_from: None,
    };

    match queue::enqueue(&db, &new).await {
        Ok(enqueued) if enqueued.is_new() => Response::json(StatusCode::ACCEPTED, enqueued.run())
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        // A retried delivery: answer 200 with the run already made, harmlessly.
        Ok(enqueued) => Response::json(StatusCode::OK, enqueued.run())
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(error) => ApiError::from(error).into_response(),
    }
}

/// Checks fields that end up in a `git` argument list - a sender-controlled
/// `--upload-pack=...` where a ref was expected runs an attacker's program.
fn validate(event: &TriggerEvent) -> Result<(), String> {
    checkout::validate_ref(&event.git_ref).map_err(|error| error.to_string())?;
    checkout::validate_sha(&event.sha).map_err(|error| error.to_string())?;

    if event.delivery_id.trim().is_empty() {
        return Err("delivery id is empty".to_string());
    }
    if event.owner.trim().is_empty() || event.name.trim().is_empty() {
        return Err("the event names no repository".to_string());
    }
    Ok(())
}

/// Accepted but not acted on - 202, since a 4xx/5xx would just get retried for nothing.
fn ignored(reason: &str) -> Response {
    tracing::debug!("delivery ignored: {reason}");
    Response::json(StatusCode::ACCEPTED, &json!({ "ignored": reason }))
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

/// Raw headers as `http::HeaderMap` - see `Actor` in `routers/api/mod.rs` for why not `Request`.
pub struct RawHeaders(pub http::HeaderMap);

#[async_trait::async_trait]
impl quench_http::prelude::FromRequest for RawHeaders {
    async fn from_request(req: &mut Request) -> Result<Self, quench_http::prelude::HttpError> {
        Ok(Self(req.headers().clone()))
    }
}

pub fn register_routes() {
    let _ = receive as fn(_, _, _, _, _, _) -> _;
}
