---
title: API reference
parent: gatehouse-client
grand_parent: Libraries
nav_order: 1
---

Everything is in `src/lib.rs`. Re-exports: `pub use quench_client::blocking::{Client, Response};`.

## `Error`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error { Network(String), Unauthorized, MfaRequired, Status(u16), AccountLocked, AccountDisabled, EmailUnverified }
```
Implements `Display` and `std::error::Error`.

| Variant | Meaning | `Display` text |
|---|---|---|
| `Network(String)` | cannot reach the services or the answer is unreadable | `cannot reach the server: {why}` |
| `Unauthorized` | wrong user name or password, or the refresh token is no longer good | `the user name or password is wrong, or the session ended` |
| `MfaRequired` | the account has a second factor, which the client cannot ask for | `this account needs a second factor: turn MFA off for it to use forklift` |
| `Status(u16)` | any other status | `the server answered {code}` |
| `AccountLocked` | too many wrong passwords | `this account is locked after too many wrong passwords: try again in about 15 minutes, or have it unlocked in gatehouse` |
| `AccountDisabled` | disabled by an administrator | `this account is disabled` |
| `EmailUnverified` | email never confirmed | `confirm the email address of this account first (gatehouse sent a link)` |

Mapping of a token response (`tokens`): `200` parses `{access_token, refresh_token, expires_in (default 0)}`; any status in 400..=403 whose body contains `mfa_required` is `MfaRequired`; `400|401|403` with body containing `account_locked`, `account_disabled`, `email_unverified` map to the matching variant; other `400|401|403` is `Unauthorized`; any other status is `Status(code)`. Matching is on substrings of the body text (Gatehouse sends `{"error": "<code>"}`). An unparsable `200` body is `Network`.

```rust
#[must_use] pub fn network(e: impl fmt::Display) -> Error   // Error::Network(e.to_string())
```

## Clients and timeouts

```rust
pub fn set_timeouts(call: Duration, transfer: Duration)   // once, before the first call; the first call fixes them
#[must_use] pub fn client() -> &'static Client            // whole-call timeout; default 20 s
#[must_use] pub fn transfer_client() -> &'static Client   // for files; default 15 minutes
```
Both clients are created once (a client owns the threads its requests run on). `set_timeouts` after the first `client()` / `transfer_client()` call, or a second call of it, has no effect (`OnceLock`).

## Tracing

```rust
pub fn trace_var(name: &'static str)                       // call once; first call wins
pub fn trace(method: &str, url: &str, outcome: &str)
```
When the environment variable `name` is set (any value), `trace` prints `gatehouse-client: {method} {path} -> {outcome}` on standard error; `path` is the URL without its query. Never a token, password or body. Every request through this crate traces (`POST .../auth/login`, `.../auth/refresh`, and `Session::call`).

## `login`

```rust
pub fn login(base: &str, username: &str, password: &str) -> Result<Session, Error>
```
`base` is trimmed and stripped of trailing `/`. Sends `POST {base}/gatehouse/api/v1/auth/login` with JSON `{"username","password"}` using `client()`. The password is not kept.

## `Session`

`#[derive(Clone)]`; holds `base`, access token, refresh token, `expires_in`, the instant the access token was obtained, and the optional hooks. Its `Debug` prints only `base` and `expires_in`, never the tokens.

| Method | Description |
|---|---|
| `#[must_use] pub fn resume(base: &str, refresh_token: &str) -> Self` | a session from a kept refresh token; no access token yet (the first request refreshes) |
| `#[must_use] pub fn refresh_token(&self) -> &str` | the token to store: the new one after every refresh |
| `pub fn fresh_token(&mut self) -> Result<String, Error>` | an access token with life left: refreshes when there is none, or when the elapsed time is at least `expires_in * 4 / 5` seconds (or no time is known); for a host that sends the request itself |
| `#[must_use] pub const fn expires_in(&self) -> i64` | seconds the access token lives (0 before the first token) |
| `#[must_use] pub fn keeping_tokens(self, store: impl Fn(&str) + Send + Sync + 'static) -> Self` | `store` is called with every new refresh token the moment a refresh gives one, before the request that needed it is answered |
| `#[must_use] pub fn following(self, latest: impl Fn() -> Option<String> + Send + Sync + 'static) -> Self` | before a refresh, and again after a refused refresh, ask `latest` for the newest refresh token kept by anyone; used when it is non-empty and differs from the held one |
| `pub fn refresh(&mut self) -> Result<(), Error>` | `POST {base}/gatehouse/api/v1/auth/refresh` with JSON `{"refresh_token"}`; replaces both tokens (the old refresh token is spent) and calls the `keeping_tokens` hook. On `Unauthorized`, retries once with a newer token from `following`, if any. A refusal leaves the session and its hooks as they were |
| `#[must_use] pub fn base(&self) -> &str` | the server (no trailing slash) |
| `pub fn call(&mut self, method: &str, path: &str, transfer: bool, send: impl Fn(&Client, &str) -> Result<Response, quench_client::transport::Error>) -> Result<Response, Error>` | with no access token it refreshes first; calls `send(client, access_token)` (`transfer_client()` when `transfer`); on a `401` on the first attempt it refreshes and sends once more; a second `401` is returned as the server's answer. `method` and `path` are for the trace only. Transport failure is `Network` |
| `pub fn get(&mut self, path: &str, query: &[(&str, String)], transfer: bool) -> Result<Response, Error>` | `GET {base}{path}` with the query and `bearer_auth` through `call` |
