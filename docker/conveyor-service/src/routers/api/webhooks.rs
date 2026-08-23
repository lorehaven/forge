//! Receiving deliveries.
//!
//! This is the one endpoint an unauthenticated stranger can reach, so the order
//! of operations matters more here than anywhere else in the service:
//!
//! 1. read the event, to learn which repository it claims to be about;
//! 2. find that repository, which has to have been registered;
//! 3. verify the signature - with that repository's own secret if it has one -
//!    over the raw bytes;
//! 4. refuse a fork's pipeline unless the deployment asked for it;
//! 5. queue the run, once per delivery.
//!
//! Reading comes before verifying because the secret is per repository, and the
//! body is the only thing that says which repository this is. That is safe:
//! step 1 is deserialisation and step 2 is a read, and nothing is acted on
//! until the signature checks out. Someone who names a repository whose secret
//! they do not have gets no further than step 3.
//!
//! It does mean an unauthenticated caller can learn whether a given repository
//! is registered, by telling a 404 from a 401. That is a deliberate trade for
//! per-repository secrets, and the same trade every multi-tenant webhook
//! receiver makes.
//!
//! The body is taken as `web::Bytes`. Declaring it as `web::Json<T>` would have
//! actix parse and re-serialise it, and the signature covers the exact bytes
//! that were sent.

use crate::config::ConveyorConfig;
use crate::providers::{self, Providers, TriggerEvent};
use crate::routers::api::{ApiError, json_error};
use crate::scheduler::queue::{self, NewRun};
use crate::scheduler::repos;
use crate::secrets::SecretKey;
use crate::workspace::checkout;
use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse, Responder, post, web};
use quench_db::prelude::Db;
use serde_json::json;

#[post("/webhooks/{provider}")]
pub async fn receive(
    path: web::Path<String>,
    request: HttpRequest,
    body: web::Bytes,
    db: web::Data<Db>,
    providers: web::Data<Providers>,
    config: web::Data<ConveyorConfig>,
) -> impl Responder {
    let Some((provider_kind, provider)) = providers.by_name(&path) else {
        return json_error(StatusCode::NOT_FOUND, &format!("unknown provider '{path}'"));
    };

    let event = match provider.parse(request.headers(), &body) {
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
        // Registration is explicit on purpose: conveyor runs code the
        // repository supplies, so a delivery for one nobody registered is not
        // an invitation to start building it.
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

    // The repository's own secret if it has one, otherwise the estate's.
    // Without either, every delivery would be unverified and this endpoint
    // would let anyone on the network start a build.
    let key = SecretKey::from_env().ok().flatten();
    let Some(secret) = providers::webhook_secret_for(&db, key.as_ref(), &repo).await else {
        tracing::error!(
            "a {} delivery arrived for {} but no webhook secret is configured; \
             refusing to accept unverified deliveries",
            provider.name(),
            repo.slug()
        );
        return json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "conveyor is not configured to accept webhooks for this repository",
        );
    };

    if !provider.verify(request.headers(), &body, secret.as_bytes()) {
        // Deliberately terse. Telling a caller whether the signature was
        // missing, malformed or simply wrong helps them guess at it.
        tracing::warn!("rejected a {} delivery: bad signature", provider.name());
        return json_error(StatusCode::UNAUTHORIZED, "bad signature");
    }

    if !repo.enabled {
        return ignored("this repository is disabled");
    }

    if event.from_fork && !config.allow_fork_pr {
        // The pipeline in a fork is written by someone outside the estate, and
        // under the native executor it would run with this service's
        // privileges - its database and its secret key included.
        tracing::info!(
            "not building a fork's pull request for {}; set CONVEYOR_ALLOW_FORK_PR \
             to allow it, and only under an isolating executor",
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
        Ok(enqueued) if enqueued.is_new() => HttpResponse::Accepted().json(enqueued.run()),
        // A provider retries a delivery it did not get a prompt answer for.
        // Answering 200 with the run it already made keeps the retry harmless
        // and tells the sender it landed.
        Ok(enqueued) => HttpResponse::Ok().json(enqueued.run()),
        Err(error) => ApiError::from(error).into_response(),
    }
}

/// Checks the parts of an event that end up in a `git` argument list.
///
/// The ref and the sha come from a body somebody else wrote. `git` reads a
/// leading `-` as an option, and `--upload-pack=...` where a ref was expected
/// runs a program of the sender's choosing. Checked here as well as at
/// checkout, so a bad one never reaches the queue.
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

/// Accepted, and deliberately not acted on.
///
/// 202 rather than an error: a provider retries a delivery it got a 4xx or 5xx
/// for, and there is nothing here for a retry to fix.
fn ignored(reason: &str) -> HttpResponse {
    tracing::debug!("delivery ignored: {reason}");
    HttpResponse::Accepted().json(json!({ "ignored": reason }))
}

pub fn scope() -> actix_web::Scope {
    web::scope("").service(receive)
}
