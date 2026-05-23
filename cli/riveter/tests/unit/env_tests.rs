use riveter::env::manifest_path;

#[test]
fn test_manifest_path() {
    assert_eq!(manifest_path("prod"), "manifests/prod-manifests.yaml");
    assert_eq!(manifest_path("dev"), "manifests/dev-manifests.yaml");
}
