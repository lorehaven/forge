---
title: Command-line tools
nav_order: 5
has_children: true
permalink: /cli/
---

One section per tool: commands, flags, configuration, interactive modes and recipes. Every page states what was run for the book and what was documented from the source only.

| Tool | Binary | What it does |
|---|---|---|
| [anvil](anvil/index.md) | `anvil` | Workspace build, lint, test, Docker and release tool; Gatehouse login; artifact releases to the warehouse. |
| [riveter](riveter/index.md) | `riveter` | Kubernetes manifests from overlays: render, apply, diff, prune, packages, secrets, CRD validation. |
| [warehouse-cli](warehouse-cli/index.md) | `warehouse` | Client of the Warehouse service: Docker registry, Cargo registry, files, admin. |
| [welder](welder/index.md) | `welder` | Multi-agent LLM workflow runner. |
| [foreman](foreman/index.md) | `foreman` | Runs the local development estate from `foreman.toml`. |
| [conveyor-cli](conveyor-cli/index.md) | `conveyor` | Client of the Conveyor CI/CD service; local pipeline validation. |
| [pulley](pulley/index.md) | `pulley` | rsync-backed sync jobs: REPL, one-shot, daemon, background service. |
| [toolbox](toolbox/index.md) | `forge-toolbox` | Terminal table to install and update the tools above from the registry. |

Shared behaviour: the interactive modes of riveter, foreman and pulley use the `quench-cli` line editor (in-memory history, no completion); status lines use `quench-cli`'s coloured `label message` format.
