//! `handle_sse_canonical`/`handle_sse_alias` - both route to the same SSE
//! stream over a broadcast channel; this proves the framing and that a
//! subscriber sees what's sent after it subscribes.

use actix_web::web::Data;
use actix_web::{App, test};
use switchboard_service::routers::vllm::sse::{
    VllmBroadcaster, handle_sse_alias, handle_sse_canonical,
};

fn broadcaster() -> (
    Data<VllmBroadcaster>,
    tokio::sync::broadcast::Sender<String>,
) {
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    (Data::new(VllmBroadcaster(tx.clone())), tx)
}

// The response body is a live `BroadcastStream` that only ends once every
// `Sender` is gone (the `App`'s own `Data<VllmBroadcaster>` clone included) -
// `test::read_body` reads to stream end, so it would hang forever against a
// channel with a sender still alive. Drop the `App` (which owns the other
// clone) and the local `tx` before reading, so the channel actually closes.

#[actix_web::test]
async fn canonical_sse_route_streams_a_broadcast_message_as_an_sse_event() {
    let (data, tx) = broadcaster();
    let app = test::init_service(App::new().app_data(data).service(handle_sse_canonical)).await;

    let req = test::TestRequest::get().uri("/sse").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/event-stream"
    );

    tx.send("<div>hello</div>".to_string()).unwrap();
    drop(app);
    drop(tx);
    let body = test::read_body(resp).await;
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("event: vllm-instances"));
    assert!(text.contains("<div>hello</div>"));
}

#[actix_web::test]
async fn sse_event_strips_newlines_from_the_html_payload() {
    let (data, tx) = broadcaster();
    let app = test::init_service(App::new().app_data(data).service(handle_sse_canonical)).await;

    let req = test::TestRequest::get().uri("/sse").to_request();
    let resp = test::call_service(&app, req).await;

    tx.send("<div>\nline one\nline two\n</div>".to_string())
        .unwrap();
    drop(app);
    drop(tx);
    let body = test::read_body(resp).await;
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("line oneline two"));
    assert!(!text.contains("line one\nline two"));
}

#[actix_web::test]
async fn alias_sse_route_behaves_the_same_as_canonical() {
    let (data, tx) = broadcaster();
    let app = test::init_service(App::new().app_data(data).service(handle_sse_alias)).await;

    let req = test::TestRequest::get().uri("/instances/sse").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    tx.send("alias-payload".to_string()).unwrap();
    drop(app);
    drop(tx);
    let body = test::read_body(resp).await;
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("alias-payload"));
}
