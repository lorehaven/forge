use chrono::Utc;
use quench_db::prelude::Model;
use sqlx::types::Json;
use warehouse_service::domain::rivet::{RivetPackage, latest_of, sort_newest_first};

fn package(name: &str, version: &str, yanked: bool) -> RivetPackage {
    RivetPackage {
        id: RivetPackage::id_for(name, version),
        name: name.to_string(),
        version: version.to_string(),
        description: None,
        namespace: None,
        filename: format!("{name}-{version}.rivet"),
        size_bytes: 1,
        sha256: "0".repeat(64),
        manifest: Json(serde_json::json!({})),
        uploaded_by: "dev".to_string(),
        yanked,
        created_at: Utc::now(),
    }
}

#[test]
fn id_for_joins_name_and_version() {
    assert_eq!(RivetPackage::id_for("forge", "0.4.0+b1"), "forge@0.4.0+b1");
}

#[test]
fn table_name_is_schema_qualified() {
    assert!(RivetPackage::table_name().ends_with(".rivet_packages"));
}

#[test]
fn latest_compares_numerically_not_lexically() {
    let rows = [
        package("forge", "0.9.0", false),
        package("forge", "0.10.0", false),
        package("forge", "0.2.0", false),
    ];
    assert_eq!(latest_of(&rows).unwrap().version, "0.10.0");
}

#[test]
fn latest_skips_yanked_and_prereleases_rank_below_releases() {
    let rows = [
        package("forge", "1.0.0", true),
        package("forge", "0.9.0", false),
        package("forge", "0.9.1-rc.1", false),
    ];
    assert_eq!(latest_of(&rows).unwrap().version, "0.9.1-rc.1");

    let rows = [
        package("forge", "1.0.0-rc.1", false),
        package("forge", "1.0.0", false),
    ];
    assert_eq!(latest_of(&rows).unwrap().version, "1.0.0");
}

#[test]
fn latest_orders_build_metadata_and_is_none_when_all_yanked() {
    let rows = [
        package("forge", "0.4.0+b1", false),
        package("forge", "0.4.0+b2", false),
    ];
    assert_eq!(latest_of(&rows).unwrap().version, "0.4.0+b2");

    assert!(latest_of(&[package("forge", "1.0.0", true)]).is_none());
    assert!(latest_of(&[]).is_none());
}

#[test]
fn sorts_newest_first_with_unparseable_rows_last() {
    let mut rows = vec![
        package("forge", "0.2.0", false),
        package("forge", "not-semver", false),
        package("forge", "0.10.0", false),
    ];
    sort_newest_first(&mut rows);
    let versions: Vec<_> = rows.iter().map(|p| p.version.as_str()).collect();
    assert_eq!(versions, ["0.10.0", "0.2.0", "not-semver"]);
}
