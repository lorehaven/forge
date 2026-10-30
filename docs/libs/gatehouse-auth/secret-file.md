---
title: Secret file
parent: gatehouse-auth
grand_parent: Libraries
nav_order: 2
---

## Location

For tool `T`, in order of preference, in the start directory and each ancestor:

1. `.T/secret.toml`: when the tool has a `.T/` directory for its configuration (anvil uses `.anvil/secret.toml`).
2. `.T.secret.toml`.

`save` picks (1) when `dir/.T` exists as a directory, else (2).

## Content

```toml
gatehouse_url = "https://host/gatehouse"
app_token = "lht_..."

# optional values stored with set_extra(tool, start, "table", "key", "value"):
[android]
keystore_password = "..."
```

`gatehouse_url` includes Gatehouse's base path; `access_token` appends `/api/v1/token`. Mode `0600` on Unix. The file is added to `.gitignore` by `save`; never commit it.

## Lifecycle

1. `tool login` calls `login` (username, password prompt via `prompt_password`) and `save`.
2. Every run: `load` then `access_token` gives a bearer token valid for the access-token lifetime of Gatehouse (see the Gatehouse configuration page for the TTL variables).
3. Revoking the application token in Gatehouse (`DELETE /api/v1/me/tokens/{id}`) makes the next exchange fail with the "rejected" message.
