---
title: Configuration
parent: welder
grand_parent: Command-line tools
nav_order: 4
---

Welder reads `.welder/config.toml` (backend), a workflow TOML file (agents), and a few environment variables.

## .welder/config.toml

Read from the **current directory** only. Optional; all values below are the defaults used when the file is absent.

```toml
[backend]
kind = "ollama"                    # "ollama" | "vllm" | "switchboard"
ollama_url = "127.0.0.1:11434"     # host:port, no scheme; used by "ollama"
debug = false                      # REQUIRED when the file exists (no default; see below)
switchboard_url = "https://localhost:7443/switchboard"   # required for "switchboard"
switchboard_tls_verify = true      # false accepts a self-signed certificate
```

| Key | Type | Default (no file) | Meaning |
|---|---|---|---|
| `backend.kind` | string | `"ollama"` | Backend. Anything else: `Unsupported backend` / `unsupported backend: <kind>`. Required if the file exists. |
| `backend.ollama_url` | string | `127.0.0.1:11434` | `host:port` of the daemon. Not defaulted when the file exists: with `kind = "ollama"` and no `ollama_url`: `config error: backend.ollama_url must be set`. |
| `backend.debug` | bool | `false` | Verbose routing/model-manager logging. **No default when the file exists**: leaving it out makes the whole file invalid (`backend: missing field debug`) and welder falls back to the defaults with a warning. Verified. |
| `backend.switchboard_url` | string | none | Base URL of switchboard-service including path. Required for `switchboard` (`config error: backend.switchboard_url must be set`). |
| `backend.switchboard_tls_verify` | bool | `true` | TLS verification for Gatehouse login and switchboard calls. This one has a serde default. |

A file that cannot be read or parsed gives a stderr warning and the defaults (see [Commands](commands.md#errors-seen-locally)).

## Environment variables

| Variable | Used by | Meaning |
|---|---|---|
| `WELDER_DEBUG` | all | `1` or `true` (any case) turns on verbose logging, like `backend.debug`. |
| `WELDER_SWITCHBOARD_USERNAME`, `WELDER_SWITCHBOARD_PASSWORD` | switchboard | Gatehouse credentials welder logs in with. Errors: `WELDER_SWITCHBOARD_USERNAME must be set to use the switchboard backend` (and the password one). |
| `GATEHOUSE_URL` | switchboard | Gatehouse base URL; login is `POST {GATEHOUSE_URL}/api/v1/auth/login`. Error: `GATEHOUSE_URL must be set to use the switchboard backend - ...`. Failed login: `gatehouse rejected WELDER_SWITCHBOARD_USERNAME/PASSWORD`. |

A `.env` file in the current directory is loaded after backend initialisation and before agents are built, so it can hold the switchboard variables (but not `WELDER_DEBUG`'s effect on config loading, which happens earlier; `WELDER_DEBUG` is read on use, so it works from `.env` for routing logs).

## Workflow TOML

Top-level keys (`config/workflow.rs`):

| Key | Type | Required | Meaning |
|---|---|---|---|
| `[root]` `name` | string | yes | Agent that handles each prompt first. |
| `[[agent]]` | array of tables | yes | The agents. |
| `[vllm]` `timeout_seconds` | integer | no (300) | How long to wait for each spawned `vllm serve` (backend `vllm`). |
| `[switchboard]` `timeout_seconds` | integer | no (300) | How long to wait for a launched instance to be `running` (backend `switchboard`). |
| `[models.<NAME>]` | table | no | Shared model definitions, referenced by `vllm_model`. |

### [[agent]]

| Key | Type | Default | Meaning |
|---|---|---|---|
| `name` | string | required | Unique name (referenced by `root` and `children`). |
| `model` | string | required | Model name: the Ollama model, or the served model name for vllm/switchboard. |
| `instruction` | string | required | System/role text. The first sentence is shown to a router as the agent's description. |
| `children` | array of string | none | Delegates; makes the agent a router first. Names not defined as agents are left out of the routing prompt. |
| `tools` | array of string | none | Subset of `list_dir`, `read_file`, `write_file`, `replace_in_file`, `search`, `index_project`, `run_cmd`. See [Tools](tools.md). |
| `max_tool_steps` | integer | `8` | Tool-loop budget. |
| `run_cmd_allowlist` | array of string | derived | Prefixes `run_cmd` may run. |
| `temperature` | float | `0.7` | Sampling temperature. |
| `max_tokens` | integer | `2048` | Max tokens per call. |
| `vllm_model` | string | none | Name of a `[models.<NAME>]` entry. |
| `vllm` | table (`[agent.vllm]` form: `vllm = { ... }`) | none | Inline model settings instead of `vllm_model`. |

For backends `vllm` and `switchboard` an agent without `vllm_model` or `vllm` is skipped when collecting models and then fails with `references model '<m>' but no vLLM config found for it`. The `ollama` backend ignores model tables.

### Model settings (`[models.<NAME>]` or `[agent.vllm]`)

| Key | Type | Used by | Meaning |
|---|---|---|---|
| `url` | string `host:port` | vllm (required) | Where welder starts and reaches the instance. |
| `model_path` | string | vllm | Weights path/ID passed to `vllm serve` (else the model name). |
| `dtype` | string | vllm (default `auto`), switchboard hint | `--dtype`. |
| `max_model_len` | integer | both | `--max-model-len`. |
| `gpu_memory_utilization` | float | vllm (default `0.9`), switchboard hint | `--gpu-memory-utilization`. |
| `tensor_parallel_size` | integer | vllm (default `1`) | `--tensor-parallel-size`. |
| `quantization` | string | switchboard hint | Launch hint. |
| `task` | string | switchboard hint | Launch hint. |
| `limit_mm_per_prompt` | string | switchboard hint | Launch hint. |

The `vllm` backend runs `vllm serve <model_path|model> --served-model-name <model> --host <h> --port <p> --dtype <d> --gpu-memory-utilization <g> --tensor-parallel-size <t> [--max-model-len <n>]`. Switchboard hints are used only when no matching instance is already running. One instance is started per distinct `(model, url)` (vllm) or per `model` name (switchboard).

### Samples

`forge/cli/welder/samples/`:

| File | Backend | Shape |
|---|---|---|
| `agent.toml` | ollama | Coding pipeline: `coding_lead` routes to `repo_analyst`, `implementation_engineer`, `refactor_engineer`, `qa_engineer`, with tools. |
| `workflow.vllm.toml` | vllm | `project_manager` -> `content_creator` -> `researcher`/`writer`, fixed `url`. |
| `workflow.switchboard.toml` | switchboard | The same content pipeline resolved through switchboard (`[switchboard] timeout_seconds = 600`, no `url`). |
| `editorial_pipeline.toml`, `product_launch_pipeline.toml` | any | Deeper routing trees without tools. |

Minimal workflow:

```toml
[root]
name = "lead"

[[agent]]
name = "lead"
model = "qwen2.5:14b"
instruction = "You answer questions about this repository. Read real files before answering."
tools = ["index_project", "search", "read_file"]
max_tool_steps = 10
```
