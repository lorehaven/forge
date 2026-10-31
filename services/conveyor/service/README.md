# Conveyor
The Forge CI/CD service: webhooks, pipelines and runs.

## What it is
`conveyor-service` is the estate's CI service. A webhook, a person or a clock queues a run of a registered repository; a worker checks out that exact commit, reads the `.conveyor.toml` it carries, and executes its stages and jobs with the estate's own tools. Identity is Gatehouse's; the data is in Postgres.

## Use cases
- Run pipelines on push, tag, schedule or by hand.
- Keep logs, artifacts and scan reports (security, coverage) per run.
- Hold project secrets and git credentials.

## Documentation
Full documentation: [services/conveyor](../../../docs/services/conveyor/index.md)
