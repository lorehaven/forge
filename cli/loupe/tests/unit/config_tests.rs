use loupe::cli::{Cli, Footnotes, Images, Math};
use loupe::config::{Config, Effective, path};
use quench_args::Parser;
use std::path::{Path, PathBuf};

fn parse(text: &str) -> Result<Config, String> {
    Config::parse(
        text,
        Path::new("/home/me/.config/loupe/config.toml"),
        Some("/home/me"),
    )
}

#[quench_test::test]
fn the_file_is_under_xdg_config_home_or_home() {
    assert_eq!(
        path(Some("/xdg"), Some("/home/me")),
        Some(PathBuf::from("/xdg/loupe/config.toml"))
    );
    assert_eq!(
        path(None, Some("/home/me")),
        Some(PathBuf::from("/home/me/.config/loupe/config.toml"))
    );
    assert_eq!(
        path(Some("relative"), Some("/home/me")),
        Some(PathBuf::from("/home/me/.config/loupe/config.toml")),
        "a relative XDG_CONFIG_HOME is ignored, as the spec says"
    );
    assert_eq!(path(None, None), None);
}

#[quench_test::test]
fn every_setting_is_read() {
    let config = parse(
        "math_font = \"~/fonts/Math.ttf\"\nfootnotes = \"inline\"\nmath = \"text\"\nimages = \"off\"\nwidth = 100\n\
         warehouse_url = \"https://w/warehouse\"\nstorage = \"books\"\ngatehouse_url = \"https://g/gatehouse\"\n",
    )
    .unwrap();
    assert_eq!(
        config,
        Config {
            math_font: Some(PathBuf::from("/home/me/fonts/Math.ttf")),
            footnotes: Some(Footnotes::Inline),
            math: Some(Math::Text),
            images: Some(Images::Off),
            width: Some(100),
            warehouse_url: Some("https://w/warehouse".to_string()),
            storage: Some("books".to_string()),
            gatehouse_url: Some("https://g/gatehouse".to_string()),
        }
    );
    assert_eq!(parse("").unwrap(), Config::default());
    let relative = parse("math_font = \"Math.ttf\"\n").unwrap();
    assert_eq!(
        relative.math_font,
        Some(PathBuf::from("/home/me/.config/loupe/Math.ttf"))
    );
}

#[quench_test::test]
fn bad_settings_are_named() {
    assert!(
        parse("colour = \"red\"\n")
            .unwrap_err()
            .contains("unknown setting `colour`")
    );
    assert!(
        parse("math = \"images\"\n")
            .unwrap_err()
            .contains("expected one of source, text")
    );
    assert!(parse("width = 0\n").unwrap_err().contains("`width`"));
    assert!(parse("width = \"wide\"\n").unwrap_err().contains("`width`"));
    assert!(
        parse("footnotes = 3\n")
            .unwrap_err()
            .contains("must be a string")
    );
    assert!(parse("not toml at all [").is_err());
}

#[quench_test::test]
fn flags_win_over_the_file() {
    let config =
        parse("footnotes = \"inline\"\nimages = \"off\"\nwidth = 100\nmath = \"source\"\n")
            .unwrap();
    let cli = Cli::try_parse_from(["loupe", "x.md"]).unwrap();
    let options = Effective::of(&cli, &config);
    assert_eq!(
        (
            options.footnotes,
            options.images,
            options.width,
            options.math
        ),
        (
            Footnotes::Inline,
            Images::Off,
            Some(100),
            Some(Math::Source)
        )
    );
    let cli = Cli::try_parse_from([
        "loupe",
        "--footnotes",
        "end",
        "--images",
        "on",
        "--width",
        "60",
        "--math",
        "text",
        "--math-font",
        "/other.ttf",
        "x.md",
    ])
    .unwrap();
    let config = Config {
        math_font: Some(PathBuf::from("/config.ttf")),
        ..config
    };
    let options = Effective::of(&cli, &config);
    assert_eq!(
        (
            options.footnotes,
            options.images,
            options.width,
            options.math
        ),
        (Footnotes::End, Images::On, Some(60), Some(Math::Text))
    );
    assert_eq!(options.math_font, Some(PathBuf::from("/other.ttf")));
    let defaults = Effective::of(
        &Cli::try_parse_from(["loupe", "x.md"]).unwrap(),
        &Config::default(),
    );
    assert_eq!(
        (
            defaults.footnotes,
            defaults.images,
            defaults.width,
            defaults.math
        ),
        (Footnotes::End, Images::On, None, None)
    );
}
