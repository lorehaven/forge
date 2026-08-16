//! The assignee picker's user list, against a real Postgres.

use crate::support::{database, skipped};
use quench_db::prelude::Database;
use workbench_service::domain::realm_users;

#[tokio::test]
async fn a_user_with_a_display_name_is_labeled_by_it_not_the_username() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_user_with_a_display_name_is_labeled_by_it_not_the_username");
    };

    db.execute(
        "INSERT INTO auth.users (username, password, roles, display_name) \
         VALUES ('realm-users-named', 'x', '[]'::jsonb, 'Named Person') \
         ON CONFLICT (username) DO UPDATE SET display_name = EXCLUDED.display_name",
    )
    .await
    .expect("seed a named user");

    let users = realm_users::list_users(&db).await.expect("list users");
    let named = users
        .iter()
        .find(|user| user.username == "realm-users-named")
        .expect("the seeded user is listed");

    assert_eq!(named.label(), "Named Person");
}

#[tokio::test]
async fn a_user_with_no_display_name_falls_back_to_their_username() {
    let Some((db, _guard)) = database().await else {
        return skipped("a_user_with_no_display_name_falls_back_to_their_username");
    };

    db.execute(
        "INSERT INTO auth.users (username, password, roles) \
         VALUES ('realm-users-unnamed', 'x', '[]'::jsonb) \
         ON CONFLICT (username) DO UPDATE SET display_name = NULL",
    )
    .await
    .expect("seed an unnamed user");

    let users = realm_users::list_users(&db).await.expect("list users");
    let unnamed = users
        .iter()
        .find(|user| user.username == "realm-users-unnamed")
        .expect("the seeded user is listed");

    assert_eq!(unnamed.label(), "realm-users-unnamed");
}
