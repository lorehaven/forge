use clap::CommandFactory;
use riveter::cli::{Cli, help_template};

#[test]
fn help_template_embeds_the_command_tree_and_reference() {
    let text = help_template();
    assert!(text.contains("{usage-heading}"));
    assert!(text.contains("Options:"));
}

#[test]
fn the_cli_definition_is_internally_consistent() {
    // clap panics at construction time if a derive is malformed (e.g. a
    // duplicate flag or an invalid `help_template`), so this doubles as
    // the check that `help_template()` itself is well-formed.
    Cli::command().debug_assert();
}
