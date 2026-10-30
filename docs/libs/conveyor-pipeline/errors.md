---
title: Errors
parent: conveyor-pipeline
grand_parent: Libraries
nav_order: 3
---

All error enums derive `quench_core::Error` (the workspace's `thiserror` replacement), `Debug`; `ConditionError`, `GraphError` and `StepError` also `PartialEq, Eq`. Texts below are the `Display` output.

## SpecError

| Variant | Text | When |
|---|---|---|
| `Toml(#[from] quench_serde::Error)` | the TOML error | invalid TOML, unknown key, wrong type; also schedule/timezone errors |
| `NoStages` | `pipeline has no stages: add a [[stage]]` | no `[[stage]]` |
| `EmptyStageName { position }` | `stage {position} has an empty name` | blank stage name (position 0-based) |
| `NoJobs { stage }` | `stage '{stage}' has no jobs: add a [[stage.job]]` | |
| `EmptyJobName { stage }` | `a job in stage '{stage}' has an empty name` | |
| `DuplicateJob { stage, name }` | `duplicate job '{name}' in stage '{stage}'` | |
| `NoSteps { stage, job }` | `job '{job}' in stage '{stage}' has no steps` | |
| `JobNeeds { stage, job }` | `job '{job}' in stage '{stage}' declares `needs`; dependencies are between stages, so move it to the [[stage]]` | |
| `BadStep { stage, job, ordinal, reason }` | `step {ordinal} of job '{job}' in stage '{stage}': {reason}` | ordinal 1-based |
| `BadService { stage, job, position, reason }` | `service {position} of job '{job}' in stage '{stage}': {reason}` | position 1-based |
| `BadStageCondition { stage, source }` | `` `when` on stage '{stage}': {source}`` | |
| `BadJobCondition { stage, job, source }` | `` `when` on job '{job}' in stage '{stage}': {source}`` | |
| `ZeroTimeout { stage, job }` | `job '{job}' in stage '{stage}' has a timeout of zero` | |
| `EmptySecretName { stage, job }` | `job '{job}' in stage '{stage}' lists an empty secret name` | |
| `EmptyTriggerPattern { event }` | `` `on.{event}` contains an empty pattern; use "*" to match every ref`` | |
| `Graph(#[from] GraphError)` | the graph error | |

## GraphError

| Variant | Text |
|---|---|
| `DuplicateStage { name }` | `duplicate stage '{name}'` |
| `UnknownNeeds { stage, missing }` | `stage '{stage}' needs '{missing}', which is not a stage in this pipeline` |
| `SelfNeeds { stage }` | `stage '{stage}' needs itself` |
| `Cycle { path }` | `stages form a cycle: a -> b -> a` |

## ConditionError

| Variant | Text |
|---|---|
| `Empty` | `condition is empty` |
| `UnknownVariable { name, known }` | `unknown variable '{name}' (known: branch, tag, event, sha)` |
| `ExpectedVariable { found }` | `expected a variable, found {found}` |
| `ExpectedComparison { variable, found }` | `expected '==' or '!=' after '{variable}', found {found}` |
| `ExpectedValue { op, found }` | `expected a quoted value after '{op}', found {found}` |
| `Trailing { token }` | `unexpected {token} at the end of the condition` |
| `UnterminatedString { quote }` | `unterminated string: no closing {quote}` |
| `UnexpectedCharacter { found }` | `unexpected character '{found}'` plus a hint: `=` gives ` (comparison is '==', not '=')`, `&` gives ` (conjunction is '&&')`, `|` gives ` (disjunction is '||')` |

`{found}` is a token rendered as `'text'` or `the end of the condition`.

## StepError

| Variant | Text |
|---|---|
| `Unparseable { kind, command }` | `` `{kind}` arguments are not quoted correctly: {command}`` |
| `NoArguments { kind }` | `` `{kind}` has no arguments`` |
| `UnknownCommand { kind, command, known }` | `` `{kind}` has no command `{command}` (known: {known})`` |
| `Interactive { kind, command }` | `` `{kind} {command}` waits for input, which conveyor never sends; the job would hang until its timeout`` |

`kind` is the program name here: `anvil`, `anvil docker`, `anvil config`, `riveter`, `warehouse`, and for `Unparseable`/`NoArguments` `anvil`, `riveter` or `warehouse-cli`.

## CronError and StageSelectionError

See [api](api.md#cron) and [api](api.md#stageselectionerror).
