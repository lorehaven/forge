# Quench Client

`quench-client` (crate `quench_client`) is Forge's shared HTTP client abstraction: thin `reqwest`-based wrappers that add authentication (Basic, Bearer, OAuth2 `client_credentials`) and consistent JSON error handling, so services calling each other over HTTP don't each reimplement auth headers and response parsing. This page is the full reference (based on the single source file `libs/quench-client/src/lib.rs`); the crate's own README is a short pointer here. sage-service depends on it — `docker/sage-service/src/clients/switchboard.rs` uses `ClientCredentialsClient` for machine-to-machine calls to switchboard — and `cli/welder` uses `BearerAuthClient` in its switchboard model client.

## Public API / Key Types

Re-exported from `quench_client::prelude`: `HttpClient`, `BasicAuthClient`, `BearerAuthClient`, `ClientCredentialsClient`.

- **`HttpClient`** — unauthenticated base client. `HttpClient::new(base_url)` or `HttpClient::builder(base_url).tls_verify(bool).build()`. `get`/`post`/`put`/`delete`, each generic over a `DeserializeOwned` response type (and `Serialize` body for `post`/`put`). Non-2xx responses become an `anyhow::Error` carrying the status and body text.
- **`BasicAuthClient`** — wraps an `HttpClient`, sending HTTP Basic auth on every call. `BasicAuthClient::new(base_url, username, password)` or `.builder(base_url).username(..).password(..).tls_verify(..).build()`. Same `get`/`post`/`put`/`delete`, plus `delete_expect_success(path)` for endpoints returning HTML or empty bodies that can't be deserialized (treats any 2xx as success and discards the body).
- **`BearerAuthClient`** — wraps an `HttpClient` with a fixed bearer token. `BearerAuthClient::new(base_url, token)` or `with_tls_verify(base_url, token, verify)`. `get`/`post`.
- **`ClientCredentialsClient`** — authenticates via the OAuth2 `client_credentials` grant against gatehouse's token endpoint instead of sending a shared secret to the target service directly. Built with `.builder(base_url).token_url(..).client_id(..).client_secret(..).tls_verify(..).build()`. Exchanges once, caches the access token in an internal `Mutex`, and re-exchanges automatically ~30s before expiry. `get`/`post`/`delete_expect_success`.
- **`ClientError`** / **`ClientResult<T>`** — `HttpError`, `RequestFailed`, `ParseError`, `ConnectionError` variants (defined but the client methods currently surface errors as `anyhow::Error`, not these variants).

## Configuration

No environment variables are read directly by this crate — `base_url`, `token_url`, credentials, and TLS verification are all supplied by the caller at construction time (e.g. sage-service reads `GATEHOUSE_URL`/client id/secret itself and passes them into `ClientCredentialsClient::builder`).

## Usage example

```rust
use quench_client::prelude::ClientCredentialsClient;

let client = ClientCredentialsClient::builder("https://switchboard.internal")
    .token_url("https://gatehouse.internal/api/v1/token")
    .client_id("sage")
    .client_secret(&client_secret)
    .build()?;

let models: ModelsResponse = client.get("/api/v1/models").await?;
```

[Home](../README.md)
