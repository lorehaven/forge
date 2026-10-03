//! Starting and stopping a deployment. They are shown on the home page, under the package that declares them.

use crate::domain::deployments::{self, Request};
use crate::domain::service::Gantry;
use crate::routers::api::authz;
use crate::routers::ui::common::{ActorOrRedirect, redirect};
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{Form, Inject, Path, Response, post};
use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct StartForm {
    #[serde(default)]
    pub stop: String,
}

/// Start or stop, now. What it asks of the cluster is checked against what the caller may do first.
async fn run(
    name: &str,
    request: Request,
    claims: &crate::routers::ui::common::Claims,
    gantry: &Gantry,
    config: &JwtConfig,
) -> Response {
    let prepared = match deployments::prepare(gantry, name, &request).await {
        Ok(prepared) => prepared,
        Err(error) => return redirect("/home", Some(("error", &error.to_string()))),
    };
    let allowed = prepared.plan.touches.iter().all(|package| {
        authz::can_on_target(Some(claims), config, package, prepared.action.permission())
    });
    if !allowed {
        return redirect("/home", Some(("error", "not allowed")));
    }
    match prepared.run(gantry, &claims.sub).await {
        Ok(operation) => redirect(&format!("/operations/{}", operation.id), None),
        Err(error) => redirect("/home", Some(("error", &error.to_string()))),
    }
}

#[post("/ui/deployments/{name}/start")]
pub(super) async fn start(
    Path(name): Path<String>,
    actor: ActorOrRedirect,
    Inject(gantry): Inject<Gantry>,
    Inject(config): Inject<JwtConfig>,
    Form(form): Form<StartForm>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    let also_stop = if form.stop.trim().is_empty() {
        Vec::new()
    } else {
        vec![form.stop.trim().to_string()]
    };
    run(
        &name,
        Request::Start { also_stop },
        &claims,
        &gantry,
        &config,
    )
    .await
}

#[post("/ui/deployments/{name}/stop")]
pub(super) async fn stop(
    Path(name): Path<String>,
    actor: ActorOrRedirect,
    Inject(gantry): Inject<Gantry>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    run(&name, Request::Stop, &claims, &gantry, &config).await
}

pub(super) fn register_routes() {
    let _ = start as fn(_, _, _, _, _) -> _;
    let _ = stop as fn(_, _, _, _) -> _;
}
