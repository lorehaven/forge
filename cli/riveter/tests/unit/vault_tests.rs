use riveter::package::{
    InstallValues, defaults_from, merge_values, pack, parse_values_yaml, prepare_install,
};
use riveter::vault::{
    self, Vault, decrypt_value, encrypt_value, identities_from, parse_identities,
};
use std::collections::HashMap;

fn keys() -> (Vec<age::x25519::Identity>, String) {
    let (secret, public) = vault::keygen();
    (parse_identities(&secret).unwrap(), public)
}

#[test]
fn a_generated_key_is_an_age_key_and_its_recipient_matches() {
    let (secret, public) = vault::keygen();
    assert!(secret.starts_with("AGE-SECRET-KEY-1"), "{secret}");
    assert!(public.starts_with("age1"), "{public}");
    let identities = parse_identities(&format!(
        "# created: now\n# public key: {public}\n{secret}\n"
    ))
    .unwrap();
    assert_eq!(identities[0].to_public().to_string(), public);
    assert!(parse_identities("# only a comment\n").is_err());
    assert!(parse_identities("not a key").is_err());
}

#[test]
fn a_value_round_trips_and_only_its_recipients_can_open_it() {
    let (mine, my_public) = keys();
    let (theirs, their_public) = keys();
    let (stranger, _) = keys();

    let one = encrypt_value(std::slice::from_ref(&my_public), "hunter2").unwrap();
    assert!(one.starts_with("ENC[age,") && one.ends_with(']'));
    assert!(!one.contains("hunter2"));
    assert_eq!(decrypt_value(&mine, &one).unwrap(), "hunter2");
    assert!(
        decrypt_value(&stranger, &one)
            .unwrap_err()
            .to_string()
            .contains("does not open")
    );

    // To two recipients, either can open it.
    let both = encrypt_value(&[my_public, their_public], "shared").unwrap();
    assert_eq!(decrypt_value(&mine, &both).unwrap(), "shared");
    assert_eq!(decrypt_value(&theirs, &both).unwrap(), "shared");

    // Encrypting twice differs (a fresh nonce each time) but both open.
    assert_ne!(
        encrypt_value(&[keys().1], "x").unwrap(),
        encrypt_value(&[keys().1], "x").unwrap()
    );
}

#[test]
fn a_value_that_is_not_utf8_or_not_ours_is_a_clear_error() {
    let (mine, _) = keys();
    assert!(
        decrypt_value(&mine, "plain text")
            .unwrap_err()
            .to_string()
            .contains("ENC[age")
    );
    assert!(
        decrypt_value(&mine, "ENC[age,!!!]")
            .unwrap_err()
            .to_string()
            .contains("base64")
    );
    assert!(encrypt_value(&[], "x").is_err());
    assert!(encrypt_value(&["age1nope".to_string()], "x").is_err());
}

#[test]
fn a_file_keeps_names_readable_sorted_and_the_rest_untouched_when_one_value_changes() {
    let (identities, public) = keys();
    let mut file = Vault::new(vec![public]);
    file.set("ZED", "z").unwrap();
    file.set("ALPHA", "a").unwrap();
    let before = file.data.clone();

    file.set("ZED", "changed").unwrap();
    assert_eq!(
        file.data["ALPHA"], before["ALPHA"],
        "an untouched value is byte for byte the same"
    );
    assert_ne!(file.data["ZED"], before["ZED"]);

    let text = file.render().unwrap();
    assert!(text.starts_with("# Encrypted with age"));
    assert!(
        text.find("ALPHA").unwrap() < text.find("ZED").unwrap(),
        "sorted: {text}"
    );
    assert!(!text.contains("changed"));

    let back = Vault::parse(&text).unwrap();
    assert_eq!(back, file);
    assert_eq!(back.get(&identities, "ZED").unwrap(), "changed");
    assert_eq!(back.names().collect::<Vec<_>>(), ["ALPHA", "ZED"]);
}

#[test]
fn a_plaintext_value_in_a_secrets_file_is_refused_so_it_is_never_shipped() {
    let (_, public) = keys();
    let text =
        format!("riveter-secrets: 1\nrecipients:\n  - {public}\ndata:\n  PASSWORD: hunter2\n");
    let error = Vault::parse(&text).unwrap_err().to_string();
    assert!(
        error.contains("PASSWORD") && error.contains("not encrypted"),
        "{error}"
    );

    assert!(
        Vault::parse("data: {}\n")
            .unwrap_err()
            .to_string()
            .contains("marker")
    );
    assert!(
        Vault::parse("riveter-secrets: 9\ndata: {}\n")
            .unwrap_err()
            .to_string()
            .contains("version")
    );
    assert!(Vault::parse("riveter-secrets: 1\nrecipients: [nope]\n").is_err());
}

#[test]
fn rekeying_moves_access_from_the_old_key_to_the_new_one() {
    let (old, old_public) = keys();
    let (new, new_public) = keys();
    let mut file = Vault::new(vec![old_public]);
    file.set("A", "1").unwrap();
    file.set("B", "2").unwrap();

    file.rekey(&old, vec![new_public.clone()]).unwrap();
    assert_eq!(file.recipients, [new_public]);
    assert_eq!(file.get(&new, "A").unwrap(), "1");
    assert_eq!(file.get(&new, "B").unwrap(), "2");
    assert!(
        file.get(&old, "A").is_err(),
        "the old key no longer opens it"
    );

    // A key that cannot open the file cannot rekey it.
    assert!(file.rekey(&old, vec![keys().1]).is_err());
}

#[test]
fn names_must_be_usable_variable_names() {
    let (_, public) = keys();
    let mut file = Vault::new(vec![public]);
    for bad in ["", "has space", "semi;colon", "a=b"] {
        assert!(file.set(bad, "x").is_err(), "{bad}");
    }
    assert!(file.set("OK_name.1-x", "x").is_ok());
}

// ---------------------------------------------------------------- finding the key

#[test]
fn the_key_is_found_inline_then_in_a_named_file_then_in_the_default_place() {
    let (secret, _) = vault::keygen();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("age.key");
    std::fs::write(&file, format!("# comment\n{secret}\n")).unwrap();

    let env = |pairs: Vec<(&'static str, String)>| {
        let map: HashMap<&str, String> = pairs.into_iter().collect();
        move |name: &str| map.get(name).cloned()
    };

    let inline = identities_from(env(vec![("RIVETER_AGE_KEY", secret)]))
        .unwrap()
        .unwrap();
    assert_eq!(inline.len(), 1);

    let named = identities_from(env(vec![(
        "RIVETER_AGE_KEY_FILE",
        file.display().to_string(),
    )]))
    .unwrap()
    .unwrap();
    assert_eq!(
        named[0].to_public().to_string(),
        inline[0].to_public().to_string()
    );

    // Inline wins over a file.
    let (other, _) = vault::keygen();
    let both = identities_from(env(vec![
        ("RIVETER_AGE_KEY", other.clone()),
        ("RIVETER_AGE_KEY_FILE", file.display().to_string()),
    ]))
    .unwrap()
    .unwrap();
    assert_eq!(both[0].to_string().expose_secret_for_test(), other);

    // The default place, under HOME.
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".config/riveter")).unwrap();
    std::fs::copy(&file, home.join(".config/riveter/age.key")).unwrap();
    let default = identities_from(env(vec![("HOME", home.display().to_string())]))
        .unwrap()
        .unwrap();
    assert_eq!(default.len(), 1);

    // No key anywhere is not an error; a file named and missing is.
    assert!(
        identities_from(env(vec![("HOME", dir.path().display().to_string())]))
            .unwrap()
            .is_none()
    );
    let missing = identities_from(env(vec![(
        "RIVETER_AGE_KEY_FILE",
        "/nonexistent/key".to_string(),
    )]));
    assert!(missing.is_err_and(|e| e.to_string().contains("not a file")));
    assert!(identities_from(env(vec![("RIVETER_AGE_KEY", "garbage".to_string())])).is_err());
}

trait ExposeForTest {
    fn expose_secret_for_test(&self) -> String;
}
impl ExposeForTest for age::secrecy::SecretString {
    fn expose_secret_for_test(&self) -> String {
        use age::secrecy::ExposeSecret;
        self.expose_secret().to_string()
    }
}

// ---------------------------------------------------------------- values.yaml

#[test]
fn values_yaml_is_a_flat_mapping_of_scalars() {
    let values =
        parse_values_yaml("# defaults\nHOST: example.org\nPORT: 8080\nDEBUG: true\nEMPTY:\n")
            .unwrap();
    assert_eq!(values["HOST"], "example.org");
    assert_eq!(values["PORT"], "8080");
    assert_eq!(values["DEBUG"], "true");
    assert_eq!(values["EMPTY"], "");
    assert!(parse_values_yaml("").unwrap().is_empty());

    assert!(
        parse_values_yaml("LIST: [a, b]")
            .unwrap_err()
            .to_string()
            .contains("LIST")
    );
    assert!(parse_values_yaml("NESTED:\n  a: 1\n").is_err());
    assert!(parse_values_yaml("- a\n- b\n").is_err());
    assert!(parse_values_yaml("bad name: 1\n").is_err());
}

#[test]
fn an_overlay_carries_values_yaml_or_values_toml_never_both() {
    let yaml = b"A: 1\n".as_slice();
    let toml = b"A = \"2\"\n".as_slice();
    assert_eq!(
        defaults_from(|n| (n == "values.yaml").then_some(yaml)).unwrap()["A"],
        "1"
    );
    assert_eq!(
        defaults_from(|n| (n == "values.toml").then_some(toml)).unwrap()["A"],
        "2"
    );
    assert!(defaults_from(|_| None).unwrap().is_empty());
    let both = defaults_from(|n| Some(if n == "values.yaml" { yaml } else { toml })).unwrap_err();
    assert!(both.to_string().contains("keep one"), "{both}");
}

// ---------------------------------------------------------------- through a package

fn overlay_with(secret_key: Option<&vault::Vault>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("overlays/app");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("overlay.yaml"), "env: app\nnamespace_name: app-ns\n\nresources:\n{% include \"app/base.yaml.j2\" %}\n{% include \"app/config.yaml.j2\" %}\n").unwrap();
    std::fs::write(
        root.join("base.yaml.j2"),
        "- kind: namespace\n  immutable: true\n",
    )
    .unwrap();
    std::fs::write(
        root.join("config.yaml.j2"),
        "- kind: configmap\n  name: app-config\n  data:\n    HOST: ${HOST}\n    LEVEL: ${LEVEL}\n- kind: secret\n  name: app-secret\n  string_data:\n    TOKEN: ${TOKEN}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("rivet.toml"),
        "[package]\nname = \"app\"\nversion = \"1.0.0\"\n",
    )
    .unwrap();
    std::fs::write(root.join("values.yaml"), "HOST: example.org\nLEVEL: info\n").unwrap();
    if let Some(vault) = secret_key {
        vault.write(&root.join("secrets.yaml")).unwrap();
    }
    dir
}

fn packed(dir: &tempfile::TempDir) -> rivet_package::Package {
    let out = dir.path().join("out");
    let options = riveter::package::PackOptions {
        env: "app",
        overlays_dir: &dir.path().join("overlays"),
        out_dir: &out,
        version_suffix: None,
    };
    let packed = pack(&options, None).unwrap();
    rivet_package::Package::read(
        std::fs::File::open(&packed.path).unwrap(),
        &rivet_package::Limits::default(),
    )
    .unwrap()
}

#[test]
fn a_package_carries_its_values_and_its_encrypted_secrets_and_no_plaintext() {
    let (identities, public) = keys();
    let mut secrets = Vault::new(vec![public]);
    secrets.set("TOKEN", "s3cret-token").unwrap();
    let dir = overlay_with(Some(&secrets));

    let package = packed(&dir);
    assert!(package.files.contains_key("values.yaml"));
    assert!(package.files.contains_key("secrets.yaml"));
    for (name, bytes) in &package.files {
        assert!(
            !String::from_utf8_lossy(bytes).contains("s3cret-token"),
            "{name} holds the secret in the clear"
        );
    }

    // Opened with the key, every variable is there: from the values, from the secrets.
    let merged = merge_values(
        &package,
        &InstallValues {
            identities: Some(&identities),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(merged["HOST"], "example.org");
    assert_eq!(merged["TOKEN"], "s3cret-token");
}

#[test]
fn encrypted_names_count_as_supplied_and_a_missing_key_says_how_to_name_one() {
    let (_, public) = keys();
    let mut secrets = Vault::new(vec![public]);
    secrets.set("TOKEN", "x").unwrap();
    let dir = overlay_with(Some(&secrets));
    let package = packed(&dir);

    let error = merge_values(&package, &InstallValues::default()).unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("RIVETER_AGE_KEY"), "{message}");
}

#[test]
fn precedence_is_values_then_secrets_then_the_env_file_then_set() {
    let (identities, public) = keys();
    let mut secrets = Vault::new(vec![public]);
    secrets.set("HOST", "from-secrets").unwrap();
    secrets.set("TOKEN", "from-secrets").unwrap();
    let dir = overlay_with(Some(&secrets));
    let package = packed(&dir);

    let env_file = dir.path().join("extra.env");
    std::fs::write(&env_file, "LEVEL=from-env-file\nTOKEN=from-env-file\n").unwrap();
    let merged = merge_values(
        &package,
        &InstallValues {
            env_file: Some(&env_file),
            sets: &["TOKEN=from-set".to_string()],
            identities: Some(&identities),
        },
    )
    .unwrap();
    assert_eq!(merged["HOST"], "from-secrets", "secrets beat the defaults");
    assert_eq!(
        merged["LEVEL"], "from-env-file",
        "the env file beats the defaults"
    );
    assert_eq!(merged["TOKEN"], "from-set", "--set beats everything");
}

#[test]
fn an_install_renders_the_secret_with_the_decrypted_value_and_the_configmap_with_the_defaults() {
    let (identities, public) = keys();
    let mut secrets = Vault::new(vec![public]);
    secrets.set("TOKEN", "s3cret-token").unwrap();
    let dir = overlay_with(Some(&secrets));
    let package = packed(&dir);

    let vars = merge_values(
        &package,
        &InstallValues {
            identities: Some(&identities),
            ..Default::default()
        },
    )
    .unwrap();
    let installation =
        prepare_install(&package, vars, std::collections::BTreeMap::default()).unwrap();
    let rendered = riveter::env::with_workspace(installation.workspace.clone(), || {
        riveter::render::generate_manifests_selected(
            &installation.env,
            riveter::render::ResourceScope::All,
            &riveter::render::Selector::default(),
        )
    })
    .unwrap();
    let manifest = std::fs::read_to_string(&rendered.path).unwrap();
    assert!(manifest.contains("s3cret-token"), "{manifest}");
    assert!(manifest.contains("example.org"));
}

#[test]
fn a_plaintext_secrets_file_cannot_be_packed() {
    let (_, public) = keys();
    let dir = overlay_with(None);
    std::fs::write(
        dir.path().join("overlays/app/secrets.yaml"),
        format!("riveter-secrets: 1\nrecipients:\n  - {public}\ndata:\n  TOKEN: plain\n"),
    )
    .unwrap();
    let out = dir.path().join("out");
    let options = riveter::package::PackOptions {
        env: "app",
        overlays_dir: &dir.path().join("overlays"),
        out_dir: &out,
        version_suffix: None,
    };
    let error = pack(&options, None).unwrap_err();
    assert!(format!("{error:#}").contains("not encrypted"), "{error:#}");
}

// ---------------------------------------------------------------- straight from the overlay directory

#[test]
fn rendering_from_the_overlay_directory_reads_values_secrets_and_a_local_env_in_that_order() {
    let (secret, public) = vault::keygen();
    let mut secrets = Vault::new(vec![public]);
    secrets.set("TOKEN", "from-secrets").unwrap();
    secrets.set("LEVEL", "from-secrets").unwrap();
    let dir = overlay_with(Some(&secrets));
    std::fs::write(dir.path().join("overlays/app/.env"), "LEVEL=from-dot-env\n").unwrap();

    let workspace = riveter::env::Workspace {
        overlays_dir: Some(dir.path().join("overlays")),
        output_dir: Some(dir.path().join("manifests")),
        age_key: Some(secret),
        ..Default::default()
    };
    let rendered = riveter::env::with_workspace(workspace, || {
        riveter::render::generate_manifests_selected(
            "app",
            riveter::render::ResourceScope::All,
            &riveter::render::Selector::default(),
        )
    })
    .unwrap();
    let manifest = std::fs::read_to_string(&rendered.path).unwrap();
    assert!(manifest.contains("example.org"), "values.yaml: {manifest}");
    assert!(
        manifest.contains("from-secrets"),
        "secrets.yaml: {manifest}"
    );
    assert!(
        manifest.contains("from-dot-env"),
        ".env wins over both: {manifest}"
    );
    assert!(!manifest.contains("level: info"));
}

// ---------------------------------------------------------------- the commands

#[test]
fn the_secrets_commands_create_list_import_and_remove_without_printing_a_value() {
    use riveter::cli::SecretsCmd;
    use riveter::secrets_cmd::secrets_command;

    let (identities, public) = keys();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("secrets.yaml");

    secrets_command(&SecretsCmd::Set {
        file: file.clone(),
        name: "ONE".into(),
        value: Some("1".into()),
        recipients: vec![public],
    })
    .unwrap();

    let dotenv = dir.path().join(".env");
    std::fs::write(&dotenv, "TWO=2\nTHREE=\"three\"\nFOUR=4\n").unwrap();
    secrets_command(&SecretsCmd::Import {
        file: file.clone(),
        from: dotenv.clone(),
        names: vec!["TWO".into(), "THREE".into()],
        recipients: vec![],
    })
    .unwrap();

    let vault = Vault::read(&file).unwrap();
    assert_eq!(vault.names().collect::<Vec<_>>(), ["ONE", "THREE", "TWO"]);
    assert_eq!(
        vault.get(&identities, "THREE").unwrap(),
        "three",
        "a dotenv's quotes are not part of the value"
    );

    // A name that is not in the dotenv is refused before anything is written.
    let before = std::fs::read_to_string(&file).unwrap();
    assert!(
        secrets_command(&SecretsCmd::Import {
            file: file.clone(),
            from: dotenv,
            names: vec!["NOPE".into()],
            recipients: vec![],
        })
        .is_err()
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);

    secrets_command(&SecretsCmd::Remove {
        file: file.clone(),
        name: "ONE".into(),
    })
    .unwrap();
    assert!(!Vault::read(&file).unwrap().data.contains_key("ONE"));
    assert!(
        secrets_command(&SecretsCmd::Remove {
            file: file.clone(),
            name: "ONE".into()
        })
        .is_err()
    );

    // An existing file keeps its recipients; asking for others is a rekey, not a set.
    let (_, other) = keys();
    assert!(
        secrets_command(&SecretsCmd::Set {
            file,
            name: "X".into(),
            value: Some("x".into()),
            recipients: vec![other],
        })
        .is_err()
    );
}

#[test]
fn keygen_writes_a_private_key_once_with_owner_only_permissions() {
    use riveter::cli::SecretsCmd;
    use riveter::secrets_cmd::secrets_command;
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("keys/age.key");

    secrets_command(&SecretsCmd::Keygen {
        out: Some(path.clone()),
    })
    .unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("AGE-SECRET-KEY-1") && text.contains("# public key: age1"));
    assert_eq!(parse_identities(&text).unwrap().len(), 1);

    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );

    // Never overwrites a key: that would lose every secret encrypted to it.
    assert!(secrets_command(&SecretsCmd::Keygen { out: Some(path) }).is_err());
}

// ---------------------------------------------------------------- a changed config rolls the workload out

fn config_overlay(level: &str, secret: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("overlays/cfg");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("overlay.yaml"),
        "env: cfg\nnamespace_name: cfg-ns\n\nresources:\n{% include \"cfg/all.yaml.j2\" %}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("all.yaml.j2"),
        format!(
            "- kind: configmap\n  name: web-config\n  data:\n    LEVEL: {level}\n\
             - kind: secret\n  name: web-secret\n  string_data:\n    TOKEN: {secret}\n\
             - kind: deployment\n  name: reads\n  image: example/reads:1\n  replicas: 1\n  port: 80\n  \
               env_from_config_maps: [web-config]\n  env_from_secrets: [web-secret]\n\
             - kind: deployment\n  name: reads-nothing\n  image: example/plain:1\n  replicas: 1\n  port: 80\n\
             - kind: deployment\n  name: reads-elsewhere\n  image: example/else:1\n  replicas: 1\n  port: 80\n  \
               env_from_config_maps: [not-in-this-overlay]\n"
        ),
    )
    .unwrap();
    dir
}

fn render_cfg(dir: &tempfile::TempDir) -> Vec<serde_yaml::Value> {
    let workspace = riveter::env::Workspace {
        overlays_dir: Some(dir.path().join("overlays")),
        output_dir: Some(dir.path().join("manifests")),
        env_vars: Some(HashMap::new()),
        ..Default::default()
    };
    let rendered = riveter::env::with_workspace(workspace, || {
        riveter::render::generate_manifests_selected(
            "cfg",
            riveter::render::ResourceScope::All,
            &riveter::render::Selector::default(),
        )
    })
    .unwrap();
    serde_yaml::Deserializer::from_str(&std::fs::read_to_string(&rendered.path).unwrap())
        .map(|doc| serde::Deserialize::deserialize(doc).unwrap())
        .collect()
}

fn hash_of(docs: &[serde_yaml::Value], name: &str) -> Option<String> {
    docs.iter()
        .find(|d| d["kind"] == "Deployment" && d["metadata"]["name"] == name)
        .and_then(|d| {
            d["spec"]["template"]["metadata"]["annotations"]["riveter.forge/config-hash"]
                .as_str()
                .map(str::to_string)
        })
}

#[test]
fn a_workload_carries_a_hash_of_the_config_it_reads_and_it_moves_when_that_does() {
    let first = render_cfg(&config_overlay("info", "one"));
    let same = render_cfg(&config_overlay("info", "one"));
    let level_changed = render_cfg(&config_overlay("debug", "one"));
    let secret_changed = render_cfg(&config_overlay("info", "two"));

    let hash = hash_of(&first, "reads").expect("a workload reading config is stamped");
    assert_eq!(hash.len(), 16);
    assert_eq!(
        hash_of(&same, "reads").unwrap(),
        hash,
        "the same config is the same hash"
    );
    assert_ne!(
        hash_of(&level_changed, "reads").unwrap(),
        hash,
        "a ConfigMap value rolls it out"
    );
    assert_ne!(
        hash_of(&secret_changed, "reads").unwrap(),
        hash,
        "so does a Secret value"
    );

    // Nothing else is touched: no hash for a workload that reads none of this overlay's config.
    assert!(hash_of(&first, "reads-nothing").is_none());
    assert!(
        hash_of(&first, "reads-elsewhere").is_none(),
        "config the overlay does not declare is not its to hash"
    );
}

#[test]
fn the_hash_does_not_reveal_a_secret_and_a_selected_workload_still_gets_it() {
    let dir = config_overlay("info", "very-secret-value");
    let workspace = riveter::env::Workspace {
        overlays_dir: Some(dir.path().join("overlays")),
        output_dir: Some(dir.path().join("manifests")),
        env_vars: Some(HashMap::new()),
        ..Default::default()
    };
    // Only the Deployment is selected: the ConfigMap and Secret it reads are not rendered, but still count.
    let rendered = riveter::env::with_workspace(workspace, || {
        riveter::render::generate_manifests_selected(
            "cfg",
            riveter::render::ResourceScope::All,
            &riveter::render::Selector::parse(&["deployment/reads"]).unwrap(),
        )
    })
    .unwrap();
    let manifest = std::fs::read_to_string(&rendered.path).unwrap();
    assert!(manifest.contains("riveter.forge/config-hash"), "{manifest}");
    assert!(!manifest.contains("very-secret-value"));
    assert!(!manifest.contains("kind: Secret"));
}
