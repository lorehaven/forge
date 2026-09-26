//! Unit tests for `files/images.rs`.

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

#[test]
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

#[test]
fn cap_zero_strips_all_images() {
    let mut messages = vec![msg(Some(vec!["a"])), msg(Some(vec!["b"]))];
    cap_images(&mut messages, 0);
    assert!(messages.iter().all(|m| m.images.is_none()));
}

#[test]
fn cap_under_limit_is_untouched() {
    let mut messages = vec![msg(Some(vec!["a"]))];
    cap_images(&mut messages, 4);
    assert_eq!(messages[0].images.as_ref().unwrap().len(), 1);
}

#[test]
fn data_uri_format() {
    assert_eq!(
        to_data_uri("image/png", b"abc"),
        "data:image/png;base64,YWJj"
    );
}

#[test]
fn images_enabled_defaults_to_true() {
    envmnt::remove("SAGE_IMAGES_ENABLED");
    assert!(images_enabled());
}

#[test]
fn images_enabled_reads_env_override() {
    envmnt::set("SAGE_IMAGES_ENABLED", "false");
    assert!(!images_enabled());
    envmnt::remove("SAGE_IMAGES_ENABLED");
}

#[test]
fn max_images_per_request_is_zero_when_images_disabled() {
    envmnt::set("SAGE_IMAGES_ENABLED", "false");
    envmnt::set("SAGE_MAX_IMAGES_PER_REQUEST", "5");
    assert_eq!(max_images_per_request(), 0);
    envmnt::remove("SAGE_IMAGES_ENABLED");
    envmnt::remove("SAGE_MAX_IMAGES_PER_REQUEST");
}
