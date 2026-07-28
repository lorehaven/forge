# conveyor-pipeline

The Conveyor pipeline language: the `.conveyor.toml` a commit declares, and what
a given run makes of it.

Parsing, `when` condition evaluation, trigger and ref matching, the stage/job
graph that decides execution order, and what each step means as an argument
vector. Nothing in here touches the network, the database or the filesystem — it
takes the text of a pipeline and a description of the run, and returns what
should happen.

```rust
use conveyor_pipeline::{EvalContext, parse, plan};

let spec = parse(&source)?;
let context = EvalContext::new("push", "refs/heads/master", "abc1234");
for stage in plan(&spec, &context) {
    // ...
}
```

It is a library of its own so that `conveyor-service`, which runs pipelines, and
`conveyor validate`, which checks them without a running service, share one
parser rather than two that agree most of the time.
