---
title: Authentication
parent: Gatehouse
grand_parent: Services
nav_order: 7
---

How a person or a program becomes a token holder, and what a token carries.

## Identities and accounts

Users live in `auth.users` ([data](data.md)). A username is 1 to 64 ASCII letters, digits or `. _ - @ +` (`realm::valid_username`, enforced when an account is created; older accounts are not re-checked at login). Passwords are hashed with Argon2 (`quench_crypto::argon2`, default parameters). Three roles exist: `admin`, `user`, `service`; `admin` and `service` are wildcards (see [permissions](permissions.md)).

The first start with `AUTH_BOOTSTRAP` true (the default for this service) creates the admin from `SERVICE_USERNAME`/`SERVICE_PASSWORD` (both required, the process panics if missing) and, if `SERVICE_TECH_USERNAME` and `SERVICE_TECH_PASSWORD` are both non-empty, a `service` account. An existing user is never overwritten. Both hold a wildcard role and no stored grants.

## Login outcomes

`realm::authenticate` checks, in this order: unknown user (treated as wrong credentials); disabled (`disabled_at` set); locked (`locked_until` in the future); password; unconfirmed address; MFA. A wrong password increments `failed_login_attempts`; at `GATEHOUSE_LOGIN_MAX_ATTEMPTS` (default 5) the account is locked for `GATEHOUSE_LOCKOUT_DURATION_SECS` (default 900). A successful login clears the counter and sets `last_login_at`. An address is "unconfirmed" when `email` is set, `email_verified_at` is null and the user has no wildcard role; such an account cannot log in. Accounts with no address are exempt.

| Outcome | JSON API (`/api/v1/auth/login`) | Browser (`/ui/login`) |
|---|---|---|
| success | 200 token pair | cookies + 302 |
| wrong password / unknown user | 401 `invalid_credentials` | 302 `?err=1` |
| disabled | 401 `account_disabled` | 302 `?err=ui_login_account_disabled` |
| locked | 401 `account_locked` | 302 `?err=ui_login_account_locked` |
| address unconfirmed | 401 `email_unverified` | 302 `?err=ui_login_email_unverified` |
| MFA enabled | 401 `mfa_required` (no second step over JSON) | 302 `/ui/login/mfa?pending=...` |

## Tokens

* **Access token**: EdDSA JWT. Header `kid` names the signing key. Claims (`quench_auth::domain::jwt::Claims`): `sub` (username or client id), `aud` (audiences), `scope`, `iat`, `exp`, `sid` (session id, absent for `client_credentials`, `app_token` and test tokens). Lifetime `ACCESS_TOKEN_TTL_SECS` (default 900).
* **`scope`**: space-separated: the roles, then (unless a wildcard role is held) one `service:action` per grant. `admin` alone, or `user sage:read sage:write`.
* **`aud`**: for a wildcard role every service of the permission catalog (plus Gatehouse itself); otherwise the services the user holds grants on, narrowed to the catalog, plus `gatehouse` always.
* **Refresh token**: `<uuid>.<uuid>`, opaque; only its SHA-256 is stored (cache store, TTL `REFRESH_TOKEN_TTL_SECS`, default 604800 = 7 days). Rotation takes the old token atomically and issues a new one; presenting the consumed token again within 30 seconds returns the same result as the first rotation (for clients that never learned the new token). Rotations in this process are serialised by a mutex.
* **Signing keys**: Ed25519 in `auth.signing_keys`; the private half is ChaCha20-Poly1305 ciphertext under a key derived by SHA-256 from `GATEHOUSE_KEY_ENCRYPTION_KEY`. At start, if no key without `not_after` exists, one is generated. Rotation (`POST /api/v1/admin/keys/rotate`) sets `not_after = now + ACCESS_TOKEN_TTL_SECS` on the active key and creates a new one; keys past `not_after` are not loaded or published. Public keys: `GET /.well-known/jwks.json` (`kty` `OKP`, `crv` `Ed25519`, `kid`, `x`).

## Cookies

| Cookie | Name (default) | Content | Attributes |
|---|---|---|---|
| session | `AUTH_COOKIE_NAME` (`forge_session`) | access token | `Path=/`, `HttpOnly`, `Secure`, `SameSite=Lax`, session cookie (no Max-Age), `Domain` = `AUTH_COOKIE_DOMAIN` if set |
| refresh | `AUTH_REFRESH_COOKIE_NAME` (`forge_refresh`) | refresh token | as above plus `Max-Age` = `REFRESH_TOKEN_TTL_SECS` |
| state | `forge_authorize_state` | PKCE state of a relying party | set by relying parties, never domain-scoped |
| language | `qlocale`, `qlocale_chosen` | UI language and the marker that it was chosen | set by the page script and by login |

Logout clears both with `Max-Age=0`.

## Browser sign-in and the redirect

Relying parties send the browser to `/ui/login?redirect=<their URL>`. `redirect` is accepted only if it is a rooted same-origin path (starts with `/`, not `//` or `/\`) or starts with a prefix listed in `AUTH_REDIRECT_HOSTS` followed by `/`, `?`, `#` or the end. A refresh cookie that is still good skips the form (silent renewal). `GET /ui/status` and `POST /ui/refresh` serve the page's session watcher.

## OAuth 2 authorization code with PKCE

1. Relying party redirects the browser to `GET /api/v1/authorize?client_id&redirect_uri&state&code_challenge[&code_challenge_method=S256]`.
2. No session cookie: redirect to login and back. With a session: a code (32 random bytes, base64url; stored as SHA-256; 60 seconds; single use) is created and the browser goes to `redirect_uri?code=...&state=...`.
3. The relying party calls `POST /api/v1/token` with `grant_type=authorization_code`, `code`, `redirect_uri`, `client_id`, `client_secret`, `code_verifier`. Checks: client known and secret matches; code exists, unused, unexpired, same client and redirect; `base64url(sha256(verifier))` equals the stored challenge. The code is marked consumed before tokens are issued. The access token's audience is the client's `allowed_scopes` narrowed to the catalog.

Clients come from `config/clients.toml`; see [permissions](permissions.md#oauth-clients).

## Machine identities

`client_credentials`: `client_id` + `client_secret` give an access-only token (empty `refresh_token`), `scope` fixed to `service` (a wildcard role), audience the client's `allowed_scopes`. A token is only as broad as the audience: `conveyor-publish` can reach `warehouse` only.

## Application tokens

A person mints `lht_<43 chars>` (`POST /api/v1/me/tokens`); the SHA-256 is stored in `auth.app_tokens`. Maximum lifetime 365 days (also the default). Creation needs a token with a session id. Exchange: `POST /api/v1/token` `grant_type=app_token&app_token=...` gives an access token without refresh token; its grants are the owner's current grants intersected with the token's `scope` entries (an empty scope is the owner's full access; entries the owner no longer holds are dropped), audience narrowed accordingly plus `gatehouse`. Revoked or expired tokens and unknown secrets give `401`. `last_used_at` is updated on each exchange.

## Second factor (TOTP)

Enrolment: `GET /api/v1/ui/mfa-enroll` returns a fresh base32 secret, the `otpauth://` URI (issuer `Forge`) and an SVG QR code as a data URI; `POST` with the secret and a current code stores the secret encrypted (same key as signing keys) and sets `mfa_enabled`. Login with MFA: after the password step the server signs `username:expiry:HMAC-SHA256` (key derived from `GATEHOUSE_KEY_ENCRYPTION_KEY`, valid 120 seconds) as the `pending` value; `POST /ui/login/mfa` checks it and the code. A wrong code counts toward the lockout. An administrator can turn a user's MFA off; a person can turn their own off without a code.

## Sessions

Cache-store keys: `session:{sid}`, `refresh:{token hash}`, `user:{username}` (set of sids), `reuse:{old hash}`; see [data](data.md#cache-store). Revoking is deleting. A change of a user's roles or grants, a password reset, an invitation acceptance, a deletion, or an administrator's edit of another user ends all that user's sessions. Disabling an account does not end sessions. A person's own password change does not either.
