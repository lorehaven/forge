---
title: Usage
parent: gatehouse-client
grand_parent: Libraries
nav_order: 2
---

## 1. Sign in and keep the refresh token

```rust
use gatehouse_client::login;
let mut s = login("https://host", "alice", "pw")?.keeping_tokens(|t| save_somewhere(t));
```

`keeping_tokens` runs inside `refresh`, before the request is answered: a refresh spends the old refresh token, so a token not stored would sign the user out at the next start.

## 2. Resume at the next start

```rust
let mut s = gatehouse_client::Session::resume("https://host", &stored);
let bearer = s.fresh_token()?;        // refreshes: no access token yet
```

`Error::Unauthorized` means the stored token is no longer good: sign in again.

## 3. Two programs, one user

```rust
let s = Session::resume(base, &t).following(|| read_token_file());
```

If the background check refreshed first, the app's held token is spent; `following` makes it use the newer one before refreshing and again if the refresh is refused.

## 4. A request

```rust
let r = s.call("GET", "/warehouse/api/v1/files", false, |client, token| {
    client.get(format!("{}/warehouse/api/v1/files", "https://host")).bearer_auth(token).send()
})?;
```

A `401` refreshes and sends again once. Use `transfer = true` for uploads and downloads (longer timeout).

## 5. Trace

```rust
gatehouse_client::trace_var("FORKLIFT_TRACE");   // then run with FORKLIFT_TRACE=1
```

Prints `gatehouse-client: POST https://host/gatehouse/api/v1/auth/refresh -> 200` lines on standard error.

## Pitfalls

* Blocking: do not call on a UI thread.
* The base path `/gatehouse` is fixed inside `login` and `refresh`.
* Accounts with MFA cannot sign in (`MfaRequired`).

*Unverified:* the snippets on this page were written from the sources and the crate's tests; they were not compiled or run here.
