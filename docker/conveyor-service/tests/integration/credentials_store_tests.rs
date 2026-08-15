//! The credential store, against a real Postgres.

use crate::support::{TEST_USER, database, register_repo, skipped};
use conveyor_service::credentials::store::{self, CredentialError, NewCredential, Scope};
use conveyor_service::domain::Provider;
use conveyor_service::scheduler::projects::{self, NewProject};
use conveyor_service::scheduler::queue;
use conveyor_service::scheduler::repos::{self, NewRepo};
use conveyor_service::secrets::crypto::SecretKey;

// 32 bytes of hex, distinct from the pipeline-secrets test key - the whole
// point of `CONVEYOR_CREDENTIAL_KEY` being its own variable.
const HEX_KEY: &str = "202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f";

fn key() -> SecretKey {
    SecretKey::parse("CONVEYOR_CREDENTIAL_KEY", HEX_KEY).expect("key")
}

fn token<'a>(name: &'a str, kind: &'a str, username: &'a str, token: &'a str) -> NewCredential<'a> {
    NewCredential {
        name,
        kind,
        username,
        token,
    }
}

async fn nested_project(
    db: &quench_db::prelude::Db,
    name: &str,
    parent_id: Option<&str>,
) -> String {
    projects::create(
        db,
        &NewProject {
            name: name.to_string(),
            parent_id: parent_id.map(str::to_string),
        },
    )
    .await
    .expect("create project")
    .id
}

async fn repo_under(
    db: &quench_db::prelude::Db,
    project_id: &str,
    name: &str,
) -> conveyor_service::domain::Repo {
    repos::create(
        db,
        &NewRepo {
            provider: Provider::Generic,
            owner: "tests".to_string(),
            name: name.to_string(),
            clone_url: "file:///nowhere".to_string(),
            default_branch: "master".to_string(),
            registered_by: TEST_USER.to_string(),
            project_id: project_id.to_string(),
        },
    )
    .await
    .expect("register the repository")
}

#[tokio::test]
async fn a_repo_credential_round_trips_through_resolve() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_repo_credential_round_trips_through_resolve");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;
    let scope = Scope::Repo(repo.id.clone());

    store::put(
        &db,
        &key(),
        &scope,
        &token(
            "GITHUB_TOKEN",
            "http_token",
            "x-access-token",
            "a-real-token",
        ),
        TEST_USER,
    )
    .await
    .expect("put");

    let resolved = store::resolve(&db, Some(&key()), &repo)
        .await
        .expect("resolve")
        .expect("a credential should resolve");

    assert_eq!(resolved.username, "x-access-token");
    assert_eq!(resolved.token, "a-real-token");
}

#[tokio::test]
async fn the_token_is_not_readable_from_the_table() {
    let Some((db, _guard)) = database().await else {
        return skipped("the_token_is_not_readable_from_the_table");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;

    store::put(
        &db,
        &key(),
        &Scope::Repo(repo.id.clone()),
        &token("TOKEN", "http_token", "bot", "a-recognisable-token"),
        TEST_USER,
    )
    .await
    .expect("put");

    let schema = queue::schema();
    let pool = queue::pool(&db).expect("pool");
    let rows: Vec<(Vec<u8>,)> = sqlx::query_as(sqlx::AssertSqlSafe(
        format!("SELECT ciphertext FROM {schema}.credentials").as_str(),
    ))
    .fetch_all(pool)
    .await
    .expect("read the raw column");

    assert_eq!(rows.len(), 1);
    let stored = String::from_utf8_lossy(&rows[0].0);
    assert!(
        !stored.contains("a-recognisable-token"),
        "the plaintext is in the table"
    );
}

#[tokio::test]
async fn writing_again_replaces_whatever_was_there() {
    let Some((db, _guard)) = database().await else {
        return skipped("writing_again_replaces_whatever_was_there");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;
    let scope = Scope::Repo(repo.id.clone());

    store::put(
        &db,
        &key(),
        &scope,
        &token("FIRST", "http_token", "bot", "first-value"),
        TEST_USER,
    )
    .await
    .expect("put");
    store::put(
        &db,
        &key(),
        &scope,
        &token("SECOND", "http_token", "other-bot", "second-value"),
        TEST_USER,
    )
    .await
    .expect("put again");

    let shown = store::show(&db, &scope)
        .await
        .expect("show")
        .expect("one row");
    assert_eq!(shown.name, "SECOND");
    assert_eq!(shown.username, "other-bot");

    let resolved = store::resolve(&db, Some(&key()), &repo)
        .await
        .expect("resolve")
        .expect("a credential");
    assert_eq!(resolved.token, "second-value");
}

#[tokio::test]
async fn a_repos_own_credential_wins_over_its_projects() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_repos_own_credential_wins_over_its_projects");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;

    store::put(
        &db,
        &key(),
        &Scope::Project(repo.project_id.clone()),
        &token("PROJECT", "http_token", "project-bot", "project-value"),
        TEST_USER,
    )
    .await
    .expect("put project");
    store::put(
        &db,
        &key(),
        &Scope::Repo(repo.id.clone()),
        &token("REPO", "http_token", "repo-bot", "repo-value"),
        TEST_USER,
    )
    .await
    .expect("put repo");

    let resolved = store::resolve(&db, Some(&key()), &repo)
        .await
        .expect("resolve")
        .expect("a credential");
    assert_eq!(resolved.username, "repo-bot");
    assert_eq!(resolved.token, "repo-value");
}

#[tokio::test]
async fn a_projects_credential_covers_a_repo_with_none_of_its_own() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_projects_credential_covers_a_repo_with_none_of_its_own");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;

    store::put(
        &db,
        &key(),
        &Scope::Project(repo.project_id.clone()),
        &token("PROJECT", "http_token", "project-bot", "project-value"),
        TEST_USER,
    )
    .await
    .expect("put");

    let resolved = store::resolve(&db, Some(&key()), &repo)
        .await
        .expect("resolve")
        .expect("a credential");
    assert_eq!(resolved.username, "project-bot");
}

#[tokio::test]
async fn a_nearer_project_wins_over_a_further_ancestor() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_nearer_project_wins_over_a_further_ancestor");
    };
    let grandparent = nested_project(&db, "grandparent", None).await;
    let parent = nested_project(&db, "parent", Some(&grandparent)).await;
    let repo = repo_under(&db, &parent, "leaf").await;

    store::put(
        &db,
        &key(),
        &Scope::Project(grandparent.clone()),
        &token("FAR", "http_token", "far-bot", "far-value"),
        TEST_USER,
    )
    .await
    .expect("put on the grandparent");
    store::put(
        &db,
        &key(),
        &Scope::Project(parent.clone()),
        &token("NEAR", "http_token", "near-bot", "near-value"),
        TEST_USER,
    )
    .await
    .expect("put on the parent");

    let resolved = store::resolve(&db, Some(&key()), &repo)
        .await
        .expect("resolve")
        .expect("a credential");
    assert_eq!(
        resolved.username, "near-bot",
        "the nearer project's credential should win"
    );
}

#[tokio::test]
async fn one_repos_credential_is_invisible_to_another() {
    let Some((db, _guard)) = database().await else {
        return skipped("one_repos_credential_is_invisible_to_another");
    };
    let alpha = register_repo(&db, "alpha", "file:///nowhere").await;
    let beta = register_repo(&db, "beta", "file:///nowhere").await;

    store::put(
        &db,
        &key(),
        &Scope::Repo(alpha.id.clone()),
        &token("TOKEN", "http_token", "bot", "alphas-token"),
        TEST_USER,
    )
    .await
    .expect("put");

    assert!(
        store::resolve(&db, Some(&key()), &beta)
            .await
            .expect("resolve")
            .is_none(),
        "beta should not see alpha's credential"
    );
}

#[tokio::test]
async fn a_repo_with_no_credential_anywhere_resolves_to_none() {
    // Not an error, unlike a pipeline secret nobody set: a repository with no
    // credential is assumed public, and the clone is attempted unauthenticated
    // exactly as it would be if this feature did not exist.
    let Some((db, _guard)) = database().await else {
        return skipped("a_repo_with_no_credential_anywhere_resolves_to_none");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;

    assert!(
        store::resolve(&db, Some(&key()), &repo)
            .await
            .expect("resolve")
            .is_none()
    );
}

#[tokio::test]
async fn resolving_without_a_key_returns_none_rather_than_an_error() {
    // A deployment that has never set CONVEYOR_CREDENTIAL_KEY builds every
    // public repository perfectly well.
    let Some((db, _guard)) = database().await else {
        return skipped("resolving_without_a_key_returns_none_rather_than_an_error");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;

    assert!(
        store::resolve(&db, None, &repo)
            .await
            .expect("resolve")
            .is_none()
    );
}

#[tokio::test]
async fn showing_never_carries_the_token() {
    let Some((db, _guard)) = database().await else {
        return skipped("showing_never_carries_the_token");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;

    store::put(
        &db,
        &key(),
        &Scope::Repo(repo.id.clone()),
        &token("TOKEN", "http_token", "bot", "a-secret-token-value"),
        TEST_USER,
    )
    .await
    .expect("put");

    let shown = store::show(&db, &Scope::Repo(repo.id.clone()))
        .await
        .expect("show")
        .expect("one row");

    assert!(!shown.preview.contains("a-secret-token-value"));
    let rendered = serde_json::to_string(&shown).expect("serialise");
    assert!(!rendered.contains("a-secret-token-value"), "{rendered}");
}

#[tokio::test]
async fn a_token_too_short_is_refused() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_token_too_short_is_refused");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;

    let error = store::put(
        &db,
        &key(),
        &Scope::Repo(repo.id.clone()),
        &token("TOKEN", "http_token", "bot", "ab"),
        TEST_USER,
    )
    .await
    .expect_err("should be refused");
    assert!(matches!(error, CredentialError::TooShort), "{error:?}");
}

#[tokio::test]
async fn material_with_a_control_character_is_refused() {
    let Some((db, _guard)) = database().await else {
        return skipped("material_with_a_control_character_is_refused");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;

    let error = store::put(
        &db,
        &key(),
        &Scope::Repo(repo.id.clone()),
        &token("TOKEN", "http_token", "bot", "a-token\nwith-a-newline"),
        TEST_USER,
    )
    .await
    .expect_err("should be refused");
    assert!(matches!(error, CredentialError::BadMaterial), "{error:?}");
}

#[tokio::test]
async fn a_name_that_is_not_a_valid_identifier_is_refused() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_name_that_is_not_a_valid_identifier_is_refused");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;

    for bad in ["", "with space", "with-dash", "1LEADING"] {
        let error = store::put(
            &db,
            &key(),
            &Scope::Repo(repo.id.clone()),
            &token(bad, "http_token", "bot", "a-real-value"),
            TEST_USER,
        )
        .await
        .expect_err("should be refused");
        assert!(matches!(error, CredentialError::BadName { .. }), "{bad:?}");
    }
}

#[tokio::test]
async fn deleting_removes_it() {
    let Some((db, _guard)) = database().await else {
        return skipped("deleting_removes_it");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;
    let scope = Scope::Repo(repo.id.clone());

    store::put(
        &db,
        &key(),
        &scope,
        &token("TOKEN", "http_token", "bot", "a-value"),
        TEST_USER,
    )
    .await
    .expect("put");

    assert!(store::delete(&db, &scope).await.expect("delete"));
    assert!(!store::delete(&db, &scope).await.expect("again"));
    assert!(store::show(&db, &scope).await.expect("show").is_none());
}

#[tokio::test]
async fn deleting_a_repository_takes_its_credential_with_it() {
    let Some((db, _guard)) = database().await else {
        return skipped("deleting_a_repository_takes_its_credential_with_it");
    };
    let repo = register_repo(&db, "one", "file:///nowhere").await;
    let scope = Scope::Repo(repo.id.clone());

    store::put(
        &db,
        &key(),
        &scope,
        &token("TOKEN", "http_token", "bot", "a-value"),
        TEST_USER,
    )
    .await
    .expect("put");

    repos::delete(&db, &repo.id)
        .await
        .expect("delete the repository");

    assert!(store::show(&db, &scope).await.expect("show").is_none());
}

#[tokio::test]
async fn list_all_returns_every_credential_across_every_scope() {
    let Some((db, _guard)) = database().await else {
        return skipped("list_all_returns_every_credential_across_every_scope");
    };
    let alpha = register_repo(&db, "alpha", "file:///nowhere").await;
    let beta = register_repo(&db, "beta", "file:///nowhere").await;

    store::put(
        &db,
        &key(),
        &Scope::Repo(alpha.id.clone()),
        &token("ALPHA", "http_token", "bot", "alpha-value"),
        TEST_USER,
    )
    .await
    .expect("put alpha");
    store::put(
        &db,
        &key(),
        &Scope::Project(beta.project_id.clone()),
        &token("BETA_PROJECT", "http_token", "bot", "beta-value"),
        TEST_USER,
    )
    .await
    .expect("put beta project");

    let all = store::list_all(&db).await.expect("list_all");
    let names: Vec<&str> = all.iter().map(|c| c.name.as_str()).collect();

    assert_eq!(all.len(), 2, "{names:?}");
    // Ordered by name, same as every other listing in this store.
    assert_eq!(names, vec!["ALPHA", "BETA_PROJECT"]);
}
