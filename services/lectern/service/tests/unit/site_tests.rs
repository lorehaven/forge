use crate::support::MemorySource;
use lectern_service::library::Library;
use lectern_service::site::Site;
use quench_core::bytes::Bytes;
use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri};
use quench_http::request::Request;
use std::sync::Arc;

async fn site(base: &str) -> (Site, Arc<Library>) {
    let source = MemorySource::default();
    source.release(
        "forge",
        "7",
        &[
            ("index.html", r#"<a href="/guides/">guides</a>"#),
            ("guides/index.html", "guides"),
            ("guides/a.html", "a"),
            ("assets/app.css", "body{}"),
            ("404.html", "the book's own missing page"),
        ],
        None,
    );
    let library = Arc::new(Library::new(base));
    library.refresh(&source).await.unwrap();
    (Site::new(library.clone(), base), library)
}

async fn get(
    site: &Site,
    method: Method,
    uri: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, HeaderMap, String) {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let mut map = HeaderMap::new();
    for (name, value) in headers {
        map.insert(*name, HeaderValue::from_str(value).unwrap());
    }
    let request = Request::new(
        method,
        uri.parse::<Uri>().unwrap(),
        map,
        InboundBody::from_bytes(Bytes::new()),
        container,
    );
    let response = site.call(request).await;
    let status = response.status();
    let parts = response.into_inner();
    let body = parts.body.collect_limited(usize::MAX).await.unwrap();
    (
        status,
        parts.headers,
        String::from_utf8_lossy(&body).into_owned(),
    )
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
}

#[quench_test::test]
async fn the_front_page_lists_the_books_in_the_frame_of_the_other_services() {
    let (site, _) = site("/").await;
    let (status, headers, body) = get(&site, Method::GET, "/", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(header(&headers, "content-type").starts_with("text/html"));
    assert!(header(&headers, "content-security-policy").contains("script-src 'self'"));
    // forge's top bar and card, the book linked and described
    assert!(body.contains(r#"<header class="fui-topbar">"#), "{body}");
    assert!(
        body.contains(r#"href="/forge/""#) && body.contains("FORGE"),
        "{body}"
    );
    assert!(body.contains("Release 7 · 5 pages"), "{body}");
    // the style and script are forge's, from this service
    assert!(
        body.contains(r#"href="/ui/assets/style.css""#)
            && body.contains(r#"src="/ui/assets/shell.js""#)
    );
    // nobody signs in: no account menu, no dashboard button, no login link
    for absent in [
        "fui-avatar",
        "ui_user_menu",
        "dashboard",
        "/login",
        "gatehouse",
    ] {
        assert!(!body.to_lowercase().contains(absent), "{absent} in {body}");
    }
}

#[quench_test::test]
async fn the_page_follows_the_cookies_and_the_browsers_language() {
    let (site, _) = site("/").await;
    let (_, _, body) = get(
        &site,
        Method::GET,
        "/",
        &[(
            "cookie",
            "qtheme=default-light; fui_palette=rose; qlocale=pl-PL",
        )],
    )
    .await;
    assert!(
        body.contains(r#"lang="pl-PL" data-fui-theme="light" data-fui-palette="rose""#),
        "{body}"
    );
    assert!(
        body.contains("Dokumentacja") && body.contains("Wydanie 7 · 5 stron"),
        "{body}"
    );
    assert!(
        body.contains(r#"<option value="pl-PL" selected>"#),
        "{body}"
    );

    let (_, _, body) = get(
        &site,
        Method::GET,
        "/",
        &[("accept-language", "de-AT,de;q=0.9,en;q=0.5")],
    )
    .await;
    assert!(
        body.contains(r#"lang="de-DE""#) && body.contains("Dokumentation"),
        "{body}"
    );
}

#[quench_test::test]
async fn the_pages_own_files_are_served() {
    let (site, _) = site("/").await;
    let (status, headers, body) = get(&site, Method::GET, "/ui/assets/style.css", &[]).await;
    assert_eq!(
        (status, header(&headers, "content-type")),
        (StatusCode::OK, "text/css; charset=utf-8")
    );
    assert!(
        body.contains("--fui-brand-500") && body.contains(".lc-grid"),
        "forge's tokens and Lectern's own rules"
    );
    let (status, headers, body) = get(&site, Method::GET, "/ui/assets/shell.js", &[]).await;
    assert_eq!(
        (status, header(&headers, "content-type")),
        (StatusCode::OK, "text/javascript; charset=utf-8")
    );
    assert!(body.contains("qtheme"));
    let (status, _, body) = get(&site, Method::GET, "/ui/assets/lectern.js", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("location.reload"));
    let (status, headers, _) = get(&site, Method::GET, "/ui/assets/favicon.png", &[]).await;
    assert_eq!(
        (status, header(&headers, "content-type")),
        (StatusCode::OK, "image/png")
    );
}

#[quench_test::test]
async fn an_empty_library_says_so() {
    let library = Arc::new(Library::new("/"));
    let site = Site::new(library, "/");
    let (status, _, body) = get(&site, Method::GET, "/", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("No documentation has been published yet"),
        "{body}"
    );
}

#[quench_test::test]
async fn pages_directories_and_assets_are_served_with_their_types() {
    let (site, _) = site("/").await;

    let (status, headers, body) = get(&site, Method::GET, "/forge/", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(header(&headers, "content-type"), "text/html; charset=utf-8");
    assert_eq!(header(&headers, "cache-control"), "no-cache");
    // links were written from the root; they now lead below the book
    assert!(body.contains(r#"href="/forge/guides/""#), "{body}");

    let (status, _, body) = get(&site, Method::GET, "/forge/guides/a.html", &[]).await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "a"));

    let (status, headers, _) = get(&site, Method::GET, "/forge/assets/app.css", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(header(&headers, "content-type"), "text/css; charset=utf-8");
    // revalidated against the release, not kept for a while: a new release must not meet an old stylesheet
    assert_eq!(header(&headers, "cache-control"), "no-cache");
    assert_eq!(header(&headers, "etag"), "W/\"7\"");
}

#[quench_test::test]
async fn a_directory_without_its_slash_is_sent_to_the_slash() {
    let (site, _) = site("/").await;
    let (status, headers, _) = get(&site, Method::GET, "/forge", &[]).await;
    assert_eq!(status, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(header(&headers, "location"), "/forge/");
    let (status, headers, _) = get(&site, Method::GET, "/forge/guides", &[]).await;
    assert_eq!(status, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(header(&headers, "location"), "/forge/guides/");
}

#[quench_test::test]
async fn what_is_not_there_is_a_404_with_the_books_own_page_when_it_has_one() {
    let (site, _) = site("/").await;
    let (status, _, body) = get(&site, Method::GET, "/forge/nope.html", &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, "the book's own missing page");
    let (status, _, _) = get(&site, Method::GET, "/other/", &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = get(&site, Method::GET, "/forge/..%2f..%2fetc/passwd", &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[quench_test::test]
async fn a_page_the_client_already_has_is_not_sent_again() {
    let (site, _) = site("/").await;
    let (_, headers, _) = get(&site, Method::GET, "/forge/guides/a.html", &[]).await;
    let etag = header(&headers, "etag").to_string();
    assert_eq!(etag, "W/\"7\"");
    let (status, _, body) = get(
        &site,
        Method::GET,
        "/forge/guides/a.html",
        &[("if-none-match", &etag)],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_MODIFIED);
    assert!(body.is_empty());
}

#[quench_test::test]
async fn only_reading_is_allowed() {
    let (site, _) = site("/").await;
    for method in [Method::POST, Method::PUT, Method::DELETE] {
        let (status, headers, _) = get(&site, method, "/forge/", &[]).await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(header(&headers, "allow"), "GET, HEAD");
    }
}

#[quench_test::test]
async fn under_a_base_path_everything_lives_below_it() {
    let (site, _) = site("/lectern").await;
    let (status, headers, _) = get(&site, Method::GET, "/lectern", &[]).await;
    assert_eq!(
        (status, header(&headers, "location")),
        (StatusCode::MOVED_PERMANENTLY, "/lectern/")
    );
    let (status, _, body) = get(&site, Method::GET, "/lectern/", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#"href="/lectern/forge/""#), "{body}");
    assert!(
        body.contains(r#"href="/lectern/ui/assets/style.css""#),
        "{body}"
    );
    let (status, _, body) = get(&site, Method::GET, "/lectern/forge/", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#"href="/lectern/forge/guides/""#), "{body}");
    let (status, _, _) = get(&site, Method::GET, "/forge/", &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = get(&site, Method::GET, "/lecternx/", &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[quench_test::test]
async fn the_books_are_listed_as_json_and_the_probes_answer() {
    let (site, library) = site("/").await;
    let (status, _, body) = get(&site, Method::GET, "/api/v1/books", &[]).await;
    assert_eq!(status, StatusCode::OK);
    let books: quench_serde::Value = quench_serde::json::from_str(&body).unwrap();
    assert_eq!(books[0]["name"], "forge");
    assert_eq!(books[0]["version"], "7");
    assert_eq!(books[0]["url"], "/forge/");

    for path in ["/health", "/health/live", "/health/ready"] {
        let (status, _, _) = get(&site, Method::GET, path, &[]).await;
        assert_eq!(status, StatusCode::OK, "{path}");
    }

    let cold = Site::new(Arc::new(Library::new("/")), "/");
    let (status, _, _) = get(&cold, Method::GET, "/health/ready", &[]).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let (status, _, _) = get(&cold, Method::GET, "/health", &[]).await;
    assert_eq!(status, StatusCode::OK);
    drop(library);
}

#[quench_test::test]
fn the_route_table_matches_paths_by_segment() {
    use lectern_service::site::matches;
    assert!(matches("/", "/") && !matches("/", "/x"));
    assert!(
        matches("/{book}", "/forge") && !matches("/{book}", "/forge/") && !matches("/{book}", "/")
    );
    assert!(matches("/{book}/{path}", "/forge/") && matches("/{book}/{path}", "/forge/a/b.html"));
    assert!(!matches("/{book}/{path}", "/forge") && !matches("/{book}/{path}", "//x"));
    assert!(matches("/health/live", "/health/live") && !matches("/health/live", "/health"));
}

const DOCUMENT: (&str, &str) = ("sec-fetch-dest", "document");

#[quench_test::test]
async fn a_page_opened_in_the_window_comes_in_lecterns_frame_with_the_other_books_to_go_to() {
    let source = MemorySource::default();
    source.release(
        "forge",
        "7",
        &[("guides/a.html", "the book's own page")],
        None,
    );
    source.release("quench", "3", &[("index.html", "q")], None);
    let library = Arc::new(Library::new("/lectern"));
    library.refresh(&source).await.unwrap();
    let site = Site::new(library, "/lectern");

    let (status, headers, body) = get(
        &site,
        Method::GET,
        "/lectern/forge/guides/a.html?x=1",
        &[DOCUMENT],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(header(&headers, "vary"), "Sec-Fetch-Dest");
    assert!(header(&headers, "content-security-policy").contains("frame-src 'self'"));
    // the top bar stays, the book's page is the frame's content, at the same address
    assert!(body.contains(r#"<header class="fui-topbar">"#), "{body}");
    assert!(
        body.contains(r#"<iframe id="book-frame""#)
            && body.contains(r#"src="/lectern/forge/guides/a.html?x=1""#),
        "{body}"
    );
    assert!(
        body.contains(r#"src="/lectern/ui/assets/frame.js""#),
        "{body}"
    );
    // the home button, and a menu of every book with the one being read marked
    assert!(
        body.contains(r#"id="home""#) && body.contains(r#"href="/lectern/""#),
        "{body}"
    );
    assert!(
        body.contains(r#"href="/lectern/forge/" data-book="forge" aria-current="page""#),
        "{body}"
    );
    assert!(
        body.contains(r#"href="/lectern/quench/" data-book="quench">"#),
        "{body}"
    );
    assert!(
        !body.contains("the book's own"),
        "the page itself is not in the frame page"
    );
}

#[quench_test::test]
async fn the_frame_asks_for_the_page_itself_and_so_does_anything_else() {
    let (site, _) = site("/").await;
    for headers in [
        vec![("sec-fetch-dest", "iframe")],
        vec![("sec-fetch-dest", "empty")],
        vec![], // a browser that does not say, a download tool
    ] {
        let (status, response, body) =
            get(&site, Method::GET, "/forge/guides/a.html", &headers).await;
        assert_eq!(
            (status, body.as_str()),
            (StatusCode::OK, "a"),
            "{headers:?}"
        );
        assert_eq!(header(&response, "vary"), "Sec-Fetch-Dest");
    }
    // only pages are framed: a stylesheet opened on its own is the stylesheet
    let (_, _, body) = get(&site, Method::GET, "/forge/assets/app.css", &[DOCUMENT]).await;
    assert_eq!(body, "body{}");
}

#[quench_test::test]
async fn the_framed_and_the_plain_page_do_not_share_a_validator() {
    let (site, _) = site("/").await;
    let (_, framed, _) = get(&site, Method::GET, "/forge/guides/a.html", &[DOCUMENT]).await;
    let (_, plain, _) = get(&site, Method::GET, "/forge/guides/a.html", &[]).await;
    assert_ne!(header(&framed, "etag"), header(&plain, "etag"));
    // what the plain page's validator names is not the frame
    let (status, _, body) = get(
        &site,
        Method::GET,
        "/forge/guides/a.html",
        &[DOCUMENT, ("if-none-match", header(&plain, "etag"))],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("book-frame"), "{body}");
}

#[quench_test::test]
async fn the_frame_script_is_served() {
    let (site, _) = site("/").await;
    let (status, headers, body) = get(&site, Method::GET, "/ui/assets/frame.js", &[]).await;
    assert_eq!(
        (status, header(&headers, "content-type")),
        (StatusCode::OK, "text/javascript; charset=utf-8")
    );
    assert!(
        body.contains("history.replaceState") && body.contains("book-frame"),
        "{body}"
    );
}
