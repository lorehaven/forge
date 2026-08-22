use clap::Parser;
use conveyor_cli::cli::*;

#[test]
fn repo_add_defaults_provider_and_branch() {
    let cli = Cli::try_parse_from([
        "conveyor",
        "repo",
        "add",
        "owner/name",
        "https://example.com/owner/name.git",
        "--project",
        "proj-1",
    ])
    .unwrap();
    let Commands::Repo {
        command: RepoCommands::Add(args),
    } = cli.command
    else {
        panic!("expected RepoCommands::Add");
    };
    assert_eq!(args.provider, "github");
    assert_eq!(args.default_branch, "master");
    assert_eq!(args.project, "proj-1");
}

#[test]
fn repo_add_requires_a_project() {
    let result = Cli::try_parse_from([
        "conveyor",
        "repo",
        "add",
        "owner/name",
        "https://example.com/owner/name.git",
    ]);
    assert!(result.is_err());
}

#[test]
fn repo_set_branch_parses_repo_and_branch() {
    let cli =
        Cli::try_parse_from(["conveyor", "repo", "set-branch", "owner/name", "master"]).unwrap();
    let Commands::Repo {
        command: RepoCommands::SetBranch(args),
    } = cli.command
    else {
        panic!("expected RepoCommands::SetBranch");
    };
    assert_eq!(args.repo, "owner/name");
    assert_eq!(args.branch, "master");
}

#[test]
fn project_add_parent_is_optional() {
    let cli = Cli::try_parse_from(["conveyor", "project", "add", "forge"]).unwrap();
    let Commands::Project {
        command: ProjectCommands::Add(args),
    } = cli.command
    else {
        panic!("expected ProjectCommands::Add");
    };
    assert_eq!(args.name, "forge");
    assert_eq!(args.parent, None);
}

#[test]
fn project_move_parent_and_to_root_are_mutually_exclusive() {
    let result = Cli::try_parse_from([
        "conveyor",
        "project",
        "move",
        "proj-1",
        "--parent",
        "proj-2",
        "--to-root",
    ]);
    assert!(result.is_err());
}

#[test]
fn runs_defaults_limit_to_twenty() {
    let cli = Cli::try_parse_from(["conveyor", "runs"]).unwrap();
    let Commands::Runs(args) = cli.command else {
        panic!("expected Commands::Runs");
    };
    assert_eq!(args.limit, 20);
    assert_eq!(args.repo, None);
}

#[test]
fn validate_defaults_path_to_conveyor_toml() {
    let cli = Cli::try_parse_from(["conveyor", "validate"]).unwrap();
    let Commands::Validate(args) = cli.command else {
        panic!("expected Commands::Validate");
    };
    assert_eq!(args.path, ".conveyor.toml");
}

#[test]
fn global_flags_are_readable_after_a_subcommand_is_parsed() {
    let cli = Cli::try_parse_from([
        "conveyor",
        "--url",
        "https://localhost:9443/conveyor",
        "--insecure",
        "repo",
        "list",
    ])
    .unwrap();
    assert_eq!(cli.url.as_deref(), Some("https://localhost:9443/conveyor"));
    assert!(cli.insecure);
}

#[test]
fn run_requires_a_repo_argument() {
    let result = Cli::try_parse_from(["conveyor", "run"]);
    assert!(result.is_err());
}

#[test]
fn secret_set_value_is_optional_so_it_can_be_read_from_stdin() {
    let cli = Cli::try_parse_from(["conveyor", "secret", "set", "API_KEY"]).unwrap();
    let Commands::Secret {
        command: SecretCommands::Set(args),
    } = cli.command
    else {
        panic!("expected SecretCommands::Set");
    };
    assert_eq!(args.name, "API_KEY");
    assert_eq!(args.value, None);
}

#[test]
fn credential_set_token_is_optional_so_it_can_be_read_from_stdin() {
    let cli = Cli::try_parse_from([
        "conveyor",
        "credential",
        "set",
        "GITHUB_TOKEN",
        "--git-username",
        "x-access-token",
        "--repo",
        "owner/name",
    ])
    .unwrap();
    let Commands::Credential {
        command: CredentialCommands::Set(args),
    } = cli.command
    else {
        panic!("expected CredentialCommands::Set");
    };
    assert_eq!(args.name, "GITHUB_TOKEN");
    assert_eq!(args.token, None);
    assert_eq!(args.git_username, "x-access-token");
    assert_eq!(args.repo.as_deref(), Some("owner/name"));
    assert_eq!(args.project, None);
}

#[test]
fn credential_set_requires_a_username() {
    let result = Cli::try_parse_from([
        "conveyor",
        "credential",
        "set",
        "TOKEN",
        "--repo",
        "owner/name",
    ]);
    assert!(result.is_err());
}

#[test]
fn credential_sets_git_username_does_not_clobber_the_login_username() {
    // Regression test for the collision `--git-username` exists to avoid:
    // before the rename, this same command line silently overwrote the
    // global `--username` (the realm login account) instead of setting
    // `CredentialSetArgs.username`.
    let cli = Cli::try_parse_from([
        "conveyor",
        "--username",
        "admin",
        "credential",
        "set",
        "GITHUB_TOKEN",
        "--git-username",
        "x-access-token",
        "--repo",
        "owner/name",
    ])
    .unwrap();
    assert_eq!(cli.username.as_deref(), Some("admin"));
    let Commands::Credential {
        command: CredentialCommands::Set(args),
    } = cli.command
    else {
        panic!("expected CredentialCommands::Set");
    };
    assert_eq!(args.git_username, "x-access-token");
}
