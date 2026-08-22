use actix_web::test::TestRequest;
use warehouse_service::middleware::limits::{WarehouseLimits, is_upload_mutation};

#[test]
fn is_upload_mutation_true_for_write_methods_on_the_uploads_path() {
    for method_req in [
        TestRequest::post(),
        TestRequest::patch(),
        TestRequest::put(),
    ] {
        let req = method_req
            .uri("/v2/my/repo/blobs/uploads/some-uuid")
            .to_srv_request();
        assert!(is_upload_mutation(&req));
    }
}

#[test]
fn is_upload_mutation_false_for_reads_even_on_the_uploads_path() {
    let req = TestRequest::get()
        .uri("/v2/my/repo/blobs/uploads/some-uuid")
        .to_srv_request();
    assert!(!is_upload_mutation(&req));
}

#[test]
fn is_upload_mutation_false_for_writes_elsewhere() {
    let req = TestRequest::post()
        .uri("/v2/my/repo/manifests/latest")
        .to_srv_request();
    assert!(!is_upload_mutation(&req));
}

/// A macro, not a function: `test::init_service`'s return type is opaque
/// and can't be named without pulling in `actix-http` as a direct
/// dev-dependency just to spell `actix_http::Request` - see the same
/// pattern in `middleware_auth_tests`.
macro_rules! test_app {
    ($permits:expr) => {{
        use actix_web::{App, HttpResponse, web};
        actix_web::test::init_service(App::new().wrap(WarehouseLimits::new($permits)).route(
            "/v2/{tail:.*}",
            web::route().to(|| async {
                // A real request holds its permit for the length of
                // the handler; a short sleep gives a second
                // concurrent request time to observe the semaphore
                // as exhausted before this one releases it.
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                HttpResponse::Ok().finish()
            }),
        ))
        .await
    }};
}

#[actix_web::test]
async fn non_upload_requests_bypass_the_limiter_entirely() {
    let app = test_app!(0);
    let req = TestRequest::get()
        .uri("/v2/my/repo/manifests/latest")
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}

#[actix_web::test]
async fn a_second_concurrent_upload_beyond_the_limit_is_throttled() {
    use std::rc::Rc;

    let app = Rc::new(test_app!(1));
    let app_for_first = app.clone();

    let first = tokio::task::spawn_local(async move {
        let req = TestRequest::post()
            .uri("/v2/my/repo/blobs/uploads/one")
            .to_request();
        actix_web::test::call_service(&*app_for_first, req)
            .await
            .status()
    });
    // Give the first request time to acquire its permit before the
    // second one tries.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let req = TestRequest::post()
        .uri("/v2/my/repo/blobs/uploads/two")
        .to_request();
    let second_status = actix_web::test::call_service(&*app, req).await.status();
    let first_status = first.await.expect("first request task");

    assert_eq!(first_status, actix_web::http::StatusCode::OK);
    assert_eq!(
        second_status,
        actix_web::http::StatusCode::TOO_MANY_REQUESTS
    );
}

#[actix_web::test]
async fn a_new_max_concurrent_uploads_of_zero_is_treated_as_at_least_one() {
    // `WarehouseLimits::new(0)` must not create a semaphore with zero
    // permits, which would make every upload request fail forever.
    let app = test_app!(0);
    let req = TestRequest::post()
        .uri("/v2/my/repo/blobs/uploads/one")
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
}
