//! What `prune` is willing to call an orphan: the filtering of a listing, which has to tell what riveter created from what
//! the cluster's own controllers derived from it.

use riveter::cluster::Listed;
use riveter::repl::orphan_candidates;

/// One live object as a listing reports it: kind, name, the kinds that own it, and whether it has a selector.
fn row(kind: &str, name: &str, owners: &str, selector: &str) -> Listed {
    Listed {
        kind: kind.to_string(),
        name: name.to_string(),
        owner_kinds: owners.split_whitespace().map(str::to_string).collect(),
        selects: !selector.is_empty(),
    }
}

fn names(listing: &[Listed]) -> Vec<String> {
    orphan_candidates(listing)
        .into_iter()
        .map(|r| r.to_string())
        .collect()
}

#[quench_test::test]
fn objects_riveter_created_are_listed_by_lowercased_kind() {
    let listing = vec![
        row(
            "Deployment",
            "api",
            "",
            "{\"matchLabels\":{\"app\":\"api\"}}",
        ),
        row("ClusterRole", "viewer", "", ""),
        row("ConfigMap", "settings", "", ""),
    ];

    assert_eq!(
        names(&listing),
        ["deployment/api", "clusterrole/viewer", "configmap/settings"]
    );
}

#[quench_test::test]
fn what_another_controller_created_is_never_an_orphan() {
    // cert-manager's Certificate for an Ingress, a Service's EndpointSlice, a Deployment's ReplicaSet and
    // Pod: all carry riveter's labels (copied down), all have an owner.
    let listing = vec![
        row("Certificate", "api-tls", "Ingress", ""),
        row("EndpointSlice", "api-x7k2p", "Service", ""),
        row("ReplicaSet", "api-5f9b6b444b", "Deployment", ""),
        row("Pod", "api-5f9b6b444b-abcde", "ReplicaSet", ""),
    ];

    assert_eq!(names(&listing), [] as [String; 0]);
}

#[quench_test::test]
fn an_object_with_several_owners_is_still_owned() {
    assert_eq!(
        names(&[row("Pod", "p", "ReplicaSet StatefulSet", "")]),
        [] as [String; 0]
    );
}

#[quench_test::test]
fn a_certificate_riveter_rendered_itself_has_no_owner_so_it_stays_prunable() {
    assert_eq!(
        names(&[row("Certificate", "hand-written", "", "")]),
        ["certificate/hand-written"]
    );
}

#[quench_test::test]
fn the_endpoints_of_a_service_with_a_selector_are_kubernetes_own() {
    // Endpoints carry no owner reference, so the only tell is the Service of the same name.
    let listing = vec![
        row("Service", "api", "", "{\"app\":\"api\"}"),
        row("Endpoints", "api", "", ""),
    ];

    assert_eq!(names(&listing), ["service/api"]);
}

#[quench_test::test]
fn the_endpoints_of_a_selectorless_service_are_hand_written_and_stay_prunable() {
    // The case riveter's `endpoints` template exists for: a Service pointing at something outside the
    // cluster, with its Endpoints written by hand.
    let listing = vec![
        row("Service", "external-db", "", ""),
        row("Endpoints", "external-db", "", ""),
    ];

    assert_eq!(
        names(&listing),
        ["service/external-db", "endpoints/external-db"]
    );
}

#[quench_test::test]
fn endpoints_with_no_service_in_the_listing_stay_prunable() {
    assert_eq!(
        names(&[row("Endpoints", "stray", "", "")]),
        ["endpoints/stray"]
    );
}

#[quench_test::test]
fn a_selecting_service_only_hides_the_endpoints_of_its_own_name() {
    let listing = vec![
        row("Service", "api", "", "{\"app\":\"api\"}"),
        row("Endpoints", "api", "", ""),
        row("Endpoints", "other", "", ""),
    ];

    assert_eq!(names(&listing), ["service/api", "endpoints/other"]);
}

#[quench_test::test]
fn a_service_of_another_kind_with_the_same_name_does_not_hide_anything() {
    // Only a *Service's* selector counts, not any object that happens to have the name.
    let listing = vec![
        row("Deployment", "api", "", "{\"matchLabels\":{}}"),
        row("Endpoints", "api", "", ""),
    ];

    assert_eq!(names(&listing), ["deployment/api", "endpoints/api"]);
}

#[quench_test::test]
fn the_forge_namespace_that_proposed_31_deletions_now_proposes_only_the_stale_jobs() {
    // The shape that showed the bug: every service has an Ingress-owned Certificate, an owned
    // EndpointSlice and kubernetes' own Endpoints, around two old release Jobs that really are orphans.
    let mut listing = Vec::new();
    for service in ["conveyor", "gatehouse", "sage", "warehouse"] {
        listing.push(row("Deployment", service, "", "x"));
        listing.push(row("Service", service, "", "x"));
        listing.push(row("Endpoints", service, "", ""));
        listing.push(row(
            "EndpointSlice",
            &format!("{service}-4zpdn"),
            "Service",
            "",
        ));
        listing.push(row("Certificate", &format!("{service}-tls"), "Ingress", ""));
    }
    listing.push(row("Job", "foundry-0-2-14", "", ""));
    listing.push(row("Job", "foundry-0-2-15", "", ""));

    let live = orphan_candidates(&listing);
    assert_eq!(live.iter().filter(|r| r.kind == "job").count(), 2);
    assert!(live.iter().all(|r| !matches!(
        r.kind.as_str(),
        "certificate" | "endpoints" | "endpointslice"
    )));
    assert_eq!(live.len(), 4 * 2 + 2);
}

#[quench_test::test]
fn an_empty_listing_has_no_candidates() {
    assert_eq!(names(&[]), [] as [String; 0]);
}
