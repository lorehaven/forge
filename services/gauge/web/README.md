# gauge-web
Gauge in the browser.

## What it is
`gauge-web` is the WebAssembly build of `gauge-app` over `fetch`, with the session cookie the service's sign-in set. The service serves the module from its `WEB_DIR`.

## Use cases
- Build with `cargo build --target wasm32-unknown-unknown --profile wasm -p gauge-web`.

## Documentation
Full documentation: [services/gauge](../../../docs/services/gauge/index.md)
