---
title: REPL
parent: welder
grand_parent: Command-line tools
nav_order: 2
---

After start-up welder shows the workflow and reads prompts. Not run for the book (needs a model backend); documented from `run.rs`, `ui.rs` and `engine/executor.rs`.

## Prompt loop

```text
────────────────────────────────────────────────────────────
  ▸ WORKFLOW  samples/agent.toml
────────────────────────────────────────────────────────────
  ◆ lead  llama3.1:8b
    └─ analyst  llama3.1:8b  [3 tools]
────────────────────────────────────────────────────────────
  type 'exit' to quit

You ▸
```

*Unverified:* layout from `ui::print_workflow_header` (agent names, models and tool counts are illustrative; the rule characters come from `quench_cli`'s `SEP`/`SEP_THIN`).

* The graph is drawn from the `[root]` agent down through `children`.
* Prompt: `You ▸ `. A line is read with plain `read_line` (no history, no line editing beyond the terminal's, no completion).
* **Only one command exists: `exit`** (case-insensitive, after trimming). Anything else, including an empty line, is sent to the root agent. End of input (Ctrl-D) is not handled specially: `read_line` returns an empty string, which is run as a prompt, so a closed stdin loops forever calling the model with an empty request. Use `exit`.
* An error while executing a prompt (a failed LLM call, an unknown agent) ends the whole program with `Error: ...` (the `?` in the loop); it does not return to the prompt.

## Execution trace

Each prompt prints a trace to stdout:

```text
────────────────────────────────────────────────────────────
  ▸ lead  llama3.1:8b
────────────────────────────────────────────────────────────
  routing…
  ↳ analyst
  ...
  executing…
  ✓ analyst

────────────────────────────────────────────────────────────
  ▸ Answer
────────────────────────────────────────────────────────────
<answer text>
────────────────────────────────────────────────────────────
```

Rules:

1. Execution starts at `[root].name`. An agent named but not defined: `Agent not found: <name>`.
2. An agent with `children` first routes: it sends the model `Pick ONE delegate for the request. Reply with ONLY the delegate name, nothing else.`, the list `- <child>: <first sentence of its instruction>` and `Request: <input>`. The first child (in `children` order) whose name appears, case-insensitively, anywhere in the reply is chosen, and the request is executed there with the **original input** (depth + 1). If no child name appears: `no delegate matched — executing as SELF`.
3. Without `tools`, the agent calls the model once with `<instruction>\n\nUser request:\n<input>`.
4. With `tools`, it runs the [tool loop](tools.md#the-tool-loop).
5. The answer is printed trimmed.

With `WELDER_DEBUG=1|true` (or `debug = true` in the config) routing prints `[routing-debug] raw: ... | matched: ... | children: [...]` to stderr.

Every call uses the agent's `temperature` (default `0.7`) and `max_tokens` (default `2048`). There is no conversation memory between prompts: each prompt starts a fresh execution.
