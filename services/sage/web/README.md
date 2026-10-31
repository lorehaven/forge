# sage-web
Sage in the browser.

## What it is
`sage-web` is the WebAssembly build of `sage-app` over `fetch` and `EventSource`, with the session cookie the service's sign-in set. The service serves the module from its `WEB_DIR`.

## Use cases
- Build with `cargo build --target wasm32-unknown-unknown --profile wasm -p sage-web`.

## Documentation
Full documentation: [services/sage](../../../docs/services/sage/index.md)
