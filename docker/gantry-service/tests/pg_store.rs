//! `PgStore` against a real database. Skipped unless `GANTRY_TEST_DATABASE_URL` points at one with the
//! gantry module installed (`foundry-service apply -i gantry`).
use gantry_service::domain::operation::*;
use gantry_service::domain::steps::Plan;
use quench_db::prelude::Db;

#[tokio::test]
async fn pg_store_round_trip() {
    let Ok(url) = std::env::var("GANTRY_TEST_DATABASE_URL") else {
        return;
    };
    let db = Db::connect(&url).await.unwrap();
    let store = PgStore::new(db);
    let plan = Plan {
        title: "t".into(),
        summary: vec!["a".into()],
        touches: vec!["ml".into()],
        ..Plan::default()
    };
    let op = store
        .create(&NewOperation {
            kind: "k".into(),
            title: "T".into(),
            scope: format!("s{}", quench_core::uuid::Uuid::new_v4()),
            plan: plan.clone(),
            requested_by: "me".into(),
        })
        .await
        .unwrap();
    assert_eq!(op.plan, plan);
    assert!(store.get(&op.id).await.unwrap().is_some());
    assert!(!store.list(10).await.unwrap().is_empty());
    let claimed = store.claim_next().await.unwrap();
    assert!(claimed.is_some());
    store
        .finish(
            &op.id,
            State::Succeeded,
            None,
            Some("log"),
            Some(quench_serde::json!({"ok": true})),
        )
        .await
        .unwrap();
    let done = store.get(&op.id).await.unwrap().unwrap();
    assert_eq!(done.result, Some(quench_serde::json!({"ok": true})));
    store
        .set_inventory(
            "ml",
            &Inventory {
                version: "1.0.0".into(),
                resources: vec![],
            },
        )
        .await
        .unwrap();
    assert_eq!(
        store.all_inventories().await.unwrap()["ml"].version,
        "1.0.0"
    );
    store
        .set_deployment_state("d", "stopped", "me")
        .await
        .unwrap();
    assert_eq!(store.deployment_states().await.unwrap()["d"], "stopped");
    let saved = store
        .save_plan(&NewPlan {
            target: "ml".into(),
            action: "upgrade".into(),
            version: Some("1".into()),
            basis: None,
            plan,
            created_by: "me".into(),
        })
        .await
        .unwrap();
    assert!(store.claim_plan(&saved.id).await.unwrap());
    assert!(!store.claim_plan(&saved.id).await.unwrap());
    store.settle_plan(&saved.id, Some(&op.id)).await.unwrap();
    assert_eq!(
        store
            .plan(&saved.id)
            .await
            .unwrap()
            .unwrap()
            .operation_id
            .as_deref(),
        Some(op.id.as_str())
    );
}
