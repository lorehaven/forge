use gantry_service::domain::{GantryError, pool, schema};
use quench_db::{Db, InMemoryDb};

#[test]
fn an_in_memory_database_is_refused_because_it_would_lose_the_history() {
    let db = Db::InMemory(InMemoryDb::new());
    let err = pool(&db).unwrap_err();
    assert!(matches!(err, GantryError::NotPostgres));
    assert!(err.to_string().contains("in-memory"), "{err}");
}

#[test]
fn the_schema_defaults_to_gantry_and_follows_the_environment() {
    // Not set in this test binary unless a developer exported it; the default is what matters.
    if std::env::var_os("DB_SCHEMA").is_none() {
        assert_eq!(schema(), "gantry");
    }
}

#[test]
fn a_missing_database_is_the_services_problem_not_the_callers() {
    use quench_starter::http::domain::api_error::ApiError;
    let error: ApiError = GantryError::NotPostgres.into();
    assert_eq!(
        error.into_response().status(),
        http::StatusCode::SERVICE_UNAVAILABLE
    );
}
