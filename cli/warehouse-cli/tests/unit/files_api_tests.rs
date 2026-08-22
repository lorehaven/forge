use warehouse_cli::api::files_api::{
    FilesApi, base_url, filename_from_content_disposition, remote_path_for_upload, url_encode,
};
use warehouse_cli::domain::{RegistryConfig, RegistryDockerConfig, RegistryFilesConfig};
use wiremock::matchers::{method, path as path_matcher, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn registry(url: &str) -> RegistryConfig {
    RegistryConfig {
        base_path: String::new(),
        files: RegistryFilesConfig {
            url: url.to_string(),
            insecure_tls: false,
        },
        ..RegistryConfig::default()
    }
}

#[test]
fn url_encode_keeps_or_escapes_the_slash_as_requested() {
    assert_eq!(url_encode("a/b c", true), "a/b%20c");
    assert_eq!(url_encode("a/b c", false), "a%2Fb%20c");
    assert_eq!(url_encode("safe-._~09AZaz", false), "safe-._~09AZaz");
}

#[test]
fn filename_from_content_disposition_extracts_a_quoted_name() {
    assert_eq!(
        filename_from_content_disposition(Some(r#"attachment; filename="report.pdf""#)),
        Some("report.pdf".to_string())
    );
}

#[test]
fn filename_from_content_disposition_is_none_without_a_filename_part() {
    assert_eq!(filename_from_content_disposition(Some("attachment")), None);
    assert_eq!(filename_from_content_disposition(None), None);
}

#[test]
fn base_url_prefers_files_then_docker_then_crates() {
    let mut reg = registry("");
    reg.docker = RegistryDockerConfig {
        url: "https://docker.example.net".to_string(),
        ..RegistryDockerConfig::default()
    };
    assert_eq!(
        base_url(&reg, "/x").unwrap(),
        "https://docker.example.net/x"
    );

    reg.files.url = "https://files.example.net".to_string();
    assert_eq!(base_url(&reg, "/x").unwrap(), "https://files.example.net/x");
}

#[test]
fn base_url_errors_when_every_url_is_empty() {
    assert!(base_url(&registry(""), "/x").is_err());
}

#[test]
fn remote_path_for_upload_joins_the_filename_under_a_remote_dir() {
    assert_eq!(
        remote_path_for_upload("/local/report.pdf", Some("/backups/")).unwrap(),
        "backups/report.pdf"
    );
    assert_eq!(
        remote_path_for_upload("/local/report.pdf", None).unwrap(),
        "report.pdf"
    );
    assert_eq!(
        remote_path_for_upload("/local/report.pdf", Some("")).unwrap(),
        "report.pdf"
    );
}

#[test]
fn remote_path_for_upload_errors_without_a_filename() {
    assert!(remote_path_for_upload("/", None).is_err());
}

#[tokio::test]
async fn storages_parses_the_response_list() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_matcher("/api/v1/files/storages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "storages": [{ "name": "default", "root": "/data" }]
        })))
        .mount(&server)
        .await;

    let reg = registry(&server.uri());
    let api = FilesApi::new(&reg).expect("build client");
    let storages = api.storages(&reg).await.expect("storages");
    assert_eq!(storages[0].name, "default");
}

#[tokio::test]
async fn list_parses_directory_entries() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/api/v1/files/default/entries$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "storage": "default",
            "path": "/",
            "entries": [{ "path": "/a.txt", "is_dir": false, "size_bytes": 10 }]
        })))
        .mount(&server)
        .await;

    let reg = registry(&server.uri());
    let api = FilesApi::new(&reg).expect("build client");
    let listing = api.list(&reg, "default", "/").await.expect("list");
    assert_eq!(listing.entries.len(), 1);
    assert_eq!(listing.entries[0].path, "/a.txt");
}

#[tokio::test]
async fn upload_sends_the_bytes_as_octet_stream() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path_regex(r"^/api/v1/files/default/file$"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let reg = registry(&server.uri());
    let api = FilesApi::new(&reg).expect("build client");
    api.upload(&reg, "default", "a.txt", b"hello".to_vec())
        .await
        .expect("upload");
}

#[tokio::test]
async fn preview_parses_the_response() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/api/v1/files/default/preview$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "storage": "default",
            "path": "/a.txt",
            "kind": "text",
            "content": "hi",
            "truncated": false
        })))
        .mount(&server)
        .await;

    let reg = registry(&server.uri());
    let api = FilesApi::new(&reg).expect("build client");
    let preview = api
        .preview(&reg, "default", "/a.txt")
        .await
        .expect("preview");
    assert_eq!(preview.content, "hi");
    assert!(!preview.truncated);
}

#[tokio::test]
async fn mkdir_and_rmdir_hit_the_folder_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(r"^/api/v1/files/default/folder$"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path_regex(r"^/api/v1/files/default/folder$"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let reg = registry(&server.uri());
    let api = FilesApi::new(&reg).expect("build client");
    api.mkdir(&reg, "default", "/new").await.expect("mkdir");
    api.rmdir(&reg, "default", "/new").await.expect("rmdir");
}

#[tokio::test]
async fn delete_file_hits_the_file_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path_regex(r"^/api/v1/files/default/file$"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let reg = registry(&server.uri());
    let api = FilesApi::new(&reg).expect("build client");
    api.delete_file(&reg, "default", "/a.txt")
        .await
        .expect("delete");
}

#[tokio::test]
async fn bulk_delete_and_bulk_download_send_the_path_list() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path_matcher("/api/v1/files/default/bulk"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_matcher("/api/v1/files/default/bulk-download"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"zip-bytes".to_vec()))
        .mount(&server)
        .await;

    let reg = registry(&server.uri());
    let api = FilesApi::new(&reg).expect("build client");
    let paths = vec!["/a.txt".to_string(), "/b.txt".to_string()];
    api.bulk_delete(&reg, "default", &paths)
        .await
        .expect("bulk delete");
    let bytes = api
        .bulk_download(&reg, "default", &paths)
        .await
        .expect("bulk download");
    assert_eq!(bytes, b"zip-bytes".to_vec());
}

#[tokio::test]
async fn download_reads_the_body_and_filename_header() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/api/v1/files/default/download$"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-disposition", r#"attachment; filename="a.txt""#)
                .set_body_bytes(b"contents".to_vec()),
        )
        .mount(&server)
        .await;

    let reg = registry(&server.uri());
    let api = FilesApi::new(&reg).expect("build client");
    let (bytes, filename) = api
        .download(&reg, "default", "/a.txt")
        .await
        .expect("download");
    assert_eq!(bytes, b"contents".to_vec());
    assert_eq!(filename.as_deref(), Some("a.txt"));
}

#[tokio::test]
async fn download_reports_a_non_success_status() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/api/v1/files/default/download$"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let reg = registry(&server.uri());
    let api = FilesApi::new(&reg).expect("build client");
    let error = api.download(&reg, "default", "/missing").await.unwrap_err();
    assert!(error.to_string().contains("request failed"));
}
