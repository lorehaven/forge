# gatehouse-app
Gatehouse's screens.

## What it is
`gatehouse-app` is Gatehouse's interface: sign-in, the services dashboard, the account and user administration, built with `quench-ui`. It does no I/O: it asks its host for the data and for every change (`live::Request`) and is told what came (`live::Msg`), so the same screens run in the browser and in tests.

## Use cases
- Render the screens in the browser through the `web` crate.
- Test the screens natively with a scripted host.

## Documentation
Full documentation: [services/gatehouse](../../../docs/services/gatehouse/index.md)
