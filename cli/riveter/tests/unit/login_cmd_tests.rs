use riveter::login_cmd::default_token_name;

#[quench_test::test]
fn default_token_name_mentions_the_host() {
    assert_eq!(default_token_name("forge-1"), "riveter on forge-1");
}
