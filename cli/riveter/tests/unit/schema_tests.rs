use riveter::schema::{
    CrdSchema, Outcome, SchemaSet, Source, TEMPLATED_CRDS, bundled, display_path, normalize,
};
use serde_json::{Value, json};
use std::sync::{Mutex, OnceLock};

/// A CRD as `kubectl get crd -o json` prints it, with the given per-version schemas.
fn crd(group: &str, kind: &str, versions: &[(&str, bool, Value)]) -> Value {
    json!({
        "apiVersion": "apiextensions.k8s.io/v1",
        "kind": "CustomResourceDefinition",
        "metadata": {"name": format!("{}s.{group}", kind.to_lowercase())},
        "spec": {
            "group": group,
            "scope": "Namespaced",
            "names": {"kind": kind, "plural": format!("{}s", kind.to_lowercase())},
            "versions": versions.iter().map(|(name, served, schema)| json!({
                "name": name, "served": served, "storage": false,
                "schema": {"openAPIV3Schema": schema}
            })).collect::<Vec<_>>(),
        }
    })
}

fn widget_schema() -> Value {
    json!({
        "type": "object",
        "description": "A widget.",
        "required": ["spec"],
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"type": "object"},
            "spec": {
                "type": "object",
                "required": ["size"],
                "properties": {
                    "size": {"type": "integer", "minimum": 1},
                    "mode": {"type": "string", "enum": ["fast", "slow"]},
                    "port": {"x-kubernetes-int-or-string": true},
                    "note": {"type": "string", "nullable": true},
                    "tags": {"type": "array", "items": {"type": "string"}},
                    "labels": {"type": "object", "additionalProperties": {"type": "string"}},
                    "extra": {"type": "object", "x-kubernetes-preserve-unknown-fields": true},
                    "free": {"type": "object"},
                }
            }
        }
    })
}

fn widgets() -> SchemaSet {
    let schema = CrdSchema::from_crd(&crd(
        "example.io",
        "Widget",
        &[("v1", true, widget_schema())],
    ))
    .unwrap();
    let mut set = SchemaSet::empty();
    set.insert(schema, Source::Cache);
    set
}

fn widget(spec: impl Into<Value>) -> Value {
    json!({"apiVersion": "example.io/v1", "kind": "Widget", "metadata": {"name": "w"}, "spec": spec.into()})
}

fn issues(set: &SchemaSet, doc: &Value) -> Vec<String> {
    match set.check(doc) {
        Outcome::Invalid(issues) => issues.iter().map(ToString::to_string).collect(),
        other => panic!("expected invalid, got {other:?}"),
    }
}

// --- reading a CRD -----------------------------------------------------

#[test]
fn reads_names_scope_and_only_served_versions() {
    let schema = CrdSchema::from_crd(&crd(
        "example.io",
        "Widget",
        &[
            ("v1", true, widget_schema()),
            ("v1beta1", true, widget_schema()),
            ("v0", false, widget_schema()),
        ],
    ))
    .unwrap();

    assert_eq!(schema.group, "example.io");
    assert_eq!(schema.kind, "Widget");
    assert_eq!(schema.crd_name(), "widgets.example.io");
    assert_eq!(schema.file_name(), "example.io_widgets.json");
    assert_eq!(schema.scope, "Namespaced");
    assert_eq!(
        schema.versions.keys().collect::<Vec<_>>(),
        ["v1", "v1beta1"]
    );
}

#[test]
fn a_crd_with_no_served_schema_is_an_error() {
    let none = crd("example.io", "Widget", &[("v1", false, widget_schema())]);
    let err = CrdSchema::from_crd(&none).unwrap_err();
    assert!(err.to_string().contains("serves no version"), "{err}");

    let mut broken = crd("example.io", "Widget", &[("v1", true, widget_schema())]);
    broken["spec"]["names"] = json!({});
    assert!(CrdSchema::from_crd(&broken).is_err());
}

#[test]
fn stripping_keeps_fields_that_are_named_like_annotations() {
    // `default`, `title`, `description` and `example` are legitimate field names; as keys of a
    // `properties` map they are fields, not documentation, and must survive.
    let schema = json!({
        "type": "object",
        "description": "documentation",
        "title": "documentation",
        "default": {},
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "default": {"type": "string", "description": "a field named default", "default": "x"},
            "title": {"type": "string", "example": "doc"},
            "description": {"type": "string"},
            "example": {"type": "object", "properties": {"title": {"type": "integer"}}},
        }
    });
    let parsed =
        CrdSchema::from_crd(&crd("example.io", "Widget", &[("v1", true, schema)])).unwrap();
    let kept = &parsed.versions["v1"];

    assert!(kept.get("description").is_none());
    assert!(kept.get("title").is_none());
    assert!(kept.get("default").is_none());
    let fields = kept["properties"].as_object().unwrap();
    assert_eq!(fields.len(), 6, "{fields:?}");
    assert!(
        fields["default"].get("description").is_none(),
        "its documentation goes"
    );
    assert_eq!(fields["example"]["properties"]["title"]["type"], "integer");

    // And they still validate as the fields they are.
    let mut set = SchemaSet::empty();
    set.insert(parsed, Source::Cache);
    let doc = json!({"apiVersion": "example.io/v1", "kind": "Widget", "default": "a", "title": "b", "description": "c"});
    assert_eq!(set.check(&doc), Outcome::Valid);
}

#[test]
fn cel_rules_are_counted_not_evaluated() {
    let schema = json!({
        "type": "object",
        "x-kubernetes-validations": [{"rule": "true"}, {"rule": "false"}],
        "properties": {"spec": {
            "type": "object",
            "x-kubernetes-validations": [{"rule": "self.a > 1"}],
            "properties": {"a": {"type": "integer"}}
        }}
    });
    let parsed =
        CrdSchema::from_crd(&crd("example.io", "Widget", &[("v1", true, schema)])).unwrap();
    assert_eq!(parsed.cel_rules, 3);
    assert!(
        parsed.versions["v1"]
            .get("x-kubernetes-validations")
            .is_none()
    );
}

// --- normalising -------------------------------------------------------

#[test]
fn objects_that_list_properties_are_closed() {
    let n = normalize(
        &json!({"type": "object", "properties": {"a": {"type": "object", "properties": {"b": {"type": "string"}}}}}),
    );
    assert_eq!(n["additionalProperties"], false);
    assert_eq!(n["properties"]["a"]["additionalProperties"], false);
}

#[test]
fn objects_that_may_hold_anything_stay_open() {
    let free = normalize(&json!({"type": "object"}));
    assert!(free.get("additionalProperties").is_none());

    let preserved = normalize(
        &json!({"type": "object", "x-kubernetes-preserve-unknown-fields": true, "properties": {"a": {"type": "string"}}}),
    );
    assert!(preserved.get("additionalProperties").is_none());
    assert!(
        preserved
            .get("x-kubernetes-preserve-unknown-fields")
            .is_none()
    );

    let map = normalize(
        &json!({"type": "object", "properties": {"a": {}}, "additionalProperties": {"type": "string"}}),
    );
    assert_eq!(map["additionalProperties"], json!({"type": "string"}));
}

#[test]
fn combinator_branches_are_not_closed() {
    // A branch lists only the properties it constrains; closing it would reject everything else.
    let n = normalize(&json!({
        "type": "object",
        "properties": {"a": {}, "b": {}},
        "oneOf": [{"properties": {"a": {"type": "string"}}}, {"properties": {"b": {"type": "string"}}}],
    }));
    assert_eq!(n["additionalProperties"], false);
    assert!(n["oneOf"][0].get("additionalProperties").is_none());
    assert!(n["oneOf"][1].get("additionalProperties").is_none());
}

#[test]
fn int_or_string_and_nullable_are_translated() {
    let n =
        normalize(&json!({"x-kubernetes-int-or-string": true, "type": "string", "pattern": "^a"}));
    assert_eq!(n["anyOf"], json!([{"type": "integer"}, {"type": "string"}]));
    assert!(n.get("type").is_none());
    assert_eq!(n["pattern"], "^a");

    let n = normalize(&json!({"type": "string", "nullable": true}));
    assert_eq!(n["type"], json!(["string", "null"]));
    assert!(n.get("nullable").is_none());

    let n = normalize(&json!({"nullable": true, "enum": ["a"]}));
    assert_eq!(n["anyOf"][1], json!({"type": "null"}));
}

#[test]
fn keywords_json_schema_lacks_are_removed() {
    let n = normalize(&json!({
        "type": "integer", "minimum": 1, "exclusiveMinimum": true, "format": "int64",
        "x-kubernetes-list-type": "set", "x-kubernetes-map-type": "atomic",
    }));
    assert_eq!(n, json!({"type": "integer", "minimum": 1}));

    // A numeric exclusiveMinimum is already JSON Schema and stays.
    let n = normalize(&json!({"type": "integer", "exclusiveMinimum": 0}));
    assert_eq!(n["exclusiveMinimum"], 0);
}

// --- checking documents ------------------------------------------------

#[test]
fn a_conforming_document_is_valid() {
    let doc = widget(json!({
        "size": 3, "mode": "fast", "port": 8080, "note": null, "tags": ["a"],
        "labels": {"k": "v"}, "extra": {"anything": [1, 2]}, "free": {"x": 1},
    }));
    assert_eq!(widgets().check(&doc), Outcome::Valid);
    assert_eq!(
        widgets().check(&widget(json!({"size": 1, "port": "http"}))),
        Outcome::Valid
    );
}

#[test]
fn documents_without_a_schema_are_skipped() {
    let set = widgets();
    assert_eq!(
        set.check(&json!({"apiVersion": "apps/v1", "kind": "Deployment"})),
        Outcome::Skipped
    );
    assert_eq!(
        set.check(&json!({"apiVersion": "v1", "kind": "Service"})),
        Outcome::Skipped
    );
    assert_eq!(set.check(&json!({"kind": "Widget"})), Outcome::Skipped);
    assert_eq!(set.check(&json!("not an object")), Outcome::Skipped);
    assert_eq!(
        SchemaSet::empty().check(&widget(json!({}))),
        Outcome::Skipped
    );
}

#[test]
fn an_unknown_field_is_an_error_with_a_suggestion() {
    let found = issues(&widgets(), &widget(json!({"size": 1, "mdoe": "fast"})));
    assert_eq!(found, ["spec: unknown field `mdoe` (did you mean `mode`?)"]);
}

#[test]
fn a_swap_of_two_letters_is_one_edit() {
    let found = issues(&widgets(), &widget(json!({"size": 1, "tgas": []})));
    assert!(found[0].contains("did you mean `tags`"), "{found:?}");
}

#[test]
fn a_name_nothing_like_a_field_gets_no_suggestion() {
    let found = issues(
        &widgets(),
        &widget(json!({"size": 1, "completely-unrelated": 1})),
    );
    assert_eq!(found, ["spec: unknown field `completely-unrelated`"]);
}

#[test]
fn several_unknown_fields_are_reported_together_without_guessing() {
    let found = issues(&widgets(), &widget(json!({"size": 1, "aaa": 1, "bbb": 2})));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("`aaa`, `bbb`"), "{found:?}");
    assert!(!found[0].contains("did you mean"));
}

#[test]
fn types_required_fields_enums_and_minimums_are_enforced() {
    let set = widgets();
    assert!(issues(&set, &widget(json!({"size": "big"})))[0].contains("spec.size"));
    assert!(issues(&set, &widget(json!({})))[0].contains("\"size\" is a required property"));
    assert!(issues(&set, &widget(json!({"size": 1, "mode": "medium"})))[0].contains("spec.mode"));
    assert!(issues(&set, &widget(json!({"size": 0})))[0].contains("spec.size"));
    assert!(issues(&set, &widget(json!({"size": 1, "tags": "a"})))[0].contains("spec.tags"));
    assert!(issues(&set, &widget(json!({"size": 1, "tags": [1]})))[0].contains("spec.tags[0]"));
    assert!(
        issues(&set, &widget(json!({"size": 1, "labels": {"k": 1}})))[0].contains("spec.labels.k")
    );
    assert!(issues(&set, &widget(json!({"size": 1, "port": 1.5})))[0].contains("spec.port"));
}

#[test]
fn a_missing_top_level_field_is_reported_at_the_document() {
    let doc = json!({"apiVersion": "example.io/v1", "kind": "Widget", "metadata": {"name": "w"}});
    let found = issues(&widgets(), &doc);
    assert!(
        found[0].contains("\"spec\" is a required property"),
        "{found:?}"
    );
    assert!(
        !found[0].starts_with(": "),
        "the document's own path is empty, not a dangling colon"
    );
}

#[test]
fn a_version_the_crd_does_not_serve_is_named_with_those_it_does() {
    let doc = json!({"apiVersion": "example.io/v7", "kind": "Widget", "metadata": {"name": "w"}, "spec": {"size": 1}});
    let found = issues(&widgets(), &doc);
    assert_eq!(found.len(), 1);
    assert!(
        found[0].starts_with("apiVersion: example.io/v7 is not a version of widgets.example.io"),
        "{found:?}"
    );
    assert!(found[0].contains("serves: v1"));
}

#[test]
fn each_served_version_is_checked_against_its_own_schema() {
    let v1 = widget_schema();
    let mut v2 = widget_schema();
    v2["properties"]["spec"]["properties"]["size"] = json!({"type": "string"});
    let mut set = SchemaSet::empty();
    set.insert(
        CrdSchema::from_crd(&crd(
            "example.io",
            "Widget",
            &[("v1", true, v1), ("v2", true, v2)],
        ))
        .unwrap(),
        Source::Cache,
    );

    let as_v = |version: &str, size: Value| json!({"apiVersion": format!("example.io/{version}"), "kind": "Widget", "spec": {"size": size}});
    assert_eq!(set.check(&as_v("v1", json!(2))), Outcome::Valid);
    assert_eq!(set.check(&as_v("v2", json!("2"))), Outcome::Valid);
    assert!(matches!(
        set.check(&as_v("v2", json!(2))),
        Outcome::Invalid(_)
    ));
}

#[test]
fn repeated_checks_of_one_kind_agree() {
    // The compiled validator is cached; the second answer must be the first's.
    let set = widgets();
    let bad = widget(json!({"size": 1, "mdoe": "x"}));
    assert_eq!(set.check(&bad), set.check(&bad));
    assert_eq!(
        set.check(&widget(json!({"size": 1}))),
        set.check(&widget(json!({"size": 2})))
    );
}

#[test]
fn core_group_documents_have_an_empty_group() {
    let schema = CrdSchema::from_crd(&crd("", "Thing", &[("v1", true, json!({"type": "object", "properties": {"kind": {"type": "string"}, "apiVersion": {"type": "string"}, "spec": {"type": "object", "properties": {"a": {"type": "string"}}}}}))])).unwrap();
    let mut set = SchemaSet::empty();
    set.insert(schema, Source::Cache);
    let doc = json!({"apiVersion": "v1", "kind": "Thing", "spec": {"a": 1}});
    assert!(matches!(set.check(&doc), Outcome::Invalid(_)));
}

// --- holding schemas ---------------------------------------------------

#[test]
fn a_cached_schema_replaces_a_bundled_one_of_the_same_kind() {
    let mut set = bundled().unwrap();
    let before = set
        .get("cert-manager.io", "Certificate")
        .unwrap()
        .versions
        .clone();

    let permissive = CrdSchema::from_crd(&crd(
        "cert-manager.io",
        "Certificate",
        &[("v1", true, json!({"type": "object"}))],
    ))
    .unwrap();
    set.insert(permissive, Source::Cache);

    let held = set
        .list()
        .into_iter()
        .find(|(s, _)| s.kind == "Certificate")
        .unwrap();
    assert_eq!(held.1, Source::Cache);
    assert_ne!(held.0.versions, before);
}

#[test]
fn load_dir_reads_schemas_ignores_a_missing_directory_and_refuses_junk() {
    let dir = tempfile::tempdir().unwrap();
    let mut set = SchemaSet::empty();

    set.load_dir(&dir.path().join("does-not-exist"), Source::Cache)
        .unwrap();
    assert!(set.list().is_empty());

    let schema = CrdSchema::from_crd(&crd(
        "example.io",
        "Widget",
        &[("v1", true, widget_schema())],
    ))
    .unwrap();
    std::fs::write(
        dir.path().join(schema.file_name()),
        serde_json::to_string(&schema).unwrap(),
    )
    .unwrap();
    std::fs::write(dir.path().join("README.txt"), "not a schema, and not .json").unwrap();
    set.load_dir(dir.path(), Source::Cache).unwrap();
    assert_eq!(set.list().len(), 1);

    std::fs::write(dir.path().join("junk.json"), "{\"nope\": 1}").unwrap();
    let err = SchemaSet::empty()
        .load_dir(dir.path(), Source::Cache)
        .unwrap_err();
    assert!(format!("{err:#}").contains("junk.json"), "{err:#}");
}

#[test]
fn the_list_is_sorted_by_group_then_kind() {
    let mut set = SchemaSet::empty();
    for (group, kind) in [("z.io", "A"), ("a.io", "B"), ("a.io", "A")] {
        set.insert(
            CrdSchema::from_crd(&crd(
                group,
                kind,
                &[("v1", true, json!({"type": "object"}))],
            ))
            .unwrap(),
            Source::Cache,
        );
    }
    let order: Vec<String> = set
        .list()
        .iter()
        .map(|(s, _)| format!("{}/{}", s.group, s.kind))
        .collect();
    assert_eq!(order, ["a.io/A", "a.io/B", "z.io/A"]);
}

// --- the bundle --------------------------------------------------------

#[test]
fn the_bundle_holds_exactly_the_crds_riveter_has_templates_for() {
    let set = bundled().unwrap();
    let held: Vec<String> = set.list().iter().map(|(s, _)| s.crd_name()).collect();

    let mut expected: Vec<String> = TEMPLATED_CRDS
        .iter()
        .map(|(g, p)| format!("{p}.{g}"))
        .collect();
    expected.sort();
    let mut held_sorted = held;
    held_sorted.sort();
    assert_eq!(held_sorted, expected);
    assert!(
        set.list()
            .iter()
            .all(|(_, source)| *source == Source::Bundled)
    );
}

#[test]
fn every_bundled_schema_compiles_and_rejects_a_typo() {
    let set = bundled().unwrap();
    for (schema, _) in set.list() {
        for version in schema.versions.keys() {
            let doc = json!({
                "apiVersion": format!("{}/{version}", schema.group),
                "kind": schema.kind,
                "metadata": {"name": "x"},
                "spec": {"definitelyNotAField": 1},
            });
            // Compiles (not "no such version"), and a field no CRD has is refused.
            let found = issues(&set, &doc);
            assert!(
                found.iter().any(|i| i.contains("definitelyNotAField")),
                "{}/{version}: {found:?}",
                schema.kind
            );
        }
    }
}

#[test]
fn a_certificate_is_checked_against_the_real_cert_manager_schema() {
    let set = bundled().unwrap();
    let good = json!({
        "apiVersion": "cert-manager.io/v1", "kind": "Certificate", "metadata": {"name": "c"},
        "spec": {"secretName": "s", "dnsNames": ["a.example"], "issuerRef": {"name": "i", "kind": "ClusterIssuer"},
                 "duration": "2160h", "privateKey": {"algorithm": "ECDSA", "size": 256}},
    });
    assert_eq!(set.check(&good), Outcome::Valid);

    let mut bad = good;
    bad["spec"]["privateKey"]["algorithm"] = json!("Blowfish");
    assert!(issues(&set, &bad)[0].contains("spec.privateKey.algorithm"));
}

// --- paths -------------------------------------------------------------

#[test]
fn json_pointers_read_as_field_paths() {
    assert_eq!(display_path(""), "");
    assert_eq!(display_path("/spec"), "spec");
    assert_eq!(display_path("/spec/dnsNames/0"), "spec.dnsNames[0]");
    assert_eq!(
        display_path("/spec/routes/2/services/0/port"),
        "spec.routes[2].services[0].port"
    );
    assert_eq!(display_path("/spec/a~1b/c~0d"), "spec.a/b.c~d");
}

// --- where the cache lives ---------------------------------------------

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn the_cache_directory_follows_the_documented_precedence() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let saved: Vec<_> = ["RIVETER_SCHEMA_DIR", "XDG_CACHE_HOME", "HOME"]
        .iter()
        .map(|k| (*k, std::env::var_os(k)))
        .collect();

    envmnt::set("RIVETER_SCHEMA_DIR", "/explicit");
    envmnt::set("XDG_CACHE_HOME", "/xdg");
    envmnt::set("HOME", "/home/u");
    assert_eq!(
        riveter::schema::cache_dir().unwrap(),
        std::path::Path::new("/explicit")
    );

    envmnt::remove("RIVETER_SCHEMA_DIR");
    assert_eq!(
        riveter::schema::cache_dir().unwrap(),
        std::path::Path::new("/xdg/riveter/schemas")
    );

    envmnt::remove("XDG_CACHE_HOME");
    assert_eq!(
        riveter::schema::cache_dir().unwrap(),
        std::path::Path::new("/home/u/.cache/riveter/schemas")
    );

    envmnt::remove("HOME");
    assert!(riveter::schema::cache_dir().is_err());

    for (key, value) in saved {
        match value {
            Some(v) => envmnt::set(key, v),
            None => envmnt::remove(key),
        }
    }
}
