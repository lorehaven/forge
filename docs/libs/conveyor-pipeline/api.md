---
title: API reference
parent: conveyor-pipeline
grand_parent: Libraries
nav_order: 2
---

Every public item by module, in source order. All types own their data; none is interior-mutable, so all are `Send + Sync`.

## Crate root

Modules: `condition`, `cron`, `graph`, `parser`, `spec`, `steps`. Re-exports:

```rust
pub use condition::{Condition, EvalContext};
pub use cron::{Cron, CronError};
pub use graph::{Decision, JobPlan, StagePlan, plan};
pub use parser::{SpecError, parse};
pub use spec::{Job, PIPELINE_FILE, PipelineSpec, Readiness, Schedule, Service, Stage,
               StageSelectionError, Step, Triggers};
```

## `spec`

```rust
pub const PIPELINE_FILE: &str = ".conveyor.toml";
pub const RESERVED_SERVICE_NAMES: [&str; 2] = ["checkout", "steps"];
pub const MAX_SERVICE_NAME: usize = 63;
pub fn glob_match(pattern: &str, value: &str) -> bool
```

`glob_match`: `*` matches any run of characters (including none and `/`); every other character is literal. Backtracking matcher, linear in practice. `glob_match("**", x)` is true; `glob_match("*", "")` is true.

### `PipelineSpec`

```rust
#[derive(Clone, Debug)]
pub struct PipelineSpec { pub on: Triggers, pub stages: Vec<Stage>, pub schedules: Vec<Schedule> /* + private order */ }
```

Only `parse` (and `only`) builds one; there is no public constructor, so every value is valid.

| Method | Description |
|---|---|
| `pub fn only(&self, names: &[String]) -> Result<Self, StageSelectionError>` | Reduces the pipeline to the named stages (names trimmed, blanks skipped, duplicates collapsed). Each kept stage gets `when = None` (runs whatever its condition says) and loses `needs` that are not kept. Job `when`s stay. Declaration order kept; `on` and `schedules` copied; the original is untouched. Error when nothing is named or a name is unknown |
| `pub fn allows(&self, event: &str, git_ref: &str) -> bool` | `schedule` event: `!schedules.is_empty()`; otherwise `Triggers::allows` |
| `pub fn order(&self) -> &[usize]` | stage indices, every stage after what it needs |
| `pub fn stages_in_order(&self) -> impl Iterator<Item = &Stage>` | stages in that order |
| `pub fn stage(&self, name: &str) -> Option<&Stage>` | lookup by exact name |
| `pub fn job_count(&self) -> usize` | total jobs |

### `StageSelectionError`

`#[derive(Clone, Debug, PartialEq, Eq)] pub struct { pub unknown: Vec<String>, pub available: Vec<String> }`, implements `Display` and `std::error::Error`. Text: `no stages were selected; this pipeline has: a, b` (when `unknown` is empty) or `no such stage: x, y; this pipeline has: a, b`.

### `Schedule`

`#[derive(Clone, Debug, PartialEq, Eq)] pub struct { pub expression: String, pub timezone: String }`.

`pub fn next_after(&self, after: quench_core::time::Time) -> Option<Time>`: next firing strictly after `after`; `None` if the expression or zone no longer parses or it never fires.

### `Triggers`

`#[derive(Clone, Debug, PartialEq, Eq)] pub struct { pub push: Vec<String>, pub pull_request: Vec<String>, pub tag: Vec<String> }`. `Default`: `push = ["*"]`, others empty.

`pub fn allows(&self, event: &str, git_ref: &str) -> bool`: see [format](format.md#on-triggers).

### `Stage`

`#[derive(Clone, Debug)] pub struct { pub name: String, pub needs: Vec<String>, pub when: Option<Condition>, pub jobs: Vec<Job> }`.

### `Job`

`#[derive(Clone, Debug)]`

| Field | Type | Meaning |
|---|---|---|
| `name` | `String` | unique in its stage |
| `when` | `Option<Condition>` | in addition to the stage's |
| `env` | `BTreeMap<String, String>` | |
| `secrets` | `Vec<String>` | |
| `timeout` | `Option<u64>` | seconds; `None`: service default |
| `image` | `Option<String>` | |
| `artifacts` | `Vec<String>` | |
| `services` | `Vec<Service>` | |
| `steps` | `Vec<Step>` | |

### `Service`

`#[derive(Clone, Debug, PartialEq, Eq)] pub struct { pub name: String, pub image: String, pub env: BTreeMap<String, String>, pub args: Vec<String>, pub ready: Readiness }`.

### `Readiness`

`#[derive(Clone, Debug, PartialEq, Eq)] pub enum Readiness { Command(String), Port(u16) }`.

### `Step`

`#[derive(Clone, Debug, PartialEq, Eq)] pub enum Step { Run(String), Anvil(String), Riveter(String), Warehouse(String) }`

| Item | Description |
|---|---|
| `pub const KINDS: [&'static str; 4]` | `["run", "anvil", "riveter", "warehouse"]` |
| `pub const fn kind(&self) -> &'static str` | the tag |
| `pub fn command(&self) -> &str` | the command text |
| `pub fn new(kind: &str, command: impl Into<String>) -> Option<Self>` | `None` for an unknown kind; no validation of the command |

## `parser`

```rust
pub fn parse(source: &str) -> Result<PipelineSpec, SpecError>
```

Deserialises (unknown keys rejected), then validates in this order: at least one stage; schedules (`on.timezone` first, then every `on.schedule` entry); triggers; each stage (name, jobs present, `when`, each job: name, `needs` refused, steps present, `when`, timeout, secrets, services, steps); finally the stage graph (so name/step mistakes are reported in preference to graph errors). The first error is returned.

`pub enum SpecError` and its texts: [errors](errors.md#specerror).

## `condition`

| Item | Description |
|---|---|
| `pub enum Variable { Branch, Tag, Event, Sha }` | `Clone, Copy, Debug, PartialEq, Eq`; `Display` prints the identifier |
| `Variable::ALL: [Self; 4]` | in the order above |
| `pub const fn as_str(self) -> &'static str` | `branch`, `tag`, `event`, `sha` |
| `pub fn parse(raw: &str) -> Option<Self>` | lookup by identifier |
| `pub enum CompareOp { Eq, Ne }` | `==`, `!=` |
| `pub struct EvalContext { pub branch, pub tag, pub event, pub sha: String }` | `Clone, Debug, Default` |
| `EvalContext::new(event: &str, git_ref: &str, sha: &str) -> Self` | splits the ref: `refs/tags/X` gives `tag`; else `refs/heads/` stripped (bare refs are branches) |
| `pub enum Condition { Compare { variable, op, value: String }, And(Box<Condition>, Box<Condition>), Or(Box<..>, Box<..>) }` | `Clone, Debug, PartialEq, Eq`, `Display` renders `branch == 'master' && event != 'schedule'` (always single quotes) |
| `Condition::parse(source: &str) -> Result<Self, ConditionError>` | grammar in [format](format.md#when-conditions) |
| `Condition::evaluate(&self, context: &EvalContext) -> bool` | |
| `pub enum ConditionError` | [errors](errors.md#conditionerror) |

## `cron`

```rust
#[derive(Debug, Clone, PartialEq, Eq)] pub struct CronError(/* String */);   // Display = message; std::error::Error
#[derive(Debug, Clone, PartialEq, Eq)] pub struct Cron { /* private */ }
```

| Method | Description |
|---|---|
| `Cron::parse(source: &str) -> Result<Self, CronError>` | trims, expands macros, needs exactly 5 whitespace-separated fields |
| `#[must_use] as_str(&self) -> &str` | the trimmed source as written (macros not expanded); also `Display` |
| `#[must_use] matches(&self, civil: &quench_core::time::Civil) -> bool` | whether the local reading is a firing minute (seconds ignored) |
| `#[must_use] next_after(&self, after: Time, zone: &quench_tz::Zone) -> Option<Time>` | first firing strictly after `after`, at a whole minute; `None` if none within `8 * 366` days or after 200 000 search steps |

Error texts: `unknown schedule macro `@x``; `expected 5 fields (minute hour day-of-month month day-of-week), found N`; `<field>: `s` is not a valid step`; `<field>: a step of 0 never advances`; `<field>: range `r` runs backwards`; `<field>: `f` allows no value`; `<field>: V is outside min-max`; `<field>: `t` is not a number or name`. Field names: `minute`, `hour`, `day-of-month`, `month`, `day-of-week`.

## `graph`

```rust
pub fn topological_order(stages: &[Stage]) -> Result<Vec<usize>, GraphError>
pub fn plan(spec: &PipelineSpec, context: &EvalContext) -> Vec<StagePlan>
```

`topological_order`: Kahn's algorithm, lowest declaration index first among ready stages.

`plan` returns one `StagePlan` per stage **in execution order** (`spec.order()`): the stage is `Blocked { by }` if any stage in `needs` (first in the `needs` order) has a non-`Run` decision, else `Excluded` if its `when` is false, else `Run`. Each stage lists all its jobs in declaration order: if the stage does not run every job carries the stage's decision; otherwise a job is `Excluded` when its own `when` is false, else `Run`.

| Type | Definition |
|---|---|
| `GraphError` | [errors](errors.md#grapherror) |
| `Decision` | `Clone, Debug, PartialEq, Eq` enum `Run`, `Excluded`, `Blocked { by: String }` |
| `Decision::will_run(&self) -> bool` | `const fn`; true for `Run` |
| `Decision::reason(&self) -> Option<String>` | `None` for `Run`; ``excluded by a `when` condition``; ``stage 'X' did not run`` |
| `JobPlan` | `{ pub index: usize, pub decision: Decision }`; index into the stage's `jobs` |
| `StagePlan` | `{ pub index: usize, pub decision: Decision, pub jobs: Vec<JobPlan> }`; index into `spec.stages` |

## `steps`

```rust
pub fn argv(step: &Step) -> Result<Vec<String>, StepError>
pub fn validate(step: &Step) -> Result<(), StepError>
```

`argv`: see the table in [format](format.md#steps). `validate`: `run` always passes; tool steps are split, must have a non-blank argument, then their module validates. `StepError` texts in [errors](errors.md#steperror).

| Submodule | Items |
|---|---|
| `steps::shell` | `pub const SHELL: &str = "sh"`; `pub fn argv(command: &str) -> Vec<String>` gives `["sh", "-c", command, "conveyor-step"]` |
| `steps::anvil` | `COMMANDS: [&str; 17]`, `DOCKER_COMMANDS: [&str; 6]`, `CONFIG_COMMANDS: [&str; 1]`, `validate(argv: &[String]) -> Result<(), StepError>` |
| `steps::riveter` | `COMMANDS: [&str; 25]`, `validate(argv: &[String])` (the `repl` guard and `--env` skipping are private) |
| `steps::warehouse` | `COMMANDS: [&str; 4]`, `validate(argv: &[String])` (the per-group subcommand table is private) |

Unknown anvil subcommand kinds are reported with `kind` `anvil`, `anvil docker` or `anvil config`.
