---
title: conveyor-pipeline
parent: Libraries
has_children: true
permalink: /libs/conveyor-pipeline/
---

`conveyor-pipeline` (library `conveyor_pipeline`) is the parser and planner of the `.conveyor.toml` pipeline language. It takes the text of a pipeline file and a description of a run (event, ref, commit) and returns the validated pipeline and, for a given run, which stages and jobs execute. It touches no network, database or filesystem: every check happens in `parse`, so a bad pipeline is an error naming the stage, job or step, not a run that fails halfway. It is a separate crate so `conveyor-service` (runs pipelines) and `conveyor` CLI (`conveyor validate`) share one parser.

## Facts

| | |
|---|---|
| Version | 0.2.1 |
| Edition | 2024 (workspace) |
| License | MIT |
| Library name | `conveyor_pipeline` |
| Test harness | `harness = false`, `quench-test` (`tests/unit.rs`) |
| Unsafe | none (the crate has no `[lints]` table and no `unsafe` blocks) |
| Lints | `#![warn(missing_docs)]` |

## Install

```toml
[dependencies]
conveyor-pipeline = { version = "0.2.1", registry = "ennor" }
```

Inside the workspace: `conveyor-pipeline = { workspace = true }` (`path = "libs/conveyor-pipeline"`, version `0.2.0` in `[workspace.dependencies]`).

## Features

None.

## Dependencies

| Crate | Kind | Why |
|---|---|---|
| `quench-serde` | normal | TOML deserialisation, `Value`, `Deserialize` derive |
| `quench-core` | normal | `Error` derive, `time::{Time, Civil, Duration, Weekday}`, `shell::split` |
| `quench-tz` | normal | IANA `Zone` for schedules |
| `quench-test` | dev | test harness |

No third-party crates. (An older page of this book named `thiserror` and `shlex`; neither is used.)

## Where to go next

* [Pipeline file format](format.md): every key of `.conveyor.toml`, stage/job/step semantics, triggers, schedules, `when` language.
* [Usage](usage.md): parse, plan, select stages, schedules.
* [API reference](api.md): every public item.
* [Errors](errors.md): every error type and text.
* [Testing](testing.md).
* The service that runs pipelines: `conveyor` service; the CLI: `conveyor` CLI (see [Command-line tools](../../cli/index.md)).

## Minimal example

```rust
use conveyor_pipeline::{EvalContext, parse, plan};

let source = r#"
[[stage]]
name = "build"
[[stage.job]]
steps = [{ anvil = "build --all" }]
"#;
let spec = parse(source)?;
let context = EvalContext::new("push", "refs/heads/master", "abc1234");
for stage in plan(&spec, &context) {
    assert!(stage.decision.will_run());
}
# Ok::<(), conveyor_pipeline::SpecError>(())
```
