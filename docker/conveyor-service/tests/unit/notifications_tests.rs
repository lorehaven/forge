//! The parts of run-result notifications that need no database.

use conveyor_service::notifications::gatehouse::{Delivery, classify};
use conveyor_service::notifications::outbox::backoff_secs;

#[test]
fn a_malformed_request_is_never_retried() {
    let error = "HTTP 400 Bad Request: {\"error\":\"the variable \\\"url\\\" is empty\"}";
    assert!(matches!(classify(error), Delivery::Rejected(_)));
}

#[test]
fn a_mail_failure_gatehouse_calls_permanent_is_not_retried() {
    let error =
        "HTTP 502 Bad Gateway: {\"status\":\"failed\",\"reason\":\"rejected\",\"retryable\":false}";
    assert!(matches!(classify(error), Delivery::Rejected(_)));
}

#[test]
fn a_transient_mail_failure_is_retried() {
    let error =
        "HTTP 502 Bad Gateway: {\"status\":\"failed\",\"reason\":\"timeout\",\"retryable\":true}";
    assert!(matches!(classify(error), Delivery::Retry(_)));
}

#[test]
fn being_unreachable_or_unauthorised_is_retried_because_it_can_be_fixed() {
    for error in [
        "Failed to send POST request",
        "client_credentials grant was refused: invalid client credentials",
        "HTTP 401 Unauthorized: ",
        "HTTP 403 Forbidden: ",
        "HTTP 503 Service Unavailable: ",
    ] {
        assert!(matches!(classify(error), Delivery::Retry(_)), "{error}");
    }
}

#[test]
fn the_wait_grows_with_each_failure_and_stops_at_an_hour() {
    let waits: Vec<f64> = (1..=10).map(backoff_secs).collect();
    assert_eq!(waits[0], 30.0);
    assert_eq!(waits[1], 60.0);
    assert_eq!(waits[2], 120.0);
    assert!(waits.windows(2).all(|pair| pair[1] >= pair[0]));
    assert!(waits.iter().all(|wait| *wait <= 3600.0));
    // A nonsense count still yields the shortest wait rather than panicking.
    assert_eq!(backoff_secs(0), 30.0);
    assert_eq!(backoff_secs(i32::MAX), 3600.0);
}
