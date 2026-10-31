# sage-app
Sage's screens.

## What it is
`sage-app` is Sage's interface: the chat with its sidebar, and the screen that waits for the models, built with `quench-ui`. It does no I/O: it asks its host for the data and for every change (`live::Request`) and is told what came (`live::Msg`), so the same screens run in the browser and in tests.

## Use cases
- Render the screens in the browser through the `web` crate.
- Test the screens natively with a scripted host.

## Documentation
Full documentation: [services/sage](../../../docs/services/sage/index.md)
