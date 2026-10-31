# gatehouse-web
Gatehouse in the browser.

## What it is
`gatehouse-web` is the WebAssembly build of `gatehouse-app` over `fetch`, with the session cookie the service's sign-in set. The service serves the module from its `WEB_DIR`.

## Use cases
- Build with `cargo build --target wasm32-unknown-unknown --profile wasm -p gatehouse-web`.

## Documentation
Full documentation: [services/gatehouse](../../../docs/services/gatehouse/index.md)
