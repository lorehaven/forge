# gatehouse-auth
Application-token credentials for the command-line tools.

## What it is
`gatehouse-auth` lets `anvil` and `riveter` sign in without a password. A person mints a long-lived, revocable application token once (`anvil login`), stored in a git-ignored `.<tool>/secret.toml`. Each run exchanges it at Gatehouse's `POST /api/v1/token` (`grant_type=app_token`) for a short-lived access token, so nothing has to be copied by hand and nothing short-lived is stored.

## Use cases
- Read the secret file and exchange the token.
- Run the login flow that mints an application token.

## Example
```toml
gatehouse_url = "https://host/gatehouse"
app_token = "lht_..."
```

## Documentation
Full documentation: [libs/gatehouse-auth](../../docs/libs/gatehouse-auth/index.md)
