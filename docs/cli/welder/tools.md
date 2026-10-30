---
title: Tools
parent: welder
grand_parent: Command-line tools
nav_order: 3
---

An agent with a non-empty `tools` list works in a loop of tool calls. Source: `engine/tools.rs`, `engine/executor.rs`, `config/allowlists.toml`. The unit tests in `tests/unit/engine_tools_tests.rs` and `executor_tests.rs` cover the sandbox, allowlist matching and JSON extraction.

## The tool loop

For at most `max_tool_steps` steps (default `8`, minimum 1), welder sends the model a prompt with sections `### ROLE` (the agent's instruction), `### OUTPUT`, `### TOOLS`, `### RULES`, `### TASK` (the request), `### HISTORY` and `Step N/M. Output your JSON now:`. The model must reply with one JSON object:

```json
{"action":"tool","tool":"read_file","args":{"path":"src/main.rs","start_line":1,"end_line":50}}
{"action":"final","content":"the full answer"}
```

* The JSON is taken from the reply as is if it starts with `{`, else after stripping a `` ```json `` fence, else from the first `{` to the last `}`. If the reply contains no parseable object, the **raw reply is returned as the answer**.
* `action: "final"` returns `content`. Any other action but `tool` is recorded in the history as `step N: invalid action '<a>' from model` and the loop continues. A tool not in the agent's list: `step N: tool '<t>' is not in allowed tools`.
* A tool result is appended to the history as `step N: tool=<t> args=<json> output:\n<result>`; errors as `... error: <message>`. Each tool output is truncated to 4000 bytes (`...[truncated X of Y bytes]`); the whole history is capped at 12000 bytes, keeping the tail and prefixing `...[earlier tool output truncated]`.
* After the last step: `Unable to complete task after N tool step(s). Last context:\n<history>`.
* If the agent has `index_project`, welder first runs it on `.` and puts `preindex: indexed N file(s); detected tech: rust, node` (or `unknown`) in the history; this also drives the default `run_cmd` allowlist.

## The tools

All paths are relative to the working directory.

| Tool | Arguments | Result |
|---|---|---|
| `list_dir` | `path` (default `.`) | Sorted lines `dir\t<name>` / `file\t<name>`. |
| `read_file` | `path`, `start_line` (default 1), `end_line` (default last) | Lines formatted `{n:>5} | {text}`. `invalid line range` if `start_line` is 0 or `end_line < start_line`. |
| `write_file` | `path`, `content` | Creates parent directories, overwrites; `wrote <path>`. |
| `replace_in_file` | `path`, `find`, `replace` | Replaces **all** occurrences; `replaced N occurrence(s) in <path>` (also when N is 0). |
| `search` | `pattern` (regex), `path` (default `.`) | Lines `<file>:<line>:<text>` for every match, honouring standard ignore files (`.gitignore` etc.); non-UTF-8 lines are skipped. |
| `index_project` | `path` (default `.`) | One file path per line (standard ignore filters). |
| `run_cmd` | `cmd` | `status: <exit status>\nstdout:\n...\nstderr:\n...`. |

An unknown tool name in `run_tool` fails with `unknown tool: <name>`; unrecognised names in an agent's `tools` list are omitted from the help text.

## Sandbox

`safe_rel_path` rejects: absolute paths (`absolute paths are not allowed`), any `..` component (`path traversal is not allowed`), and any path whose longest existing prefix resolves through a symlink to outside the working directory (`path escapes the working directory`). It applies to every file tool.

## run_cmd

* `cmd` is split with shell-like quoting (`invalid shell-like command syntax` on failure); an empty command is `command is empty`.
* The executable must be a bare name (`only bare executable names are allowed` if it contains `/` or `\`).
* It must match an allowlist entry by **whitespace-token prefix**: entry `cargo check` allows `cargo check -p welder` but not `cargo build`. Otherwise `command blocked by run_cmd allowlist. command='...', allowlist=[...]`.
* The command runs without a shell, in the working directory; output is captured whole.

Allowlist selection per agent: `run_cmd_allowlist` if non-empty; otherwise derived from the indexed files: the common entries plus those of every detected technology; if no technology is detected, the common entries plus **all** technologies' entries. Built-in lists (`config/allowlists.toml`, compiled in by `build.rs`, which fails the build if `common` or a tech list is empty):

| Group | Entries |
|---|---|
| common | `rg`, `ls`, `cat`, `sed -n`, `git status`, `git diff` |
| rust | `cargo check`, `cargo test`, `cargo clippy`, `cargo fmt --check` |
| node | `npm run`, `pnpm run`, `yarn`, `bun run` |
| python | `python -m pytest`, `python -m unittest`, `python -m ruff`, `python -m mypy` |
| go | `go test`, `go vet`, `go build` |
| jvm | `mvn test`, `mvn verify`, `gradle test`, `gradle check` |
| dotnet | `dotnet test`, `dotnet build` |
| ruby | `bundle exec rspec`, `bundle exec rubocop` |
| php | `phpunit`, `composer test` |
| elixir | `mix test` |
| swift | `swift test` |
| cpp | `cmake --build`, `ctest`, `make test` |

Technology detection looks at file names and extensions (for example `Cargo.toml` or `.rs` is rust; `package.json`, `.ts`, `.js` is node; `go.mod`/`.go` is go; `pom.xml`, `build.gradle`, `.java`, `.kt` is jvm; `.csproj`, `.sln`, `.cs` is dotnet).

Note that the common list includes `cat` and `sed -n`, which read any file the process can read: the file-path sandbox does not apply to `run_cmd` arguments.
