//! `handle_sse_canonical`/`handle_sse_alias` - both route to the same SSE
//! stream over a broadcast channel; this proves the framing and that a
//! subscriber sees what's sent after it subscribes.

use http_body_util::BodyExt;
use quench_http::prelude::Inject;
use std::sync::Arc;
use switchboard_service::routers::vllm::sse::{
    VllmBroadcaster, handle_sse_alias, handle_sse_canonical,
};

fn broadcaster() -> (
    Inject<VllmBroadcaster>,
    tokio::sync::broadcast::Sender<String>,
) {
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    (Inject(Arc::new(VllmBroadcaster(tx.clone()))), tx)
}

#[tokio::test]
async fn canonical_sse_route_streams_a_broadcast_message_as_an_sse_event() {
    let (broadcaster, tx) = broadcaster();
    let resp = handle_sse_canonical(broadcaster)
        .await
        .expect("sse response");
    assert!(resp.status().is_success());

    tx.send("<div>hello</div>".to_string()).unwrap();
    // Drop every sender so the never-ending broadcast stream actually
    // closes before collecting the body waits for end-of-stream.
    drop(tx);

    let hyper_resp = resp.into_hyper();
    assert_eq!(
        hyper_resp.headers().get("content-type").unwrap(),
        "text/event-stream"
    );
    let collected = hyper_resp
        .into_body()
        .collect()
        .await
        .expect("stream collects cleanly");
    let text = String::from_utf8(collected.to_bytes().to_vec()).unwrap();
    assert!(text.contains("event: vllm-instances"));
    assert!(text.contains("<div>hello</div>"));
}

#[tokio::test]
async fn sse_event_strips_newlines_from_the_html_payload() {
    let (broadcaster, tx) = broadcaster();
    let resp = handle_sse_canonical(broadcaster)
        .await
        .expect("sse response");

    tx.send("<div>\nline one\nline two\n</div>".to_string())
        .unwrap();
    drop(tx);

    let collected = resp
        .into_hyper()
        .into_body()
        .collect()
        .await
        .expect("stream collects cleanly");
    let text = String::from_utf8(collected.to_bytes().to_vec()).unwrap();
    assert!(text.contains("line oneline two"));
    assert!(!text.contains("line one\nline two"));
}

#[tokio::test]
async fn alias_sse_route_behaves_the_same_as_canonical() {
    let (broadcaster, tx) = broadcaster();
    let resp = handle_sse_alias(broadcaster).await.expect("sse response");
    assert!(resp.status().is_success());

    tx.send("alias-payload".to_string()).unwrap();
    drop(tx);

    let collected = resp
        .into_hyper()
        .into_body()
        .collect()
        .await
        .expect("stream collects cleanly");
    let text = String::from_utf8(collected.to_bytes().to_vec()).unwrap();
    assert!(text.contains("alias-payload"));
}
