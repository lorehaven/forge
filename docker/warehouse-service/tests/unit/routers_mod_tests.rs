use warehouse_service::routers::feature_enabled;

#[test]
fn feature_enabled_recognizes_common_truthy_and_falsy_spellings() {
    let key = "WAREHOUSE_TEST_FEATURE_FLAG_TRUTHY";
    for value in ["1", "true", "TRUE", "yes", "on"] {
        envmnt::set(key, value);
        assert!(feature_enabled(key, false), "{value} should be truthy");
    }
    for value in ["0", "false", "FALSE", "no", "off"] {
        envmnt::set(key, value);
        assert!(!feature_enabled(key, true), "{value} should be falsy");
    }
    envmnt::remove(key);
}

#[test]
fn feature_enabled_falls_back_to_the_default_for_unset_or_garbage_values() {
    let key = "WAREHOUSE_TEST_FEATURE_FLAG_GARBAGE";
    envmnt::remove(key);
    assert!(!feature_enabled(key, false));
    assert!(feature_enabled(key, true));

    envmnt::set(key, "not-a-bool");
    assert!(!feature_enabled(key, false));
    assert!(feature_enabled(key, true));
    envmnt::remove(key);
}

#[test]
fn feature_enabled_trims_and_lowercases_the_value() {
    let key = "WAREHOUSE_TEST_FEATURE_FLAG_TRIM";
    envmnt::set(key, "  TrUe  ");
    assert!(feature_enabled(key, false));
    envmnt::remove(key);
}
