# Welder

Welder is a multi-agent execution engine for building, routing, and coordinating structured LLM workflows. A workflow is a TOML file describing a tree of agents — each with an instruction, a model, and optionally child agents or tools — and welder walks that tree at runtime: routing a prompt down through delegating agents until one of them answers directly or works the task with tools (reading/writing files, searching, running allow-listed shell commands). It exists so multi-agent pipelines in this workspace are declared as data (one TOML file per workflow) rather than hand-wired per use case, and so the same engine can target a local Ollama daemon, a locally-spawned vLLM process, or a shared switchboard-managed vLLM fleet without changing the workflow file.

## Features

- Tree-structured agent workflows: a root agent plus any number of `[[agent]]` entries, each optionally listing `children` it can delegate to.
- LLM-driven routing: an agent with children asks its model to pick exactly one delegate name for the incoming request, falling back to handling the request itself if no delegate name comes back.
- Tool-using agents: agents can be given a `tools` list (`list_dir`, `read_file`, `write_file`, `replace_in_file`, `search`, `index_project`, `run_cmd`) and loop, calling one tool per step (as JSON) up to `max_tool_steps`, until they emit a final answer.
- Sandboxed filesystem tools: all file paths are resolved relative to the working directory, with absolute paths, `..` traversal, and symlink escapes all rejected.
- Allow-listed shell execution: `run_cmd` only runs commands matching a prefix in the agent's `run_cmd_allowlist`; if an agent doesn't set one, welder auto-detects the project's tech stack (via `index_project`) from `config/allowlists.toml` (baked in at compile time by `build.rs`) and picks a sane default per language.
- Three interchangeable backends (`ollama`, `vllm`, `switchboard`) selected by `.welder/config.toml`, all implementing the same `Backend` trait so the rest of the engine doesn't care which one is active.
- Interactive REPL: after a workflow loads, welder prints the agent graph and accepts prompts in a loop, showing the routing/execution trace for each one.
- Tool-output and history truncation so a long-running tool-using agent's prompt doesn't grow unbounded across steps.

## Requirements

- One of the following, depending on the configured backend:
  - `ollama` — a reachable `ollama serve` daemon (welder will try to spawn one itself if it isn't already running).
  - `vllm` — the `vllm` CLI on `PATH`; welder spawns `vllm serve` per model itself.
  - `switchboard` — a reachable [switchboard-service](../docker/switchboard-service), plus `WELDER_SWITCHBOARD_USERNAME`, `WELDER_SWITCHBOARD_PASSWORD`, and `GATEHOUSE_URL` in the environment (or a `.env` file) so welder can log into gatehouse and get a bearer token for switchboard.
- A workflow TOML file to run (see `samples/`).

## Usage

```bash
cargo run -p welder -- <workflow.toml>
# or, once built:
welder <workflow.toml>
welder --version   # / -V
```

Welder loads the workflow, resolves and starts/discovers every referenced model, then drops into an interactive session:

```
You ▸ summarize what this repo does
```

Enter any prompt to execute the workflow starting from the root agent; type `exit` to quit.

Sample workflows in `cli/welder/samples/` cover each backend and topology:

- `agent.toml` — a five-agent coding pipeline (lead → analyst/implementer/refactorer/QA) with tool access, for the `ollama` backend.
- `workflow.vllm.toml` — a project-manager/content pipeline using the local `vllm` backend with a fixed `url` per model.
- `workflow.switchboard.toml` — the same content pipeline, but resolved through `switchboard` instead of a fixed URL.
- `editorial_pipeline.toml`, `product_launch_pipeline.toml` — deeper routing trees (director → leads → specialists) with no tools, just delegation.

## Configuration

### `.welder/config.toml`

Selects the backend:

```toml
[backend]
kind = "switchboard"                                    # "ollama" | "vllm" | "switchboard"
switchboard_url = "https://localhost:7443/switchboard"
switchboard_tls_verify = false                           # accept a self-signed dev cert
```

`kind` defaults to `"ollama"` (with `ollama_url` defaulting to `127.0.0.1:11434`) if the file is missing or fails to parse. Set `debug = true` (or `WELDER_DEBUG=1`/`true`) for verbose routing and model-manager logging.

### Workflow TOML

```toml
[root]
name = "project_manager"

[[agent]]
name = "project_manager"
model = "llama3.1:8b"
instruction = "You are a project manager who oversees content creation projects. ..."
children = ["content_creator"]

[[agent]]
name = "content_creator"
model = "llama3.1:8b"
instruction = "..."
tools = ["index_project", "search", "read_file", "run_cmd"]
max_tool_steps = 12
run_cmd_allowlist = ["cargo check", "cargo test"]
temperature = 0.7
max_tokens = 2048
```

- `[root].name` — which agent handles the first prompt of a session.
- `[[agent]]` — one entry per agent: `name`, `model`, `instruction`, and optionally `children` (delegate names), `tools`, `max_tool_steps` (default 8), `run_cmd_allowlist`, `temperature` (default 0.7), `max_tokens` (default 2048).
- For the `vllm` and `switchboard` backends, an agent also needs model connection details, either inline as `[agent.vllm]` or shared via a top-level `[models.NAME]` table referenced by `vllm_model = "NAME"`. `vllm` requires `url` (`host:port`) on every referenced model entry; `switchboard` doesn't need `url` — `quantization`, `dtype`, `max_model_len`, `gpu_memory_utilization`, `limit_mm_per_prompt`, and `task` are used only as launch hints if no matching instance is already running.
- `[vllm].timeout_seconds` / `[switchboard].timeout_seconds` (both default 300s) bound how long welder waits for a model process/instance to become reachable before failing.

## Testing

```bash
cargo test -p welder
```

Unit tests cover workflow-model resolution (`extract_vllm_instances`, `extract_switchboard_instances`), the tool-call JSON extraction and history-truncation logic in the executor, and the filesystem sandbox (`safe_rel_path`, including a Unix-only symlink-escape test) and `run_cmd` allowlist matching in `engine/tools.rs`.

[Home](../README.md)
