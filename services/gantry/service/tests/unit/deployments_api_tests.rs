use crate::support;
use gantry_service::domain::cluster::Workload;
use quench_auth::domain::jwt::Claims;
use quench_http::http::{Method, StatusCode};
use quench_serde::json;

fn workload(name: &str, desired: i32) -> Workload {
    Workload {
        kind: "deployment".into(),
        name: name.into(),
        namespace: "ml".into(),
        package: "ml".into(),
        version: Some("1.0.0".into()),
        desired,
        ready: desired,
    }
}

async fn ml(auth: bool) -> support::Rig {
    let rig = support::rig(auth).await;
    rig.registry.publish_with(
        "ml",
        "1.0.0",
        Some("ml"),
        json!({"deployment": [
            {"name": "inference", "resources": ["deployment/sage"], "conflicts_with": ["training"]},
            {"name": "training", "resources": ["deployment/trainer"], "default": "stopped"},
        ]}),
    );
    rig.cluster
        .set_workloads(vec![workload("sage", 1), workload("trainer", 0)]);
    rig
}

fn token(scope: &str) -> Claims {
    Claims::for_audiences(
        "ana".to_string(),
        vec!["gantry".to_string()],
        scope.to_string(),
        None,
        3600,
    )
}

#[quench_test::test]
async fn the_api_lists_deployments_with_their_state() {
    let rig = ml(false).await;
    let body = support::json_body(
        rig.app
            .call(support::req(
                Method::GET,
                "/api/v1/deployments",
                &rig.container,
            ))
            .await,
    )
    .await;
    let inference = body
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == "inference")
        .unwrap();
    assert_eq!(inference["observed"], "running");
    assert_eq!(inference["desired"], "running");
    let training = body
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == "training")
        .unwrap();
    assert_eq!(training["observed"], "stopped");
    assert_eq!(training["conflicting_running"], json!(["inference"]));
}

#[quench_test::test]
async fn starting_b_over_the_api_is_a_swap_plan_that_needs_activate_to_confirm() {
    let rig = ml(true).await;
    let mut request = support::req_json(
        Method::POST,
        "/api/v1/deployments/training/plan",
        &rig.container,
        r#"{"action":"start"}"#,
    );
    request.extensions_mut().insert(token("gantry:read"));
    let response = rig.app.call(request).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let plan = support::json_body(response).await;
    assert_eq!(plan["action"], "swap");
    let id = plan["id"].as_str().unwrap().to_string();

    let confirm = |scope: &str| {
        let mut request = support::req_json(
            Method::POST,
            &format!("/api/v1/plans/{id}/confirm"),
            &rig.container,
            "{}",
        );
        request.extensions_mut().insert(token(scope));
        request
    };
    // Starting and stopping alone is `scale`; a swap stops something else, which is `activate`.
    assert_eq!(
        rig.app.call(confirm("gantry:scale")).await.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        rig.app.call(confirm("gantry:activate")).await.status(),
        StatusCode::CREATED
    );
}

#[quench_test::test]
async fn a_bad_action_or_an_already_stopped_deployment_is_refused() {
    let rig = ml(false).await;
    let post = |name: &str, body: &str| {
        support::req_json(
            Method::POST,
            &format!("/api/v1/deployments/{name}/plan"),
            &rig.container,
            body,
        )
    };
    assert_eq!(
        rig.app
            .call(post("training", r#"{"action":"explode"}"#))
            .await
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        rig.app
            .call(post("training", r#"{"action":"stop"}"#))
            .await
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        rig.app
            .call(post("nope", r#"{"action":"stop"}"#))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[quench_test::test]
async fn the_client_draws_start_on_a_stopped_deployment_and_stop_on_a_running_one() {
    use quench_serde::Deserialize;
    let rig = ml(false).await;
    let body = support::json_body(
        rig.app
            .call(support::req(
                Method::GET,
                "/api/v1/deployments",
                &rig.container,
            ))
            .await,
    )
    .await;
    let views: Vec<gantry_core::DeploymentView> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|d| gantry_core::DeploymentView::deserialize(d).unwrap())
        .collect();
    let find = |name: &str| views.iter().find(|d| d.name == name).unwrap();
    assert!(find("training").can_start() && !find("training").can_stop());
    assert!(find("inference").can_stop() && !find("inference").can_start());
    assert_eq!(find("inference").conflicts_with, ["training"]);
    assert!(find("training").declared && find("training").target == "ml");
}

#[quench_test::test]
async fn starting_from_the_client_is_a_plan_confirmed_at_once_and_runs_the_swap() {
    let rig = ml(false).await;
    let plan = support::json_body(
        rig.app
            .call(support::req_json(
                Method::POST,
                "/api/v1/deployments/training/plan",
                &rig.container,
                r#"{"action":"start","stop":[]}"#,
            ))
            .await,
    )
    .await;
    let id = plan["id"].as_str().unwrap().to_string();
    let confirmed = rig
        .app
        .call(support::req_json(
            Method::POST,
            &format!("/api/v1/plans/{id}/confirm"),
            &rig.container,
            "{}",
        ))
        .await;
    assert_eq!(confirmed.status(), StatusCode::CREATED);
    let operation = support::json_body(confirmed).await;
    assert_eq!(operation["title"], "Swap to training");
    let read = support::json_body(
        rig.app
            .call(support::req(
                Method::GET,
                &format!("/api/v1/operations/{}", operation["id"].as_str().unwrap()),
                &rig.container,
            ))
            .await,
    )
    .await;
    assert!(
        read["step_lines"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l == "delete deployment.apps/sage"),
        "{read}"
    );

    // Starting what is running is refused with the reason, not run.
    let refused = rig
        .app
        .call(support::req_json(
            Method::POST,
            "/api/v1/deployments/inference/plan",
            &rig.container,
            r#"{"action":"start","stop":[]}"#,
        ))
        .await;
    assert_eq!(refused.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(support::body_text(refused).await.contains("error"));
}
