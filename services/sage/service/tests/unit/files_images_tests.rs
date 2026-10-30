//! Unit tests for `files/images.rs`.

use crate::env_support::env_lock;
use sage_service::clients::vllm::ChatMessage;
use sage_service::files::images::*;

fn msg(images: Option<Vec<&str>>) -> ChatMessage {
    ChatMessage {
        role: "user".to_string(),
        content: "text".to_string(),
        tool_calls: None,
        images: images.map(|v| v.into_iter().map(String::from).collect()),
    }
}

#[quench_test::test]
fn cap_keeps_newest_images() {
    let mut messages = vec![
        msg(Some(vec!["old-1", "old-2"])),
        msg(None),
        msg(Some(vec!["new-1"])),
    ];
    cap_images(&mut messages, 2);
    assert_eq!(
        messages[0].images.as_deref(),
        Some(&["old-1".to_string()][..])
    );
    assert_eq!(messages[1].images, None);
    assert_eq!(
        messages[2].images.as_deref(),
        Some(&["new-1".to_string()][..])
    );
}

#[quench_test::test]
fn cap_zero_strips_all_images() {
    let mut messages = vec![msg(Some(vec!["a"])), msg(Some(vec!["b"]))];
    cap_images(&mut messages, 0);
    assert!(messages.iter().all(|m| m.images.is_none()));
}

#[quench_test::test]
fn cap_under_limit_is_untouched() {
    let mut messages = vec![msg(Some(vec!["a"]))];
    cap_images(&mut messages, 4);
    assert_eq!(messages[0].images.as_ref().unwrap().len(), 1);
}

#[quench_test::test]
fn data_uri_format() {
    assert_eq!(
        to_data_uri("image/png", b"abc"),
        "data:image/png;base64,YWJj"
    );
}

#[quench_test::test]
fn images_enabled_defaults_to_true() {
    let _env = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    quench_core::env::remove("SAGE_IMAGES_ENABLED");
    assert!(images_enabled());
}

#[quench_test::test]
fn images_enabled_reads_env_override() {
    let _env = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    quench_core::env::set("SAGE_IMAGES_ENABLED", "false");
    assert!(!images_enabled());
    quench_core::env::remove("SAGE_IMAGES_ENABLED");
}

#[quench_test::test]
fn max_images_per_request_is_zero_when_images_disabled() {
    let _env = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    quench_core::env::set("SAGE_IMAGES_ENABLED", "false");
    quench_core::env::set("SAGE_MAX_IMAGES_PER_REQUEST", "5");
    assert_eq!(max_images_per_request(), 0);
    quench_core::env::remove("SAGE_IMAGES_ENABLED");
    quench_core::env::remove("SAGE_MAX_IMAGES_PER_REQUEST");
}
