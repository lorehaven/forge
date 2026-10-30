---
title: welder
parent: Command-line tools
nav_order: 4
has_children: true
permalink: /cli/welder/
---

Welder is a multi-agent workflow runner. A workflow is a TOML file describing a tree of agents (an instruction, a model, optional child agents and tools). Welder starts or finds the model backend, loads the workflow, and then reads prompts from a prompt loop, routing each one down the tree until an agent answers directly or works the task with tools.

Version: `welder 0.2.16` (`forge/cli/welder/Cargo.toml`). Binary and library crate: `welder`.

## Install

```bash
cd forge/cli/welder
cargo install --path .
```

Runtime requirements depend on the backend chosen in `.welder/config.toml` (see [Configuration](configuration.md#welderconfigtoml)):

| Backend | Needs |
|---|---|
| `ollama` (the default) | A reachable `ollama` daemon at `ollama_url` (`127.0.0.1:11434`). If nothing listens there, welder requires the `ollama` binary on `PATH` and runs `ollama serve` itself (output discarded). |
| `vllm` | The `vllm` binary on `PATH`; welder spawns one `vllm serve` per distinct model/URL. |
| `switchboard` | A reachable switchboard-service (`switchboard_url`) and the variables `WELDER_SWITCHBOARD_USERNAME`, `WELDER_SWITCHBOARD_PASSWORD`, `GATEHOUSE_URL`. |

## Synopsis

```text
welder <WORKFLOW.toml>
welder --version | -V
```

There is no `--help`, no subcommands and no other flags: the parser is hand-written (`run.rs`). `--version` / `-V` is recognised only as the **first** argument and prints `welder 0.2.16`. Any other first argument is taken as the workflow path; arguments after it are ignored. See [Commands](commands.md).

## Exit codes

| Code | When |
|---|---|
| `0` | `--version`, or the prompt loop ended (`exit`, or end of input is not handled: see the [REPL page](repl.md)). |
| `1` | Any error, printed as `Error: <message>` (backend not reachable within 30 s, missing workflow file, invalid TOML, unsupported backend, model start-up failure, an LLM call that fails during a prompt). |

## Where to go next

* [Commands](commands.md): startup sequence and output.
* [REPL](repl.md): the prompt loop and the execution trace.
* [Configuration](configuration.md): `.welder/config.toml`, the workflow format, environment variables.
* [Tools](tools.md): the seven agent tools, the sandbox and the `run_cmd` allowlists.
* [Recipes](recipes.md).
