# Gatehouse Service
The Forge authentication service: one login, one identity store.

## What it is
`gatehouse` is the estate's only login, token issuer and identity store. It serves sign-in, the services dashboard, account pages and user administration, and issues the tokens the other services verify through `quench-auth`.

## Use cases
- Sign people in once for every service.
- Mint and revoke application tokens for command-line tools.
- Administer users and OAuth clients.

## Documentation
Full documentation: [services/gatehouse](../../../docs/services/gatehouse/index.md)
