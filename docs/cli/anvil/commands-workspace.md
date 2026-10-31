---
title: Workspace commands
parent: anvil
grand_parent: Command-line tools
nav_order: 1
---

Commands that wrap `cargo` and the dependency checkers. None of them reads the configuration file. `--silent` and `--tail[=N]` apply to all that stream output (see [index](index.md#global-options)).

## anvil build

```text
anvil build [--all] [--all-features] [--release] [-p, --package <PACKAGE>]
```

| Flag | Type | Default | Meaning |
|---|---|---|---|
| `--all` | flag | off | Build the whole workspace. |
| `--all-features` | flag | off | Pass `--all-features`. |
| `--release` | flag | off | Pass `--release`. |
| `-p`, `--package <PACKAGE>` | string | none | Build one package. |

Runs `cargo build [--workspace] [--all-features] [--release] [--package <p>]`. `--workspace` is added when `--all` is given **or** no `--package` is given, so `anvil build` and `anvil build --all` are the same. With `--package`, `--workspace` is not added (and `--all` together with `--package` still adds it: `cargo build --workspace --package p`). Log: `target/anvil-logs/build.log`.

```bash
anvil build -p quench-args --release
```

```text
anvil running build operation...
   Compiling quench-args v0.1.4 (...)
anvil build operation completed successfully (3.21s)
```

*Unverified:* the sample output above is the shape of the status lines taken from `util.rs`; the compiler lines depend on the workspace.

## anvil clean

```text
anvil clean
```

Runs `cargo clean` (whole `target/`). No flags besides the global ones. Log: `target/anvil-logs/clean.log`, which lives in `target/` and is therefore deleted by the clean itself.

## anvil test

```text
anvil test [--all] [-p, --package <PACKAGE>] [--ignored] [--list] [--json] [TEST_NAME]
```

| Argument | Meaning |
|---|---|
| `TEST_NAME` | Positional test-name filter, passed to cargo before the `--`. |
| `--all` | Whole workspace. |
| `-p`, `--package <PACKAGE>` | One package. |
| `--ignored` | `-- --ignored` (run only ignored tests). |
| `--list` | `-- --list` (list tests, run none). |
| `--json` | Run `cargo +nightly test ... -- ... -Z unstable-options --format json`; stdout carries only the libtest JSON events. |

Runs `cargo test [--workspace] [--package <p>] [TEST_NAME] [-- --ignored --list -Z unstable-options --format json]`. `--workspace` is added when `--all` is given or `--package` is absent. Log: `target/anvil-logs/test.log` (not written for `--json`).

```bash
anvil test -p quench-args parse::
anvil test --json > results.jsonl
```

`--json` needs the `nightly` toolchain installed; anvil does not check for it.

## anvil nextest

```text
anvil nextest [--all] [-p, --package <PACKAGE>] [--ignored] [--json] [TEST_NAME]
```

Requires `cargo-nextest` on `PATH` (otherwise: `cargo-nextest not found. Install with: cargo install cargo-nextest`).

Runs `cargo nextest run [--message-format libtest-json-plus] [--workspace] [--package <p>] [--run-ignored ignored-only] [TEST_NAME]`; with `--json` it sets `NEXTEST_EXPERIMENTAL_LIBTEST_JSON=1`. Flag meanings match `test`, except there is no `--list`. Doctests are not run by nextest. Log: `target/anvil-logs/nextest.log`.

## anvil lint

```text
anvil lint [--all-targets] [--all-features] [--deny-warnings] [--json]
```

| Flag | Default | Meaning |
|---|---|---|
| `--all-targets` | `true` | `--all-targets` to clippy. |
| `--all-features` | `true` | `--all-features` to clippy. |
| `--deny-warnings` | `true` | `-- -D warnings`. |
| `--json` | off | Emit clippy's `--message-format=json` on stdout and nothing else. |

The three boolean flags are declared `#[arg(long, default_value = "true")]`, and the parser treats them as plain flags: they are always on, and `--all-targets=false` is a parse error (exit 2). There is no way to switch them off from the command line (see the [report](../../_reports/cli.md)). The `--help` text shows no default.

Without `--json` anvil runs `cargo clippy --message-format=json-diagnostic-rendered-ansi --all-targets --all-features -- -D warnings`, replays each rendered diagnostic on stderr (unless `--silent` or `--tail`), counts diagnostics per package (a diagnostic repeated for the lib and its tests counts once) and prints, after the run:

```text
Lint issues per module:
module        issues
quench-core   7
quench-args   2
total         9
```

Rows are sorted by count (descending), then name. A non-zero clippy status ends with `Error: lint failed with status: ...`. No log file is written. With `--json` the command is `cargo clippy --message-format=json --all-targets --all-features -- -D warnings`, passed through unchanged.

## anvil format

```text
anvil format [--check]
```

Runs `cargo +nightly fmt [--check]`. Nightly is used on purpose: workspaces rely on unstable rustfmt options (`group_imports`, `imports_granularity`) that stable rustfmt ignores. `--check` reports without changing files. Log: `target/anvil-logs/format.log`.

## anvil list

```text
anvil list [--format <FORMAT>]
```

| Flag | Default | Values |
|---|---|---|
| `--format` | `names` | `names` (one package name per line), `json` (pretty-printed `cargo metadata --no-deps --format-version=1`). Anything else: `Error: Unknown format: <value>` (exit 1). |

Runs `cargo metadata --no-deps --format-version=1`; the banner is printed on stdout in both formats except that any `--json` argument suppresses it (note `--format json` is not `--json`, so the banner **is** printed before the JSON; see the report).

```bash
anvil list | head -3
```

## anvil upgrade

```text
anvil upgrade [--incompatible]
```

Requires `cargo-upgrade` (`cargo install cargo-edit`). Runs `cargo upgrade [--incompatible]`. Log: `target/anvil-logs/upgrade.log`.

## anvil audit

```text
anvil audit [--json]
```

Requires `cargo-audit`. Runs `cargo audit`; with `--json`, `cargo audit --format json` passed through. No log file.

## anvil machete

```text
anvil machete [--json]
```

Requires `cargo-machete`. Runs `cargo machete`. With `--json` anvil captures its text report, copies its stderr to stderr, and prints one JSON line on stdout:

```json
{"findings":[{"package":"my-crate","manifest":"./crates/my-crate/Cargo.toml","unused":["serde","tokio"]}]}
```

The parser reads header lines `<package> -- <manifest>:` followed by tab-indented dependency names. A failing `cargo machete` ends with `machete operation failed with status: ...`.

## anvil deny

```text
anvil deny [--json]
```

Requires `cargo-deny`. Runs `cargo deny check` (configuration: `deny.toml`). With `--json` it runs `cargo deny --format json check` and **swaps the streams**: cargo-deny writes its JSON diagnostics to stderr, anvil writes them to its own stdout (and cargo-deny's stdout to stderr). Log: `target/anvil-logs/deny.log` (not for `--json`).

## anvil semver-check

```text
anvil semver-check -p, --package <PACKAGE> [--baseline-rev <REV>]
```

| Flag | Required | Meaning |
|---|---|---|
| `-p`, `--package` | yes | Library package to check. |
| `--baseline-rev <REV>` | no | Git revision to diff against. |

Requires `cargo-semver-checks`. Baseline selection when `--baseline-rev` is absent, in order:

1. The newest git tag `<package>-v<X.Y.Z>` whose version is `<=` the package's current version (`git tag --list '<package>-v*'`).
2. The previous commit that changed the package's `Cargo.toml` (`git log --skip=1 -1 --format=%H -- <manifest>`).
3. Neither exists (first release): anvil prints `<package> has no earlier version to diff against - comparing against an empty API`, writes a temporary empty crate with the same name and version `0.0.0`, and passes it as `--baseline-root`.

Runs `cargo semver-checks --package <p> (--baseline-rev <rev> | --baseline-root <dir>)`. Log: `target/anvil-logs/semver-check.log`.

## anvil run

```text
anvil run [-p, --package <PACKAGE>] [--serve] [--watch-interval-ms <MS>]
```

| Flag | Default | Meaning |
|---|---|---|
| `-p`, `--package` | none | Package to build and run. |
| `--serve` | off | Watch files; rebuild and restart on change. |
| `--watch-interval-ms` | `1000` | Poll interval; values below `200` are raised to `200`. |

Without `--serve`: `cargo build --all-features [--package <p>]` (operation `build`), then `cargo run [--package <p>]` (operation `run`); with a package, the run happens in the **package's directory** (resolved with `cargo metadata`). The run is captured through the output pump like any command, so interactive programs read the inherited stdin.

With `--serve`: initial build and start, then every interval anvil snapshots modification times of files under `.` (skipping any path with a component named `.git`, `target`, `.ferrous`, `.idea`, `.vscode`, `.direnv`, `.cache`, `storage`, `dist`, `tmp`, `.tmp`), and on any change rebuilds and restarts the child (stdout/stderr inherited, stdin null). A failed build keeps the old child stopped and waits for the next change.

Hotkeys (terminal raw mode; ignored when stdin is not a terminal):

| Key | Action |
|---|---|
| `r` | Rebuild and restart now. |
| `R` | Toggle automatic rebuild on change (starts enabled). |
| `q`, `Q`, `e`, `E` | Stop the child and exit with code 0. |

Status lines: `starting serve mode (watch + rebuild)`, `hotkeys: 'r' rebuild, 'R' auto-rebuild, 'q'/'Q'/'e'/'E' quit`, `detected file change. rebuilding...`, `run process exited with status <s>. waiting for file changes...`.

```bash
anvil run -p hoist --serve --watch-interval-ms 500
```
