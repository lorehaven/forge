//! Unit tests for `lib.rs`.

use quench_config::*;

#[test]
fn test_config_loader_env() {
    unsafe {
        std::env::set_var("TEST_VAR", "test_value");
    }
    let loader = ConfigLoader::new("TEST");
    let value = loader.env_string("VAR", "default");
    assert_eq!(value, "test_value");
}

#[test]
fn test_config_loader_env_list() {
    unsafe {
        std::env::set_var("TEST_LIST", "item1, item2, item3");
    }
    let loader = ConfigLoader::new("TEST");
    let list = loader.env_list("LIST", &[]);
    assert_eq!(list.len(), 3);
    assert_eq!(list[0], "item1");
}

#[test]
fn test_config_loader_bool() {
    unsafe {
        std::env::set_var("TEST_ENABLED", "true");
    }
    let loader = ConfigLoader::new("TEST");
    assert!(loader.env_bool("ENABLED", false));

    unsafe {
        std::env::set_var("TEST_DISABLED", "false");
    }
    assert!(!loader.env_bool("DISABLED", true));
}
