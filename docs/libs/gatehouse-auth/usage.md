---
title: Usage
parent: gatehouse-auth
grand_parent: Libraries
nav_order: 3
---

## 1. `login` command of a tool

```rust
use gatehouse_auth::{NewToken, login, prompt_password, save};

let password = prompt_password("Password: ")?;
let creds = login(
    "https://host/gatehouse", "alice", &password,
    &NewToken { name: "anvil on laptop".into(), scope: vec![], expires_in_days: Some(90) },
)?;
let path = save("anvil", std::path::Path::new("."), &creds)?;
println!("saved {}", path.display());
```

Effect: a token named `anvil on laptop`, valid 90 days, full access of alice, stored in `./.anvil.secret.toml` (or `./.anvil/secret.toml`). Pitfall: a narrower `scope` such as `["warehouse:write"]` limits what the token can do.

## 2. Authenticated call

```rust
let Some(creds) = gatehouse_auth::load("anvil", &std::env::current_dir()?)? else {
    quench_core::bail!("run `anvil login` first");
};
let header = gatehouse_auth::bearer(&gatehouse_auth::access_token(&creds)?);
```

Pass `header` as the `Authorization` header, for example to Warehouse. Pitfall: the exchange happens on every call of `access_token`; cache the result within a run.

## 3. Keep a secret next to the token

```rust
gatehouse_auth::set_extra("anvil", &cwd, "android", "keystore_password", "...")?;
let v = gatehouse_auth::extra("anvil", &cwd, "android", "keystore_password")?;
```

A later `login` keeps the table.

*Unverified:* the snippets on this page were written from the sources and the crate's tests; they were not compiled or run here.
