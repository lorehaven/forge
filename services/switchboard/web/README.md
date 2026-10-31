# switchboard-web
Switchboard in the browser.

## What it is
`switchboard-web` is the WebAssembly build of `switchboard-app` over `fetch`, with the session cookie the service's sign-in set. The service serves the module from its `WEB_DIR`.

## Use cases
- Build with `cargo build --target wasm32-unknown-unknown --profile wasm -p switchboard-web`.

## Documentation
Full documentation: [services/switchboard](../../../docs/services/switchboard/index.md)
