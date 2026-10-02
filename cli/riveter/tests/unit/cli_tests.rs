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

/// Conveyor validates a pipeline's `riveter` steps against its own list of commands, because it
/// cannot depend on this crate. Two lists that must agree are two lists that drift - `images`
/// was already missing from it - so this fails the moment either side changes alone.
#[test]
fn conveyors_riveter_step_accepts_every_command_riveter_has() {
    use clap::CommandFactory;
    use conveyor_pipeline::steps::riveter::COMMANDS;

    // Interactive: it waits for input a pipeline never sends, and conveyor refuses it by design.
    const NOT_FOR_PIPELINES: [&str; 1] = ["repl"];

    let cli = riveter::cli::Cli::command();
    for sub in cli.get_subcommands() {
        let names = std::iter::once(sub.get_name()).chain(sub.get_visible_aliases());
        for name in names {
            if NOT_FOR_PIPELINES.contains(&name) {
                assert!(
                    !COMMANDS.contains(&name),
                    "conveyor must refuse the interactive `{name}`"
                );
            } else {
                assert!(
                    COMMANDS.contains(&name),
                    "`riveter {name}` exists, but conveyor-pipeline's riveter step would reject it: \
                     add it to `COMMANDS` in libs/conveyor-pipeline/src/steps/riveter.rs"
                );
            }
        }
    }
}

/// And the other way: nothing conveyor accepts may be a command riveter lacks, or a pipeline
/// would parse and then fail at deploy time.
#[test]
fn riveter_has_every_command_conveyors_step_accepts() {
    use clap::CommandFactory;
    use conveyor_pipeline::steps::riveter::COMMANDS;

    let cli = riveter::cli::Cli::command();
    let known: Vec<&str> = cli
        .get_subcommands()
        .flat_map(|sub| std::iter::once(sub.get_name()).chain(sub.get_visible_aliases()))
        .collect();

    for command in COMMANDS {
        assert!(
            known.contains(&command),
            "conveyor accepts `riveter {command}`, which riveter does not have"
        );
    }
}
