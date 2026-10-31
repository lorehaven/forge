# conveyor-pipeline
The Conveyor pipeline language: parsing, conditions and run planning.

## What it is
`conveyor-pipeline` parses the `.conveyor.toml` a commit carries, validates it, and for a described run (event, ref, commit) says which stages and jobs execute. It touches no network, database or filesystem, and is shared by `conveyor-service` and `conveyor-cli`.

## Use cases
- Validate a pipeline file offline.
- Plan a run: which stages and jobs a push, tag or schedule triggers.
- Evaluate job conditions.

## Features
- None.

## Documentation
Full documentation: [libs/conveyor-pipeline](../../docs/libs/conveyor-pipeline/index.md)
