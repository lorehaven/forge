use rivet_package::Manifest;
use rivet_package::manifest::{is_valid_name, parse_package_requirement};

#[test]
fn parses_a_minimal_manifest() {
    let m = Manifest::parse("[package]\nname = \"forge\"\nversion = \"0.4.0\"\n").unwrap();
    assert_eq!(m.package.name, "forge");
    assert_eq!(m.version().unwrap().to_string(), "0.4.0");
    assert!(m.requires.is_empty());
}

#[test]
fn accepts_build_metadata_in_the_version() {
    let m = Manifest::parse("[package]\nname = \"forge\"\nversion = \"0.4.0+b123\"\n").unwrap();
    assert_eq!(m.version().unwrap().build.as_str(), "b123");
}

#[test]
fn rejects_bad_names_and_versions() {
    for source in [
        "[package]\nname = \"Forge\"\nversion = \"1.0.0\"\n",
        "[package]\nname = \"-forge\"\nversion = \"1.0.0\"\n",
        "[package]\nname = \"forge\"\nversion = \"1.0\"\n",
        "[package]\nname = \"forge\"\nversion = \"latest\"\n",
        "[package]\nname = \"\"\nversion = \"1.0.0\"\n",
    ] {
        assert!(Manifest::parse(source).is_err(), "{source}");
    }
}

#[test]
fn rejects_unknown_package_keys_so_typos_surface() {
    let source = "[package]\nname = \"forge\"\nversion = \"1.0.0\"\nnamspace = \"x\"\n";
    assert!(Manifest::parse(source).is_err());
}

#[test]
fn validates_requirements() {
    let ok = "[package]\nname = \"a\"\nversion = \"1.0.0\"\n[requires]\nriveter = \">=0.3\"\npackages = [\"postgres >=1\", \"redis\"]\n";
    let m = Manifest::parse(ok).unwrap();
    assert_eq!(m.requires.packages.len(), 2);

    for bad in [
        "[requires]\nriveter = \"nonsense\"\n",
        "[requires]\npackages = [\"Bad Name >=1\"]\n",
        "[requires]\npackages = [\"redis >=x\"]\n",
    ] {
        let source = format!("[package]\nname = \"a\"\nversion = \"1.0.0\"\n{bad}");
        assert!(Manifest::parse(&source).is_err(), "{bad}");
    }
}

#[test]
fn package_requirement_without_a_constraint_matches_any_version() {
    let (name, req) = parse_package_requirement("redis").unwrap();
    assert_eq!(name, "redis");
    assert!(req.matches(&semver::Version::new(9, 9, 9)));
}

#[test]
fn rejects_a_multiline_description() {
    let mut m = Manifest::new("a", "1.0.0");
    m.package.description = Some("one\ntwo".to_string());
    assert!(m.validate().is_err());
}

#[test]
fn round_trips_through_toml() {
    let mut m = Manifest::new("forge", "0.4.0+b1");
    m.package.namespace = Some("forge".to_string());
    m.requires.riveter = Some(">=0.3".to_string());
    m.meta.insert("gpu".to_string(), toml::Value::Boolean(true));
    let text = m.to_toml().unwrap();
    assert_eq!(Manifest::parse(&text).unwrap(), m);
}

#[test]
fn name_rule_matches_dns_1123_labels() {
    assert!(is_valid_name("a"));
    assert!(is_valid_name("media-2"));
    assert!(is_valid_name(&"a".repeat(63)));
    assert!(!is_valid_name(&"a".repeat(64)));
    assert!(!is_valid_name("a_b"));
    assert!(!is_valid_name("a-"));
}

const HEADER: &str = "[package]\nname = \"ml\"\nversion = \"1.0.0\"\n";

#[test]
fn deployments_are_declared_and_survive_a_round_trip() {
    let source = format!(
        "{HEADER}\n\
         [[deployment]]\nname = \"inference\"\nresources = [\"deployment/sage\", \"deployment/switchboard\"]\n\
         conflicts_with = [\"training\"]\n\
         [[deployment.also_stops]]\nselector = \"app=vllm\"\n\n\
         [[deployment]]\nname = \"training\"\ndefault = \"stopped\"\nresources = [\"statefulset/trainer\"]\n"
    );
    let manifest = Manifest::parse(&source).unwrap();
    assert_eq!(manifest.deployments.len(), 2);
    assert_eq!(
        manifest.deployments[0].resources,
        ["deployment/sage", "deployment/switchboard"]
    );
    assert_eq!(
        manifest.deployments[0].default,
        rivet_package::DefaultState::Running
    );
    assert_eq!(manifest.deployments[0].also_stops[0].selector, "app=vllm");
    assert_eq!(
        manifest.deployments[1].default,
        rivet_package::DefaultState::Stopped
    );

    let again = Manifest::parse(&manifest.to_toml().unwrap()).unwrap();
    assert_eq!(again, manifest);
}

#[test]
fn a_manifest_without_deployments_is_unchanged() {
    let manifest = Manifest::parse(HEADER).unwrap();
    assert!(manifest.deployments.is_empty());
    assert!(!manifest.to_toml().unwrap().contains("deployment"));
}

#[test]
fn bad_deployments_are_refused_with_the_reason() {
    let bad = |body: &str| {
        Manifest::parse(&format!("{HEADER}\n[[deployment]]\n{body}"))
            .unwrap_err()
            .to_string()
    };

    assert!(bad("name = \"Bad\"\nresources = [\"deployment/a\"]").contains("DNS-1123"));
    assert!(bad("name = \"a\"\nresources = []").contains("no resources"));
    assert!(bad("name = \"a\"\nresources = [\"secret/a\"]").contains("deployment/<name>"));
    assert!(
        bad("name = \"a\"\nresources = [\"daemonset/a\"]").contains("only those can be scaled")
    );
    assert!(
        bad("name = \"a\"\nresources = [\"deployment/a\"]\nconflicts_with = [\"a\"]")
            .contains("itself")
    );
    assert!(bad("name = \"a\"\nresources = [\"deployment/a\"]\n[[deployment.also_stops]]\nselector = \"-A\"").contains("selector"));
    assert!(
        bad("name = \"a\"\nresources = [\"deployment/a\"]\nsurprise = 1").contains("unknown field")
    );

    let twice = format!(
        "{HEADER}\n[[deployment]]\nname = \"a\"\nresources = [\"deployment/a\"]\n[[deployment]]\nname = \"a\"\nresources = [\"deployment/b\"]\n"
    );
    assert!(
        Manifest::parse(&twice)
            .unwrap_err()
            .to_string()
            .contains("twice")
    );
}
