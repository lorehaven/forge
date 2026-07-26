//! Unit tests for `store.rs`.

#![cfg(feature = "redis")]

use quench_cache::store::parse_primary;

#[test]
fn keeps_healthy_primaries() {
    let line = "07c37d 127.0.0.1:7001@17001 myself,master - 0 1690000000000 1 connected 0-5460";
    assert_eq!(parse_primary(line), Some(("127.0.0.1".to_string(), 7001)));
}

#[test]
fn skips_replicas() {
    let line = "e7d1ee 127.0.0.1:7004@17004 slave 07c37d 0 1690000000000 4 connected";
    assert_eq!(parse_primary(line), None);
}

#[test]
fn skips_primaries_the_cluster_has_given_up_on() {
    let failed = "07c37d 127.0.0.1:7001@17001 master,fail - 0 1690000000000 1 disconnected";
    let suspect = "07c37d 127.0.0.1:7002@17002 master,fail? - 0 1690000000000 1 connected";
    assert_eq!(parse_primary(failed), None);
    assert_eq!(parse_primary(suspect), None);
}

/// Redis 7 appends a hostname and auxiliary fields to the address; the port we
/// want is still the client port, not the cluster bus port after the `@`.
#[test]
fn reads_the_client_port_from_a_decorated_address() {
    let line = "07c37d 10.0.0.5:6379@16379,node-a.redis.svc master - 0 1690000000000 1 connected";
    assert_eq!(parse_primary(line), Some(("10.0.0.5".to_string(), 6379)));
}

#[test]
fn ignores_lines_that_are_not_node_records() {
    assert_eq!(parse_primary(""), None);
    assert_eq!(parse_primary("07c37d 127.0.0.1:7001@17001"), None);
}
