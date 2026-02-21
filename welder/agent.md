# Coding Agent Workflow

This workflow documents the instructions that the agents in `agent.toml` actually execute.

## Goals
- Understand the repository quickly.
- Search and index code before making assumptions.
- Implement, refactor, and improve code safely.
- Validate changes with local checks.

## Architecture
- `coding_lead` is the root coordinator. Its instructions tell it to route requests to the first specialist whose remit matches the request, falling back to `SELF` when none does.
- `repo_analyst` handles indexing, discovery, and code understanding.
- `implementation_engineer` performs feature work and concrete edits.
- `refactor_engineer` improves structure/readability without changing behavior.
- `qa_engineer` runs checks and highlights regressions or risks.

## Agent definitions
- `coding_lead` (model `llama3.1:8b`): routes tasks to the approved specialists (`repo_analyst`, `implementation_engineer`, `refactor_engineer`, `qa_engineer`) and otherwise answers as `SELF`.
- `repo_analyst` (model `llama3.1:8b`, `max_tool_steps=10`): builds understanding from tools before answering, using `index_project`, `search`, `list_dir`, and `read_file`.
- `implementation_engineer` (model `llama3.1:8b`, `max_tool_steps=12`): inspects code via tools, makes focused edits, prefers minimal diffs, and may run commands; it can call `index_project`, `search`, `list_dir`, `read_file`, `write_file`, `replace_in_file`, and `run_cmd`.
- `refactor_engineer` (model `llama3.1:8b`, `max_tool_steps=12`): improves design and readability while preserving behavior, confirms call sites, and summarizes refactor impact; it can run the same tools as `implementation_engineer`.
- `qa_engineer` (model `llama3.1:8b`, `max_tool_steps=10`): runs relevant checks, reports failures, and surfaces risks; it uses `index_project`, `search`, `read_file`, and `run_cmd`.

## Tooling
- `index_project`: list files in scope for orientation.
- `search`: ripgrep-based text search for symbols/usages.
- `list_dir`: list directory contents.
- `read_file`: read full files or slices.
- `write_file`: create or overwrite files.
- `replace_in_file`: deterministic replacements.
- `run_cmd`: run local commands (e.g., checks/tests/formatting). Command execution is constrained by the active `run_cmd_allowlist`.

## run_cmd allowlist behavior
- Agents may carry an explicit `run_cmd_allowlist`. If it is empty, Welder compiles defaults from `config/allowlists.toml` during build and merges per-technology prefixes based on the detected stack when indexing the project.

## Expected behavior
- Route tasks to the best specialist as described above.
- Use allowed tools before asserting repository facts.
- Keep changes scoped and verifiable.
- Return concise user-facing summaries.

## Run
- `cargo run -p welder -- agent.toml`
