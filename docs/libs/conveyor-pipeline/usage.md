---
title: Usage
parent: conveyor-pipeline
grand_parent: Libraries
nav_order: 4
---

## 1. Validate a pipeline file (what `conveyor validate` does)

```rust
let text = std::fs::read_to_string(conveyor_pipeline::PIPELINE_FILE)?;
match conveyor_pipeline::parse(&text) {
    Ok(spec) => println!("{} stages, {} jobs", spec.stages.len(), spec.job_count()),
    Err(error) => eprintln!("{error}"),
}
```

Effect: the first problem is printed with the stage, job and step named. Pitfall: only the first error is returned.

## 2. Decide whether a pipeline runs for an event

```rust
let runs = spec.allows("push", "refs/heads/release/1.2");
let tag_runs = spec.allows("push", "refs/tags/v1.0");   // only if `on.tag` matches
let scheduled = spec.allows("schedule", "refs/heads/master");
```

Pitfall: with `on = { push = ["*"] }` a tag push does not run (tags use `on.tag` only).

## 3. Plan a run

```rust
use conveyor_pipeline::{EvalContext, parse, plan};
let context = EvalContext::new("push", "refs/heads/master", "abc1234");
for stage in plan(&spec, &context) {
    let name = &spec.stages[stage.index].name;
    println!("{name}: {:?}", stage.decision);
    for job in &stage.jobs {
        println!("  {}: {:?}", spec.stages[stage.index].jobs[job.index].name, job.decision);
    }
}
```

Output for the repository's pipeline on a push to `master`: `check`, `build`, `test` are `Run`; `security` is `Excluded`. Pitfall: if a stage is `Excluded` every stage that needs it is `Blocked { by }`.

## 4. Run only some stages

```rust
let only = spec.only(&["security".to_string()])?;   // runs although its `when` is false
let plans = conveyor_pipeline::plan(&only, &context);
```

An unknown name returns `StageSelectionError` whose `Display` lists the stages that exist.

## 5. Next firing of a schedule

```rust
use quench_core::time::Time;
for schedule in &spec.schedules {
    if let Some(next) = schedule.next_after(Time::from_unix(1_700_000_000)) {
        println!("{} ({}) -> {}", schedule.expression, schedule.timezone, next.timestamp());
    }
}
```

A caller that keys scheduled runs should key them on the instant, because a repeated local hour (fall back) fires twice.

## 6. Turn a step into a command

```rust
use conveyor_pipeline::{Step, steps};
let argv = steps::argv(&Step::Anvil("test --all".into()))?;
assert_eq!(argv, ["anvil", "test", "--all"]);
let sh = steps::argv(&Step::Run("make | tee log".into()))?;
assert_eq!(sh, ["sh", "-c", "make | tee log", "conveyor-step"]);
```

## 7. Evaluate a condition on its own

```rust
use conveyor_pipeline::{Condition, EvalContext};
let c = Condition::parse("branch == 'master' || tag != ''")?;
assert!(c.evaluate(&EvalContext::new("push", "refs/tags/v1", "abc")));
```

*Unverified:* the snippets on this page were written from the sources and the crate's tests; they were not compiled or run here.
