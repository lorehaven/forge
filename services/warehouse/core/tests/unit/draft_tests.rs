use warehouse_core::draft::valid_storage_name;
use warehouse_core::{DraftError, DynamicStorage, EditStorageDraft, NewStorageDraft};

fn new_draft() -> NewStorageDraft {
    NewStorageDraft {
        name: " backups ".into(),
        owner: " dev ".into(),
        quota_gib: "1.5".into(),
        max_file_mib: String::new(),
        sync: true,
    }
}

#[quench_test::test]
fn a_new_storage_is_trimmed_and_sized_in_bytes() {
    let body = new_draft().validate().unwrap();
    assert_eq!(body.name, "backups");
    assert_eq!(body.owner, "dev");
    assert_eq!(body.quota_bytes, Some(1_610_612_736));
    assert_eq!(body.max_file_bytes, None);
    assert!(body.sync_enabled);
}

#[quench_test::test]
fn a_blank_quota_is_the_deployments_default() {
    let draft = NewStorageDraft {
        quota_gib: "  ".into(),
        ..new_draft()
    };
    assert_eq!(draft.validate().unwrap().quota_bytes, None);
}

#[quench_test::test]
fn the_name_the_owner_and_the_sizes_are_checked() {
    let bad = |draft: NewStorageDraft| draft.validate().unwrap_err();
    assert_eq!(
        bad(NewStorageDraft {
            name: "a b".into(),
            ..new_draft()
        }),
        DraftError::InvalidStorageName
    );
    assert_eq!(
        bad(NewStorageDraft {
            name: String::new(),
            ..new_draft()
        }),
        DraftError::InvalidStorageName
    );
    assert_eq!(
        bad(NewStorageDraft {
            owner: "  ".into(),
            ..new_draft()
        }),
        DraftError::OwnerRequired
    );
    assert_eq!(
        bad(NewStorageDraft {
            quota_gib: "-1".into(),
            ..new_draft()
        }),
        DraftError::InvalidQuota
    );
    assert_eq!(
        bad(NewStorageDraft {
            quota_gib: "lots".into(),
            ..new_draft()
        }),
        DraftError::InvalidQuota
    );
    assert_eq!(
        bad(NewStorageDraft {
            max_file_mib: "NaN".into(),
            ..new_draft()
        }),
        DraftError::InvalidMaxFile
    );
    assert_eq!(DraftError::InvalidQuota.key(), "api_error_invalid_quota");
}

#[quench_test::test]
fn storage_names_are_url_safe() {
    assert!(valid_storage_name("a-b_9"));
    assert!(!valid_storage_name(""));
    assert!(!valid_storage_name("a/b"));
    assert!(!valid_storage_name("a b"));
}

fn storage() -> DynamicStorage {
    DynamicStorage {
        name: "s".into(),
        owner: "dev".into(),
        max_file_bytes: Some(5 * 1024 * 1024),
        quota_bytes: 2 * 1024 * 1024 * 1024,
        used_bytes: 0,
        sync_enabled: false,
        created: "2026-10-29 10:00 UTC".into(),
    }
}

#[quench_test::test]
fn the_edit_form_opens_on_the_current_values() {
    let draft = EditStorageDraft::from_storage(&storage());
    assert_eq!(draft.quota_gib, "2.00");
    assert_eq!(draft.max_file_mib, "5.00");
    assert!(!draft.sync && !draft.clear_max_file);
    let changes = draft.validate().unwrap();
    assert_eq!(changes.quota_bytes, Some(2 * 1024 * 1024 * 1024));
    assert_eq!(changes.max_file_bytes, Some(5 * 1024 * 1024));
}

#[quench_test::test]
fn clearing_the_max_file_size_ignores_what_is_typed() {
    let draft = EditStorageDraft {
        max_file_mib: "garbage".into(),
        clear_max_file: true,
        ..EditStorageDraft::from_storage(&storage())
    };
    let changes = draft.validate().unwrap();
    assert!(changes.clear_max_file);
    assert_eq!(changes.max_file_bytes, None);
    let blank = EditStorageDraft {
        quota_gib: String::new(),
        ..EditStorageDraft::default()
    };
    assert_eq!(blank.validate().unwrap().quota_bytes, None);
}
