# Report: command-line tools (anvil, riveter, warehouse-cli, welder, foreman, conveyor-cli, pulley, toolbox)

Scope: `forge/cli/*`. Books pages: `docs/cli/<tool>/`. Binaries were built locally (`cargo build -p ...`) and `--help` captured; nothing that contacts a service, registry, cluster, Gatehouse or Warehouse was run. Never read: `.anvil/secret.toml`, `~/.config/conveyor/config.toml`.

## 1. Stale or wrong statements found (old pages and in-tool help)

Old pages (`docs/cli/<tool>.md`, now deleted) were mostly accurate; the differences:

* **anvil old page**: usage block omitted `--json` on `test`/`nextest` and the `--silent`/`--tail` flags being `global` (accepted after the subcommand too). `anvil lint` was described as having `--all-targets/--all-features/--deny-warnings` "defaulting on" as if switchable: the flags are `default_value = "true"` booleans that cannot be turned off (`anvil lint --all-targets=false` is a parse error, exit 2).
* **anvil `release --help`**: says "`--all` Release all packages configured in [publish].packages"; the key is `[release].packages` (`config.rs`, `config_check.rs`).
* **anvil `registry-token --help`**: the description contains literal backslashes (`[\"cargo:token-from-stdout\", ...]`), a typo in the `about` string (`cli.rs`).
* **riveter `--help` / REPL `help` (generated from `src/help.rs`)**: lists `--replicas` and `--except` under `apply`, `--against` under `diff` for the REPL, and `--no-wait`/`--timeout` for REPL `apply`. Real behaviour: `riveter apply --replicas x=1` fails with `unexpected argument '--replicas'` (they belong to `install`); in the REPL `apply --no-wait`, `render --no-wait` and `diff --against x` fail with `unknown option` (verified). The help also omits `--replicas/--except/--inventory/--registry-auth` under `install`/`publish`/`pull`, and its REPL examples show `--env canary,prod apply`, which the REPL cannot do.
* **riveter old page**: links to `../libs/rivet-package.md` and `../docker/warehouse-service.md` (pages that are being replaced); rewritten without links across books. Its statement that `requires.riveter` is "enforced" is right (an unsatisfied requirement is an error, not a warning), `requires.packages` is only noted.
* **welder old page**: example `.welder/config.toml` omitted `debug`. With the real parser a config without `debug` is rejected (`Warning: Invalid .welder.toml: backend: missing field debug. Using defaults.`, verified) and welder silently falls back to the **ollama** backend. The warning names `.welder.toml` although the file is `.welder/config.toml`. The old page also said "`welder <workflow.toml>`" only; the real start order is backend init before the argument is checked (no argument still starts/needs the backend first: `Error: ...ollama not found...` before `Usage: welder <workflow.toml>`).
* **warehouse-cli `files` options help**: "Registry name; defaults to active docker registry from config" on all `files` subcommands; code uses the active **files** registry (`resolve_files_registry_name`). Old page did not mention that the three kinds share one `registries/<name>.toml` file.
* **conveyor-cli `main.rs` comment**: "One line, on stderr"; `print_status` writes to stdout, so errors and status lines are on stdout.
* **toolbox status note**: tells the user to "run forge-toolbox self-update"; there is no such command (the CLI struct has no arguments). Update with `cargo install forge-toolbox --registry ennor --force`.
* **pulley example config / help**: `example.pulley.toml` has a job with a remote source (`user@server.com:/home/user/photos`); rsync is spawned with `current_dir(src)`, so it fails before running (`Error: Os { code: 2, kind: NotFound ... }`, verified, exit 1).
* **READMEs** (`cli/*/README.md`, outside the book, not edited): all still link `docs/cli/<tool>.md`, which no longer exists; point them at `docs/cli/<tool>/index.md`. `cli/anvil/README.md` also tells readers to see that page for the config layout.

## 2. Existing in the code, undocumented before

* anvil: `keystore-password [--key]`, the `[android]` table of the secret file (`keystore_password`, `key_password`) and its env mapping (`QUENCH_KEYSTORE_PASSWORD`, `QUENCH_KEY_PASSWORD`); secret-file lookup upwards; `ANVIL_APP_TOKEN`/`ANVIL_GATEHOUSE_URL`; log files `target/anvil-logs/<op>.log`; `--tail` requires `=`; `stdout_is_data` banner suppression; baseline selection order of `semver-check` (release tag, then previous manifest commit, then empty crate); `config check` rule list; `artifact list`.
* anvil release plan details: release-relevant extension list, untracked (`publish = false`) dependency handling, tag pushes after each package, `version = { workspace = true }` replacement.
* riveter: `.riveter.toml` `warehouse_url`; `login`; `phase-N` manifests for gated applies; REPL option grammar and its limits; `images --overlays-dir`; `install --inventory`; the `$HOME/.config/riveter/age.key` default key and `.env` lookup of key variables.
* warehouse-cli: `registry add` auto-activates the first registry of a kind; credentials stored in plain text with default file mode; `admin gc` uses the docker registry by default and crates URL as base; files commands send no auth header.
* welder: tool loop prompt/JSON protocol, truncation limits (4000 / 12000 bytes), the built-in allowlists and tech detection, `WELDER_DEBUG`, the 30 s backend wait.
* foreman: defaults (`scheme` falls back to `https`, `stop_timeout_secs` 5), `${service}`/`${name}` in `pre_stop` hooks, `foreman test` exit code, picker commands.
* conveyor-cli: `--stage` comma lists, `credential --git-username`, config-file precedence.
* pulley: daemon never asks for confirmation, job log layout and 7-day retention, systemd unit text, runit paths.
* toolbox: nine registry searches at start, exact key bindings.

## 3. Open questions and suspected bugs (not fixed)

* `anvil/src/cli.rs` `Lint`: bool flags with `default_value = "true"` cannot be disabled (`--no-...` form absent).
* `anvil list --format json` prints the banner first (stdout_is_data only checks for the literal `--json`), so its output is not pure JSON.
* `anvil/src/commands/release.rs`: `set_manifest_version` turns `version = { workspace = true }` into a literal version (package leaves the workspace version). Needs a decision.
* `anvil release` uses the secret file only (`registry_token_env`); `ANVIL_APP_TOKEN` env credentials work for `registry-token` and `artifact release` but not for `release`'s `cargo publish` token (`commands/login.rs:registry_token_env`).
* `riveter` REPL `apply`: `args.contains(&"--no-wait")` at `repl.rs` (the `apply` arm) is unreachable because `parse_args` rejects `--no-wait` first.
* `riveter` help table (`src/help.rs`): `apply` entry lists `--replicas`/`--except` (install-only) — likely copy/paste from `install`.
* `riveter diff` exits 0 whether or not the cluster differs (only a warning). Scripts cannot detect drift.
* `welder/src/config/app_config.rs`: `BackendConfig` lacks `#[serde(default)]` on `kind`/`debug`/`ollama_url` when the file exists; a partial `[backend]` table invalidates the file. Warning text says `.welder.toml`.
* `welder` REPL: end of input (Ctrl-D) is not detected (`read_line` returns 0 bytes, treated as an empty prompt) and an error in a prompt terminates the whole program (`?` in `run_repl`).
* `welder` `run_cmd`: allowlist prefixes like `cat`/`sed -n` can read any file the process can read; the path sandbox applies only to the file tools.
* `warehouse-cli`: `registry add` writes `base_path` even when `--base-path` was not given, resetting it for other kinds (verified: `files registry add` after `crates registry add --base-path /warehouse` leaves `base_path = ""`). Docker and files `registry list` show registries that have no URL for that kind (verified). Secrets are written with default file mode (0644 observed); `docker login` takes the password as a command-line argument.
* `warehouse-cli/src/api/files_api.rs`: no authorization is sent for any `files` call; unknown whether the service requires it.
* `conveyor-cli`: `run` prints the id after a status line on stdout; no machine-readable mode. `secret set` reads all of stdin (not one line).
* `pulley/src/rsync.rs`: dry run always passes `--delete`, so it reports deletions the update will not perform for `delete = false` jobs; `current_dir(src)` breaks remote sources; `update` sends rsync's stdout to **stderr**. A stray empty directory `cli/pulley/C:\Users\me` is in the source tree.
* `pulley daemon`: `Config` is loaded once; edits require a restart (REPL has `reload`).
* `foreman env <service>` prints environment values in clear text (may include secrets from `[defaults].env`).
* `foreman`: the docker container named by a config may already exist (`docker run --name` fails with a clear Docker error, not handled specially); `foreman status` read the container state of a `postgres` container that was already running on this machine, so it is shared with the real estate (nothing was started or stopped).
* `toolbox`: an extra argument is expected to be rejected by the parser but was not run (a call that were accepted would start registry queries).
* Not verified at all (documented from source): everything that talks to a cluster, Gatehouse, Warehouse, Conveyor, a registry, ollama, vLLM, rsync, systemd/runit/schtasks, or starts foreman services.
