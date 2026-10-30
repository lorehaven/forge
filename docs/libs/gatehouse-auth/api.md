---
title: API reference
parent: gatehouse-auth
grand_parent: Libraries
nav_order: 1
---

Everything is in `src/lib.rs`. `Result` is `quench_core::Result` (an error type with context chains). Functions are blocking and use no global state.

## `Credentials`

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credentials {
    pub gatehouse_url: String,   // Gatehouse including its base path, e.g. "https://host/gatehouse"
    pub app_token: String,       // the long-lived application token ("lht_...")
}
```
Serialised field names are the Rust names; both are required when reading.

## File naming

```rust
#[must_use] pub fn secret_file_name(tool: &str) -> String   // ".{tool}.secret.toml"
#[must_use] pub fn secret_dir_file(tool: &str) -> String    // ".{tool}/secret.toml"
#[must_use] pub fn find_secret_file(tool: &str, start: &Path) -> Option<PathBuf>
```
`find_secret_file` walks `start` and each parent (`Path::ancestors`); in each directory it tries `.<tool>/secret.toml` first, then `.<tool>.secret.toml`, and returns the first that `is_file()`.

## Reading and writing

```rust
pub fn load(tool: &str, start: &Path) -> Result<Option<Credentials>>
```
`Ok(None)` when no secret file is found. Errors with context `reading <path>` or `parsing <path>` (a malformed file, or a missing `gatehouse_url`/`app_token`). Extra keys in the file are ignored.

```rust
pub fn save(tool: &str, dir: &Path, credentials: &Credentials) -> Result<PathBuf>
```
Writes into `dir`: `.<tool>/secret.toml` when the directory `dir/.<tool>` exists, otherwise `dir/.<tool>.secret.toml`. If the target already exists and parses as a TOML table, its other keys are kept and the two credential keys are replaced (so values stored with `set_extra` survive a new login). The file is created/truncated with mode `0600` on Unix (plain `fs::write` elsewhere). Then `dir/.gitignore` gets the written pattern appended (`.<tool>/secret.toml` or `.<tool>.secret.toml`) unless a line already equals it, `.*.secret.toml` or `.*/secret.toml`. A secret file of the other kind in `dir` is removed, so one login leaves one token. Returns the path written.

```rust
pub fn extra(tool: &str, start: &Path, table: &str, key: &str) -> Result<Option<String>>
```
Value of `[table] key = "..."` from the found secret file; `None` when there is no file, table, key or the value is not a string.

```rust
pub fn set_extra(tool: &str, start: &Path, table: &str, key: &str, value: &str) -> Result<PathBuf>
```
Stores `[table] key = "value"` in the existing secret file (found like `load`), keeping everything else; rewritten with mode `0600`. Errors: `no secret file: run `<tool> login` first`; unreadable/unparsable file; `<path> is not a TOML table`.

## Network

```rust
pub fn access_token(credentials: &Credentials) -> Result<String>
```
`POST {gatehouse_url without trailing /}/api/v1/token` as `application/x-www-form-urlencoded` with `grant_type=app_token` and `app_token=<token>`; 30 s timeout. `200`: returns `access_token` from the JSON body. `401`: error `Gatehouse rejected the application token (revoked, expired or unknown) - run the tool's `login` again`. Any other status: `Gatehouse answered <status> to the token exchange`. Transport failure: context `reaching Gatehouse at <url>`.

```rust
#[derive(Debug, Clone, Default)]
pub struct NewToken { pub name: String, pub scope: Vec<String>, pub expires_in_days: Option<i64> }

pub fn login(gatehouse_url: &str, username: &str, password: &str, new: &NewToken) -> Result<Credentials>
```
1. `POST {base}/api/v1/auth/login` with JSON `{"username","password"}`; a non-success status is `Gatehouse refused the login (<status>)`. The response's `access_token` is used as bearer.
2. `POST {base}/api/v1/me/tokens` with JSON `{"name","scope","expires_in_days"}` (`scope` is a list of `service:action`; empty is the owner's full access; `expires_in_days` null when `None`). A non-success status is `Gatehouse refused to create the token (<status>): <body>`.
3. Returns `Credentials { gatehouse_url: base (trailing / removed), app_token: <secret from the created token> }`.

The password is used for this call only and never stored. An account with a second factor cannot complete step 1 in this flow (the login endpoint's response is not handled specially).

```rust
pub fn prompt_password(prompt: &str) -> Result<String>   // no echo, via quench_cli::term
#[must_use] pub fn bearer(token: &str) -> String         // "Bearer <token>"
```
`bearer` gives the verbatim form Cargo registries and Warehouse expect in `Authorization`.
