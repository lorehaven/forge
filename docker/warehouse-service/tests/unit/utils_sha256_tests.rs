use warehouse_service::utils::sha256::sha256_hex;

#[test]
fn hashes_the_empty_input_to_the_well_known_digest() {
    assert_eq!(
        sha256_hex(""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn hashes_a_short_string_to_the_well_known_digest() {
    assert_eq!(
        sha256_hex("abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn accepts_both_str_and_bytes() {
    assert_eq!(sha256_hex("abc"), sha256_hex(b"abc"));
}

#[test]
fn different_input_hashes_differently() {
    assert_ne!(sha256_hex("a"), sha256_hex("b"));
}
