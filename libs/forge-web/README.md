# forge-web
Serves a service's browser client.

## What it is
`forge-web` serves the HTML page that starts a `quench-ui` wasm module and the five files that page loads (loader script, stylesheet, wasm module, shell script, icon). A service declares four to six small routes and hands each to a `Client`. The page carries a strict content security policy with a per-response nonce.

## Use cases
- Serve a service's wasm client from `WEB_DIR`.
- Get the CSP and nonce handling without writing it again.

## Documentation
Full documentation: [libs/forge-web](../../docs/libs/forge-web/index.md)
