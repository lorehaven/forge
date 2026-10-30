use crate::support;
use gantry_service::domain::cluster::{Extra, LiveResource, Workload};
use gantry_service::domain::operation::Inventory;
use gantry_service::domain::resources::InventoryItem;
use gantry_service::domain::service::Gantry;
use quench_auth::domain::jwt::Claims;
use quench_http::http::{Method, StatusCode};
use quench_serde::json;

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
            owned: false,
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
async fn delete_and_apply_need_scale_and_edit_needs_deploy_each_on_that_package() {
    let rig = rig(true).await;
    let post = |path: &str, body: quench_serde::Value, scope: &str| {
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

/// What the client sends for the buttons of the package page: the bodies of `gantry-core`, as they are written.
#[quench_test::test]
async fn the_bodies_the_client_sends_delete_apply_and_edit_a_resource() {
    use gantry_core::{PackageBody, ResourceRef};
    let rig = rig(false).await;
    let resource = |kind: &str, name: &str, api: &str| ResourceRef {
        package: "ml".into(),
        api_version: api.into(),
        kind: kind.into(),
        name: name.into(),
        namespace: "ml".into(),
    };
    let post =
        |path: &str, body: String| support::req_json(Method::POST, path, &rig.container, &body);

    let sage = resource("Deployment", "sage", "apps/v1");
    let deleted = rig
        .app
        .call(post(
            "/api/v1/resources/delete",
            json::to_string(&sage.body()),
        ))
        .await;
    assert_eq!(deleted.status(), StatusCode::CREATED);
    assert!(support::json_body(deleted).await["id"].is_string());

    let trainer = resource("Deployment", "trainer", "apps/v1");
    let applied = rig
        .app
        .call(post(
            "/api/v1/resources/apply",
            json::to_string(&trainer.body()),
        ))
        .await;
    assert_eq!(applied.status(), StatusCode::CREATED);
    let everything = rig
        .app
        .call(post(
            "/api/v1/resources/apply",
            json::to_string(&PackageBody {
                package: "ml".into(),
            }),
        ))
        .await;
    assert_eq!(everything.status(), StatusCode::CREATED);
    let refreshed = rig
        .app
        .call(post(
            "/api/v1/resources/refresh",
            json::to_string(&PackageBody {
                package: "ml".into(),
            }),
        ))
        .await;
    assert_eq!(refreshed.status(), StatusCode::CREATED);

    // Refusals come back as a message the page can show, not as a stack trace.
    let refused = rig
        .app
        .call(post(
            "/api/v1/resources/delete",
            json::to_string(&trainer.body()),
        ))
        .await;
    assert_eq!(refused.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(support::json_body(refused).await["error"].is_string());

    let config = resource("ConfigMap", "sage-config", "v1");
    let saved = rig
        .app
        .call(post(
            "/api/v1/resources/edit",
            json::to_string(&config.edit_body(
                "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: sage-config\n  namespace: ml\ndata:\n  LEVEL: debug\n".into(),
            )),
        ))
        .await;
    assert_eq!(saved.status(), StatusCode::CREATED);

    // Describing a different resource is refused, with the reason.
    let wrong = rig
        .app
        .call(post(
            "/api/v1/resources/edit",
            json::to_string(
                &config.edit_body(
                    "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: other\n  namespace: ml\n"
                        .into(),
                ),
            ),
        ))
        .await;
    assert_eq!(wrong.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(support::json_body(wrong).await["error"].is_string());
}
