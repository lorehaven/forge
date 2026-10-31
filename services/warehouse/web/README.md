# warehouse-web
Warehouse in the browser.

## What it is
`warehouse-web` is the WebAssembly build of `warehouse-app` over `fetch`, with the session cookie the service's sign-in set. The service serves the module from its `WEB_DIR`.

## Use cases
- Build with `cargo build --target wasm32-unknown-unknown --profile wasm -p warehouse-web`.

## Documentation
Full documentation: [services/warehouse](../../../docs/services/warehouse/index.md)
