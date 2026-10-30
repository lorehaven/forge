---
title: anvil
parent: Command-line tools
nav_order: 1
has_children: true
permalink: /cli/anvil/
---

Anvil is the workspace build and release tool of the Forge Cargo workspace. It wraps `cargo` (build, test, clippy, fmt, publish), the dependency checkers (`cargo-audit`, `cargo-deny`, `cargo-machete`, `cargo-semver-checks`), `docker`, and the Gatehouse/warehouse release flow, with the same defaults for every crate and service.

Version: `anvil 0.4.24` (`forge/cli/anvil/Cargo.toml`). Binary name: `anvil`.

## Install

```bash
cd forge/cli/anvil
cargo install --path .
```

Anvil requires `cargo` on `PATH` at start-up (`require_binary("cargo", ...)` in `main.rs`); everything else is required only by the command that uses it.

| Tool on `PATH` | Needed by |
|---|---|
| `cargo` | every command |
| `cargo-nextest` | `nextest` |
| `cargo-edit` (`cargo-upgrade`) | `upgrade` |
| `cargo-audit` | `audit` |
| `cargo-machete` | `machete` |
| `cargo-deny` | `deny` |
| `cargo-semver-checks` | `semver-check` |
| `rustfmt` for the `nightly` toolchain | `format` (runs `cargo +nightly fmt`) |
| `nightly` toolchain | `test --json` (runs `cargo +nightly test ... -Z unstable-options`) |
| `docker` | `docker ...`, `release` of a Docker package, `artifact release` with an `image` |
| `git` | `release`, `semver-check`, `artifact release` |
| `jekyll` (Ruby gem; `[docs].jekyll` can name another command) | `docs build`, `docs release` |

## Global options

Both are defined once on the top-level command and are `global`, so clap accepts them before or after the subcommand (`anvil --silent build` and `anvil build --silent` both work; the `--help` of every subcommand lists them).

| Option | Meaning |
|---|---|
| `--silent` | Do not stream the underlying command's output. Only anvil's own status lines are shown; the last 80 lines are printed if the command fails. Conflicts with `--tail`. |
| `--tail[=<N>]` | Do not stream; print only the last `N` lines (default `80` when no value is given) once the command finishes, on success and on failure. The value must be attached with `=` (`require_equals`), so `--tail 200` is read as `--tail` followed by the argument `200`. |
| `-h`, `--help` | Help. |
| `-V`, `--version` | Prints `anvil 0.4.24`. |

`--silent` and `--tail` have no effect on `--json` runs (`run_command_json` passes everything through) and on `list`, `config check`, `artifact`, `docs`, `login`, `keystore-password` and `registry-token`, which do not run through the output pump.

## Output behaviour

* Unless stdout is data (any `--json` in argv, or the first non-flag argument is `registry-token`), anvil prints a boxed banner (`Anvil CLI` / `workspace build and release`) to stdout.
* Commands that shell out print a status line `anvil running <operation> operation...`, then the child's output, then `anvil <operation> operation completed successfully (<elapsed>)` or `... failed with status: <status>`.
* `build`, `clean`, `test`, `nextest`, `format`, `upgrade`, `deny`, `install`, `release`/docker/cargo steps are teed to `target/anvil-logs/<operation>.log` (the operation name lower-cased, non-alphanumerics replaced by `-`). `lint`, `audit` and `machete` are not logged to a file; `--json` runs are not either.
* When stdout or stderr is a terminal, anvil sets `CARGO_TERM_COLOR=always`, `CLICOLOR_FORCE=1` and `FORCE_COLOR=1` for the child so it keeps its colours although its output is piped.
* The child's stdin is inherited.

## Exit codes

| Code | When |
|---|---|
| `0` | The command succeeded (also: `release` answered "Release cancelled." or "Nothing to release"). |
| `1` | Anvil returned an error: the message is printed as `Error: <message>` on stderr. This covers a failed child command, a missing tool, an invalid configuration (`config check`), a refused login, etc. |
| `2` | Command-line parse error (unknown flag, missing required argument), as printed by `quench-args`. Observed: `anvil lint --all-targets=false` exits 2. |

Anvil does not forward a child's own exit code: any non-zero child status becomes exit code 1 with the child's status in the message.

## Configuration and secrets

Configuration lives in `.anvil.toml` and/or `.anvil/*.toml` at the directory anvil is started in (it reads `.`; it does not search upwards). If it is missing or invalid anvil prints a warning and continues with an empty configuration, so commands that need configuration then fail with their own message (for example `anvil docker build -p x` fails with `Module not found for package: x`). Use [`anvil config check`](commands-credentials.md#anvil-config-check) to see the real error. See [Configuration](configuration.md) for every key and the merge rules.

Credentials are kept in `.anvil/secret.toml` (or `.anvil.secret.toml` when the workspace has no `.anvil/` directory) by `anvil login`; the layout and lookup rules are on the [Configuration](configuration.md#the-secret-file) page. Anvil never prints stored tokens.

## Environment variables

| Variable | Read by | Meaning |
|---|---|---|
| `ANVIL_APP_TOKEN` and `ANVIL_GATEHOUSE_URL` | `registry-token`, `artifact release`, `docs release` | Credentials for CI without a secret file. Both must be set and non-empty; they win over the secret file. |
| `CARGO_REGISTRIES_<NAME>_TOKEN` | `release` (cargo publish), `docker` builds | If set, `release` does not fetch a token from Gatehouse for that registry (`<NAME>` is the registry name upper-cased, `-` replaced by `_`). Docker builds forward it as a build arg when `[docker].cargo_registry` names the registry. |
| `CARGO_REGISTRIES_<NAME>_INDEX` | `docker` builds | Index URL for `[docker].cargo_registry`; wins over `cargo_registry_index` and `~/.cargo/config.toml`. |
| `CARGO_HOME` | `docker` builds | Where `config.toml` with `[registries.<name>]` is looked up (else `$HOME/.cargo`, else `%USERPROFILE%/.cargo`). |
| `QUENCH_KEYSTORE_PASSWORD`, `QUENCH_KEY_PASSWORD` | `artifact release` (passed to the build) | Android signing passwords. Already-set variables win over the secret file. |
| `HOSTNAME` | `login` | Used for the default token name `anvil on <HOSTNAME>` (else `this machine`). |
| `ANVIL_VERSION_NAME`, `ANVIL_VERSION_CODE` | set **by** `artifact release` for each build command | See [Artifacts](commands-credentials.md#anvil-artifact). |
| `DOCKER_BUILDKIT=1`, `BUILDKIT_PROGRESS=plain` | set **by** `docker build` | |
| `NEXTEST_EXPERIMENTAL_LIBTEST_JSON=1` | set **by** `nextest --json` | |

## Where to go next

* [Workspace commands](commands-workspace.md): `build`, `clean`, `test`, `nextest`, `lint`, `format`, `list`, `upgrade`, `audit`, `machete`, `deny`, `semver-check`, `run`.
* [Release and Docker commands](commands-release.md): `install`, `release`, `docker ...`.
* [Documentation commands](commands-docs.md): `docs list|check|build|release`.
* [Credentials, config and artifacts](commands-credentials.md): `login`, `keystore-password`, `registry-token`, `config check`, `artifact release|list`.
* [Configuration](configuration.md) and [Recipes](recipes.md).
