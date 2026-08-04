# Conveyor Pipeline

`conveyor-pipeline` (crate `conveyor_pipeline`) is the parsing and planning engine for the `.conveyor.toml` pipeline language: turning the text of a pipeline file plus a description of a run into an ordered, filtered plan of stages and jobs. It touches no network, database or filesystem beyond reading the string it's given — the pipeline definition is validated fully at parse time (unique stage names, resolvable `needs`, an acyclic graph, well-formed `when` conditions and steps) so a bad pipeline fails fast with a specific error rather than partway through a run. It exists as its own crate so `conveyor-service` (which runs pipelines) and `conveyor-cli`'s `conveyor validate` (which checks them without a running service) share one parser instead of two that drift apart; `conveyor-service` and `cli/conveyor-cli` both depend on it directly; its own README is a short pointer to this page.

## Public API / Key Types

Re-exported from the crate root:

- `parse(source: &str) -> Result<PipelineSpec, SpecError>` — parses and fully validates a `.conveyor.toml` document. `PIPELINE_FILE` is the conventional filename (`".conveyor.toml"`).
- `PipelineSpec` — validated pipeline: `on: Triggers`, `stages: Vec<Stage>`, plus `.order()` (topological stage order), `.stages_in_order()`, `.stage(name)`, `.job_count()`.
- `Triggers` — `push`/`pull_request`/`tag` glob-pattern lists; `Triggers::allows(event, git_ref)` decides whether a pipeline runs for a given event/ref (a `manual` event always passes; `on` omitted defaults to `push = ["*"]`). `glob_match(pattern, value)` is the underlying `*`-only glob matcher (deliberately `/`-agnostic).
- `Stage` / `Job` / `Step` — a `Stage` has `name`, `needs`, an optional `when: Condition`, and `jobs`; a `Job` has `when`, `env`, `secrets`, `timeout`, `image`, `artifacts`, `steps`. `Step` is `Run(String) | Anvil(String) | Riveter(String) | Warehouse(String)`; `Step::KINDS` lists the four tags accepted in TOML (`run`, `anvil`, `riveter`, `warehouse`).
- `Condition` / `EvalContext` — the `when` expression language: variables `branch`, `tag`, `event`, `sha`; operators `==`/`!=`; connectives `&&`/`||` (no parentheses). `Condition::parse(source)`, `Condition::evaluate(&context)`. `EvalContext::new(event, git_ref, sha)` splits a ref into branch or tag (never both).
- `plan(spec: &PipelineSpec, context: &EvalContext) -> Vec<StagePlan>` — decides what a specific run will actually do. Every stage and job appears in the result with a `Decision`: `Run`, `Excluded` (its own `when` was false), or `Blocked { by }` (something it needs didn't run) — exclusion propagates downstream through `needs`.
- `steps::argv(step) -> Result<Vec<String>, StepError>` — turns a step into the argv an executor would spawn (`run` goes through `sh -c`; tool steps are split with `shlex` and prefixed with the tool's binary name, e.g. `anvil`, `riveter`, `warehouse-cli`). `steps::validate(step)` checks a tool step's command word against that tool's known commands (`steps::anvil::COMMANDS`/`DOCKER_COMMANDS` and the equivalents for `riveter`/`warehouse`) without spawning anything — called automatically during `parse`, so an unknown subcommand is a parse-time error.
- `SpecError`, `GraphError`, `ConditionError`, `StepError` — specific, position-aware error enums (`thiserror`-derived) naming the offending stage/job/step rather than a generic parse failure.

## Testing

`libs/conveyor-pipeline/tests/unit.rs` wires an extensive unit suite under `tests/unit/`: `parser_tests.rs`, `graph_tests.rs`, `condition_tests.rs`, `spec_tests.rs`, `steps_mod_tests.rs`, `steps_tools_tests.rs` (nearly 1,700 lines total), covering trigger matching, cycle detection, condition parsing/evaluation, and step validation.

## Usage example

```rust
use conveyor_pipeline::{EvalContext, parse, plan};

let spec = parse(&source)?;
let context = EvalContext::new("push", "refs/heads/master", "abc1234");
for stage in plan(&spec, &context) {
    // stage.decision tells you whether it (and its jobs) will run
}
```

[Home](../README.md)
