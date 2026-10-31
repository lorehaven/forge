# conveyor-web
Conveyor in the browser.

## What it is
`conveyor-web` is the WebAssembly build of `conveyor-app` over `fetch` and `EventSource`, with the session cookie the service's sign-in set. The service serves the module from its `WEB_DIR`.

## Use cases
- Build with `cargo build --target wasm32-unknown-unknown --profile wasm -p conveyor-web`.

## Documentation
Full documentation: [services/conveyor](../../../docs/services/conveyor/index.md)
