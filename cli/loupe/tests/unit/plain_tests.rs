use crate::support::{NoNet, Scratch};
use loupe::app::{Env, Outcome, run};
use loupe::cli::Cli;
use quench_args::Parser;
use quench_markdown_term::display_width;
use std::io::Cursor;

/// Runs a command line with `stdin` and the environment; returns the outcome, stdout and stderr.
fn loupe(args: &[&str], env: &Env, stdin: &str) -> (Outcome, String, String) {
    let mut words = vec!["loupe"];
    words.extend_from_slice(args);
    let cli = Cli::try_parse_from(words).unwrap();
    let mut input = Cursor::new(stdin.as_bytes().to_vec());
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let outcome = run(&cli, env, &NoNet, &mut input, &mut out, &mut err);
    (
        outcome,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

fn piped() -> Env {
    Env::default()
}

fn status(outcome: &Outcome) -> Option<i32> {
    match outcome {
        Outcome::Exit(s) => Some(*s),
        _ => None,
    }
}

const PAGE: &str = "---\ntitle: A page\nnav_order: 2\n---\n# Heading\n\nSome *text* with [a link](https://example.com).\n";

#[quench_test::test]
fn a_file_with_front_matter_prints_without_it() {
    let dir = Scratch::new("front");
    let file = dir.write("page.md", PAGE);
    let (outcome, out, err) = loupe(&[file.to_str().unwrap()], &piped(), "");
    assert_eq!(status(&outcome), Some(0), "{err}");
    assert!(
        !out.contains("title:") && !out.contains("nav_order"),
        "{out}"
    );
    assert!(out.starts_with("Heading\n═"), "{out}");
    assert!(out.contains("Some text with a link [1]."), "{out}");
    assert!(out.contains("[1] https://example.com"), "{out}");
    assert!(
        !out.contains('\x1b'),
        "plain output has no escapes: {out:?}"
    );
}

#[quench_test::test]
fn stdin_is_read_for_a_dash() {
    let (outcome, out, _) = loupe(&["-"], &piped(), PAGE);
    assert_eq!(status(&outcome), Some(0));
    assert!(out.starts_with("Heading\n"), "{out}");
}

#[quench_test::test]
fn width_sets_the_layout() {
    let text = "word ".repeat(60);
    let (_, out, _) = loupe(&["--width", "30", "-"], &piped(), &text);
    assert!(out.lines().count() > 5);
    assert!(out.lines().all(|l| display_width(l) <= 30), "{out}");
    assert!(out.lines().any(|l| display_width(l) > 24), "{out}");
}

#[quench_test::test]
fn width_zero_is_refused() {
    let (outcome, _, err) = loupe(&["--width", "0", "-"], &piped(), "x");
    assert_eq!(status(&outcome), Some(2));
    assert!(err.contains("--width"), "{err}");
}

#[quench_test::test]
fn formulas_are_text_or_source() {
    let page = "Area $\\pi r^2$.\n";
    let (_, out, _) = loupe(&["-"], &piped(), page);
    assert_eq!(out, "Area πr².\n");
    let (_, out, _) = loupe(&["--math", "source", "-"], &piped(), page);
    assert_eq!(out, "Area $\\pi r^2$.\n");
}

#[quench_test::test]
fn footnotes_inline_follow_their_paragraph() {
    let page = "Text[^a].\n\nMore.\n\n[^a]: The note.\n";
    let (_, out, _) = loupe(&["--footnotes", "inline", "-"], &piped(), page);
    assert_eq!(out, "Text¹.\n\n  ¹ The note.\n\nMore.\n");
}

#[quench_test::test]
fn a_terminal_gets_the_pager_unless_plain_or_no_color() {
    let tty = Env {
        stdout_tty: true,
        size: Some((60, 20)),
        ..Env::default()
    };
    let (outcome, out, _) = loupe(&["-"], &tty, PAGE);
    match outcome {
        Outcome::Pager(pager, options) => {
            assert!(options.images && options.typeset);
            assert_eq!(pager.width(), 60);
        }
        other => panic!("expected the pager, got {other:?}"),
    }
    assert_eq!(out, "");
    let (outcome, out, _) = loupe(&["--plain", "-"], &tty, PAGE);
    assert_eq!(status(&outcome), Some(0));
    assert!(out.starts_with("Heading"));
    let no_color = Env {
        no_color: true,
        ..tty.clone()
    };
    let (outcome, _, _) = loupe(&["-"], &no_color, PAGE);
    assert_eq!(status(&outcome), Some(0));
    let (outcome, _, _) = loupe(&["--images", "off", "-"], &tty, PAGE);
    assert!(matches!(outcome, Outcome::Pager(_, o) if !o.images && !o.typeset));
    // Formulas as text, but the page's images are still drawn.
    let (outcome, _, _) = loupe(&["--math", "text", "-"], &tty, PAGE);
    assert!(matches!(outcome, Outcome::Pager(_, o) if o.images && !o.typeset));
}

#[quench_test::test]
fn a_missing_file_is_an_error() {
    let (outcome, _, err) = loupe(&["/no/such/file.md"], &piped(), "");
    assert_eq!(status(&outcome), Some(1));
    assert!(
        err.starts_with("loupe: cannot read /no/such/file.md"),
        "{err}"
    );
}

#[quench_test::test]
fn nothing_to_show_is_a_usage_error() {
    let (outcome, _, err) = loupe(&[], &piped(), "");
    assert_eq!(status(&outcome), Some(2));
    assert!(err.contains("nothing to show"), "{err}");
}

#[quench_test::test]
fn doctor_is_handed_back() {
    let (outcome, _, _) = loupe(&["doctor"], &piped(), "");
    assert!(matches!(outcome, Outcome::Doctor(font) if font.fonts.is_ok()));
}

/// A book with a broken link, a page without a title, and a page without front matter.
fn broken_book() -> Scratch {
    let dir = Scratch::new("broken");
    dir.write("_config.yml", "title: Broken\n");
    dir.write(
        "index.md",
        "---\ntitle: Home\nnav_order: 1\n---\nSee [nothing](missing.md).\n",
    );
    dir.write("untitled.md", "---\nnav_order: 2\n---\nNo title.\n");
    dir.write("bare.md", "No front matter at all.\n");
    dir
}

#[quench_test::test]
fn check_reports_a_books_problems() {
    let dir = broken_book();
    let (outcome, out, _) = loupe(&["--check", dir.path().to_str().unwrap()], &piped(), "");
    assert_eq!(status(&outcome), Some(1), "{out}");
    assert!(out.contains("index.md: broken link `missing.md`"), "{out}");
    assert!(out.contains("untitled.md"), "{out}");
    assert!(out.contains("bare.md"), "{out}");
    assert!(out.trim_end().ends_with("problems"), "{out}");
}

#[quench_test::test]
fn check_passes_forges_own_book() {
    let docs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
    if !docs.join("_config.yml").is_file() {
        return; // not in a forge checkout
    }
    let (outcome, out, err) = loupe(&["--check", docs.to_str().unwrap()], &piped(), "");
    assert_eq!(status(&outcome), Some(0), "{out}{err}");
    assert!(out.contains("no problems"), "{out}");
}

#[quench_test::test]
fn check_of_a_missing_directory_fails() {
    let (outcome, _, err) = loupe(&["--check", "/no/such/book"], &piped(), "");
    assert_eq!(status(&outcome), Some(1));
    assert!(err.contains("does not exist"), "{err}");
}

#[quench_test::test]
fn a_book_page_prints_by_route() {
    let dir = Scratch::new("route");
    dir.write("_config.yml", "title: Book\nbaseurl: /book\n");
    dir.write("index.md", "---\ntitle: Home\n---\nWelcome.\n");
    dir.write(
        "guide/setup.md",
        "---\ntitle: Setup\n---\n---\n\nAfter a rule, [home]({{ site.baseurl }}/index.html).\n",
    );
    let book = dir.path().to_str().unwrap();
    let (outcome, out, err) = loupe(&[book, "guide/setup"], &piped(), "");
    assert_eq!(status(&outcome), Some(0), "{err}");
    assert!(out.starts_with('─'), "a leading rule is kept: {out}");
    assert!(out.contains("[1] /book/index.html"), "{out}");
    let (_, out, _) = loupe(&[book], &piped(), "");
    assert_eq!(out, "Welcome.\n");
    let (outcome, _, err) = loupe(&[book, "nowhere"], &piped(), "");
    assert_eq!(status(&outcome), Some(1));
    assert!(err.contains("no page `nowhere`"), "{err}");
}

#[quench_test::test]
fn a_math_font_that_cannot_be_used_warns_once_and_formulas_stay_text() {
    let dir = Scratch::new("badfont");
    let font = dir.write("Latin.otf", "OTTO and then nothing");
    let font = font.to_str().unwrap();
    let page = "Area $\\pi r^2$ and $x^2$.\n";
    let (outcome, out, err) = loupe(&["--math-font", font, "-"], &piped(), page);
    assert_eq!(status(&outcome), Some(0));
    assert_eq!(out, "Area πr² and x².\n");
    assert_eq!(
        err,
        format!(
            "loupe: no math font: {font} is a CFF font: CFF outlines are not read yet; formulas are shown as text\n"
        )
    );
    // No formula, no warning; `--math text` asks for text, so no warning either.
    let (_, _, err) = loupe(&["--math-font", font, "-"], &piped(), "plain words\n");
    assert_eq!(err, "");
    let (_, _, err) = loupe(
        &["--math-font", font, "--math", "text", "-"],
        &piped(),
        page,
    );
    assert_eq!(err, "");
    // The pager has it on its status line.
    let tty = Env {
        stdout_tty: true,
        size: Some((80, 20)),
        ..Env::default()
    };
    let (outcome, _, err) = loupe(&["--math-font", font, "-"], &tty, page);
    assert_eq!(err, "");
    match outcome {
        Outcome::Pager(pager, options) => {
            assert!(
                pager.screen().status.contains("! no math font:"),
                "{}",
                pager.screen().status
            );
            assert!(options.font.fonts.is_err());
        }
        other => panic!("{other:?}"),
    }
}

#[quench_test::test]
fn the_config_file_sets_the_defaults() {
    let env = Env {
        config: Some((
            "/home/me/.config/loupe/config.toml".into(),
            "width = 30\nmath = \"source\"\n".to_string(),
        )),
        ..Env::default()
    };
    let (_, out, err) = loupe(&["-"], &env, &format!("{} $x^2$\n", "word ".repeat(10)));
    assert_eq!(err, "");
    assert!(out.lines().all(|l| display_width(l) <= 30), "{out}");
    assert!(out.contains("$x^2$"), "{out}");
    // A flag wins.
    let (_, out, _) = loupe(&["--math", "text", "-"], &env, "$x^2$\n");
    assert_eq!(out, "x²\n");
    // A broken file is reported and left out.
    let broken = Env {
        config: Some(("/c.toml".into(), "colour = 1\n".to_string())),
        ..Env::default()
    };
    let (outcome, out, err) = loupe(&["-"], &broken, "$x^2$\n");
    assert_eq!(status(&outcome), Some(0));
    assert_eq!(out, "x²\n");
    assert!(
        err.starts_with("loupe: /c.toml: unknown setting `colour`"),
        "{err}"
    );
    assert!(
        err.trim_end().ends_with("the config file is ignored"),
        "{err}"
    );
}

#[quench_test::test]
fn doctor_gets_the_chosen_font() {
    let env = Env {
        config: Some((
            "/c.toml".into(),
            "math_font = \"/no/Math.ttf\"\n".to_string(),
        )),
        ..Env::default()
    };
    let (outcome, _, _) = loupe(&["doctor"], &env, "");
    match outcome {
        Outcome::Doctor(font) => {
            assert_eq!(
                font.source.describe(),
                "/no/Math.ttf (the config file's math_font)"
            );
            assert!(font.fonts.is_err());
        }
        other => panic!("{other:?}"),
    }
}
