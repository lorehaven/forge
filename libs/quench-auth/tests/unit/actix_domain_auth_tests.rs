//! Unit tests for `actix/domain/auth.rs`.

use quench_auth::actix::domain::auth::*;

#[test]
fn passwords_use_argon2id_with_unique_salts() {
    let first = User::new("first".into(), "password".into(), vec![Role::User]).unwrap();
    let second = User::new("second".into(), "password".into(), vec![Role::User]).unwrap();

    assert!(first.password.starts_with("$argon2id$"));
    assert_ne!(first.password, second.password);
    assert!(first.verify_password("password"));
    assert!(!first.verify_password("wrong"));
}
