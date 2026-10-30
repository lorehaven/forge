//! The raw-SQL row helpers against a real database (any Postgres works; the test makes its own temp table).
//! Skipped unless `SAGE_TEST_DATABASE_URL` is set.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_pg::{PgArray, Pool, ToSql};
use quench_serde::Deserialize;
use sage_service::domain::rows;

#[derive(Debug, PartialEq, Deserialize)]
struct Item {
    id: String,
    size: i64,
    note: Option<String>,
}

#[quench_test::test]
async fn rows_read_into_structs_and_tuples() {
    let Ok(url) = std::env::var("SAGE_TEST_DATABASE_URL") else {
        return;
    };
    let pool = Pool::connect(&url).await.expect("connect");
    let table = format!("sage_rows_{}", quench_core::uuid::Uuid::new_v4().simple());
    pool.batch_execute(&format!(
        "CREATE TABLE {table} (id text PRIMARY KEY, size bigint NOT NULL, note text, data bytea)"
    ))
    .await
    .unwrap();
    for (id, size, note, data) in [
        ("a", 1_i64, Some("first"), b"AB".to_vec()),
        ("b", 2, None, b"CD".to_vec()),
        ("c", 3, Some("third"), vec![0, 255]),
    ] {
        pool.execute(
            &format!("INSERT INTO {table} VALUES ($1, $2, $3, $4)"),
            &[&id as &dyn ToSql, &size, &note, &data],
        )
        .await
        .unwrap();
    }

    let all: Vec<Item> = rows::fetch_all(
        &pool,
        &format!("SELECT id, size, note FROM {table} ORDER BY id"),
        &[],
    )
    .await
    .unwrap();
    assert_eq!(
        all[1],
        Item {
            id: "b".into(),
            size: 2,
            note: None
        }
    );
    let one: Item = rows::fetch_one(
        &pool,
        &format!("SELECT id, size, note FROM {table} WHERE id = $1"),
        &[&"c" as &dyn ToSql],
    )
    .await
    .unwrap();
    assert_eq!(one.note.as_deref(), Some("third"));
    assert!(
        rows::fetch_optional::<Item>(
            &pool,
            &format!("SELECT id, size, note FROM {table} WHERE id = $1"),
            &[&"zz" as &dyn ToSql]
        )
        .await
        .unwrap()
        .is_none()
    );

    let pairs: Vec<(String, i64)> = rows::fetch_all_tuples(
        &pool,
        &format!("SELECT id, size FROM {table} ORDER BY id"),
        &[],
    )
    .await
    .unwrap();
    assert_eq!(
        pairs,
        [
            ("a".to_string(), 1),
            ("b".to_string(), 2),
            ("c".to_string(), 3)
        ]
    );
    let (data,): (Vec<u8>,) = rows::fetch_one_tuple(
        &pool,
        &format!("SELECT data FROM {table} WHERE id = 'c'"),
        &[],
    )
    .await
    .unwrap();
    assert_eq!(data, [0, 255]);
    let (count,): (i64,) = rows::fetch_one_tuple(
        &pool,
        &format!("SELECT count(*) FROM {table} WHERE id = ANY($1)"),
        &[&PgArray(vec!["a".to_string(), "c".to_string()]) as &dyn ToSql],
    )
    .await
    .unwrap();
    assert_eq!(count, 2);
    assert!(
        rows::fetch_optional_tuple::<(String,)>(
            &pool,
            &format!("SELECT id FROM {table} WHERE size > 99"),
            &[]
        )
        .await
        .unwrap()
        .is_none()
    );

    pool.batch_execute(&format!("DROP TABLE {table}"))
        .await
        .unwrap();
}

quench_test::main!();
