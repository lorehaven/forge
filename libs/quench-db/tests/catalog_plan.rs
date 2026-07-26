//! Catalog loading and dependency resolution - the part of the migration
//! pipeline that runs before any database is touched.

use quench_db::prelude::{Catalog, InstallRequest, MigrationPlan, PlanError};
use std::path::Path;
use tempfile::TempDir;

/// Writes `<dir>/<module>/module.toml` plus the given migration files.
fn module(root: &Path, name: &str, manifest: &str, migrations: &[(&str, &str)]) {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).expect("create module dir");
    std::fs::write(dir.join("module.toml"), manifest).expect("write manifest");
    for (file, body) in migrations {
        std::fs::write(dir.join(file), body).expect("write migration");
    }
}

fn base_catalog() -> TempDir {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path();

    module(
        root,
        "core",
        "name = \"core\"\nversion = \"0.1.0\"\n",
        &[(
            "0001-schema.toml",
            r#"
[[migrations]]
id = "0001-schema"
author = "test"
since = "0.1.0"

[[migrations.changes]]
createSchema = { name = "${schema}" }
"#,
        )],
    );

    module(
        root,
        "ext",
        "name = \"ext\"\nversion = \"0.1.0\"\nscope = \"database\"\ndefault_schema = \"public\"\n",
        &[(
            "0001-ext.toml",
            r#"
[[migrations]]
id = "0001-ext"
author = "test"
since = "0.1.0"

[[migrations.changes]]
createExtension = { name = "vector" }
"#,
        )],
    );

    module(
        root,
        "app",
        r#"
name = "app"
version = "0.2.0"
default_schema = "app"
requires = ["core@^0.1", "ext@^0.1"]

[variables]
retention = "30"
"#,
        &[
            (
                "0001-early.toml",
                r#"
[[migrations]]
id = "0001-early"
author = "test"
since = "0.1.0"

[[migrations.changes]]
sql = "CREATE TABLE ${schema}.early (id TEXT PRIMARY KEY, days INT DEFAULT ${retention});"
"#,
            ),
            (
                "0002-late.toml",
                r#"
[[migrations]]
id = "0002-late"
author = "test"
since = "0.2.0"

[[migrations.changes]]
sql = "ALTER TABLE ${schema}.early ADD COLUMN note TEXT;"
"#,
            ),
        ],
    );

    temp
}

fn plan_for(catalog: &Catalog, specs: &[&str]) -> Result<MigrationPlan, PlanError> {
    let requests: Vec<InstallRequest> = specs
        .iter()
        .map(|spec| InstallRequest::parse(spec).expect("valid spec"))
        .collect();
    MigrationPlan::resolve(catalog, &requests)
}

#[test]
fn resolves_dependencies_before_dependents() {
    let temp = base_catalog();
    let catalog = Catalog::load(temp.path()).expect("load catalog");
    let plan = plan_for(&catalog, &["app"]).expect("resolve");

    let order: Vec<&str> = plan.modules.iter().map(|m| m.module.as_str()).collect();
    assert_eq!(order.last(), Some(&"app"));
    assert!(order.contains(&"core"));
    assert!(order.contains(&"ext"));

    let core = order.iter().position(|m| *m == "core").expect("core");
    let app = order.iter().position(|m| *m == "app").expect("app");
    assert!(core < app, "core must be installed before app: {order:?}");
}

#[test]
fn dependencies_inherit_the_dependent_schema() {
    let temp = base_catalog();
    let catalog = Catalog::load(temp.path()).expect("load catalog");
    let plan = plan_for(&catalog, &["app:tenant_a", "app:tenant_b"]).expect("resolve");

    let core_schemas: Vec<&str> = plan
        .modules
        .iter()
        .filter(|m| m.module == "core")
        .map(|m| m.schema.as_str())
        .collect();
    assert_eq!(core_schemas, vec!["tenant_a", "tenant_b"]);

    // Database-scoped modules resolve to exactly one instance regardless of how
    // many schemas pull them in.
    let ext_instances = plan.modules.iter().filter(|m| m.module == "ext").count();
    assert_eq!(ext_instances, 1);
}

#[test]
fn version_gates_which_migrations_are_planned() {
    let temp = base_catalog();
    let catalog = Catalog::load(temp.path()).expect("load catalog");

    let pinned = plan_for(&catalog, &["app@0.1.0"]).expect("resolve pinned");
    let app_ids: Vec<&str> = pinned
        .migrations
        .iter()
        .filter(|m| m.module == "app")
        .map(|m| m.migration_id.as_str())
        .collect();
    assert_eq!(app_ids, vec!["0001-early"]);

    let latest = plan_for(&catalog, &["app"]).expect("resolve latest");
    let app_ids: Vec<&str> = latest
        .migrations
        .iter()
        .filter(|m| m.module == "app")
        .map(|m| m.migration_id.as_str())
        .collect();
    assert_eq!(app_ids, vec!["0001-early", "0002-late"]);
}

#[test]
fn renders_schema_and_module_variables() {
    let temp = base_catalog();
    let catalog = Catalog::load(temp.path()).expect("load catalog");
    let plan = plan_for(&catalog, &["app:custom"]).expect("resolve");

    let early = plan
        .migrations
        .iter()
        .find(|m| m.migration_id == "0001-early")
        .expect("planned migration");
    assert!(early.statements[0].contains("custom.early"));
    assert!(early.statements[0].contains("DEFAULT 30"));
}

#[test]
fn plans_are_reproducible() {
    let temp = base_catalog();
    let catalog = Catalog::load(temp.path()).expect("load catalog");

    let first = plan_for(&catalog, &["app"]).expect("first");
    let second = plan_for(&catalog, &["app"]).expect("second");

    let ids = |plan: &MigrationPlan| -> Vec<String> {
        plan.migrations.iter().map(|m| m.id.clone()).collect()
    };
    let sums = |plan: &MigrationPlan| -> Vec<String> {
        plan.migrations.iter().map(|m| m.checksum.clone()).collect()
    };
    assert_eq!(ids(&first), ids(&second));
    assert_eq!(sums(&first), sums(&second));
}

#[test]
fn rejects_a_version_the_catalog_does_not_define() {
    let temp = base_catalog();
    let catalog = Catalog::load(temp.path()).expect("load catalog");

    let error = plan_for(&catalog, &["app@9.0.0"]).expect_err("should fail");
    assert!(
        matches!(error, PlanError::VersionNotInCatalog { .. }),
        "unexpected error: {error}"
    );
}

#[test]
fn rejects_an_unknown_module() {
    let temp = base_catalog();
    let catalog = Catalog::load(temp.path()).expect("load catalog");

    let error = plan_for(&catalog, &["nope"]).expect_err("should fail");
    assert!(error.to_string().contains("unknown module"), "{error}");
}

#[test]
fn rejects_a_pinned_version_that_breaks_a_requirement() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path();
    module(root, "core", "name = \"core\"\nversion = \"0.2.0\"\n", &[]);
    module(
        root,
        "app",
        "name = \"app\"\nversion = \"0.1.0\"\nrequires = [\"core@^0.2\"]\n",
        &[],
    );
    let catalog = Catalog::load(root).expect("load catalog");

    // core is pinned below what app requires, in the schema app pulls it into.
    let error = plan_for(&catalog, &["app:app", "core@0.1.0:app"]).expect_err("should fail");
    assert!(
        error.to_string().contains("requires"),
        "unexpected error: {error}"
    );
}

#[test]
fn detects_dependency_cycles() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path();
    module(
        root,
        "a",
        "name = \"a\"\nversion = \"0.1.0\"\nrequires = [\"b@^0.1\"]\n",
        &[],
    );
    module(
        root,
        "b",
        "name = \"b\"\nversion = \"0.1.0\"\nrequires = [\"a@^0.1\"]\n",
        &[],
    );
    let catalog = Catalog::load(root).expect("load catalog");

    let error = plan_for(&catalog, &["a"]).expect_err("should fail");
    assert!(
        matches!(error, PlanError::DependencyCycle { .. }),
        "unexpected error: {error}"
    );
}

#[test]
fn rejects_unknown_variables_in_migrations() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path();
    module(
        root,
        "broken",
        "name = \"broken\"\nversion = \"0.1.0\"\n",
        &[(
            "0001-bad.toml",
            r#"
[[migrations]]
id = "0001-bad"
author = "test"

[[migrations.changes]]
sql = "CREATE TABLE ${nope}.t (id TEXT);"
"#,
        )],
    );
    let catalog = Catalog::load(root).expect("load catalog");

    let error = plan_for(&catalog, &["broken"]).expect_err("should fail");
    assert!(error.to_string().contains("nope"), "unexpected: {error}");
}

#[test]
fn rejects_a_migration_tagged_ahead_of_its_module() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path();
    module(
        root,
        "ahead",
        "name = \"ahead\"\nversion = \"0.1.0\"\n",
        &[(
            "0001-future.toml",
            r#"
[[migrations]]
id = "0001-future"
author = "test"
since = "0.9.0"

[[migrations.changes]]
sql = "SELECT 1;"
"#,
        )],
    );

    let error = Catalog::load(root).expect_err("should fail");
    assert!(
        error.to_string().contains("ahead of"),
        "unexpected: {error}"
    );
}

#[tokio::test]
async fn find_by_rejects_columns_the_model_does_not_declare() {
    use quench_db::prelude::{Crud, Db, Model};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
    struct Row {
        id: String,
        owner: String,
    }

    impl Model for Row {
        fn table_name() -> String {
            "rows".to_string()
        }
        fn columns() -> Vec<&'static str> {
            vec!["id", "owner"]
        }
    }

    let repo = Db::InMemory(quench_db::InMemoryDb::new()).repository::<Row>();
    repo.create(&Row {
        id: "1".to_string(),
        owner: "alice".to_string(),
    })
    .await
    .expect("create");

    assert_eq!(repo.find_by("owner", "alice").await.expect("find").len(), 1);
    assert_eq!(repo.find_by("owner", "bob").await.expect("find").len(), 0);

    let error = repo
        .find_by("owner; DROP TABLE rows", "alice")
        .await
        .expect_err("unknown column must be rejected");
    assert!(error.to_string().contains("unknown column"), "{error}");
}

#[test]
fn parses_install_specs() {
    let spec = InstallRequest::parse("sage@0.1.9:sage_test").expect("parse");
    assert_eq!(spec.module, "sage");
    assert_eq!(
        spec.version.map(|v| v.to_string()).as_deref(),
        Some("0.1.9")
    );
    assert_eq!(spec.schema.as_deref(), Some("sage_test"));

    let spec = InstallRequest::parse("sage").expect("parse");
    assert!(spec.version.is_none());
    assert!(spec.schema.is_none());

    assert!(InstallRequest::parse("sage@not-a-version").is_err());
}
