# gantry-web
Gantry in the browser.

## What it is
`gantry-web` is the WebAssembly build of `gantry-app` over `fetch`, with the session cookie the service's sign-in set. The service serves the module from its `WEB_DIR`.

## Use cases
- Build with `cargo build --target wasm32-unknown-unknown --profile wasm -p gantry-web`.

## Documentation
Full documentation: [services/gantry](../../../docs/services/gantry/index.md)
