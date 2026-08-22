use warehouse_service::routers::feature_enabled;

#[test]
fn feature_enabled_recognizes_common_truthy_and_falsy_spellings() {
    let key = "WAREHOUSE_TEST_FEATURE_FLAG_TRUTHY";
    for value in ["1", "true", "TRUE", "yes", "on"] {
        unsafe { std::env::set_var(key, value) };
        assert!(feature_enabled(key, false), "{value} should be truthy");
    }
    for value in ["0", "false", "FALSE", "no", "off"] {
        unsafe { std::env::set_var(key, value) };
        assert!(!feature_enabled(key, true), "{value} should be falsy");
    }
    unsafe { std::env::remove_var(key) };
}

#[test]
fn feature_enabled_falls_back_to_the_default_for_unset_or_garbage_values() {
    let key = "WAREHOUSE_TEST_FEATURE_FLAG_GARBAGE";
    unsafe { std::env::remove_var(key) };
    assert!(!feature_enabled(key, false));
    assert!(feature_enabled(key, true));

    unsafe { std::env::set_var(key, "not-a-bool") };
    assert!(!feature_enabled(key, false));
    assert!(feature_enabled(key, true));
    unsafe { std::env::remove_var(key) };
}

#[test]
fn feature_enabled_trims_and_lowercases_the_value() {
    let key = "WAREHOUSE_TEST_FEATURE_FLAG_TRIM";
    unsafe { std::env::set_var(key, "  TrUe  ") };
    assert!(feature_enabled(key, false));
    unsafe { std::env::remove_var(key) };
}
