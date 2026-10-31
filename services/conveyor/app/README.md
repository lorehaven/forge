# conveyor-app
Conveyor's screens.

## What it is
`conveyor-app` is Conveyor's interface: pipelines, runs, repositories, scans and credentials, built with `quench-ui`. It does no I/O: it asks its host for the data and for every change (`live::Request`) and is told what came (`live::Msg`), so the same screens run in the browser and in tests.

## Use cases
- Render the screens in the browser through the `web` crate.
- Test the screens natively with a scripted host.

## Documentation
Full documentation: [services/conveyor](../../../docs/services/conveyor/index.md)
