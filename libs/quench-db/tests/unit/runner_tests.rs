//! Unit tests for `runner.rs`.

use quench_db::runner::*;

#[test]
fn rejects_injection_in_table_names() {
    assert!(validate_identifier("ledger table", "forge_migrations").is_ok());
    assert!(validate_identifier("ledger table", "forge migrations").is_err());
    assert!(validate_identifier("ledger table", "x; DROP TABLE y").is_err());
    assert!(validate_identifier("ledger table", "1st").is_err());
    assert!(validate_identifier("ledger table", "").is_err());
}

#[test]
fn advisory_lock_key_is_stable() {
    assert_eq!(
        advisory_lock_key("public.forge_migrations"),
        advisory_lock_key("public.forge_migrations")
    );
    assert_ne!(
        advisory_lock_key("public.forge_migrations"),
        advisory_lock_key("other.forge_migrations")
    );
}
