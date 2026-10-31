# gantry-app
Gantry's screens.

## What it is
`gantry-app` is Gantry's interface: packages, resources, deployments, versions, operations and the YAML editor, built with `quench-ui`. It does no I/O: it asks its host for the data and for every change (`live::Request`) and is told what came (`live::Msg`), so the same screens run in the browser and in tests.

## Use cases
- Render the screens in the browser through the `web` crate.
- Test the screens natively with a scripted host.

## Documentation
Full documentation: [services/gantry](../../../docs/services/gantry/index.md)
