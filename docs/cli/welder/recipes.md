---
title: Recipes
parent: welder
grand_parent: Command-line tools
nav_order: 5
---

All recipes start a model backend or contact switchboard and Gatehouse, so they are *Unverified* (not run for the book).

## Local Ollama

```bash
ollama pull qwen2.5:14b                # once
cd my-project
welder /path/to/forge/cli/welder/samples/agent.toml
```

No `.welder/config.toml` is needed (the defaults select ollama). If you create one, include every `[backend]` key shown on the [Configuration](configuration.md#welderconfigtoml) page, in particular `debug`.

## Local vLLM

```bash
mkdir -p .welder
cat > .welder/config.toml <<'TOML'
[backend]
kind = "vllm"
debug = false
TOML
welder samples/workflow.vllm.toml
```

Every agent needs a model entry with a `url`; welder starts one `vllm serve` per distinct model/URL and stops waiting after `[vllm] timeout_seconds`.

## Through switchboard

```bash
cat > .welder/config.toml <<'TOML'
[backend]
kind = "switchboard"
debug = false
switchboard_url = "https://host/switchboard"
switchboard_tls_verify = true
TOML
cat > .env <<'ENV'
GATEHOUSE_URL=https://host/gatehouse
WELDER_SWITCHBOARD_USERNAME=me
WELDER_SWITCHBOARD_PASSWORD=...
ENV
welder samples/workflow.switchboard.toml
```

## Debug a routing decision

```bash
WELDER_DEBUG=1 welder samples/agent.toml
```

Look for `[routing-debug] raw: ... | matched: '...'` on stderr: the router's raw reply and which child name was found in it.
