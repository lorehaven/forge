# Welder

Welder is a multi-agent execution engine for building, routing, and coordinating structured LLM workflows.

## Run

```bash
cargo run -p welder -- <workflow.toml>
```

## Command Help

- `welder <workflow.toml>`: start interactive workflow session
- `welder --version` or `welder -V`: print version

Interactive session:

- Enter any prompt text to execute the workflow from the configured root agent.
- `exit`: quit the session.

## Backends

Set `[backend].kind` in `.welder/config.toml` to choose how welder reaches models:

- `ollama` — talks to a local `ollama serve` daemon (`backend.ollama_url`, default `127.0.0.1:11434`).
- `vllm` — welder spawns and owns a `vllm serve` process per model, using the fixed `url` in each `[models.NAME]` entry. See `samples/workflow.vllm.toml`.
- `switchboard` — model instances are launched and tracked by a [switchboard-service](../../docker/switchboard-service), not by welder. welder asks switchboard for a running, chat-capable instance of each model, launches one if none exists, waits for it to come up, then talks to its resolved `host:port` over the same OpenAI-compatible chat endpoint the `vllm` backend uses. Requires `backend.switchboard_url` and the `WELDER_SWITCHBOARD_USERNAME` / `WELDER_SWITCHBOARD_PASSWORD` environment variables. See `samples/workflow.switchboard.toml`.

```toml
# .welder/config.toml
[backend]
kind = "switchboard"
switchboard_url = "https://localhost:7443/switchboard"
switchboard_tls_verify = false  # accept a self-signed dev certificate
```
