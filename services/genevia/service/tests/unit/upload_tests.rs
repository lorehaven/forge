//! Reading the `file` field of an import.

use genevia_service::import::read;
use quench_core::bytes::Bytes;
use quench_http::di::ContainerBuilder;
use quench_http::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri};
use quench_http::request::Request;

const BOUNDARY: &str = "X-GENEVIA-TEST";

async fn request(content_type: &str, body: Vec<u8>) -> Request {
    let mut headers = HeaderMap::new();
    headers.insert("content-type", HeaderValue::from_str(content_type).unwrap());
    let container = ContainerBuilder::new().build().await.unwrap();
    Request::new(
        Method::POST,
        "/api/v1/trees".parse::<Uri>().unwrap(),
        headers,
        quench_http::body::InboundBody::from_bytes(Bytes::from(body)),
        std::sync::Arc::new(container),
    )
}

fn form(field: &str, file: &str, content: &[u8]) -> Vec<u8> {
    let mut body = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{field}\"; filename=\"{file}\"\r\nContent-Type: text/plain\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(content);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    body
}

fn multipart() -> String {
    format!("multipart/form-data; boundary={BOUNDARY}")
}

#[quench_test::test]
async fn the_file_field_is_the_upload() {
    let mut req = request(
        &multipart(),
        form("file", "Rodzina.ged", b"0 HEAD\n0 TRLR\n"),
    )
    .await;
    let upload = read(&mut req).await.expect("read");
    assert_eq!(upload.name, "Rodzina.ged");
    assert_eq!(upload.text, "0 HEAD\n0 TRLR\n");
}

#[quench_test::test]
async fn a_body_without_the_file_field_is_refused() {
    let mut req = request(&multipart(), form("other", "a.ged", b"x")).await;
    let refused = read(&mut req).await.expect_err("refused");
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert!(refused.message.contains("file"), "{}", refused.message);
}

#[quench_test::test]
async fn a_file_that_is_not_utf8_is_refused_with_advice() {
    let mut req = request(&multipart(), form("file", "a.ged", &[0xFF, 0xFE, 0x00])).await;
    let refused = read(&mut req).await.expect_err("refused");
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.message.contains("UTF-8"), "{}", refused.message);
}

#[quench_test::test]
async fn a_body_that_is_not_a_form_is_refused() {
    let mut req = request("application/json", b"{}".to_vec()).await;
    let refused = read(&mut req).await.expect_err("refused");
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
}
