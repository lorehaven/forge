use gatehouse_core::{Account, Dashboard, ServiceCard};
use quench_serde::json;

#[quench_test::test]
fn the_dashboard_round_trips() {
    let dashboard = Dashboard {
        admin: true,
        services: vec![ServiceCard {
            url: "http://x/workbench".into(),
            title_key: "ui_service_workbench_title".into(),
            desc_key: "ui_service_workbench_desc".into(),
            label: "Workbench".into(),
            description: None,
            card_class: "home-card-workbench".into(),
        }],
    };
    let text = json::to_string(&dashboard);
    assert_eq!(json::from_str::<Dashboard>(&text).unwrap(), dashboard);
}

#[quench_test::test]
fn an_account_with_only_the_required_fields_reads() {
    let text = r#"{"username":"ola","has_avatar":false,"email_verified":false,"mfa_enabled":false,"has_notice_address":false,"timezones":[],"locales":["en-US"],"subscriptions":[]}"#;
    let account: Account = json::from_str(text).unwrap();
    assert_eq!(account.username, "ola");
    assert_eq!(account.display_name, None);
    assert_eq!(account.locales, vec!["en-US".to_string()]);
}
