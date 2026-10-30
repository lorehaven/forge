---
title: Commands
parent: welder
grand_parent: Command-line tools
nav_order: 1
---

Welder has one invocation form. Verified locally: `welder --version`, and the start-up errors without a backend (this machine has no `ollama`). Not run (start a model backend or contact switchboard/gatehouse): everything past backend start-up.

## welder --version

```text
$ welder --version
welder 0.2.16
```

`-V` is the same. It must be the first argument, and it is checked before anything else, so it works without a backend.

## welder \<workflow.toml\>

The start-up sequence (`run::run`):

1. **Version check** (above).
2. **Configuration**: `.welder/config.toml` in the current directory is loaded (lazily, once). A missing file means the defaults; an unreadable or invalid file prints `Warning: Invalid .welder.toml: <error>. Using defaults.` (or `Failed to read`) to stderr and uses the defaults. The warning names `.welder.toml` although the file is `.welder/config.toml` (see the [report](../../_reports/cli.md)).
3. **Backend initialisation** (before the workflow argument is even looked at): `ollama` starts `ollama serve` if the daemon is not reachable (`` Error: `ollama` not found on PATH. backend.kind = "ollama" needs the ollama daemon; install it from https://ollama.com or switch backend.kind `` otherwise); `vllm` does nothing yet; `switchboard` needs `switchboard_url` (`config error: backend.switchboard_url must be set`). An unknown `kind`: `unsupported backend: <kind>`.
4. **Wait for the backend**: polls every 500 ms for up to 30 s (`BACKEND_READY_TIMEOUT_SECS`): ollama = TCP connect to `ollama_url`; switchboard = TCP connect to the host and port of `switchboard_url` (default port 80 for `http`, else 443); vllm = always ready. On timeout: `backend '<kind>' did not become reachable within 30s; check [backend] in .welder/config.toml`. On success it prints a banner (`LLM BACKEND READY`, `type`, `url`; for vllm `(managed by workflow)`).
5. `.env` in the current directory is loaded into the environment (`quench_core::dotenv::load`; failures ignored).
6. **Workflow path**: the first argument; with none: `Error: Usage: welder <workflow.toml>`. (So with no argument the backend is still started first.)
7. `[welder] Loading workflow from: <path>`, parse (`[welder] ✓ Workflow loaded`).
8. **Build agents** for the backend (`[welder] Building agents...`):
   * `ollama`: one client per agent against `http://<ollama_url>` (`/api/chat`), no model management.
   * `vllm`: every agent needs a model entry with `url` (see [Configuration](configuration.md#workflow-toml)). Welder prints `Found N vLLM instance(s)` and one line per instance, then starts one `vllm serve` per distinct `(model, url)` and waits for each to answer, up to `[vllm] timeout_seconds` (default 300); then `✓ Model manager ready`.
   * `switchboard`: logs in to Gatehouse (`POST {GATEHOUSE_URL}/api/v1/auth/login`), lists switchboard instances (`GET /api/v1/vllm/instances`), launches missing ones (`POST /api/v1/vllm/instances`) and waits for each to be running, up to `[switchboard] timeout_seconds` (default 300). Prints `Resolving N model(s) via switchboard` and `✓ Switchboard model(s) ready`.
9. `[welder] ✓ N agent(s) ready`, a `[welder]   - <agent>` line per agent (hash-map order, not sorted), `[welder] Starting REPL...`.
10. The [prompt loop](repl.md).

Chat calls go to `{host}/v1/chat/completions` (vllm and switchboard-resolved instances, OpenAI-compatible) or `{ollama}/api/chat`.

### Errors seen locally

```text
$ welder
Error: `ollama` not found on PATH. backend.kind = "ollama" needs the ollama daemon; install it from https://ollama.com or switch backend.kind
```

```text
$ cat .welder/config.toml
[backend]
kind = "switchboard"
switchboard_url = "https://x/switchboard"
$ welder
Warning: Invalid .welder.toml: backend: missing field `debug`. Using defaults.
Error: `ollama` not found on PATH. ...
```

The second case is a real trap: `debug` has no default in `BackendConfig`, so a config without it is discarded entirely and welder silently falls back to ollama. Always set `debug` (see [Configuration](configuration.md#welderconfigtoml)).

Workflow errors (from `run.rs`): `Agent '<a>' references model '<m>' but no [models] table is defined`, `... which is not defined in [models]`, `Agent '<a>' vllm config is missing 'url' (required for the local vllm backend)`, `vLLM not configured for any agents in workflow`, `switchboard backend requires at least one model reference in the workflow`, `Agent '<a>' references model '<m>' but no vLLM config found for it`.
