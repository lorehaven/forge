use forge_toolbox::{parse_installed_list, parse_search_line};
use semver::Version;

#[test]
fn parse_search_line_matches_exact_package_line() {
    let line = r#"anvil = "0.1.22"    # Anvil CLI - workspace build tools for cargo projects"#;
    let result = parse_search_line("anvil", line).unwrap();
    assert_eq!(result, Some(Version::parse("0.1.22").unwrap()));
}

#[test]
fn parse_search_line_ignores_lines_for_other_packages() {
    let line = r#"anvil-extra = "0.1.0""#;
    // "anvil-extra" must not match a search for "anvil" (prefix + `" "` guards this).
    let result = parse_search_line("anvil", line).unwrap();
    assert_eq!(result, None);
}

#[test]
fn parse_search_line_ignores_unrelated_lines() {
    let result = parse_search_line("anvil", "... and 3 crates not shown").unwrap();
    assert_eq!(result, None);
}

#[test]
fn parse_search_line_errors_on_unterminated_quote() {
    let line = r#"anvil = "0.1.22"#; // missing closing quote
    assert!(parse_search_line("anvil", line).is_err());
}

#[test]
fn parse_search_line_errors_on_invalid_semver() {
    let line = r#"anvil = "not-a-version""#;
    assert!(parse_search_line("anvil", line).is_err());
}

#[test]
fn parse_search_line_handles_leading_whitespace() {
    let line = "   anvil = \"1.0.0\"";
    let result = parse_search_line("anvil", line).unwrap();
    assert_eq!(result, Some(Version::parse("1.0.0").unwrap()));
}

#[test]
fn parse_installed_list_extracts_package_and_version() {
    let stdout = "anvil v0.1.22:\n    anvil\nriveter v0.2.3:\n    riveter\n";
    let versions = parse_installed_list(stdout);
    assert_eq!(
        versions.get("anvil"),
        Some(&Version::parse("0.1.22").unwrap())
    );
    assert_eq!(
        versions.get("riveter"),
        Some(&Version::parse("0.2.3").unwrap())
    );
    assert_eq!(versions.len(), 2);
}

#[test]
fn parse_installed_list_handles_registry_suffix() {
    let stdout = "riveter v0.2.3 (registry `ennor`):\n    riveter\n";
    let versions = parse_installed_list(stdout);
    // The parser only reads the first two whitespace-separated tokens, so a
    // trailing "(registry ...)" annotation doesn't break the version parse -
    // but the line must still end with ':' to be considered a header line.
    assert_eq!(
        versions.get("riveter"),
        Some(&Version::parse("0.2.3").unwrap())
    );
}

#[test]
fn parse_installed_list_skips_indented_binary_lines() {
    let stdout = "anvil v0.1.22:\n    anvil\n    anvil-helper\n";
    let versions = parse_installed_list(stdout);
    assert_eq!(versions.len(), 1);
    assert!(!versions.contains_key("anvil-helper"));
}

#[test]
fn parse_installed_list_skips_malformed_version() {
    let stdout = "broken-package vNOTASEMVER:\n    broken-package\n";
    let versions = parse_installed_list(stdout);
    assert!(versions.is_empty());
}

#[test]
fn parse_installed_list_empty_input() {
    let versions = parse_installed_list("");
    assert!(versions.is_empty());
}

#[test]
fn parse_installed_list_ignores_lines_without_trailing_colon() {
    let stdout = "some free text with no colon\n    anvil\n";
    let versions = parse_installed_list(stdout);
    assert!(versions.is_empty());
}
