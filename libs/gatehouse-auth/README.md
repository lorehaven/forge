# gatehouse-auth

Application-token credentials for the command-line tools (`anvil`, `riveter`).

A person mints a long-lived, revocable application token once (`anvil login`),
which is stored in a git-ignored `.<tool>.secret.toml`:

```toml
gatehouse_url = "https://host/gatehouse"
app_token = "lht_..."
```

Each run exchanges it at Gatehouse's `POST /api/v1/token`
(`grant_type=app_token`) for a short-lived access token, so nothing has to be
copied by hand and nothing short-lived is stored.
