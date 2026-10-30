---
title: Tools and profiles
parent: Sage
grand_parent: Services
nav_order: 12
---

Sources: `service/src/tools/*`, `routers/turns.rs`, `startup/state.rs`.

## Tool calls

The model writes tool calls as `<toolcall>{...}</toolcall>` (also `<tool_call>`) containing JSON; two JSON shapes are accepted: `{"type":"function","function":{"name":...,"arguments":{...}}}` and `{"type":"search","name":...,"arguments":{...}}` (`parser.rs`, tolerant of malformed closing tags and nested braces). The system prompt gets a section "AVAILABLE TOOLS" with the definitions of the profile's tools and these two formats. Unless grounding is on, the definitions are also sent as native OpenAI `tools`. After the answer is generated, the service parses the calls from the final text, runs them through the per-request registry, and replaces each call marker in the stored answer with an HTML block (`<div class="tool-result tool-success|tool-error">...`); the client's Markdown renderer (`sage-core::markdown`) recognises these cards in the stored text and draws them as tool blocks with its own elements; it does not insert the HTML. A question that asks about the tools (a "meta" question) runs no tool. Tool-call blocks in a grounded answer are stripped.

| Tool | Arguments (required in bold) | Behaviour |
|---|---|---|
| `web_search` | **`query`** | Search with the request's provider (see below). |
| `web_fetch` | **`url`** (http or https) | Fetch a page and extract its text. |
| `calculator` | **`expression`** | Evaluate arithmetic (for example `2 + 2`, `sqrt(16)`). |
| `file_ops` | **`operation`** (`read`, `write`, `list`, `exists`), **`path`**, `content` | Files below `FILE_OPS_BASE_PATH` (default `.`): absolute paths, `../`, `..\`, NUL and `~` are rejected and the canonical path must stay under the base. Requires confirmation. |
| `file_search` | **`query`**, `limit` (default 4) | Semantic search over the uploaded files of the conversation and its project. |
| `file_list` | `ready_only` | List the uploaded files. |
| `command` | **`cmd`** | Run a shell command whose first word is in the allow-list `cat grep ls find pwd date echo wc head tail sort uniq cut tr sed awk stat file which type du df ps env whoami id uname uptime free` and which contains none of the blocked substrings (`rm `, `mv `, `cp `, `>`, `>>`, `|`, `&`, `;`, `sudo`, `su `, `chmod`, `chown`, `mkfs`, `dd`, `shutdown`, `reboot`, `kill`, `pkill`); otherwise `Command not allowed: ...`. At most 5 per second per executor. Requires confirmation. |
| `code_executor` | **`language`** (python, javascript), **`code`** | Run Python or JavaScript after a substring check: Python code containing `os.system`, `subprocess`, `exec`, `eval`, `__import__`, `open(`, `input(`, `compile(` and JavaScript code containing `require`, `eval`, `Function`, `setTimeout`, `setInterval`, `fetch`, `XMLHttpRequest`, `window.location`, `document.write` is refused. The code runs with the host interpreter (`python3` or `node`) as a child process with a 10 second timeout; no other isolation is applied by the service. |

The registry (`ToolRegistry`) applies, in order: the rate limit ([Observability](observability.md)); the profile (a tool not in the profile answers `Tool '<name>' is not available in the '<profile>' capability profile`); confirmation (`command` and `file_ops` answer `... requires explicit confirmation before execution` unless the request lists the tool in `tool_confirmations`); the per-tool timeout, with retry on transient errors; metrics and cost recording; audit log entries.

## Capability profiles

`SAGE_CAPABILITY_PROFILE` selects the profile; a turn may narrow it with `capability_profile` in `POST /api/v1/turns` (an unknown name, or a profile enabling any tool the configured one does not, falls back to the configured profile; the browser client never sends it).

| Profile | Tools | Default timeout / per tool (s) | Rate limit (calls/min, burst per conversation) | Context window (tokens, warn at) |
|---|---|---|---|---|
| `web_assistant` (default) | web_search, web_fetch, calculator, file_search, file_list | 30; web_search 30, web_fetch 20, calculator 5, file_search 30, file_list 10 | 120, 30 | 4096, 80 percent |
| `code_assistant` | web_search, web_fetch, calculator, code_executor, file_ops, file_search, file_list | 60; web_search 30, web_fetch 20, calculator 5, code_executor 90, file_ops 15, file_search 30, file_list 10 | 60, 15 | 8192, 75 percent |
| `cli_agent` | all eight | 120; as code_assistant plus command 120 | 30, 5 | 8192, 75 percent |

A profile name is matched case-insensitively. An unknown profile name is rate limited at 60 calls/min, burst 15.

`GET /api/v1/chat/capabilities` returns the configured profile, its description, the sorted tool names and the available search providers.

## Search providers

`SearchProviderRegistry` (`startup/state.rs`, `tools/search_providers`): each provider returns text of the form `Search results for ...` followed by entries `snippet\nSource: url` separated by `---`.

| id | Label | Endpoint | Registered when |
|---|---|---|---|
| `duckduckgo` | DuckDuckGo | `https://html.duckduckgo.com/html/?q=` (HTML scrape) | always |
| `searxng` | SearXNG | `$SEARXNG_INSTANCE_URL` (default `https://searxng.be`) | always (a missing URL uses the default instance) |
| `brave` | Brave Search | `https://api.search.brave.com/res/v1/web/search?q=` | `BRAVE_SEARCH_API_KEY` set |
| `serpapi` | SerpAPI | `https://serpapi.com/search?q=...&engine=google` | `SERPAPI_API_KEY` set |

`available_search_providers` (and the picker) list `duckduckgo` and `searxng`, plus `brave` and `serpapi` when their key variable is set, sorted. The default provider is `SEARCH_PROVIDER` (default `duckduckgo`) if it is registered. A turn's `search_provider` picks another for that turn. Grounded searches fall back across `SAGE_SEARCH_FALLBACK` ([Grounding](grounding.md)). DuckDuckGo throttles automated traffic; prefer a self-hosted SearXNG.
