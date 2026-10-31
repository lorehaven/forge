# gauge-app
Gauge's screens.

## What it is
`gauge-app` is Gauge's interface: the hosts, and one host with everything Gauge knows of it, built with `quench-ui`. It does no I/O: it asks its host for the data and for every change (`live::Request`) and is told what came (`live::Msg`), so the same screens run in the browser and in tests.

## Use cases
- Render the screens in the browser through the `web` crate.
- Test the screens natively with a scripted host.

## Documentation
Full documentation: [services/gauge](../../../docs/services/gauge/index.md)
