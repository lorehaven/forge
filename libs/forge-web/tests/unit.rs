#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use forge_web::{Client, MODULE_PATH, SCRIPT_PATH, STYLE_PATH};

fn css() -> String {
    "body{margin:0}".to_string()
}

const CLIENT: Client = Client::new("Gauge", css);

fn dir(name: &str, with_module: bool) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("forge-web-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    if with_module {
        std::fs::write(dir.join("app.wasm"), b"\0asm").unwrap();
    }
    dir
}

async fn bytes(response: quench_http::prelude::Response) -> Vec<u8> {
    response
        .into_inner()
        .body
        .collect_limited(usize::MAX)
        .await
        .expect("body")
        .to_vec()
}

async fn text(response: quench_http::prelude::Response) -> String {
    String::from_utf8_lossy(&bytes(response).await).into_owned()
}

fn header(response: &quench_http::prelude::Response, name: &str) -> String {
    response
        .headers()
        .get(name)
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default()
}

#[quench_test::test]
async fn the_page_starts_the_module_with_a_strict_policy() {
    let d = dir("page", true);
    let page = CLIENT.page_from(&d);
    assert_eq!(page.status().as_u16(), 200);
    let policy = header(&page, "content-security-policy");
    assert!(
        policy.contains("script-src 'self' 'nonce-") && policy.contains("'wasm-unsafe-eval'"),
        "{policy}"
    );
    assert!(
        policy.contains("style-src 'self'") && !policy.contains("unsafe-inline"),
        "{policy}"
    );
    assert_eq!(header(&page, "cache-control"), "no-store");
    let html = text(page).await;
    assert!(html.contains("<title>Gauge</title>"), "{html}");
    for path in [SCRIPT_PATH, STYLE_PATH, MODULE_PATH] {
        assert!(html.contains(path), "{path} missing: {html}");
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[quench_test::test]
fn every_page_has_a_nonce_of_its_own() {
    let d = dir("nonce", true);
    let policy = |r: &quench_http::prelude::Response| header(r, "content-security-policy");
    assert_ne!(policy(&CLIENT.page_from(&d)), policy(&CLIENT.page_from(&d)));
    let _ = std::fs::remove_dir_all(&d);
}

#[quench_test::test]
async fn without_the_module_the_page_says_so_and_the_module_is_not_found() {
    let d = dir("none", false);
    let page = CLIENT.page_from(&d);
    assert_eq!(page.status().as_u16(), 404);
    assert!(text(page).await.contains("not installed"));
    assert_eq!(CLIENT.module_from(&d).status().as_u16(), 404);
    let _ = std::fs::remove_dir_all(&d);
}

#[quench_test::test]
async fn the_module_is_served_as_wasm_and_the_files_have_their_types() {
    let d = dir("module", true);
    let module = CLIENT.module_from(&d);
    assert_eq!(header(&module, "content-type"), "application/wasm");
    assert_eq!(bytes(module).await, b"\0asm");
    assert!(header(&CLIENT.script(), "content-type").starts_with("text/javascript"));
    let style = CLIENT.style();
    assert!(header(&style, "content-type").starts_with("text/css"));
    assert_eq!(
        text(style).await,
        "body{margin:0}",
        "no frame, no forge style"
    );
    let framed = CLIENT.framed();
    let style = text(framed.style()).await;
    assert!(style.starts_with(forge_shell::STYLE) && style.ends_with("body{margin:0}"));
    assert!(header(&CLIENT.shell(), "content-type").starts_with("text/javascript"));
    assert_eq!(text(CLIENT.shell()).await, forge_shell::SCRIPT);
    assert_eq!(CLIENT.favicon().status(), 404, "no icon given");
    let with_icon = CLIENT.framed().icon(b"\x89PNG");
    assert_eq!(header(&with_icon.favicon(), "content-type"), "image/png");
    assert_eq!(bytes(with_icon.favicon()).await, b"\x89PNG");
    let page = text(with_icon.page_from(&d)).await;
    assert!(
        page.contains("rel=\"icon\"") && page.contains("/ui/assets/shell.js"),
        "{page}"
    );
    let _ = std::fs::remove_dir_all(&d);
}

quench_test::main!();
