use warehouse_service::routers::feature_enabled;

#[test]
fn feature_enabled_recognizes_common_truthy_and_falsy_spellings() {
    let key = "WAREHOUSE_TEST_FEATURE_FLAG_TRUTHY";
    for value in ["1", "true", "TRUE", "yes", "on"] {
        quench_core::env::set(key, value);
        assert!(feature_enabled(key, false), "{value} should be truthy");
    }
    for value in ["0", "false", "FALSE", "no", "off"] {
        quench_core::env::set(key, value);
        assert!(!feature_enabled(key, true), "{value} should be falsy");
    }
    quench_core::env::remove(key);
}

#[test]
fn feature_enabled_falls_back_to_the_default_for_unset_or_garbage_values() {
    let key = "WAREHOUSE_TEST_FEATURE_FLAG_GARBAGE";
    quench_core::env::remove(key);
    assert!(!feature_enabled(key, false));
    assert!(feature_enabled(key, true));

    quench_core::env::set(key, "not-a-bool");
    assert!(!feature_enabled(key, false));
    assert!(feature_enabled(key, true));
    quench_core::env::remove(key);
}

#[test]
fn feature_enabled_trims_and_lowercases_the_value() {
    let key = "WAREHOUSE_TEST_FEATURE_FLAG_TRIM";
    quench_core::env::set(key, "  TrUe  ");
    assert!(feature_enabled(key, false));
    quench_core::env::remove(key);
}
