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
