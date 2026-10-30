---
title: Recipes
parent: anvil
grand_parent: Command-line tools
nav_order: 6
---

End-to-end workflows. The commands that publish are *Unverified* (not run for the book).

## Check before pushing

```bash
anvil format --check
anvil lint
anvil test --all
anvil deny
anvil audit
anvil machete
```

`lint` is strict by default (all targets, all features, warnings are errors) and ends with a per-crate issue table.

## Machine-readable quality reports

```bash
anvil lint --json     > clippy.jsonl
anvil nextest --json  > tests.jsonl
anvil audit --json    > audit.json
anvil deny --json     > deny.jsonl
anvil machete --json  > machete.json
```

stdout carries only the report; the banner is suppressed because `--json` is in the arguments.

## First-time credentials for releases

```bash
anvil config check
anvil login --scope warehouse:write       # asks username and password; stores the token in .anvil/secret.toml
anvil keystore-password --key             # only for Android releases
```

*Unverified.* To let plain `cargo` use the private registry without copying tokens, add to `~/.cargo/config.toml`:

```toml
[registries.ennor]
credential-provider = ["cargo:token-from-stdout", "anvil", "registry-token"]
```

## Release crates

```bash
anvil release --all --dry-run     # layers and what changed since each package's last tag
anvil release --all               # prints the plan, asks "Proceed with release? (yes/no)"
```

Releases are idempotent per package: a package with no changes since its tag is "up to date" and skipped.

## Release services as Docker images

Configure `[docker.modules.*]` and `[docker]` (see [Configuration](configuration.md)), then:

```bash
anvil docker build -p hoist-service        # local build only
anvil docker release -p hoist-service      # build + tag + push
anvil release -p hoist-service             # bump patch, commit, push, docker release, tag
```

## Publish an app to the warehouse

```bash
anvil artifact list
anvil artifact release hoist --dry-run
anvil artifact release hoist --target android
anvil artifact release hoist --skip-existing   # re-run after a partial failure
```

## Check, build and publish the documentation

```bash
anvil docs check                       # front matter, navigation, links, OpenAPI against the routes
anvil docs build forge                 # Jekyll with the installed just-the-docs gem -> target/docs/forge/
anvil docs release forge --dry-run     # build, show what would be sent
anvil docs release                     # every book, to the warehouse files storage
```

See [Documentation commands](commands-docs.md).

## Develop with auto-restart

```bash
anvil run -p hoist-service --serve
# r = rebuild now, R = toggle auto-rebuild, q = quit
```
