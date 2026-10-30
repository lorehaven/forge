use gantry_core::{DraftError, EditDraft, ResourceRef};

fn reference() -> ResourceRef {
    ResourceRef {
        package: "ml".into(),
        api_version: "v1".into(),
        kind: "ConfigMap".into(),
        name: "sage-config".into(),
        namespace: "ml".into(),
    }
}

#[quench_test::test]
fn the_yaml_becomes_the_edit_body() {
    let body = EditDraft {
        resource: reference(),
        yaml: "kind: ConfigMap\n".into(),
    }
    .validate()
    .unwrap();
    assert_eq!(body.package, "ml");
    assert_eq!(body.namespace.as_deref(), Some("ml"));
    assert_eq!(body.yaml, "kind: ConfigMap\n");
}

#[quench_test::test]
fn blank_yaml_is_refused_with_a_reason() {
    let error = EditDraft {
        resource: reference(),
        yaml: " \n ".into(),
    }
    .validate()
    .unwrap_err();
    assert_eq!(error, DraftError::EmptyYaml);
    assert!(error.message().contains("empty"));
}
