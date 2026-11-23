use gantry_service::domain::browse::{self, summarize};
use quench_serde::json;

#[quench_test::test]
fn a_pod_says_what_its_containers_say_not_just_its_phase() {
    let running = json!({
        "spec": {"nodeName": "n1", "containers": [{"name": "a"}, {"name": "b"}]},
        "status": {"phase": "Running", "containerStatuses": [
            {"ready": true, "restartCount": 1}, {"ready": false, "restartCount": 2}]}
    });
    let (status, bad, detail) = summarize("Pod", &running);
    assert_eq!((status.as_str(), bad), ("Running", false));
    assert_eq!(detail, "1/2 · 3 restarts · n1");

    let crashing = json!({
        "spec": {"containers": [{"name": "a"}]},
        "status": {"phase": "Running", "containerStatuses": [
            {"ready": false, "restartCount": 9, "state": {"waiting": {"reason": "CrashLoopBackOff"}}}]}
    });
    let (status, bad, _) = summarize("Pod", &crashing);
    assert_eq!((status.as_str(), bad), ("CrashLoopBackOff", true));

    let starting = json!({
        "spec": {"containers": [{"name": "a"}]},
        "status": {"phase": "Pending", "containerStatuses": [
            {"ready": false, "state": {"waiting": {"reason": "ContainerCreating"}}}]}
    });
    assert!(!summarize("Pod", &starting).1);
}

#[quench_test::test]
fn workloads_show_ready_over_wanted_and_a_short_one_needs_attention() {
    let short = json!({"spec": {"replicas": 3}, "status": {"readyReplicas": 1}});
    let (status, bad, _) = summarize("Deployment", &short);
    assert_eq!((status.as_str(), bad), ("1/3", true));
    let stopped = json!({"spec": {"replicas": 0}, "status": {}});
    assert_eq!(summarize("Deployment", &stopped).0, "0/0");
    assert!(!summarize("Deployment", &stopped).1);
}

#[quench_test::test]
fn nodes_are_ready_or_not_and_say_what_they_are() {
    let node = json!({
        "metadata": {"labels": {"node-role.kubernetes.io/control-plane": ""}},
        "status": {"nodeInfo": {"kubeletVersion": "v1.31"},
                   "conditions": [{"type": "Ready", "status": "False"}]}
    });
    let (status, bad, detail) = summarize("Node", &node);
    assert_eq!((status.as_str(), bad), ("NotReady", true));
    assert_eq!(detail, "v1.31 · control-plane");
}

#[quench_test::test]
fn an_entry_needs_a_name_and_keeps_where_it_lives() {
    let object = json!({"metadata": {"name": "web", "namespace": "ns", "creationTimestamp": "2026-01-01T00:00:00Z"},
                        "spec": {"type": "NodePort", "clusterIP": "10.0.0.1", "ports": [{"port": 80, "protocol": "TCP"}]}});
    let entry = browse::entry("v1", "Service", &object).unwrap();
    assert_eq!(entry.namespace.as_deref(), Some("ns"));
    assert_eq!(entry.status, "NodePort");
    assert_eq!(entry.detail, "10.0.0.1 · 80/TCP");
    assert!(browse::entry("v1", "Service", &json!({"metadata": {}})).is_none());
}

#[quench_test::test]
fn the_yaml_drops_noise_and_the_pod_lists_its_containers() {
    let mut pod = json!({
        "metadata": {"name": "p", "managedFields": [1], "annotations": {"kubectl.kubernetes.io/last-applied-configuration": "{}", "keep": "1"}},
        "spec": {"initContainers": [{"name": "init"}], "containers": [{"name": "app"}]}
    });
    assert_eq!(browse::containers(&pod), ["init", "app"]);
    browse::clean_for_reading(&mut pod);
    assert!(pod.pointer("/metadata/managedFields").is_none());
    assert!(pod.pointer("/metadata/annotations/keep").is_some());
    assert!(
        pod.pointer("/metadata/annotations/kubectl.kubernetes.io~1last-applied-configuration")
            .is_none()
    );
}

#[quench_test::test]
fn quantities_read_the_way_kubernetes_writes_them() {
    assert_eq!(browse::millicores("250m"), 250);
    assert_eq!(browse::millicores("2"), 2000);
    assert_eq!(browse::millicores("134000000n"), 134);
    assert_eq!(browse::bytes("512Ki"), 524_288);
    assert_eq!(browse::bytes("3Gi"), 3 * 1024 * 1024 * 1024);
    assert_eq!(browse::bytes("100M"), 100_000_000);
    assert_eq!(browse::human_bytes(1_288_490_189), "1.2Gi");
    assert_eq!(browse::human_bytes(356_515_840), "340Mi");
}

#[quench_test::test]
fn usage_is_added_to_the_entries_it_names() {
    let metrics = json!({"items": [
        {"metadata": {"name": "web", "namespace": "ml"},
         "containers": [{"usage": {"cpu": "100m", "memory": "100Mi"}}, {"usage": {"cpu": "50m", "memory": "28Mi"}}]},
    ]});
    let mut entries = vec![
        browse::entry(
            "v1",
            "Pod",
            &json!({"metadata": {"name": "web", "namespace": "ml"}}),
        )
        .unwrap(),
        browse::entry(
            "v1",
            "Pod",
            &json!({"metadata": {"name": "other", "namespace": "ml"}}),
        )
        .unwrap(),
    ];
    browse::add_usage(&mut entries, &browse::usage_of(&metrics));
    assert_eq!(entries[0].usage, "150m · 128Mi");
    assert_eq!(entries[1].usage, "");
}

#[quench_test::test]
fn workloads_and_services_say_which_pods_are_theirs() {
    let deployment = json!({"spec": {"selector": {"matchLabels": {"app": "web", "tier": "api"}}}});
    assert_eq!(
        browse::pod_selector("Deployment", &deployment).as_deref(),
        Some("app=web,tier=api")
    );
    let service = json!({"spec": {"selector": {"app": "web"}}});
    assert_eq!(
        browse::pod_selector("Service", &service).as_deref(),
        Some("app=web")
    );
    assert!(browse::pod_selector("ConfigMap", &json!({})).is_none());
}

#[quench_test::test]
fn the_overview_adds_up_nodes_pods_and_warnings() {
    let nodes = [
        json!({"status": {"allocatable": {"cpu": "4", "memory": "8Gi"},
        "conditions": [{"type": "Ready", "status": "True"}]}}),
    ];
    let usage =
        json!({"items": [{"metadata": {"name": "n"}, "usage": {"cpu": "500m", "memory": "2Gi"}}]});
    let pods = [
        json!({"metadata": {"name": "a", "namespace": "x"}, "spec": {"containers": [{"name": "c"}]}, "status": {"phase": "Running"}}),
        json!({"metadata": {"name": "b", "namespace": "x"}, "spec": {"containers": [{"name": "c"}]},
               "status": {"phase": "Running", "containerStatuses": [{"state": {"waiting": {"reason": "CrashLoopBackOff"}}}]}}),
    ];
    let events = [
        json!({"type": "Warning", "reason": "BackOff", "message": "m", "lastTimestamp": "2026-01-01T00:00:00Z"}),
        json!({"type": "Normal", "reason": "Pulled"}),
    ];
    let o = browse::overview(&nodes, Some(&usage), &pods, &events);
    assert_eq!((o.nodes_ready, o.nodes_total), (1, 1));
    assert_eq!((o.cpu_used_m, o.cpu_total_m), (500, 4000));
    assert_eq!(o.memory_total, 8 * 1024 * 1024 * 1024);
    assert_eq!((o.pods_running, o.pods_total), (1, 2));
    assert_eq!(o.pods_attention.len(), 1);
    assert_eq!(o.warnings.len(), 1);
    assert!(o.metrics);
}

#[quench_test::test]
fn the_facade_links_each_ingress_once_at_its_page_and_marks_what_runs() {
    let tls = json!([{"hosts": ["a.example"]}]);
    let rule = |host: &str, paths: &[&str], service: &str| {
        json!({"host": host, "http": {"paths": paths.iter()
            .map(|p| json!({"path": p, "backend": {"service": {"name": service}}})).collect::<Vec<_>>()}})
    };
    let ingresses = [
        // Several paths: the UI one is the link, the API and asset paths are not.
        json!({"metadata": {"name": "sage", "namespace": "ml"}, "spec": {"tls": tls, "rules": [
            rule("a.example", &["/sage/api/v1", "/sage/ui", "/sage/assets"], "web")]}}),
        json!({"metadata": {"name": "gone", "namespace": "ml"}, "spec": {"rules": [
            rule("b.example", &["/"], "gone")]}}),
        // A regular expression is cut back to what is literal.
        json!({"metadata": {"name": "kindle", "namespace": "ml"}, "spec": {"rules": [
            rule("c.example", &["/kindle(/|$)(.*)"], "web")]}}),
        // Only machinery: nothing to open.
        json!({"metadata": {"name": "registry", "namespace": "ml"}, "spec": {"rules": [
            rule("d.example", &["/v2", "/token"], "web")]}}),
    ];
    let services = [
        json!({"metadata": {"name": "web", "namespace": "ml"}, "spec": {"selector": {"app": "web"}}}),
    ];
    let pods = [
        json!({"metadata": {"namespace": "ml", "labels": {"app": "web"}}, "status": {"phase": "Running"}}),
    ];
    let entries = browse::facade(&ingresses, &services, &pods);
    let urls: Vec<(&str, bool)> = entries
        .iter()
        .map(|e| (e.url.as_str(), e.available))
        .collect();
    assert_eq!(
        urls,
        [
            ("https://a.example/sage/ui", true),
            ("http://b.example/", false),
            ("http://c.example/kindle", true),
        ]
    );
}
