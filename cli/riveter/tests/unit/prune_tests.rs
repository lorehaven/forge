//! What `prune` is willing to call an orphan: the parsing of `kubectl get -o jsonpath`, which has to tell
//! what riveter created from what the cluster's own controllers derived from it.

use riveter::repl::parse_live_listing;

/// One line as `LISTING` prints it: kind, name, owner kinds, service selector.
fn row(kind: &str, name: &str, owners: &str, selector: &str) -> String {
    format!("{kind}\t{name}\t{owners}\t{selector}\n")
}

fn names(listing: &str) -> Vec<String> {
    parse_live_listing(listing)
        .into_iter()
        .map(|r| r.to_string())
        .collect()
}

#[test]
fn objects_riveter_created_are_listed_by_lowercased_kind() {
    let listing = [
        row(
            "Deployment",
            "api",
            "",
            "{\"matchLabels\":{\"app\":\"api\"}}",
        ),
        row("ClusterRole", "viewer", "", ""),
        row("ConfigMap", "settings", "", ""),
    ]
    .concat();

    assert_eq!(
        names(&listing),
        ["deployment/api", "clusterrole/viewer", "configmap/settings"]
    );
}

#[test]
fn what_another_controller_created_is_never_an_orphan() {
    // cert-manager's Certificate for an Ingress, a Service's EndpointSlice, a Deployment's ReplicaSet and
    // Pod: all carry riveter's labels (copied down), all have an owner.
    let listing = [
        row("Certificate", "api-tls", "Ingress", ""),
        row("EndpointSlice", "api-x7k2p", "Service", ""),
        row("ReplicaSet", "api-5f9b6b444b", "Deployment", ""),
        row("Pod", "api-5f9b6b444b-abcde", "ReplicaSet", ""),
    ]
    .concat();

    assert!(names(&listing).is_empty());
}

#[test]
fn an_object_with_several_owners_is_still_owned() {
    assert!(names(&row("Pod", "p", "ReplicaSet StatefulSet", "")).is_empty());
}

#[test]
fn a_certificate_riveter_rendered_itself_has_no_owner_so_it_stays_prunable() {
    assert_eq!(
        names(&row("Certificate", "hand-written", "", "")),
        ["certificate/hand-written"]
    );
}

#[test]
fn the_endpoints_of_a_service_with_a_selector_are_kubernetes_own() {
    // Endpoints carry no owner reference, so the only tell is the Service of the same name.
    let listing = [
        row("Service", "api", "", "{\"app\":\"api\"}"),
        row("Endpoints", "api", "", ""),
    ]
    .concat();

    assert_eq!(names(&listing), ["service/api"]);
}

#[test]
fn the_endpoints_of_a_selectorless_service_are_hand_written_and_stay_prunable() {
    // The case riveter's `endpoints` template exists for: a Service pointing at something outside the
    // cluster, with its Endpoints written by hand.
    let listing = [
        row("Service", "external-db", "", ""),
        row("Endpoints", "external-db", "", ""),
    ]
    .concat();

    assert_eq!(
        names(&listing),
        ["service/external-db", "endpoints/external-db"]
    );
}

#[test]
fn endpoints_with_no_service_in_the_listing_stay_prunable() {
    assert_eq!(
        names(&row("Endpoints", "stray", "", "")),
        ["endpoints/stray"]
    );
}

#[test]
fn a_selecting_service_only_hides_the_endpoints_of_its_own_name() {
    let listing = [
        row("Service", "api", "", "{\"app\":\"api\"}"),
        row("Endpoints", "api", "", ""),
        row("Endpoints", "other", "", ""),
    ]
    .concat();

    assert_eq!(names(&listing), ["service/api", "endpoints/other"]);
}

#[test]
fn a_service_of_another_kind_with_the_same_name_does_not_hide_anything() {
    // Only a *Service's* selector counts, not any object that happens to have the name.
    let listing = [
        row("Deployment", "api", "", "{\"matchLabels\":{}}"),
        row("Endpoints", "api", "", ""),
    ]
    .concat();

    assert_eq!(names(&listing), ["deployment/api", "endpoints/api"]);
}

#[test]
fn the_forge_namespace_that_proposed_31_deletions_now_proposes_only_the_stale_jobs() {
    // The shape that showed the bug: every service has an Ingress-owned Certificate, an owned
    // EndpointSlice and kubernetes' own Endpoints, around two old release Jobs that really are orphans.
    let mut listing = String::new();
    for service in ["conveyor", "gatehouse", "sage", "warehouse"] {
        listing += &row("Deployment", service, "", "{\"matchLabels\":{}}");
        listing += &row(
            "Service",
            service,
            "",
            &format!("{{\"app\":\"{service}\"}}"),
        );
        listing += &row("Endpoints", service, "", "");
        listing += &row("EndpointSlice", &format!("{service}-4zpdn"), "Service", "");
        listing += &row("Certificate", &format!("{service}-tls"), "Ingress", "");
    }
    listing += &row("Job", "foundry-0-2-14", "", "");
    listing += &row("Job", "foundry-0-2-15", "", "");

    let live = parse_live_listing(&listing);
    assert_eq!(live.iter().filter(|r| r.kind == "job").count(), 2);
    assert!(live.iter().all(|r| !matches!(
        r.kind.as_str(),
        "certificate" | "endpoints" | "endpointslice"
    )));
    assert_eq!(live.len(), 4 * 2 + 2);
}

#[test]
fn blank_malformed_and_empty_output_is_ignored() {
    assert!(names("").is_empty());
    assert!(names("\n\n").is_empty());
    assert!(names("Deployment\n").is_empty());
    assert!(names("\tname-without-kind\t\t\n").is_empty());
    assert!(names("Deployment\t\t\t\n").is_empty());
    // A row short of its trailing fields is a plain object, not a crash.
    assert_eq!(names("Deployment\tapi\n"), ["deployment/api"]);
}
