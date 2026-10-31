# gatehouse-client
A blocking client session for Gatehouse.

## What it is
`gatehouse-client` is the session every native client of Gatehouse needs: sign in, keep the rotating refresh token safe, hand out an access token with life left, and make a request with the bearer, renewing the token and trying again once when the server refuses it.

## Use cases
- Sign in from a desktop or Android app.
- Call a service with a bearer token that renews itself.

## Documentation
Full documentation: [libs/gatehouse-client](../../docs/libs/gatehouse-client/index.md)
