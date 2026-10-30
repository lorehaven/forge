use gatehouse_core::{DraftError, MfaDraft, base_of, file_name, segment};

#[quench_test::test]
fn the_code_is_needed_and_loses_its_spaces() {
    let mut draft = MfaDraft::default();
    assert_eq!(draft.validate("S").unwrap_err(), DraftError::CodeRequired);
    draft.code = " 123 456 ".into();
    let body = draft.validate("SECRET").unwrap();
    assert_eq!(
        (body.secret.as_str(), body.code.as_str()),
        ("SECRET", "123456")
    );
    assert_eq!(
        DraftError::CodeRequired.key(),
        "ui_account_mfa_code_required"
    );
}

#[quench_test::test]
fn a_file_name_is_what_follows_the_last_separator() {
    assert_eq!(file_name("C:\\fakepath\\me.png"), "me.png");
    assert_eq!(file_name("/tmp/me.png"), "me.png");
    assert_eq!(file_name("me.png"), "me.png");
    assert_eq!(file_name(""), "");
}

#[quench_test::test]
fn a_segment_is_percent_encoded() {
    assert_eq!(segment("ola.k-1_~"), "ola.k-1_~");
    assert_eq!(segment("a b/c@d+e"), "a%20b%2Fc%40d%2Be");
}

#[quench_test::test]
fn the_prefix_comes_from_the_pages_path() {
    assert_eq!(base_of("/gatehouse/ui/home"), "/gatehouse");
    assert_eq!(base_of("/ui/home"), "");
    assert_eq!(base_of("/gatehouse/ui"), "/gatehouse");
    assert_eq!(base_of("/ui"), "");
}
