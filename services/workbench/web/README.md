# workbench-web
Workbench in the browser.

## What it is
`workbench-web` is the WebAssembly build of `workbench-app` over `fetch`, with the session cookie the service's sign-in set. The service serves the module from its `WEB_DIR`.

## Use cases
- Build with `cargo build --target wasm32-unknown-unknown --profile wasm -p workbench-web`.

## Documentation
Full documentation: [services/workbench](../../../docs/services/workbench/index.md)
