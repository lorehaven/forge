use crate::support;
use quench_auth::domain::jwt::JwtConfig;
use quench_db::{Db, InMemoryDb};
use quench_http::http::{Method, StatusCode};
use warehouse_service::routers::files;
use warehouse_service::routers::files::ops::download::{download_name, is_file};

#[quench_test::test]
fn download_name_uses_the_target_s_file_name() {
    assert_eq!(
        download_name(std::path::Path::new("/a/b/report.pdf")),
        "report.pdf"
    );
}

#[quench_test::test]
fn download_name_strips_quotes_backslashes_and_control_bytes() {
    assert_eq!(
        download_name(std::path::Path::new("weird\"name\\with\x01control")),
        "weirdnamewithcontrol"
    );
}

#[quench_test::test]
fn download_name_falls_back_to_download_when_nothing_usable_remains() {
    assert_eq!(download_name(std::path::Path::new("/")), "download");
    assert_eq!(download_name(std::path::Path::new("\"\\")), "download");
}

#[quench_test::test]
async fn is_file_is_true_only_for_a_real_file_not_a_directory_or_missing_path() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let file = dir.path().join("f.txt");
    tokio::fs::write(&file, b"hi").await.unwrap();

    assert!(is_file(&file).await);
    assert!(!is_file(&dir.path().to_path_buf()).await);
    assert!(!is_file(&dir.path().join("missing")).await);
}

#[quench_test::test]
async fn handle_reports_not_found_when_file_storage_is_disabled() {
    // `FEATURE_FILES_ENABLED` is unset in this sandbox, and the flag is
    // a `LazyLock` fixed for the whole test binary - so this deterministically
    // hits the "not enabled" branch rather than ever reaching the filesystem.
    files::register_routes();
    let container = support::container_builder()
        .provide(JwtConfig::for_tests())
        .provide(Db::InMemory(InMemoryDb::new()))
        .build()
        .await
        .unwrap();
    let (app, container) = support::app(container);

    let resp = app
        .call(support::req(
            Method::GET,
            "/api/v1/files/artifacts/file?path=a.txt",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[quench_test::test]
async fn head_reports_not_found_when_file_storage_is_disabled() {
    files::register_routes();
    let container = support::container_builder()
        .provide(JwtConfig::for_tests())
        .provide(Db::InMemory(InMemoryDb::new()))
        .build()
        .await
        .unwrap();
    let (app, container) = support::app(container);

    let resp = app
        .call(support::req(
            Method::HEAD,
            "/api/v1/files/artifacts/file?path=a.txt",
            &container,
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

mod streaming {
    use crate::support;
    use quench_http::http::StatusCode;
    use warehouse_service::routers::files::FetchHeaders;
    use warehouse_service::routers::files::ops::download::{
        StreamOptions, parse_range, stream_file_with,
    };

    #[quench_test::test]
    fn a_range_is_read_as_the_browser_means_it() {
        assert_eq!(parse_range(None, 100), Ok(None));
        assert_eq!(parse_range(Some("bytes=0-9"), 100), Ok(Some((0, 9))));
        assert_eq!(parse_range(Some("bytes=90-"), 100), Ok(Some((90, 99))));
        assert_eq!(parse_range(Some("bytes=-10"), 100), Ok(Some((90, 99))));
        assert_eq!(parse_range(Some("bytes=50-5000"), 100), Ok(Some((50, 99))));
        assert_eq!(parse_range(Some("bytes=0-1,5-6"), 100), Ok(Some((0, 1))));
        assert_eq!(parse_range(Some("bytes=100-"), 100), Err(()));
        assert_eq!(parse_range(Some("bytes=9-3"), 100), Err(()));
        assert_eq!(parse_range(Some("items=0-3"), 100), Ok(None));
        assert_eq!(parse_range(Some("bytes=x-3"), 100), Ok(None));
    }

    async fn file_of(bytes: &[u8]) -> (quench_core::tmp::TempDir, std::path::PathBuf) {
        let dir = quench_core::tmp::TempDir::new().unwrap();
        let file = dir.path().join("blob");
        tokio::fs::write(&file, bytes).await.unwrap();
        (dir, file)
    }

    fn asking(range: Option<&str>, etag: Option<&str>) -> StreamOptions {
        StreamOptions {
            digest: Some("abc".into()),
            headers: FetchHeaders {
                range: range.map(str::to_string),
                if_none_match: etag.map(str::to_string),
            },
            thumb: None,
        }
    }

    #[quench_test::test]
    async fn a_range_gets_only_those_bytes_with_a_content_range() {
        let (_dir, file) = file_of(b"0123456789").await;
        let resp = stream_file_with(&file, "a.mp4", true, asking(Some("bytes=2-4"), None)).await;
        assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(resp.headers().get("content-range").unwrap(), "bytes 2-4/10");
        assert_eq!(resp.headers().get("content-length").unwrap(), "3");
        assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
        assert_eq!(&support::body_bytes(resp).await[..], b"234");
    }

    #[quench_test::test]
    async fn a_range_past_the_end_is_refused_and_no_range_is_the_whole_file() {
        let (_dir, file) = file_of(b"0123456789").await;
        let resp = stream_file_with(&file, "a.bin", true, asking(Some("bytes=10-"), None)).await;
        assert_eq!(resp.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        let resp = stream_file_with(&file, "a.bin", true, asking(None, None)).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(&support::body_bytes(resp).await[..], b"0123456789");
    }

    #[quench_test::test]
    async fn what_the_browser_already_has_is_not_sent_again() {
        let (_dir, file) = file_of(b"0123456789").await;
        let resp = stream_file_with(&file, "a.png", true, asking(None, Some("\"abc\""))).await;
        assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(resp.headers().get("etag").unwrap(), "\"abc\"");
        let resp = stream_file_with(&file, "a.png", true, asking(None, Some("\"other\""))).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get("etag").unwrap(), "\"abc\"");
    }

    #[quench_test::test]
    async fn a_thumbnail_is_a_small_png_and_other_files_fall_back_to_the_original() {
        let mut rgba = Vec::new();
        for i in 0..(200u32 * 100) {
            rgba.extend_from_slice(&[u8::try_from(i % 251).unwrap(), 40, 90, 255]);
        }
        let png = quench_image::encode_png(200, 100, &rgba);
        let (_dir, file) = file_of(&png).await;
        let mut options = asking(None, None);
        options.thumb = Some(50);
        let resp = stream_file_with(&file, "photos/a.png", true, options).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get("content-type").unwrap(), "image/png");
        let body = support::body_bytes(resp).await;
        let small = quench_image::decode(&body).unwrap();
        assert_eq!((small.width, small.height), (50, 25));

        let (_dir, text) = file_of(b"not an image").await;
        let mut options = asking(None, None);
        options.thumb = Some(50);
        let resp = stream_file_with(&text, "notes.png", true, options).await;
        assert_eq!(&support::body_bytes(resp).await[..], b"not an image");
    }
}
