---
title: Models
parent: Sage
grand_parent: Services
nav_order: 10
---

How Sage decides which models it keeps running, offers in the picker and waits for. Sources: `service/src/config/settings.rs`, `startup/default_models.rs`, `routers/models.rs`, `clients/switchboard.rs`.

## Settings and their names

`SageConfig::load` reads six settings through `ConfigLoader::new("SAGE")`: for a key `X` it looks at `SAGE_X` first and then at plain `X`. So both `SAGE_DEFAULT_MODELS` and `DEFAULT_MODELS` work, and the prefixed one wins.

| Setting | Variable | Default |
|---|---|---|
| System prompt file | `SAGE_SYSTEM_PROMPT_PATH` or `SYSTEM_PROMPT_PATH` | `config/system_prompt.txt` (a missing file: a one-line built-in prompt) |
| Capability profile | `SAGE_CAPABILITY_PROFILE` or `CAPABILITY_PROFILE` | `web_assistant` (unknown name: warning, `web_assistant`) |
| Default models | `SAGE_DEFAULT_MODELS` or `DEFAULT_MODELS` | `[{"name": "qwen2.5-coder:7b"}]` |
| Supported models | `SAGE_SUPPORTED_MODELS` or `SUPPORTED_MODELS` | `qwen*, *-instruct, llama*` |
| Default search provider | `SAGE_SEARCH_PROVIDER` or `SEARCH_PROVIDER` | `duckduckgo` |
| Stop models on shutdown | `SAGE_STOP_MODELS_ON_SHUTDOWN` or `STOP_MODELS_ON_SHUTDOWN` | `false` (`true`, `1`, `yes`, `on` enable) |

Note: the search provider registry reads the unprefixed `SEARCH_PROVIDER` only (`startup/state.rs`), while the configured default for requests reads both. Set the unprefixed name to keep them in step.

## `SAGE_DEFAULT_MODELS`

A JSON array of objects (a single object, or a bare model name, is also accepted; an empty string means none; unparsable JSON logs `Failed to parse SAGE_DEFAULT_MODELS as JSON` and means none). Each object (`DefaultModel`):

| Key | Type | Passed to Switchboard as |
|---|---|---|
| `name` (required) | string | `model` |
| `gpu_utilization` | number | `gpu_memory_utilization` |
| `context_len` | integer | `max_model_len` |
| `quant` | string | `quantization` |
| `dtype` | string | `dtype` |
| `limit_mm` | string | `limit_mm_per_prompt` (e.g. `{"image": 4}`; should cover `SAGE_MAX_IMAGES_PER_REQUEST`) |
| `max_num_seqs` | integer | `max_num_seqs` |
| `enable_tool_calling` | boolean, default false | `enable_tool_calling` |
| `task` | string | `task` (`embed` makes an embedding instance) |
| `device` | string | `device` (`cpu` for a CPU launch) |

The launch request always carries `host` `0.0.0.0`, `port` 8000, `namespace` null and `enable_prefix_caching` false (Switchboard picks a free port natively). Example:

```json
[{"name":"Qwen/Qwen3-8B","gpu_utilization":0.45,"context_len":16384,"enable_tool_calling":true},
 {"name":"Qwen/Qwen3-Embedding-0.6B","gpu_utilization":0.1,"task":"embed"}]
```

## `SAGE_SUPPORTED_MODELS`

A comma-separated list of patterns. `*` matches any run of characters and `?` any single character (they become `.*` and `.`; other regex characters are not escaped, so a `.` in a pattern matches any character). A pattern must match the whole model name, case-insensitively: the regex is `(?i)^<pattern>$`. An empty list supports nothing.

A model that no pattern supports is:

* never launched by the monitor (`Model '<name>' is in default_models but does not match any pattern in supported_models.`);
* not waited for: it is left out of `defaults` and of `ready` in `GET /api/v1/models`, so it cannot hold the chat in the initializing screen;
* not offered in the picker: `instances` lists only instances whose `model` matches a pattern, so an instance somebody started by hand in Switchboard is not Sage's to hand out. The picker used to list every chat-capable instance; since the filter it lists only supported ones.

The name compared is the instance's `model` as Switchboard reports it (the string given to `vllm serve`), so a pattern like `qwen*` does not match a model launched by absolute path (`/mnt/.../models--Qwen--Qwen3-8B/...`); the default model `name` is used for both launch and comparison, so use the same string in the patterns.

## The monitor

`spawn_monitor` runs after Switchboard and Gatehouse answer `/health/ready`, every 10 seconds:

1. List Switchboard's instances (`GET /api/v1/vllm/instances`, client credentials, 3 attempts with backoff 100 ms to 2 s, circuit breaker opening after 5 failures, closing after 3 successes, 60 s cool-down). On failure: log and skip the tick.
2. For every supported default model with a `running` instance, forget its failed attempts.
3. Choose the next model: none if any instance has status `starting` or `pending` (models are launched one at a time so they do not contend for GPU memory); otherwise the first supported default model with fewer than 3 attempts and no instance in `running`, `starting` or `pending`.
4. Count an attempt and ask Switchboard to launch it. After the third attempt the model is skipped so the others can start; it is retried only after it has been seen `running` (which resets the count).

`validate_startup` (once, after readiness): lists instances (failure is logged as an error, the service keeps running; skip with `SKIP_SWITCHBOARD_CHECK=true`) and logs the search providers.

On shutdown, with `SAGE_STOP_MODELS_ON_SHUTDOWN` on, Sage deletes (`DELETE /api/v1/vllm/instances/{id}`) the `running`, `starting` or `pending` instances of its default models that this process launched, or that started before this process did (so an instance a newer replica launched is left alone).

## State of a default model

`GET /api/v1/models` reports, per supported default model, `state`:

| `state` | Meaning |
|---|---|
| `running` | an instance with that model name has status `running` |
| `starting` | status `starting` or `pending` |
| `failed` | status `failed` (lowercase) |
| `pending` | no instance yet (the monitor will launch it) |
| `unknown` | Switchboard could not be reached |

If several instances match, the best wins: running over starting over failed over pending. Other statuses (`terminating`, `completed`, `interrupted`, `Failed`) are ignored. Switchboard's Kubernetes engine reports a crashed pod as `Failed` with a capital F, which Sage ignores (see the report: the model stays `pending`). `ready` is true when switchboard answered and every supported default model is `running`. The client shows the initializing screen while `ready` is false. `embedding` is true for a model whose `task` is `embed` or `embedding`.

## What the picker offers

`instances`: Switchboard instances that are chat-capable (task not `embed`/`embedding`) and supported, as `{id, model}`. The composer sends the chosen `id` as `instance_id`; the turn looks it up again and fails with `api_error_instance_not_found` if it is gone. Regenerate and resume use the first chat-capable instance in Switchboard's list, whether or not it is supported.
