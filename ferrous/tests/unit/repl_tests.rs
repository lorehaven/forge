use ferrous::ui::repl::{ReplCommand, ReplParseResult, parse_repl_input};

#[test]
fn parses_plain_prompt_text() {
    let parsed = parse_repl_input("explain src/main.rs");
    assert_eq!(
        parsed,
        ReplParseResult::Command(ReplCommand::UserPrompt("explain src/main.rs".to_string()))
    );
}

#[test]
fn parses_exit_aliases() {
    assert_eq!(
        parse_repl_input("quit"),
        ReplParseResult::Command(ReplCommand::Exit)
    );
    assert_eq!(
        parse_repl_input("/q"),
        ReplParseResult::Command(ReplCommand::Exit)
    );
}

#[test]
fn parses_help_variants() {
    assert_eq!(
        parse_repl_input("/help"),
        ReplParseResult::Command(ReplCommand::Help { verbose: false })
    );
    assert_eq!(
        parse_repl_input("help all"),
        ReplParseResult::Command(ReplCommand::Help { verbose: true })
    );
}

#[test]
fn validates_required_args() {
    assert_eq!(
        parse_repl_input("/load"),
        ReplParseResult::UsageError("Usage: /load <name prefix or short id>".to_string())
    );
    assert_eq!(
        parse_repl_input("delete"),
        ReplParseResult::UsageError("Usage: /delete <name prefix or short id>".to_string())
    );
}

#[test]
fn unknown_slash_command_returns_error() {
    let parsed = parse_repl_input("/nope");
    assert!(matches!(parsed, ReplParseResult::UsageError(_)));
}
