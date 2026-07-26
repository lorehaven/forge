use riveter::help::{COMMANDS, detail, find, overview, targets, unknown_topic};
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
fn overview_lists_every_command() {
    let text = overview();
    for cmd in COMMANDS {
        assert!(text.contains(cmd.name), "overview omits `{}`", cmd.name);
        assert!(
            text.contains(cmd.summary),
            "overview omits summary for `{}`",
            cmd.name
        );
    }
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
fn unknown_topic_lists_the_real_ones() {
    let msg = unknown_topic("bogus");
    assert!(msg.contains("bogus"));
    for cmd in COMMANDS {
        assert!(msg.contains(cmd.name), "hint omits `{}`", cmd.name);
    }
    assert!(msg.contains("targets"));
}
