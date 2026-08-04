# Conveyor Pipeline

Parsing and planning engine for the `.conveyor.toml` pipeline language: turns a pipeline file plus a description of a run into an ordered, filtered plan of stages and jobs. Touches no network, database or filesystem — shared by `conveyor-service` and `conveyor-cli`'s `conveyor validate`.

See [docs/libs/conveyor-pipeline.md](../../docs/libs/conveyor-pipeline.md) for full documentation.
