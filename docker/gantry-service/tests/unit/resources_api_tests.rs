use crate::support;
use gantry_service::domain::cluster::{Extra, LiveResource, Workload};
use gantry_service::domain::operation::Inventory;
use gantry_service::domain::resources::InventoryItem;
use gantry_service::domain::service::Gantry;
use http::{Method, StatusCode};
use quench_auth::domain::jwt::Claims;
use serde_json::json;

fn item(kind: &str, name: &str, ns: Option<&str>, api: &str) -> InventoryItem {
    InventoryItem {
        api_version: Some(api.into()),
        kind: kind.into(),
        name: name.into(),
        namespace: ns.map(str::to_string),
    }
}

async fn rig(auth: bool) -> support::Rig {
    let rig = support::rig(auth).await;
    rig.registry.publish("ml", "1.0.0", Some("ml"));
    rig.cluster.set_workloads(vec![Workload {
        kind: "deployment".into(),
        name: "sage".into(),
        namespace: "ml".into(),
        package: "ml".into(),
        version: Some("1.0.0".into()),
        desired: 1,
        ready: 1,
    }]);
    rig.cluster.upsert_extra(Extra {
        live: LiveResource {
            api_version: Some("v1".into()),
            kind: "ConfigMap".into(),
            name: "sage-config".into(),
            namespace: Some("ml".into()),
            package: "ml".into(),
            version: Some("1.0.0".into()),
            ready: None,
            edited: false,
        },
        yaml: "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: sage-config\n  namespace: ml\ndata:\n  LEVEL: info\n".into(),
    });
    let gantry = rig.container.get::<Gantry>().unwrap();
    gantry
        .store
        .set_inventory(
            "ml",
            &Inventory {
                version: "1.0.0".into(),
                resources: vec![
                    item("Deployment", "sage", Some("ml"), "apps/v1"),
                    item("ConfigMap", "sage-config", Some("ml"), "v1"),
                    item("Secret", "sage-secret", Some("ml"), "v1"),
                    item("Deployment", "trainer", Some("ml"), "apps/v1"),
                ],
            },
        )
        .await
        .unwrap();
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

fn with(
    claims: Claims,
    mut request: quench_http::request::Request,
) -> quench_http::request::Request {
    request.extensions_mut().insert(claims);
    request
}

fn location(response: quench_http::response::Response) -> String {
    let (parts, _) = response.into_hyper().into_parts();
    parts.headers["location"].to_str().unwrap().to_string()
}

#[tokio::test]
async fn the_api_lists_every_resource_grouped_by_package() {
    let rig = rig(false).await;
    let body = support::json_body(
        rig.app
            .call(support::req(
                Method::GET,
                "/api/v1/resources",
                &rig.container,
            ))
            .await,
    )
    .await;
    let ml = body
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["package"] == "ml")
        .unwrap();
    assert_eq!(ml["installed"], "1.0.0");
    let state = |name: &str| {
        ml["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["name"] == name)
            .unwrap()["state"]
            .clone()
    };
    assert_eq!(state("sage"), "synced");
    assert_eq!(state("trainer"), "missing");
    assert_eq!(state("sage-secret"), "hidden");
}

#[tokio::test]
async fn the_yaml_endpoint_returns_a_resource_and_refuses_a_secret() {
    let rig = rig(false).await;
    let ok = rig
        .app
        .call(support::req(
            Method::GET,
            "/api/v1/resources/yaml?package=ml&kind=ConfigMap&namespace=ml&name=sage-config",
            &rig.container,
        ))
        .await;
    assert_eq!(ok.status(), StatusCode::OK);
    assert!(
        support::json_body(ok).await["yaml"]
            .as_str()
            .unwrap()
            .contains("LEVEL: info")
    );

    let secret = rig
        .app
        .call(support::req(
            Method::GET,
            "/api/v1/resources/yaml?package=ml&kind=Secret&namespace=ml&name=sage-secret",
            &rig.container,
        ))
        .await;
    assert_eq!(secret.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn delete_and_apply_need_scale_and_edit_needs_deploy_each_on_that_package() {
    let rig = rig(true).await;
    let post = |path: &str, body: serde_json::Value, scope: &str| {
        with(
            token(scope),
            support::req_json(Method::POST, path, &rig.container, &body.to_string()),
        )
    };
    let delete = json!({"package": "ml", "kind": "Deployment", "name": "sage", "namespace": "ml", "apiVersion": "apps/v1"});

    assert_eq!(
        rig.app
            .call(post(
                "/api/v1/resources/delete",
                delete.clone(),
                "gantry:read"
            ))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        rig.app
            .call(post(
                "/api/v1/resources/delete",
                delete.clone(),
                "gantry:target:other:scale"
            ))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let created = rig
        .app
        .call(post(
            "/api/v1/resources/delete",
            delete,
            "gantry:target:ml:scale",
        ))
        .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    assert_eq!(
        support::json_body(created).await["title"],
        "Delete Deployment/sage"
    );

    let apply = json!({"package": "ml"});
    let applied = rig
        .app
        .call(post("/api/v1/resources/apply", apply, "gantry:scale"))
        .await;
    assert_eq!(applied.status(), StatusCode::CREATED);

    let edit = json!({"package": "ml", "kind": "ConfigMap", "name": "sage-config", "namespace": "ml",
        "yaml": "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: sage-config\n  namespace: ml\ndata:\n  LEVEL: debug\n"});
    assert_eq!(
        rig.app
            .call(post("/api/v1/resources/edit", edit.clone(), "gantry:scale"))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        rig.app
            .call(post("/api/v1/resources/edit", edit, "gantry:deploy"))
            .await
            .status(),
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn the_application_page_lists_resources_with_the_button_each_one_needs() {
    let rig = rig(false).await;
    let html = support::body_text(
        rig.app
            .call(support::req(Method::GET, "/ui/apps/ml", &rig.container))
            .await,
    )
    .await;

    assert!(html.contains("sage-config"));
    assert!(html.contains("trainer"));
    // A present resource can be edited and deleted (deletion asks first)...
    assert!(html.contains("/ui/resources/delete"));
    assert!(html.contains("data-confirm"));
    assert!(html.contains("/ui/resource?"), "{html}");
    // ...a missing one can be applied again, and the package can apply everything missing.
    assert!(html.contains("/ui/resources/apply"));
    assert!(html.contains("/ui/packages/ml/apply"));
    assert!(html.contains("ui_rstate_missing"));
    assert!(html.contains("ui_rstate_hidden"));
}

#[tokio::test]
async fn the_edit_page_shows_the_live_yaml_and_saving_it_runs_at_once() {
    let rig = rig(false).await;
    let page = support::body_text(
        rig.app
            .call(support::req(
                Method::GET,
                "/ui/resource?package=ml&kind=ConfigMap&name=sage-config&namespace=ml&api_version=v1",
                &rig.container,
            ))
            .await,
    )
    .await;
    assert!(page.contains("LEVEL: info"), "{page}");
    assert!(page.contains("ui_edit_save"));
    assert!(page.contains("ui_edit_note"));

    let saved = rig
        .app
        .call(support::req_form(
            Method::POST,
            "/ui/resource/edit",
            &rig.container,
            &[
                ("package", "ml"), ("kind", "ConfigMap"), ("name", "sage-config"), ("namespace", "ml"), ("api_version", "v1"),
                ("yaml", "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: sage-config\n  namespace: ml\ndata:\n  LEVEL: debug\n"),
            ],
        ))
        .await;
    assert!(location(saved).contains("/ui/operations/"));

    // Describing a different resource sends you back to the editor with the reason.
    let wrong = rig
        .app
        .call(support::req_form(
            Method::POST,
            "/ui/resource/edit",
            &rig.container,
            &[
                ("package", "ml"),
                ("kind", "ConfigMap"),
                ("name", "sage-config"),
                ("namespace", "ml"),
                ("api_version", "v1"),
                (
                    "yaml",
                    "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: other\n  namespace: ml\n",
                ),
            ],
        ))
        .await;
    let back = location(wrong);
    assert!(
        back.contains("/ui/resource?") && back.contains("error="),
        "{back}"
    );
}

#[tokio::test]
async fn delete_and_apply_from_the_page_go_straight_to_an_operation() {
    let rig = rig(false).await;
    let fields = [
        ("package", "ml"),
        ("kind", "Deployment"),
        ("name", "sage"),
        ("namespace", "ml"),
        ("api_version", "apps/v1"),
    ];
    let deleted = rig
        .app
        .call(support::req_form(
            Method::POST,
            "/ui/resources/delete",
            &rig.container,
            &fields,
        ))
        .await;
    assert!(location(deleted).contains("/ui/operations/"));

    let missing = [
        ("package", "ml"),
        ("kind", "Deployment"),
        ("name", "trainer"),
        ("namespace", "ml"),
        ("api_version", "apps/v1"),
    ];
    let applied = rig
        .app
        .call(support::req_form(
            Method::POST,
            "/ui/resources/apply",
            &rig.container,
            &missing,
        ))
        .await;
    assert!(location(applied).contains("/ui/operations/"));

    // Refusals come back as a message on the list, not as a stack trace.
    let refused = rig
        .app
        .call(support::req_form(
            Method::POST,
            "/ui/resources/delete",
            &rig.container,
            &missing,
        ))
        .await;
    assert!(location(refused).contains("error="));
}
