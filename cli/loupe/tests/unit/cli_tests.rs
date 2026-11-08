use loupe::cli::{Cli, Command, Footnotes, Images, Math};
use quench_args::{CommandFactory, Parser};

#[quench_test::test]
fn the_command_is_well_formed() {
    Cli::command().debug_assert();
}

#[quench_test::test]
fn a_file_is_the_positional_path() {
    let cli = Cli::try_parse_from(["loupe", "README.md"]).unwrap();
    assert_eq!(cli.path.as_deref(), Some("README.md"));
    assert!(cli.route.is_none() && cli.command.is_none());
    assert_eq!(cli.footnotes, None);
    assert_eq!(cli.images, None);
    assert_eq!(cli.math_font, None);
    assert_eq!(cli.math, None);
    assert!(!cli.plain);
}

#[quench_test::test]
fn a_dash_is_stdin() {
    let cli = Cli::try_parse_from(["loupe", "-"]).unwrap();
    assert_eq!(cli.path.as_deref(), Some("-"));
}

#[quench_test::test]
fn a_book_takes_a_route() {
    let cli = Cli::try_parse_from(["loupe", "docs/", "services/gauge/metrics"]).unwrap();
    assert_eq!(cli.path.as_deref(), Some("docs/"));
    assert_eq!(cli.route.as_deref(), Some("services/gauge/metrics"));
}

#[quench_test::test]
fn every_flag_parses() {
    let cli = Cli::try_parse_from([
        "loupe",
        "--plain",
        "--width",
        "100",
        "--footnotes",
        "inline",
        "--math",
        "source",
        "--images",
        "off",
        "notes.md",
    ])
    .unwrap();
    assert!(cli.plain);
    assert_eq!(cli.width, Some(100));
    assert_eq!(cli.footnotes, Some(Footnotes::Inline));
    assert_eq!(cli.math, Some(Math::Source));
    assert_eq!(cli.images, Some(Images::Off));
    let cli = Cli::try_parse_from(["loupe", "--math", "text", "x.md"]).unwrap();
    assert_eq!(cli.math, Some(Math::Text));
}

#[quench_test::test]
fn check_and_book_take_values() {
    let cli = Cli::try_parse_from(["loupe", "--check", "docs/"]).unwrap();
    assert_eq!(cli.check.as_deref(), Some(std::path::Path::new("docs/")));
    let cli = Cli::try_parse_from(["loupe", "--book", "forge", "gauge/metrics"]).unwrap();
    assert_eq!(cli.book.as_deref(), Some("forge"));
    assert_eq!(cli.path.as_deref(), Some("gauge/metrics"));
}

#[quench_test::test]
fn doctor_is_a_subcommand() {
    let cli = Cli::try_parse_from(["loupe", "doctor"]).unwrap();
    assert_eq!(cli.command, Some(Command::Doctor));
    assert!(cli.path.is_none());
}

#[quench_test::test]
fn bad_values_are_refused() {
    assert!(Cli::try_parse_from(["loupe", "--math", "images", "x.md"]).is_err());
    assert!(Cli::try_parse_from(["loupe", "--footnotes", "side", "x.md"]).is_err());
    assert!(Cli::try_parse_from(["loupe", "--width", "wide", "x.md"]).is_err());
    assert!(Cli::try_parse_from(["loupe", "--nope", "x.md"]).is_err());
}

#[quench_test::test]
fn a_double_dash_opens_a_file_named_like_a_subcommand() {
    let cli = Cli::try_parse_from(["loupe", "--", "doctor"]).unwrap();
    assert_eq!(cli.command, None);
    assert_eq!(cli.path.as_deref(), Some("doctor"));
    let cli = Cli::try_parse_from(["loupe", "./doctor"]).unwrap();
    assert_eq!(cli.path.as_deref(), Some("./doctor"));
    let cli = Cli::try_parse_from(["loupe", "--math-font", "/f.ttf", "doctor"]).unwrap();
    assert_eq!(cli.command, Some(Command::Doctor));
    assert_eq!(
        cli.math_font.as_deref(),
        Some(std::path::Path::new("/f.ttf"))
    );
}

#[quench_test::test]
fn a_subcommand_hints_at_a_file_of_its_name() {
    use loupe::app::subcommand_hint;
    let dir = crate::support::Scratch::new("hint");
    let args = |words: &[&str]| -> Vec<String> {
        std::iter::once("loupe")
            .chain(words.iter().copied())
            .map(String::from)
            .collect()
    };
    assert_eq!(
        subcommand_hint(&args(&["doctor"]), dir.path()),
        None,
        "no such file"
    );
    dir.write("doctor", "# a file\n");
    std::fs::create_dir(dir.path().join("help")).unwrap();
    let hint = subcommand_hint(&args(&["doctor"]), dir.path()).unwrap();
    assert_eq!(
        hint,
        "loupe: running `loupe doctor`; to open the file named doctor, use `loupe ./doctor` or `loupe -- doctor`"
    );
    assert!(subcommand_hint(&args(&["--width", "50", "help"]), dir.path()).is_some());
    assert!(
        subcommand_hint(&args(&["--math-font", "doctor", "x.md"]), dir.path()).is_none(),
        "a value"
    );
    assert!(subcommand_hint(&args(&["--", "doctor"]), dir.path()).is_none());
    assert!(subcommand_hint(&args(&["./doctor"]), dir.path()).is_none());
    assert!(
        subcommand_hint(&args(&["notes.md", "doctor"]), dir.path()).is_none(),
        "a route, not the first word"
    );
}

#[quench_test::test]
fn book_takes_an_optional_name_and_login_its_flags() {
    let cli = Cli::try_parse_from(["loupe", "--book"]).unwrap();
    assert_eq!(cli.book.as_deref(), Some(""), "alone: list the books");
    let cli = Cli::try_parse_from([
        "loupe",
        "--book",
        "forge@2026.11.08",
        "services/gauge/metrics",
    ])
    .unwrap();
    assert_eq!(cli.book.as_deref(), Some("forge@2026.11.08"));
    assert_eq!(cli.path.as_deref(), Some("services/gauge/metrics"));
    let cli = Cli::try_parse_from([
        "loupe",
        "login",
        "--gatehouse-url",
        "https://g",
        "--username",
        "me",
        "--scope",
        "warehouse:read",
        "--days",
        "90",
    ])
    .unwrap();
    let Some(Command::Login(args)) = cli.command else {
        panic!("not login")
    };
    assert_eq!(
        (
            args.gatehouse_url.as_deref(),
            args.username.as_deref(),
            args.scope.as_slice(),
            args.days
        ),
        (
            Some("https://g"),
            Some("me"),
            &["warehouse:read".to_string()][..],
            Some(90)
        )
    );
}
