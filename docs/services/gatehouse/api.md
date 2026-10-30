---
title: API
parent: Gatehouse
grand_parent: Services
nav_order: 1
---

Every HTTP route of `gatehouse-service`, rendered from [openapi.yaml](openapi.yaml). Paths are relative to `BASE_PATH` (deployments use `/gatehouse`).

## Conventions

* **Authentication.** The token API and user administration are not behind a middleware (Gatehouse mints the tokens): each route checks itself. A *session* is either `Authorization: Bearer <access token>` or the session cookie (`forge_session`). The token must verify against the realm's keys, name `gatehouse` in its audience, and (when it has a session id) belong to an active session; otherwise `401` with an empty body. With `SERVICE_AUTH_ENABLED` off (the estate's development switch) the user API treats every caller as an anonymous administrator.
* **Permissions.** `x-permission: gatehouse:<action>` means the token must hold that catalog action or a wildcard role (`admin`, `service`); otherwise `403` with an empty body. See [permissions](permissions.md).
* **Errors.** The user API answers `{"error": "<English text>"}`; the page API and the form posts use translation keys (`ui_admin_error_*`). A request the framework cannot read (invalid JSON or form, a missing required query field, a body over 2 MiB) is `400` with a plain-text body.
* **Form posts** (`/ui/...`) always answer with a `302`; the outcome is in the redirect's query (`?ok=<key>` or `?err=<key>`), which the client shows from a fixed list of keys.
* **Framework routes.** `quench-starter` adds, for every service and outside `openapi.yaml`: `GET /health` (`200 OK`, text), `GET /health/live` and `GET /health/ready` (`200 {"status":"live"|"ready"}`, else `503 {"status":"not_live"|"not_ready"}`), `GET /swagger-ui` and `/swagger-ui/` (permanent redirects toward `/swagger-ui/index.html`, (no route of the service serves `index.html`)), `GET <base>` and `GET <base>/` (302 to `<base>/ui`), and `GET /` on the server root (302 to `<base>/ui` when a base path is set).
* **Rate limits** apply to every route that causes mail: see [operations](operations.md#rate-limits-and-the-mail-budget).

The machine-readable description is [openapi.yaml](openapi.yaml) (OpenAPI 3.1.0, version 0.4.2). Each section below is generated from it.

## auth

Token API: password login, refresh, logout, userinfo. JSON in and out; the credential is in the body.

### POST `/api/v1/auth/login`

Password login

Checks the credentials and returns a token pair. An account with a second factor, a disabled or locked account, and an unconfirmed address are refused with a code in the body; the second-factor step exists only on the browser path (`POST /ui/login/mfa`). A wrong password counts toward the lockout (`GATEHOUSE_LOGIN_MAX_ATTEMPTS`, `GATEHOUSE_LOCKOUT_DURATION_SECS`).

**Authentication:** none (public).

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `username` | string | yes |  |
| `password` | string | yes |  |

```json
{
  "username": "alice",
  "password": "correct horse"
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Tokens issued. | `application/json` [TokenResponse](#schema-tokenresponse) |
| 401 | Refused; the code says why. | `application/json` [LoginError](#schema-loginerror) |
| 400 | Malformed JSON (text body). |  |
| 500 | The user store failed. |  |

Example `200` response:

```json
{
  "access_token": "eyJhbGciOiJFZERTQSIs...",
  "refresh_token": "Zm9vYmFy...",
  "token_type": "Bearer",
  "expires_in": 900
}
```

Example `401` response:

```json
{
  "error": "invalid_credentials"
}
```

### POST `/api/v1/auth/refresh`

Rotate a refresh token

The refresh token comes from the JSON body or, when the body is empty or not the expected JSON, from the refresh cookie. The old token is spent; presenting it again within a short grace period returns the result of the first rotation. Rotations are serialised in-process. In the cookie flow the response also sets the session and refresh cookies.

**Authentication:** `refreshCookie`.

**Request body** (`application/json`, optional)

| Field | Type | Required | Description |
|---|---|---|---|
| `refresh_token` | string | yes |  |

```json
{
  "refresh_token": "Zm9vYmFy..."
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | New token pair. Headers: `Set-Cookie`. | `application/json` [TokenResponse](#schema-tokenresponse) |
| 400 | No refresh token in the body or the cookie. |  |
| 401 | Unknown, expired or revoked token, or the user is gone. |  |
| 500 | Store or signing failure. |  |

### POST `/api/v1/auth/logout`

Revoke a session

Revokes the session behind the refresh token, realm-wide (every service sees it on its next request). Answers 204 even for an unknown token.

**Authentication:** none (public).

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `refresh_token` | string | yes |  |

```json
{
  "refresh_token": "Zm9vYmFy..."
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 204 | Revoked. |  |
| 400 | Malformed JSON (text body). |  |
| 500 | Session store failure. |  |

### GET `/api/v1/auth/userinfo`

What a token says

Accepts only an `Authorization: Bearer` header (never the cookie). The token must verify, include the `gatehouse` audience and (when it has a session id) belong to an active session.

**Authentication:** `bearerAuth`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Subject, roles, audiences, permissions. | `application/json` [UserInfo](#schema-userinfo) |
| 401 | No acceptable token (empty body). |  |

Example `200` response:

```json
{
  "sub": "alice",
  "roles": [
    "user"
  ],
  "audiences": [
    "sage",
    "gatehouse"
  ],
  "permissions": {
    "sage": [
      "read",
      "write"
    ]
  }
}
```

## oauth

OAuth 2 surface: authorization code with PKCE, the token endpoint (four grants), JWKS and signing key rotation.

### GET `/api/v1/authorize`

Authorization endpoint (code + PKCE)

Without a session cookie the browser is sent to `/ui/login?redirect=<this URL>`; with one a single-use code (valid 60 seconds) is created and the browser is redirected to `redirect_uri?code=...&state=...`. `redirect_uri` must equal one registered for the client (built from `<PREFIX>_UI_URL` or `<PREFIX>_URL` plus the client's `redirect_path`).

**Authentication:** `sessionCookie`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `client_id` | query | string | yes | A client from `config/clients.toml`. |
| `redirect_uri` | query | string | yes | Exact match of the registration. |
| `state` | query | string | yes | Opaque, returned unchanged. |
| `scope` | query | string | no | Stored with the code; the issued token's audience comes from the client's `allowed_scopes`, not from this. |
| `code_challenge` | query | string | yes | Base64url SHA-256 of the verifier. |
| `code_challenge_method` | query | string (`S256`) | no | Only `S256` is supported (also when omitted). |

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | To the login page, or back to the client with a code. Headers: `Location`. |  |
| 400 | Unknown client, unregistered redirect_uri, or a challenge method other than S256 (or a missing query field: text body). | `application/json` [Problem](#schema-problem) |
| 401 | The session's user no longer exists. |  |
| 500 | Code could not be stored. |  |

Example `400` response:

```json
{
  "error": "unknown client"
}
```

### POST `/api/v1/token`

Token endpoint

Form-encoded. `authorization_code`: code + redirect_uri + client_id + client_secret + code_verifier, returns an access token narrowed to the client's `allowed_scopes` plus a refresh token. `refresh_token`: rotates like `/auth/refresh`. `client_credentials`: access-only token for the client, scope `service`, audience = the client's `allowed_scopes`. `app_token`: exchanges an application token (`lht_...`) for an access-only token carrying the owner's current grants narrowed to the token's scope.

**Authentication:** none (public).

**Request body** (`application/x-www-form-urlencoded`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `grant_type` | string (`authorization_code`, `refresh_token`, `client_credentials`, `app_token`) | yes |  |
| `code` | string | no | authorization_code: the code from `/api/v1/authorize`. |
| `redirect_uri` | string | no | authorization_code: must equal the one the code was issued for. |
| `code_verifier` | string | no | authorization_code: the PKCE verifier (S256). |
| `client_id` | string | no | authorization_code, client_credentials. |
| `client_secret` | string | no | authorization_code, client_credentials. |
| `refresh_token` | string | no | refresh_token grant. |
| `app_token` | string | no | app_token grant: the `lht_...` secret. |

```text
grant_type=app_token&app_token=lht_...
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Tokens. | `application/json` [TokenResponse](#schema-tokenresponse) |
| 400 | Unsupported grant, missing field, unknown client, wrong secret, bad/expired/used code, redirect mismatch, PKCE failure. | `application/json` [Problem](#schema-problem) |
| 401 | refresh_token / app_token grant: unknown, expired, revoked or exhausted token, or the user is gone. |  |
| 500 | Store or signing failure. |  |

Example `200` response:

```json
{
  "access_token": "eyJ...",
  "refresh_token": "",
  "token_type": "Bearer",
  "expires_in": 900
}
```

Example `400` response:

```json
{
  "error": "PKCE verification failed"
}
```

### GET `/.well-known/jwks.json`

Public signing keys

The Ed25519 public keys (one active, plus recently rotated-out ones until their retirement). Unauthenticated; relying parties cache it.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | JWK Set. | `application/json` [Jwks](#schema-jwks) |

Example `200` response:

```json
{
  "keys": [
    {
      "kty": "OKP",
      "crv": "Ed25519",
      "kid": "0b9c...",
      "x": "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo"
    }
  ]
}
```

### POST `/api/v1/admin/keys/rotate`

Rotate the signing key

Retires the active key (it stays in JWKS for one access-token lifetime) and generates a new one.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `gatehouse:manage-signing-keys`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 204 | Rotated. |  |
| 401 | Missing, invalid or revoked token (empty body). |  |
| 403 | Authenticated but lacking the required `gatehouse` action (empty body). |  |
| 500 | Rotation failed. |  |

## users

Realm user administration and the caller's own effective access. Each route needs a `gatehouse:*` catalog action.

### GET `/api/v1/users`

List users

Sorted by username.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `gatehouse:read-users`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | All users. | `application/json` array of [UserView](#schema-userview) |
| 401 | Missing, invalid or revoked token (empty body). |  |
| 403 | Authenticated but lacking the required `gatehouse` action (empty body). |  |
| 500 | Store failure. | `application/json` [Problem](#schema-problem) |

Example `200` response:

```json
[
  {
    "username": "alice",
    "roles": [
      "user"
    ],
    "permissions": {
      "sage": [
        "read"
      ]
    },
    "wildcard": false,
    "email": "alice@example.org",
    "email_verified": true
  }
]
```

### GET `/api/v1/users/{username}`

Read one user

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `gatehouse:read-users`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `username` | path | string | yes | The account name. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The user. | `application/json` [UserView](#schema-userview) |
| 401 | Missing, invalid or revoked token (empty body). |  |
| 403 | Authenticated but lacking the required `gatehouse` action (empty body). |  |
| 404 | No such user. | `application/json` [Problem](#schema-problem) |

Example `404` response:

```json
{
  "error": "no such user"
}
```

### POST `/api/v1/users`

Create a user

Roles default to `["user"]`. Grants are checked against the catalog. With `invite` true and an `email`, the account gets a random password and an invitation link (7 days) is mailed; `invite_sent` reports whether mail was accepted. Assigning `admin` or `service` needs the literal `admin` role.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `gatehouse:create-user`.

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `username` | string | yes | 1 to 64 characters of ASCII letters, digits and `. _ - @ +`; trimmed. |
| `password` | string | no | Required unless `invite` is true. Default `""`. |
| `roles` | array of [Role](#schema-role) | no | Empty means `["user"]`. `admin`/`service` need the literal admin role. Default `[]`. |
| `permissions` | [Permissions](#schema-permissions) | no | Grants as `{service: [action, ...]}`. Actions are plain catalog names (`read`) or resource-scoped `<resource_type>:<resource_id>:<action>` (`project:7:write`). Sets: duplicates collapse, order is sorted. |
| `email` | string or null | no | Required when `invite` is true. |
| `invite` | boolean | no | Email the address an invitation to choose a password; the account gets a random password nobody is told. Default `false`. |

```json
{
  "username": "bob",
  "password": "s3cret",
  "roles": [
    "user"
  ],
  "permissions": {
    "workbench": [
      "read"
    ]
  }
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 201 | Created. | `application/json` [UserView](#schema-userview) |
| 401 | Missing, invalid or revoked token (empty body). |  |
| 403 | Missing `create-user` (empty body), or `admin`/`service` roles requested without the admin role (JSON body). | `application/json` [Problem](#schema-problem) |
| 400 | Rejected input. | `application/json` [Problem](#schema-problem) |
| 409 | A rule keeping the realm reachable. | `application/json` [Problem](#schema-problem) |
| 500 | The change could not be saved. | `application/json` [Problem](#schema-problem) |

Example `403` response:

```json
{
  "error": "only an admin may assign the admin or service role"
}
```

Example `400` response:

```json
{
  "error": "username must not be empty"
}
```

Example `409` response:

```json
{
  "error": "the realm must keep at least one admin"
}
```

Example `500` response:

```json
{
  "error": "the change could not be saved"
}
```

### PATCH `/api/v1/users/{username}`

Change password, roles or grants

Only the given fields change. Changing roles or grants, or changing another user at all, ends the target's sessions. Removing `admin` from yourself or from the last admin is refused. A password change mails a security notice to the confirmed address.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `gatehouse:edit-user`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `username` | path | string | yes | The account name. |

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `password` | string | no |  |
| `roles` | array of [Role](#schema-role) | no |  |
| `permissions` | [Permissions](#schema-permissions) | no | Grants as `{service: [action, ...]}`. Actions are plain catalog names (`read`) or resource-scoped `<resource_type>:<resource_id>:<action>` (`project:7:write`). Sets: duplicates collapse, order is sorted. |

```json
{
  "permissions": {
    "sage": [
      "read",
      "write"
    ]
  }
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Updated. | `application/json` [UserView](#schema-userview) |
| 401 | Missing, invalid or revoked token (empty body). |  |
| 403 | Missing `edit-user`, or roles need admin. | `application/json` [Problem](#schema-problem) |
| 400 | Rejected input. | `application/json` [Problem](#schema-problem) |
| 404 | No such user or template. | `application/json` [Problem](#schema-problem) |
| 409 | A rule keeping the realm reachable. | `application/json` [Problem](#schema-problem) |
| 500 | The change could not be saved. | `application/json` [Problem](#schema-problem) |

Example `403` response:

```json
{
  "error": "only an admin may assign the admin or service role"
}
```

Example `400` response:

```json
{
  "error": "username must not be empty"
}
```

Example `404` response:

```json
{
  "error": "no such user"
}
```

Example `409` response:

```json
{
  "error": "the realm must keep at least one admin"
}
```

Example `500` response:

```json
{
  "error": "the change could not be saved"
}
```

### PUT `/api/v1/users/{username}/permissions`

Replace a user's grants

Replaces the whole grant set (an empty `permissions` removes everything). Ends the target's sessions.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `gatehouse:manage-permissions`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `username` | path | string | yes | The account name. |

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `permissions` | [Permissions](#schema-permissions) | no | Grants as `{service: [action, ...]}`. Actions are plain catalog names (`read`) or resource-scoped `<resource_type>:<resource_id>:<action>` (`project:7:write`). Sets: duplicates collapse, order is sorted. |

```json
{
  "permissions": {
    "sage": [
      "read"
    ],
    "conveyor": [
      "project:7:write"
    ]
  }
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Updated. | `application/json` [UserView](#schema-userview) |
| 401 | Missing, invalid or revoked token (empty body). |  |
| 403 | Authenticated but lacking the required `gatehouse` action (empty body). |  |
| 400 | Rejected input. | `application/json` [Problem](#schema-problem) |
| 404 | No such user or template. | `application/json` [Problem](#schema-problem) |
| 500 | The change could not be saved. | `application/json` [Problem](#schema-problem) |

Example `400` response:

```json
{
  "error": "username must not be empty"
}
```

Example `404` response:

```json
{
  "error": "no such user"
}
```

Example `500` response:

```json
{
  "error": "the change could not be saved"
}
```

### POST `/api/v1/users/{username}/template`

Apply a grant template

Replaces the user's grants with the template's.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `gatehouse:manage-permissions`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `username` | path | string | yes | The account name. |

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `template` | string | yes | A template name from `[templates.*]` of the permission catalog. |

```json
{
  "template": "editor"
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Updated. | `application/json` [UserView](#schema-userview) |
| 401 | Missing, invalid or revoked token (empty body). |  |
| 403 | Authenticated but lacking the required `gatehouse` action (empty body). |  |
| 404 | No such user or no such permission template. | `application/json` [Problem](#schema-problem) |
| 500 | Not saved. | `application/json` [Problem](#schema-problem) |

Example `404` response:

```json
{
  "error": "no such permission template"
}
```

### DELETE `/api/v1/users/{username}`

Delete a user

Refused for yourself and for the last admin. Ends the user's sessions.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `gatehouse:delete-user`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `username` | path | string | yes | The account name. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 204 | Deleted. |  |
| 401 | Missing, invalid or revoked token (empty body). |  |
| 403 | Authenticated but lacking the required `gatehouse` action (empty body). |  |
| 404 | No such user. | `application/json` [Problem](#schema-problem) |
| 409 | `you cannot delete your own account` or `the realm must keep at least one admin`. | `application/json` [Problem](#schema-problem) |
| 500 | Not saved. | `application/json` [Problem](#schema-problem) |

Example `409` response:

```json
{
  "error": "you cannot delete your own account"
}
```

### GET `/api/v1/me`

The caller's own effective access

Read from the user record, not the token, so a grant added since the token was minted shows. A wildcard role lists every catalog action of every service.

**Authentication:** `bearerAuth` or `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Effective access. | `application/json` [Me](#schema-me) |
| 401 | Missing, invalid or revoked token (empty body). |  |

Example `200` response:

```json
{
  "username": "alice",
  "roles": [
    "user"
  ],
  "wildcard": false,
  "effective": {
    "sage": [
      "read",
      "write"
    ]
  }
}
```

## app-tokens

A person's long-lived application tokens for command-line tools.

### POST `/api/v1/me/tokens`

Mint an application token

Needs a login session behind the credential (a token with no session id, such as one from an app-token exchange, gets 403), so a token can always be revoked by its owner. Every `scope` entry must be a `service:action` the owner holds.

**Authentication:** `bearerAuth` or `sessionCookie`.

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes | Non-blank after trimming. |
| `scope` | array of string | no | `service:action` entries the token is limited to; each must be held by the owner. Empty is the owner's full access. Default `[]`. |
| `expires_in_days` | integer or null | no | At least 1; capped at 365. Omitted means 365. |

```json
{
  "name": "anvil on laptop",
  "scope": [
    "warehouse:write"
  ],
  "expires_in_days": 90
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 201 | Created; the secret is shown only here. | `application/json` [CreatedAppToken](#schema-createdapptoken) |
| 401 | No credential, or the user is gone. | `application/json` [Problem](#schema-problem) |
| 403 | `application tokens are created from a signed-in session`. | `application/json` [Problem](#schema-problem) |
| 422 | Blank name, `expires_in_days` below 1, malformed entry, or an entry the owner does not hold. | `application/json` [Problem](#schema-problem) |
| 500 | Not stored. |  |

Example `201` response:

```json
{
  "id": "x7Qm2pLw9aBc",
  "name": "anvil on laptop",
  "scope": [
    "warehouse:write"
  ],
  "created_at": "2026-10-30T08:00:00Z",
  "expires_at": "2027-01-28T08:00:00Z",
  "last_used_at": null,
  "revoked": false,
  "secret": "lht_abc..."
}
```

Example `401` response:

```json
{
  "error": "unknown user"
}
```

Example `403` response:

```json
{
  "error": "application tokens are created from a signed-in session"
}
```

Example `422` response:

```json
{
  "error": "you do not hold sage:write"
}
```

### GET `/api/v1/me/tokens`

List my application tokens

Includes revoked and expired ones; never the secrets.

**Authentication:** `bearerAuth` or `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The caller's tokens. | `application/json` array of [AppTokenView](#schema-apptokenview) |
| 401 | Missing, invalid or revoked token (empty body). |  |
| 500 | Store failure. |  |

### DELETE `/api/v1/me/tokens/{id}`

Revoke an application token

Idempotent. Another person's token answers 404.

**Authentication:** `bearerAuth` or `sessionCookie`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `id` | path | string | yes | The token's `id`. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 204 | Revoked (or already). |  |
| 401 | Missing, invalid or revoked token (empty body). |  |
| 404 | Not the caller's token (empty body). |  |
| 500 | Store failure. |  |

## notify

Notifications that other services ask Gatehouse to send.

### POST `/api/v1/notify`

Ask Gatehouse to notify a person

Validation comes first (unknown template, missing or extra variable, control characters, values over 300 characters, a `url` that is not under Gatehouse's public origin: 400). Then: unknown user (`skipped: no_such_user`), no confirmed address (`no_verified_email`), unsubscribed (`not_subscribed`), duplicate `dedupe_key` (`duplicate`), per-person limit 20/hour or per-person-per-template 10/hour (`rate_limited`). Mail goes out in the person's `preferred_locale`.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `gatehouse:notify`.

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `username` | string | yes |  |
| `template` | string | yes | A template id: `conveyor.run.failed`, `conveyor.run.succeeded`, `conveyor.run.recovered`. |
| `vars` | map of string to string | no | Exactly the variables the template declares (`project`, `run`, `ref`, `url` for the three conveyor templates). Default `{}`. |
| `dedupe_key` | string or null | no | Repeating a key for the same person and template within a day sends nothing. |
| `requested` | boolean | no | The person subscribed in the calling service: sends a kind that is off by default unless they opted out of it here. Default `false`. |

```json
{
  "username": "alice",
  "template": "conveyor.run.failed",
  "vars": {
    "project": "forge",
    "run": "#412",
    "ref": "master",
    "url": "https://ennor.ddns.net/conveyor/ui/runs/412"
  },
  "dedupe_key": "run-412",
  "requested": false
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 202 | Handed to the mail server. | `application/json` [NotifyResponse](#schema-notifyresponse) |
| 200 | Deliberately not sent. | `application/json` [NotifyResponse](#schema-notifyresponse) |
| 400 | Invalid request. | `application/json` [Problem](#schema-problem) |
| 401 | Missing, invalid or revoked token (empty body). |  |
| 403 | Authenticated but lacking the required `gatehouse` action (empty body). |  |
| 502 | Should have been sent and was not. | `application/json` [NotifyResponse](#schema-notifyresponse) |

Example `202` response:

```json
{
  "status": "accepted"
}
```

Example `200` response:

```json
{
  "status": "skipped",
  "reason": "not_subscribed"
}
```

Example `400` response:

```json
{
  "error": "no such notification: conveyor.run.nope"
}
```

Example `502` response:

```json
{
  "status": "failed",
  "reason": "the daily email budget is used up",
  "retryable": true
}
```

## test

Token minting for the BDD harness; answers 404 unless `GATEHOUSE_TEST_MODE=true`.

### POST `/api/v1/test/token`

Mint a token (tests only)

Signs the given claims with the real key. Answers 404 unless the environment variable `GATEHOUSE_TEST_MODE` is `true` (the service's `main` never sets it).

**Authentication:** none (public).

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `sub` | string | yes |  |
| `aud` | array of string | no |  Default `[]`. |
| `scope` | string | yes | Space separated roles and `service:action` entries. |
| `iat` | integer or null | no | Unix seconds; default now. |
| `exp` | integer or null | no | Unix seconds; default now + access TTL. |

```json
{
  "sub": "tester",
  "aud": [
    "gatehouse"
  ],
  "scope": "admin"
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The token. | `application/json` object |
| 404 | Test mode is off. |  |
| 500 | Signing failed. |  |

Example `200` response:

```json
{
  "access_token": "eyJ..."
}
```

## pages

JSON the browser client draws its screens from (`/api/v1/ui/...`).

### GET `/api/v1/ui/dashboard`

Dashboard cards

Services of the catalog with `home = true` whose `<NAME>_UI_URL` (or `<NAME>_URL`) is set and `FEATURE_<NAME>_ENABLED` is not off, filtered to those the person may use: holds `read` on it, or any grant on it, or a wildcard role. In catalog (name) order.

**Authentication:** `bearerAuth` or `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Cards. | `application/json` [Dashboard](#schema-dashboard) |
| 401 | Missing, invalid or revoked token (empty body). |  |

### GET `/api/v1/ui/account`

The caller's account

With `SERVICE_AUTH_ENABLED` off the stand-in administrator `admin` is used.

**Authentication:** `bearerAuth` or `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The account. | `application/json` [Account](#schema-account) |
| 401 | Missing, invalid or revoked token (empty body). |  |
| 404 | The user is gone (`ui_admin_error_not_found`). | `application/json` [Problem](#schema-problem) |
| 500 | Store failure. | `application/json` [Problem](#schema-problem) |

Example `404` response:

```json
{
  "error": "ui_admin_forbidden"
}
```

Example `500` response:

```json
{
  "error": "ui_admin_forbidden"
}
```

### GET `/api/v1/ui/mfa-enroll`

A fresh TOTP secret

Nothing is stored until the first code is checked.

**Authentication:** `bearerAuth` or `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Secret, URI and QR. | `application/json` [MfaEnrollment](#schema-mfaenrollment) |
| 401 | Missing, invalid or revoked token (empty body). |  |
| 500 | Secret generation failed (`ui_admin_error_internal`). | `application/json` [Problem](#schema-problem) |

Example `500` response:

```json
{
  "error": "ui_admin_forbidden"
}
```

### POST `/api/v1/ui/mfa-enroll`

Turn on MFA

Checks the code against the secret, stores the secret encrypted, enables MFA and mails a security notice to the confirmed address.

**Authentication:** `bearerAuth` or `sessionCookie`.

**Request body** (`application/json`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `secret` | string | yes |  |
| `code` | string | yes | The current code from the authenticator. |

```json
{
  "secret": "JBSWY3DPEHPK3PXP",
  "code": "123456"
}
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | MFA is on. | `application/json` [Done](#schema-done) |
| 400 | Wrong code (`ui_admin_error_mfa_code_invalid`). | `application/json` [Problem](#schema-problem) |
| 401 | Missing, invalid or revoked token (empty body). |  |
| 500 | Not saved. | `application/json` [Problem](#schema-problem) |

Example `200` response:

```json
{
  "done": true
}
```

Example `400` response:

```json
{
  "error": "ui_admin_forbidden"
}
```

Example `500` response:

```json
{
  "error": "ui_admin_forbidden"
}
```

### GET `/api/v1/ui/admin/users`

User list for the administration screen

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `gatehouse:read-users`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The list. | `application/json` [AdminUsers](#schema-adminusers) |
| 401 | Missing, invalid or revoked token (empty body). |  |
| 403 | Lacks `read-users` (`ui_admin_forbidden`). | `application/json` [Problem](#schema-problem) |

Example `403` response:

```json
{
  "error": "ui_admin_forbidden"
}
```

### GET `/api/v1/ui/admin/users/{username}`

One user with the catalog

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `gatehouse:read-users`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `username` | path | string | yes | The account name. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The user, the catalog and what the viewer may do. | `application/json` [AdminUser](#schema-adminuser) |
| 401 | Missing, invalid or revoked token (empty body). |  |
| 403 | Lacks `read-users`. | `application/json` [Problem](#schema-problem) |
| 404 | No such user (`ui_admin_error_not_found`). | `application/json` [Problem](#schema-problem) |

Example `403` response:

```json
{
  "error": "ui_admin_forbidden"
}
```

Example `404` response:

```json
{
  "error": "ui_admin_forbidden"
}
```

### GET `/api/v1/ui/confirm-email`

What an email-change link would set

The link's token is the credential: no session. Peeks without spending it.

**Authentication:** none (public).

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `token` | query | string | yes | From the link. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The new address. | `application/json` [ConfirmEmail](#schema-confirmemail) |
| 404 | Unknown or used token. | `application/json` [Problem](#schema-problem) |

Example `404` response:

```json
{
  "error": "ui_not_found"
}
```

### GET `/api/v1/ui/unsubscribe`

What an unsubscribe link ends

No session; the token is the credential.

**Authentication:** none (public).

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `token` | query | string | no | From the link. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The kind of notification. | `application/json` [UnsubscribeInfo](#schema-unsubscribeinfo) |
| 404 | Unknown token. | `application/json` [Problem](#schema-problem) |

Example `404` response:

```json
{
  "error": "ui_not_found"
}
```

## ui

Single sign-on and account pages (`/ui/...`): HTML shells, form posts that answer with redirects, and the client's static files.

### GET `/ui`

Entry point

Redirects to `/ui/home` when signed in, otherwise to the login page.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | To `/ui/home` or `/ui/login`. Headers: `Location`. |  |

### GET `/ui/`

Entry point (trailing slash)

Same as `/ui`.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | To `/ui/home` or `/ui/login`. Headers: `Location`. |  |

### GET `/ui/login`

Sign-in page

If a refresh cookie is still good the session is renewed and the browser is sent on (302, new cookies) to `?redirect=` or `/ui/home`; otherwise the client page.

**Authentication:** none (public).

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `redirect` | query | string | no | Where to go after signing in: a rooted same-origin path, or a URL under a prefix of `AUTH_REDIRECT_HOSTS`. Invalid values are ignored. |
| `err` | query | string | no | A notice code shown on the page: `1` (wrong credentials) or a translation key from the fixed list (`ui_login_verify_invalid`, `ui_login_reset_invalid`, `ui_login_account_disabled`, `ui_login_account_locked`, `ui_login_email_unverified`, `ui_login_rate_limited`, `ui_login_invite_invalid`, `ui_login_confirm_email_invalid`). |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page (HTML shell); the screen is drawn in the browser. | `text/html` string |
| 302 | Silent refresh. Headers: `Location`. |  |

### GET `/ui/login/`

Sign-in page (trailing slash)

If a refresh cookie is still good the session is renewed and the browser is sent on (302, new cookies) to `?redirect=` or `/ui/home`; otherwise the client page.

**Authentication:** none (public).

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `redirect` | query | string | no | Where to go after signing in: a rooted same-origin path, or a URL under a prefix of `AUTH_REDIRECT_HOSTS`. Invalid values are ignored. |
| `err` | query | string | no | A notice code shown on the page: `1` (wrong credentials) or a translation key from the fixed list (`ui_login_verify_invalid`, `ui_login_reset_invalid`, `ui_login_account_disabled`, `ui_login_account_locked`, `ui_login_email_unverified`, `ui_login_rate_limited`, `ui_login_invite_invalid`, `ui_login_confirm_email_invalid`). |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page (HTML shell); the screen is drawn in the browser. | `text/html` string |
| 302 | Silent refresh. Headers: `Location`. |  |

### POST `/ui/login`

Sign in

Answers a redirect in every case. Success: cookies set and `302` to the validated `redirect` or `/ui/home`; if the user's saved `preferred_locale` is supported and the browser holds no explicit language choice, a `qlocale` cookie (1 year) is also set. MFA: `302` to `/ui/login/mfa?pending=...`. Refusals: `302` to `/ui/login?err=...` (`1` wrong credentials or unknown user, `ui_login_account_disabled`, `ui_login_account_locked`, `ui_login_email_unverified`).

**Authentication:** none (public).

**Request body** (`application/x-www-form-urlencoded`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `username` | string | yes |  |
| `password` | string | yes |  |
| `redirect` | string | no | Optional; validated. |

```text
username=alice&password=...&redirect=%2Fsage%2Fui%2Fhome
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | See description. Headers: `Location`. |  |

### GET `/ui/login/mfa`

Second-factor page

Needs the `pending` query field (a framework `400` without it).

**Authentication:** none (public).

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `pending` | query | string | yes | The signed pending token from `POST /ui/login`. |
| `redirect` | query | string | no | Carried through. |
| `err` | query | string | no | `1` after a wrong code. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page (HTML shell); the screen is drawn in the browser. | `text/html` string |
| 400 | `pending` missing (text body). |  |

### POST `/ui/login/mfa`

Check the second factor

`pending` is `username:expires:HMAC` (valid 120 s). A wrong code counts toward the lockout and sends the browser back to `/ui/login/mfa?pending=...&err=1`.

**Authentication:** none (public).

**Request body** (`application/x-www-form-urlencoded`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `pending` | string | yes |  |
| `code` | string | yes |  |
| `redirect` | string | no |  |

```text
pending=alice%3A1767000000%3Aab12...&code=123456
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | Success as `POST /ui/login`; refusals to `/ui/login?err=ui_login_account_disabled\|ui_login_account_locked`; a wrong code or a bad `pending` back to the code page. Headers: `Location`. |  |

### GET `/ui/status`

Session status for the page watcher

Always 200. Reads the session cookie and its signature only (not the session store). `roles` is the token scope split on commas (`quench-auth` `auth_status`); the scope is space-separated, so for a non-wildcard user it comes back as one element.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Whether the cookie is worth anything. | `application/json` [AuthStatus](#schema-authstatus) |

Example `200` response:

```json
{
  "authenticated": true,
  "username": "alice",
  "roles": [
    "user sage:read"
  ]
}
```

### POST `/ui/refresh`

Renew the session from the refresh cookie

Rotates the refresh cookie in-process and sets both cookies.

**Authentication:** `refreshCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Renewed. Headers: `Set-Cookie`. | `application/json` [Authenticated](#schema-authenticated) |
| 401 | No refresh cookie, or it is no longer good. |  |

Example `200` response:

```json
{
  "authenticated": true
}
```

### GET `/ui/logout`

Sign out

Revokes the session behind the refresh cookie (realm-wide), clears both cookies and redirects to the validated `?redirect=` or `/ui/login`.

**Authentication:** none (public).

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `redirect` | query | string | no | Validated like the login redirect. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | To the redirect or the login page; cookies cleared. Headers: `Location`. |  |

### GET `/ui/home`

Services dashboard page

Needs a session; without one: 302 to the login page. The client draws the cards from `GET /api/v1/ui/dashboard`.

**Authentication:** `bearerAuth` or `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page (HTML shell); the screen is drawn in the browser. | `text/html` string |
| 302 | No session: to the login page. Headers: `Location`. |  |

### GET `/ui/home/`

Services dashboard page (trailing slash)

Needs a session; without one: 302 to the login page. The client draws the cards from `GET /api/v1/ui/dashboard`.

**Authentication:** `bearerAuth` or `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page (HTML shell); the screen is drawn in the browser. | `text/html` string |
| 302 | No session: to the login page. Headers: `Location`. |  |

### GET `/ui/register`

Registration page

Public.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page (HTML shell); the screen is drawn in the browser. | `text/html` string |

### GET `/ui/register/`

Registration page (trailing slash)

Public.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page (HTML shell); the screen is drawn in the browser. | `text/html` string |

### POST `/ui/register`

Register

Invalid address, rate limit (20/hour per client, 3/day per address), or a realm error come back as `302 /ui/register?err=<key>`. Success: the account is created as role `user` with the catalog's `registration.default_template` grants, a verification link (24 h) is mailed, and the browser goes to `/ui/login?registered=1`.

**Authentication:** none (public).

**Request body** (`application/x-www-form-urlencoded`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `username` | string | yes |  |
| `password` | string | yes |  |
| `email` | string | yes |  |

```text
username=carol&password=...&email=carol%40example.org
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | `/ui/login?registered=1` or `/ui/register?err=...` Headers: `Location`. |  |

### GET `/ui/verify`

Confirm an address from the verification link

Spends the token and marks the address confirmed.

**Authentication:** none (public).

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `token` | query | string | yes | From the mail. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | `/ui/login?verified=1` or `/ui/login?err=ui_login_verify_invalid` Headers: `Location`. |  |
| 400 | `token` missing (text body). |  |

### GET `/ui/forgot-password`

Forgot-password page

Public.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page (HTML shell); the screen is drawn in the browser. | `text/html` string |

### GET `/ui/forgot-password/`

Forgot-password page (trailing slash)

Public.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page (HTML shell); the screen is drawn in the browser. | `text/html` string |

### POST `/ui/forgot-password`

Request a password reset

Rate limited (20/hour per client, 3/hour per username; over the limit: `/ui/login?err=ui_login_rate_limited`). Otherwise always `302 /ui/login?reset_requested=1`, whether or not the account exists; if it has an address, a link (1 hour) is mailed.

**Authentication:** none (public).

**Request body** (`application/x-www-form-urlencoded`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `username` | string | yes |  |

```text
username=alice
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | `/ui/login?reset_requested=1` or `?err=ui_login_rate_limited` Headers: `Location`. |  |

### GET `/ui/reset-password`

Reset-password page

Needs `token` in the query.

**Authentication:** none (public).

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `token` | query | string | yes | From the mail. |
| `err` | query | string | no | A notice key. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page (HTML shell); the screen is drawn in the browser. | `text/html` string |
| 400 | `token` missing. |  |

### POST `/ui/reset-password`

Set a new password

Spends the token, sets the password, ends every session of the user, confirms an unconfirmed address, mails a password-changed notice. An empty password returns to the form with `err=ui_reset_error_password_empty`.

**Authentication:** none (public).

**Request body** (`application/x-www-form-urlencoded`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `token` | string | yes |  |
| `password` | string | yes |  |

```text
token=...&password=...
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | `/ui/login?reset=1`, `/ui/login?err=ui_login_reset_invalid` or back to the form. Headers: `Location`. |  |

### GET `/ui/resend-verification`

Resend-verification page

Public.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page (HTML shell); the screen is drawn in the browser. | `text/html` string |

### GET `/ui/resend-verification/`

Resend-verification page (trailing slash)

Public.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page (HTML shell); the screen is drawn in the browser. | `text/html` string |

### POST `/ui/resend-verification`

Resend the verification link

Limits: 20/hour per client, 3/hour per username, 1/minute per username. Always `302 /ui/login?resend_requested=1` unless limited; a link is mailed only for an enabled account whose address is unconfirmed and who needs verification.

**Authentication:** none (public).

**Request body** (`application/x-www-form-urlencoded`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `username` | string | yes |  |

```text
username=carol
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | `/ui/login?resend_requested=1` or `?err=ui_login_rate_limited` Headers: `Location`. |  |

### GET `/ui/accept-invite`

Accept-invitation page

Needs `token` in the query.

**Authentication:** none (public).

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `token` | query | string | yes | From the invitation mail. |
| `err` | query | string | no | A notice key. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page (HTML shell); the screen is drawn in the browser. | `text/html` string |
| 400 | `token` missing. |  |

### POST `/ui/accept-invite`

Choose a password for an invited account

Blank password: back to the form with `err=ui_reset_error_password_empty`. Spends the token, sets the password (ends sessions), confirms the address.

**Authentication:** none (public).

**Request body** (`application/x-www-form-urlencoded`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `token` | string | yes |  |
| `password` | string | yes |  |

```text
token=...&password=...
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | `/ui/login?invited=1`, `/ui/login?err=ui_login_invite_invalid` or back to the form. Headers: `Location`. |  |

### GET `/ui/confirm-email`

Confirm-new-address page

Shows the change only; with an unknown token redirects to `/ui/login?err=ui_login_confirm_email_invalid`.

**Authentication:** none (public).

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `token` | query | string | yes | From the mail. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page (HTML shell); the screen is drawn in the browser. | `text/html` string |
| 302 | Invalid token. Headers: `Location`. |  |
| 400 | `token` missing. |  |

### POST `/ui/confirm-email`

Confirm the new address

Spends the token and sets the address as confirmed; the previous address, if it was confirmed, is told.

**Authentication:** none (public).

**Request body** (`application/x-www-form-urlencoded`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `token` | string | yes |  |

```text
token=...
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | `/ui/login?email_changed=1` or `?err=ui_login_confirm_email_invalid` Headers: `Location`. |  |

### GET `/ui/unsubscribe`

Unsubscribe page

Public; opening only asks.

**Authentication:** none (public).

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `token` | query | string | no | From the notification mail. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page (HTML shell); the screen is drawn in the browser. | `text/html` string |

### POST `/ui/unsubscribe`

End a subscription

The token is read from `?token=` or the form field `token`. Also the target of a mail provider's one-click POST (RFC 8058). Idempotent: the token is not spent.

**Authentication:** none (public).

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `token` | query | string | no | From the mail (alternatively the form field). |

**Request body** (`application/x-www-form-urlencoded`, optional)

| Field | Type | Required | Description |
|---|---|---|---|
| `token` | string | no |  |

```text
List-Unsubscribe=One-Click
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | Done. | `application/json` [Done](#schema-done) |
| 404 | Unknown token. | `application/json` [Problem](#schema-problem) |
| 500 | Not saved. | `application/json` [Problem](#schema-problem) |

Example `200` response:

```json
{
  "done": true
}
```

Example `404` response:

```json
{
  "error": "ui_unsubscribe_invalid"
}
```

Example `500` response:

```json
{
  "error": "ui_admin_error_internal"
}
```

### GET `/ui/account`

Account page

The client draws it from `GET /api/v1/ui/account`; `?ok=` / `?err=` show the outcome of a form post.

**Authentication:** `bearerAuth` or `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page (HTML shell); the screen is drawn in the browser. | `text/html` string |
| 302 | No session: to the login page. Headers: `Location`. |  |

### POST `/ui/account`

Save the profile

Multipart. Validates the zone (IANA) and locale (one of `en-US pl-PL es-ES de-DE fr-FR`), resolves the picture (`avatar_remove`, else a file, else an http(s) `avatar_url`), updates the record. Redirects to `/ui/account?ok=saved` or `?err=<key>` (`ui_account_error_avatar_size`, `ui_account_error_avatar_type`, `ui_account_error_invalid`, a realm error key).

**Authentication:** `bearerAuth` or `sessionCookie`.

**Request body** (`multipart/form-data`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `avatar_file` | string (binary) | no | PNG, JPEG, GIF or WebP, at most 262 144 bytes (type sniffed from the bytes). |
| `avatar_url` | string | no | http(s) address; ignored when a file is uploaded. |
| `avatar_remove` | string | no | Present (any value) clears the picture; wins over the others. |
| `display_name` | string | no |  |
| `title` | string | no |  |
| `timezone` | string | no | IANA zone name. |
| `preferred_locale` | string (`en-US`, `pl-PL`, `es-ES`, `de-DE`, `fr-FR`) | no |  |

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | `/ui/account?ok=saved` or `?err=...`; no session: login page. Headers: `Location`. |  |

### POST `/ui/account/notifications`

Choose notification kinds

One checkbox per kind named `sub_<template id>` (present = wanted). Only departures from a kind's default are stored.

**Authentication:** `bearerAuth` or `sessionCookie`.

**Request body** (`application/x-www-form-urlencoded`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `sub_conveyor.run.failed` | string | no | Checkbox: present means wanted. |

```text
sub_conveyor.run.failed=on
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | `/ui/account?ok=notifications_saved` or `?err=ui_admin_error_internal`. Headers: `Location`. |  |

### GET `/ui/account/avatar`

The caller's picture

Redirects (302) to an http(s) picture address; serves an uploaded picture (sniffed `image/png|jpeg|gif|webp`, `Cache-Control: private, no-cache`, `X-Content-Type-Options: nosniff`); 404 when none or no session.

**Authentication:** `bearerAuth` or `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The image. | `image/png` string (binary) |
| 302 | External picture address. Headers: `Location`. |  |
| 404 | No picture, or no session. |  |

### POST `/ui/account/password`

Change my password

Needs `current_password`; a wrong one counts toward the lockout. Mismatch of `new_password` and `confirm_password`: `err=ui_account_error_password_mismatch`. Does not end sessions (a change of one's own password alone does not revoke); mails a password-changed notice to the confirmed address.

**Authentication:** `bearerAuth` or `sessionCookie`.

**Request body** (`application/x-www-form-urlencoded`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `current_password` | string | yes |  |
| `new_password` | string | yes |  |
| `confirm_password` | string | yes |  |

```text
current_password=a&new_password=b&confirm_password=b
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | `/ui/account?ok=password_changed` or `?err=<key>` Headers: `Location`. |  |

### POST `/ui/account/email`

Ask to change my address

Limits: 3/hour per account, 20/hour per client, 3/day per target address. Needs the current password. Mails a confirmation link (24 h) to the new address; nothing changes until it is confirmed.

**Authentication:** `bearerAuth` or `sessionCookie`.

**Request body** (`application/x-www-form-urlencoded`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `email` | string | yes |  |
| `email_current_password` | string | yes |  |

```text
email=new%40example.org&email_current_password=...
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | `/ui/account?ok=email_change_sent` or `?err=` (`ui_register_error_email_invalid`, `ui_account_error_rate_limited`, `ui_account_error_email_same`, `ui_account_error_email_not_sent`, a password error) Headers: `Location`. |  |

### GET `/ui/account/mfa/enroll`

MFA enrolment page

Data comes from `GET`/`POST /api/v1/ui/mfa-enroll`.

**Authentication:** `bearerAuth` or `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page (HTML shell); the screen is drawn in the browser. | `text/html` string |
| 302 | No session: to the login page. Headers: `Location`. |  |

### POST `/ui/account/mfa/disable`

Turn my MFA off

Mails a notice if it was on.

**Authentication:** `bearerAuth` or `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | `/ui/account?ok=mfa_disabled` or `?err=<key>` Headers: `Location`. |  |

### GET `/ui/admin/users`

User administration page

Any signed-in person gets the shell; the data call enforces `gatehouse:read-users`.

**Authentication:** `bearerAuth` or `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page (HTML shell); the screen is drawn in the browser. | `text/html` string |
| 302 | No session: to the login page. Headers: `Location`. |  |

### GET `/ui/admin/users/`

User administration page (trailing slash)

Any signed-in person gets the shell; the data call enforces `gatehouse:read-users`.

**Authentication:** `bearerAuth` or `sessionCookie`.

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page (HTML shell); the screen is drawn in the browser. | `text/html` string |
| 302 | No session: to the login page. Headers: `Location`. |  |

### GET `/ui/admin/users/{username}`

User editor page

Same as the list.

**Authentication:** `bearerAuth` or `sessionCookie`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `username` | path | string | yes | The account name. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The client page (HTML shell); the screen is drawn in the browser. | `text/html` string |
| 302 | No session: to the login page. Headers: `Location`. |  |

### POST `/ui/admin/users`

Create a user

Roles: an admin may pick `role` (`admin`, `user`, `service`; anything else is `user`), others always create `user`. With `email`, the account is invited (7-day link) and the redirect carries `?ok=invited` or `?ok=invite_failed`; otherwise `password` is required and the redirect is `?ok=created`. Success redirects to `/ui/admin/users/<name>`; errors to `/ui/admin/users?err=<key>`.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `gatehouse:create-user`.

**Request body** (`application/x-www-form-urlencoded`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `username` | string | yes |  |
| `password` | string | no | Needed without `email`. |
| `email` | string | no | Invites instead of setting a password. |
| `role` | string (`admin`, `user`, `service`) | no |  |

```text
username=bob&password=...&role=user
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | Back to the user page or list with `?ok=...` or `?err=<key>`. Headers: `Location`. |  |
| 403 | Signed in without the required action (plain 403 page). |  |

### POST `/ui/admin/users/{username}/invite`

Resend the invitation

Only for an account with an unconfirmed address (`InviteNeedsEmail` / `AlreadyConfirmed` otherwise).

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `gatehouse:edit-user`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `username` | path | string | yes | The account name. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | Back to the user page or list with `?ok=...` or `?err=<key>`. Headers: `Location`. |  |
| 403 | Signed in without the required action (plain 403 page). |  |

### POST `/ui/admin/users/{username}`

Save password, role and grants

Form fields: `password` (non-empty changes it), `role` (honoured only for a literal admin) and one checkbox `perm_<service>_<action>` per catalog action; all catalog-listed plain actions are replaced by the checked ones, while grants not representable as a checkbox (resource-scoped ones) are kept.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `gatehouse:edit-user`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `username` | path | string | yes | The account name. |

**Request body** (`application/x-www-form-urlencoded`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `password` | string | no |  |
| `role` | string | no |  |
| `perm_sage_read` | string | no | Checkbox. |

```text
role=user&perm_sage_read=on&perm_sage_write=on
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | Back to the user page or list with `?ok=...` or `?err=<key>`. Headers: `Location`. |  |
| 403 | Signed in without the required action (plain 403 page). |  |

### POST `/ui/admin/users/{username}/template`

Apply a grant template

Replaces the grants with the template's.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `gatehouse:manage-permissions`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `username` | path | string | yes | The account name. |

**Request body** (`application/x-www-form-urlencoded`, required)

| Field | Type | Required | Description |
|---|---|---|---|
| `template` | string | yes |  |

```text
template=editor
```

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | Back to the user page or list with `?ok=...` or `?err=<key>`. Headers: `Location`. |  |
| 403 | Signed in without the required action (plain 403 page). |  |

### POST `/ui/admin/users/{username}/delete`

Delete a user

Redirects to `/ui/admin/users?ok=deleted`.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `gatehouse:delete-user`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `username` | path | string | yes | The account name. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | Back to the user page or list with `?ok=...` or `?err=<key>`. Headers: `Location`. |  |
| 403 | Signed in without the required action (plain 403 page). |  |

### POST `/ui/admin/users/{username}/disable`

Disable an account

Blocks future logins without ending current sessions; not for yourself.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `gatehouse:edit-user`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `username` | path | string | yes | The account name. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | Back to the user page or list with `?ok=...` or `?err=<key>`. Headers: `Location`. |  |
| 403 | Signed in without the required action (plain 403 page). |  |

### POST `/ui/admin/users/{username}/enable`

Enable an account

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `gatehouse:edit-user`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `username` | path | string | yes | The account name. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | Back to the user page or list with `?ok=...` or `?err=<key>`. Headers: `Location`. |  |
| 403 | Signed in without the required action (plain 403 page). |  |

### POST `/ui/admin/users/{username}/unlock`

Clear a lockout

Resets the failed-attempt counter and `locked_until`.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `gatehouse:edit-user`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `username` | path | string | yes | The account name. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | Back to the user page or list with `?ok=...` or `?err=<key>`. Headers: `Location`. |  |
| 403 | Signed in without the required action (plain 403 page). |  |

### POST `/ui/admin/users/{username}/mfa/disable`

Turn a user's MFA off (recovery)

The administrator never sees the secret. Mails a notice if it was on.

**Authentication:** `bearerAuth` or `sessionCookie`. **Permission:** `gatehouse:edit-user`.

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `username` | path | string | yes | The account name. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 302 | Back to the user page or list with `?ok=...` or `?err=<key>`. Headers: `Location`. |  |
| 403 | Signed in without the required action (plain 403 page). |  |

### GET `/ui/assets/quench.js`

The wasm loader script (`quench-ui-web`)

Anonymous: the files hold no data of their own.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The wasm loader script (`quench-ui-web`) | `text/javascript` string (binary) |
| 404 | Not available. |  |

### GET `/ui/assets/app.css`

The stylesheet: `forge-shell`'s then the client's

Anonymous: the files hold no data of their own.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The stylesheet: `forge-shell`'s then the client's | `text/css` string (binary) |
| 404 | Not available. |  |

### GET `/ui/assets/shell.js`

forge-shell's script (theme, language, links)

Anonymous: the files hold no data of their own.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | forge-shell's script (theme, language, links) | `text/javascript` string (binary) |
| 404 | Not available. |  |

### GET `/ui/assets/favicon.png`

The service icon

Anonymous: the files hold no data of their own.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The service icon | `image/png` string (binary) |
| 404 | Not available. |  |

### GET `/ui/assets/icons/{name}`

A service icon for the dashboard card

Anonymous: the files hold no data of their own.

**Authentication:** none (public).

| Parameter | In | Type | Required | Description |
|---|---|---|---|---|
| `name` | path | string | yes | `conveyor`, `daybook`, `gantry`, `id-card`, `gauge`, `hoist`, `sage`, `switchboard`, `warehouse` or `workbench`, with or without `.png`; other names give 404. |

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | A service icon for the dashboard card | `image/png` string (binary) |
| 404 | Not available. |  |

### GET `/ui/assets/app.wasm`

The browser client module (404 when not installed)

Anonymous: the files hold no data of their own.

**Authentication:** none (public).

**Responses**

| Status | Description | Body |
|---|---|---|
| 200 | The browser client module (404 when not installed) | `application/wasm` string (binary) |
| 404 | Not available. |  |

## Schemas

### Schema Permissions

<a id="schema-permissions"></a>

Grants as `{service: [action, ...]}`. Actions are plain catalog names (`read`) or resource-scoped `<resource_type>:<resource_id>:<action>` (`project:7:write`). Sets: duplicates collapse, order is sorted.

Type: map of string to array of string

### Schema Role

<a id="schema-role"></a>

`admin` and `service` are wildcard roles (every permission on every service). Only `admin` may assign them.

Values: `admin`, `user`, `service`

### Schema Problem

<a id="schema-problem"></a>

The `{"error": "..."}` body of most refusals.

| Field | Type | Required | Description |
|---|---|---|---|
| `error` | string | yes | Reason in English (the user API) or a translation key (the page API and the form posts). |

### Schema LoginRequest

<a id="schema-loginrequest"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `username` | string | yes |  |
| `password` | string | yes |  |

### Schema RefreshRequest

<a id="schema-refreshrequest"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `refresh_token` | string | yes |  |

### Schema TokenResponse

<a id="schema-tokenresponse"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `access_token` | string | yes | EdDSA-signed JWT. |
| `refresh_token` | string | yes | Opaque, single use (rotated on refresh). Empty string for the `client_credentials` and `app_token` grants. |
| `token_type` | string | yes | Always `Bearer`. |
| `expires_in` | integer | yes | Access token lifetime in seconds (`ACCESS_TOKEN_TTL_SECS`, default 900). |

### Schema LoginError

<a id="schema-loginerror"></a>

Body of a refused `POST /api/v1/auth/login` (always status 401).

| Field | Type | Required | Description |
|---|---|---|---|
| `error` | string (`mfa_required`, `account_disabled`, `account_locked`, `email_unverified`, `invalid_credentials`) | yes |  |

### Schema UserInfo

<a id="schema-userinfo"></a>

What a token says. `permissions` is empty for a wildcard role.

| Field | Type | Required | Description |
|---|---|---|---|
| `sub` | string | yes | Username (or client id). |
| `roles` | array of string | yes | Scope entries without a colon (the roles). |
| `audiences` | array of string | yes | The token's `aud`. |
| `permissions` | [Permissions](#schema-permissions) | yes | Grants as `{service: [action, ...]}`. Actions are plain catalog names (`read`) or resource-scoped `<resource_type>:<resource_id>:<action>` (`project:7:write`). Sets: duplicates collapse, order is sorted. |

### Schema UserView

<a id="schema-userview"></a>

A user as the API shows them. The password hash and MFA secret never leave.

| Field | Type | Required | Description |
|---|---|---|---|
| `username` | string | yes |  |
| `roles` | array of [Role](#schema-role) | yes |  |
| `permissions` | [Permissions](#schema-permissions) | yes | Grants as `{service: [action, ...]}`. Actions are plain catalog names (`read`) or resource-scoped `<resource_type>:<resource_id>:<action>` (`project:7:write`). Sets: duplicates collapse, order is sorted. |
| `wildcard` | boolean | yes | True when a role grants everything. |
| `email` | string or null | yes |  |
| `email_verified` | boolean | yes |  |
| `invite_sent` | boolean | no | Only on the response to creating an invited user: whether the invitation was accepted for delivery. The account exists either way. |

### Schema CreateUserRequest

<a id="schema-createuserrequest"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `username` | string | yes | 1 to 64 characters of ASCII letters, digits and `. _ - @ +`; trimmed. |
| `password` | string | no | Required unless `invite` is true. Default `""`. |
| `roles` | array of [Role](#schema-role) | no | Empty means `["user"]`. `admin`/`service` need the literal admin role. Default `[]`. |
| `permissions` | [Permissions](#schema-permissions) | no | Grants as `{service: [action, ...]}`. Actions are plain catalog names (`read`) or resource-scoped `<resource_type>:<resource_id>:<action>` (`project:7:write`). Sets: duplicates collapse, order is sorted. |
| `email` | string or null | no | Required when `invite` is true. |
| `invite` | boolean | no | Email the address an invitation to choose a password; the account gets a random password nobody is told. Default `false`. |

### Schema UpdateUserRequest

<a id="schema-updateuserrequest"></a>

Every field optional; absent leaves it alone.

| Field | Type | Required | Description |
|---|---|---|---|
| `password` | string | no |  |
| `roles` | array of [Role](#schema-role) | no |  |
| `permissions` | [Permissions](#schema-permissions) | no | Grants as `{service: [action, ...]}`. Actions are plain catalog names (`read`) or resource-scoped `<resource_type>:<resource_id>:<action>` (`project:7:write`). Sets: duplicates collapse, order is sorted. |

### Schema ReplacePermissionsRequest

<a id="schema-replacepermissionsrequest"></a>

`permissions` defaults to `{}`, which removes every grant.

| Field | Type | Required | Description |
|---|---|---|---|
| `permissions` | [Permissions](#schema-permissions) | no | Grants as `{service: [action, ...]}`. Actions are plain catalog names (`read`) or resource-scoped `<resource_type>:<resource_id>:<action>` (`project:7:write`). Sets: duplicates collapse, order is sorted. |

### Schema ApplyTemplateRequest

<a id="schema-applytemplaterequest"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `template` | string | yes | A template name from `[templates.*]` of the permission catalog. |

### Schema Me

<a id="schema-me"></a>

The caller's current access, wildcard resolved (every catalog action for a wildcard role).

| Field | Type | Required | Description |
|---|---|---|---|
| `username` | string | yes |  |
| `roles` | array of [Role](#schema-role) | yes |  |
| `wildcard` | boolean | yes |  |
| `effective` | [Permissions](#schema-permissions) | yes | Grants as `{service: [action, ...]}`. Actions are plain catalog names (`read`) or resource-scoped `<resource_type>:<resource_id>:<action>` (`project:7:write`). Sets: duplicates collapse, order is sorted. |

### Schema CreateAppToken

<a id="schema-createapptoken"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes | Non-blank after trimming. |
| `scope` | array of string | no | `service:action` entries the token is limited to; each must be held by the owner. Empty is the owner's full access. Default `[]`. |
| `expires_in_days` | integer or null | no | At least 1; capped at 365. Omitted means 365. |

### Schema AppTokenView

<a id="schema-apptokenview"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes | Public handle: 12 characters. |
| `name` | string | yes |  |
| `scope` | array of string | yes |  |
| `created_at` | string (date-time) | yes |  |
| `expires_at` | string or null (date-time) | yes |  |
| `last_used_at` | string or null (date-time) | yes |  |
| `revoked` | boolean | yes |  |

### Schema CreatedAppToken

<a id="schema-createdapptoken"></a>

An `AppTokenView` plus the one-time `secret`.

Type: any

### Schema TokenRequest

<a id="schema-tokenrequest"></a>

Form fields; which are required depends on `grant_type`.

| Field | Type | Required | Description |
|---|---|---|---|
| `grant_type` | string (`authorization_code`, `refresh_token`, `client_credentials`, `app_token`) | yes |  |
| `code` | string | no | authorization_code: the code from `/api/v1/authorize`. |
| `redirect_uri` | string | no | authorization_code: must equal the one the code was issued for. |
| `code_verifier` | string | no | authorization_code: the PKCE verifier (S256). |
| `client_id` | string | no | authorization_code, client_credentials. |
| `client_secret` | string | no | authorization_code, client_credentials. |
| `refresh_token` | string | no | refresh_token grant. |
| `app_token` | string | no | app_token grant: the `lht_...` secret. |

### Schema Jwks

<a id="schema-jwks"></a>

RFC 7517 JWK Set of every non-retired key.

| Field | Type | Required | Description |
|---|---|---|---|
| `keys` | array of object | yes |  |

### Schema NotifyBody

<a id="schema-notifybody"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `username` | string | yes |  |
| `template` | string | yes | A template id: `conveyor.run.failed`, `conveyor.run.succeeded`, `conveyor.run.recovered`. |
| `vars` | map of string to string | no | Exactly the variables the template declares (`project`, `run`, `ref`, `url` for the three conveyor templates). Default `{}`. |
| `dedupe_key` | string or null | no | Repeating a key for the same person and template within a day sends nothing. |
| `requested` | boolean | no | The person subscribed in the calling service: sends a kind that is off by default unless they opted out of it here. Default `false`. |

### Schema NotifyResponse

<a id="schema-notifyresponse"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `status` | string (`accepted`, `skipped`, `failed`) | yes |  |
| `reason` | string | no | skipped: `no_such_user`, `no_verified_email`, `not_subscribed`, `duplicate`, `rate_limited`. failed: the mail error text. |
| `retryable` | boolean | no | failed only. |

### Schema TestTokenRequest

<a id="schema-testtokenrequest"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `sub` | string | yes |  |
| `aud` | array of string | no |  Default `[]`. |
| `scope` | string | yes | Space separated roles and `service:action` entries. |
| `iat` | integer or null | no | Unix seconds; default now. |
| `exp` | integer or null | no | Unix seconds; default now + access TTL. |

### Schema AuthStatus

<a id="schema-authstatus"></a>

What the page's session watcher is told. Reads the cookie and its signature only.

| Field | Type | Required | Description |
|---|---|---|---|
| `authenticated` | boolean | yes |  |
| `username` | string or null | yes |  |
| `roles` | array of string | yes |  |

### Schema ServiceCard

<a id="schema-servicecard"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `url` | string | yes |  |
| `title_key` | string | yes | `ui_service_<name>_title` |
| `desc_key` | string | yes | `ui_service_<name>_desc` |
| `label` | string | yes | Shown when no translation applies. |
| `description` | string or null | no |  |
| `card_class` | string | yes | `home-card-<name>` |

### Schema Dashboard

<a id="schema-dashboard"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `admin` | boolean | yes | Whether the person holds the `admin` role. |
| `services` | array of [ServiceCard](#schema-servicecard) | yes |  |

### Schema Subscription

<a id="schema-subscription"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes |  |
| `service` | string | yes |  |
| `label_key` | string | yes |  |
| `wanted` | boolean | yes |  |

### Schema Account

<a id="schema-account"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `username` | string | yes |  |
| `display_name` | string or null | no |  |
| `avatar_url` | string or null | no | A pasted address; never the bytes of an uploaded picture. |
| `has_avatar` | boolean | yes |  |
| `title` | string or null | no |  |
| `timezone` | string or null | no |  |
| `preferred_locale` | string or null | no |  |
| `email` | string or null | no |  |
| `email_verified` | boolean | yes |  |
| `mfa_enabled` | boolean | yes |  |
| `has_notice_address` | boolean | yes | Whether a confirmed address exists for notices. |
| `timezones` | array of string | yes | Every IANA zone name known. |
| `locales` | array of string | yes |  |
| `subscriptions` | array of [Subscription](#schema-subscription) | yes |  |

### Schema MfaEnrollment

<a id="schema-mfaenrollment"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `secret` | string | yes | Base32 TOTP secret; nothing is stored yet. |
| `uri` | string | yes | `otpauth://totp/...` |
| `qr` | string or null | no | SVG `data:` URI of the QR code. |

### Schema MfaEnrollRequest

<a id="schema-mfaenrollrequest"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `secret` | string | yes |  |
| `code` | string | yes | The current code from the authenticator. |

### Schema Grant

<a id="schema-grant"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `service` | string | yes |  |
| `actions` | array of string | yes |  |

### Schema AdminUserRow

<a id="schema-adminuserrow"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `username` | string | yes |  |
| `roles` | array of string | yes |  |
| `wildcard` | boolean | yes |  |
| `grants` | array of [Grant](#schema-grant) | yes |  |

### Schema AdminUsers

<a id="schema-adminusers"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `actor` | string | yes |  |
| `admin` | boolean | yes | Whether the viewer is a literal administrator (may pick roles). |
| `can_create` | boolean | yes |  |
| `users` | array of [AdminUserRow](#schema-adminuserrow) | yes |  |

### Schema CatalogService

<a id="schema-catalogservice"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes |  |
| `label` | string | yes |  |
| `actions` | array of string | yes |  |

### Schema AdminUser

<a id="schema-adminuser"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `username` | string | yes |  |
| `role` | string | yes | The most privileged role held: `admin`, `service` or `user`. |
| `wildcard` | boolean | yes |  |
| `held` | array of [Grant](#schema-grant) | yes |  |
| `email` | string or null | no |  |
| `email_verified` | boolean | yes |  |
| `disabled` | boolean | yes |  |
| `locked` | boolean | yes |  |
| `mfa_enabled` | boolean | yes |  |
| `created_at` | string | yes | `2026-10-29 10:00 UTC` |
| `last_login_at` | string or null | no |  |
| `is_self` | boolean | yes |  |
| `viewer_admin` | boolean | yes |  |
| `can_edit` | boolean | yes |  |
| `can_delete` | boolean | yes |  |
| `can_manage_permissions` | boolean | yes |  |
| `services` | array of [CatalogService](#schema-catalogservice) | yes |  |
| `templates` | array of string | yes |  |

### Schema ConfirmEmail

<a id="schema-confirmemail"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `new_email` | string | yes |  |

### Schema UnsubscribeInfo

<a id="schema-unsubscribeinfo"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `label_key` | string | yes |  |

### Schema Done

<a id="schema-done"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `done` | boolean | yes |  |

### Schema Authenticated

<a id="schema-authenticated"></a>

| Field | Type | Required | Description |
|---|---|---|---|
| `authenticated` | boolean | yes |  |

### Schema AvatarUpload

<a id="schema-avatarupload"></a>

multipart/form-data fields. A missing or empty field leaves the value alone (blank cannot clear).

| Field | Type | Required | Description |
|---|---|---|---|
| `avatar_file` | string (binary) | no | PNG, JPEG, GIF or WebP, at most 262 144 bytes (type sniffed from the bytes). |
| `avatar_url` | string | no | http(s) address; ignored when a file is uploaded. |
| `avatar_remove` | string | no | Present (any value) clears the picture; wins over the others. |
| `display_name` | string | no |  |
| `title` | string | no |  |
| `timezone` | string | no | IANA zone name. |
| `preferred_locale` | string (`en-US`, `pl-PL`, `es-ES`, `de-DE`, `fr-FR`) | no |  |
