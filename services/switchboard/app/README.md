# switchboard-app
Switchboard's screens.

## What it is
`switchboard-app` is Switchboard's interface: the home cards, the models registry and the vLLM instances, built with `quench-ui`. It does no I/O: it asks its host for the data and for every change (`live::Request`) and is told what came (`live::Msg`), so the same screens run in the browser and in tests.

## Use cases
- Render the screens in the browser through the `web` crate.
- Test the screens natively with a scripted host.

## Documentation
Full documentation: [services/switchboard](../../../docs/services/switchboard/index.md)
