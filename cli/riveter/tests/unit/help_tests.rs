use riveter::help::{
    COMMANDS, Surface, command_tree, detail, find, find_on, overview, targets, unknown_topic,
};
use std::collections::HashSet;

#[test]
fn every_command_and_alias_resolves() {
    for cmd in COMMANDS {
        assert!(find(cmd.name).is_some(), "`{}` not findable", cmd.name);
        for alias in cmd.aliases {
            let found = find(alias).expect("alias should resolve");
            assert_eq!(found.name, cmd.name, "alias `{alias}` resolved elsewhere");
        }
    }
}

#[test]
fn names_and_aliases_are_unique() {
    let mut seen = HashSet::new();
    for cmd in COMMANDS {
        for token in std::iter::once(&cmd.name).chain(cmd.aliases) {
            assert!(seen.insert(*token), "`{token}` is claimed twice");
        }
    }
}

#[test]
fn overview_lists_every_repl_command() {
    let text = overview();
    for cmd in COMMANDS {
        if cmd.surface == Surface::Cli {
            continue;
        }
        assert!(text.contains(cmd.name), "overview omits `{}`", cmd.name);
        assert!(
            text.contains(cmd.summary),
            "overview omits summary for `{}`",
            cmd.name
        );
    }
}

/// The REPL has no `repl` command and the CLI has no `exit`; neither tree may
/// advertise a command that surface does not have.
#[test]
fn each_tree_shows_only_its_own_commands() {
    let cli = command_tree(Surface::Cli);
    let repl = command_tree(Surface::Repl);

    assert!(cli.contains("repl"), "cli tree omits `repl`");
    assert!(!cli.contains("Leave the REPL"), "cli tree offers `exit`");

    assert!(repl.contains("Leave the REPL"), "repl tree omits `exit`");
    assert!(
        !repl.contains("Start the interactive REPL"),
        "repl tree offers `repl`"
    );

    // Commands on both surfaces appear in both trees.
    for cmd in COMMANDS.iter().filter(|c| c.surface == Surface::Both) {
        assert!(cli.contains(cmd.summary), "cli tree omits `{}`", cmd.name);
        assert!(repl.contains(cmd.summary), "repl tree omits `{}`", cmd.name);
    }
}

#[test]
fn find_on_respects_the_surface() {
    assert!(find_on("repl", Surface::Cli).is_some());
    assert!(find_on("repl", Surface::Repl).is_none());
    assert!(find_on("exit", Surface::Repl).is_some());
    assert!(find_on("exit", Surface::Cli).is_none());
    assert!(find_on("apply", Surface::Cli).is_some());
    assert!(find_on("a", Surface::Repl).is_some());
}

#[test]
fn overview_documents_targets_and_scopes() {
    let text = overview();
    assert!(text.contains("Targets:"));
    assert!(text.contains("Scopes:"));
}

#[test]
fn detail_covers_options_subcommands_and_examples() {
    for cmd in COMMANDS {
        let text = detail(cmd);
        assert!(text.starts_with(cmd.name), "detail for `{}`", cmd.name);

        for (flag, about) in cmd.options {
            assert!(text.contains(flag), "`{}` detail omits {flag}", cmd.name);
            assert!(text.contains(about), "`{}` detail omits {about}", cmd.name);
        }
        for (name, _) in cmd.subcommands {
            assert!(text.contains(name), "`{}` detail omits {name}", cmd.name);
        }
        for (example, _) in cmd.examples {
            assert!(text.contains(example), "`{}` detail omits {example}", cmd.name);
        }

        assert_eq!(
            text.contains("Targets:"),
            cmd.targets,
            "`{}` targets section does not match its flag",
            cmd.name
        );
    }
}

#[test]
fn scope_commands_explain_the_scopes() {
    for cmd in COMMANDS {
        if cmd.options.iter().any(|(f, _)| f.starts_with("--scope")) {
            assert!(
                detail(cmd).contains("Scopes:"),
                "`{}` takes --scope but does not explain it",
                cmd.name
            );
        }
    }
}

#[test]
fn targets_topic_is_indented() {
    let text = targets();
    assert!(text.starts_with("Targets:\n"));
    assert!(
        text.lines().skip(1).filter(|l| !l.is_empty()).all(|l| l.starts_with("  ")),
        "targets topic should be indented like every other block"
    );
}

#[test]
fn unknown_topic_lists_only_that_surfaces_topics() {
    for on in [Surface::Cli, Surface::Repl] {
        let msg = unknown_topic("bogus", on);
        assert!(msg.contains("bogus"));
        assert!(msg.contains("targets"));

        for cmd in COMMANDS {
            let listed = msg.contains(cmd.name);
            let available = cmd.surface == Surface::Both || cmd.surface == on;
            assert_eq!(listed, available, "`{}` on {on:?}", cmd.name);
        }
    }
}
