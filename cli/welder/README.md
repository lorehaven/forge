# welder
Multi-agent workflow runner for LLMs.

## What it is
`welder` loads a TOML workflow describing a tree of agents (an instruction, a model, optional children and tools), starts or finds the model backend (`ollama`, `vllm` or `switchboard`), and routes each prompt from a prompt loop down the tree.

## Use cases
- Route a request through specialised agents.
- Run the same workflow against a local or an estate-hosted model.

## Example
```sh
welder workflow.toml
```

## Documentation
Full documentation: [cli/welder](../../docs/cli/welder/index.md)
