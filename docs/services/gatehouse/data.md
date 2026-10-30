---
title: Data
parent: Gatehouse
grand_parent: Services
nav_order: 3
---

Gatehouse does not migrate its own schema. Foundry applies the catalog modules `auth` (version 0.2.3, `scope = "database"`, default schema `auth`) and `gatehouse` (version 0.1.0, `requires = ["auth@^0.2"]`, "all of its state is the auth module") from `services/foundry/service/migrations/{auth,gatehouse}`. The `${schema}` placeholder is `auth` unless `AUTH_DB_SCHEMA` differs. The `auth` realm is shared: all services authenticate against the same users, only Gatehouse writes them.

## Migrations of module `auth`, in order

| Id | `since` | Change |
|---|---|---|
| `0001-realm-schema` | 0.1.0 | creates the schema and `users` |
| `0002-permissions` | 0.1.1 | `users.permissions` |
| `0003-user-email` | 0.1.2 | `email`, `email_verified_at` |
| `0004-signing-keys` | 0.2.0 | `signing_keys` |
| `0005-clients` | 0.2.0 | `clients` |
| `0006-authorization-codes` | 0.2.0 | `authorization_codes` |
| `0007-user-profile` | 0.2.1 | `display_name`, `avatar_url`, `title` |
| `0008-user-lifecycle` | 0.2.1 | `created_at`, `last_login_at`, `disabled_at`, `password_changed_at` |
| `0009-user-security` | 0.2.1 | `mfa_enabled`, `mfa_secret`, `failed_login_attempts`, `locked_until` |
| `0010-user-locale` | 0.2.1 | `timezone`, `preferred_locale` |
| `0011-notification-prefs` | 0.2.2 | `notification_prefs` and its index |
| `0012-app-tokens` | 0.2.3 | `app_tokens` and its index |

## Tables

### `auth.users`

| Column | Type | Null | Default | Notes |
|---|---|---|---|---|
| `username` | TEXT | no | | primary key |
| `password` | TEXT | no | | Argon2 hash |
| `roles` | JSONB | no | | array of `admin`/`user`/`service` |
| `permissions` | JSONB | no | `'{}'` | `{service: [action, ...]}`; empty for wildcard roles |
| `email` | TEXT | yes | | |
| `email_verified_at` | TIMESTAMPTZ | yes | | |
| `display_name`, `avatar_url`, `title` | TEXT | yes | | `avatar_url` may be an http(s) address or a `data:` URI of an uploaded picture (at most 256 KiB) |
| `created_at` | TIMESTAMPTZ | no | `NOW()` | existing rows were backfilled with the migration's time |
| `last_login_at`, `disabled_at`, `password_changed_at` | TIMESTAMPTZ | yes | | |
| `mfa_enabled` | BOOLEAN | no | `FALSE` | |
| `mfa_secret` | TEXT | yes | | hex of ChaCha20-Poly1305 ciphertext |
| `failed_login_attempts` | INTEGER | no | `0` | |
| `locked_until` | TIMESTAMPTZ | yes | | |
| `timezone`, `preferred_locale` | TEXT | yes | | IANA zone; one of the five locales |

### `auth.signing_keys`

`kid` TEXT primary key; `algorithm` TEXT not null (`EdDSA`); `private_key` TEXT not null (hex of `nonce || ciphertext`); `public_key` TEXT not null (hex); `created_at` TIMESTAMPTZ not null default `now()`; `not_after` TIMESTAMPTZ null (null = the active signing key).

### `auth.clients`

`client_id` TEXT primary key; `secret_hash` TEXT not null (SHA-256 hex); `redirect_uris` TEXT[] not null default `'{}'`; `allowed_scopes` TEXT[] not null default `'{}'`; `created_at` TIMESTAMPTZ not null default `now()`.

### `auth.authorization_codes`

`code_hash` TEXT primary key; `client_id` TEXT not null references `clients` on delete cascade; `username` TEXT not null references `users` on delete cascade; `redirect_uri` TEXT not null; `scope` TEXT not null; `pkce_challenge` TEXT not null; `created_at` TIMESTAMPTZ not null default `now()`; `expires_at` TIMESTAMPTZ not null; `consumed_at` TIMESTAMPTZ null. Consumed rows are kept (a replay is distinguishable in logs); nothing deletes expired rows.

### `auth.notification_prefs`

`key` TEXT primary key (`username|template`); `username` TEXT not null; `template` TEXT not null; `enabled` BOOLEAN not null; `updated_at` TIMESTAMPTZ not null default `now()`. Index `notification_prefs_username` on `username`. No foreign key.

### `auth.app_tokens`

`token_hash` TEXT primary key (SHA-256 hex of the secret); `id` TEXT not null unique; `username` TEXT not null references `users` on delete cascade; `name` TEXT not null; `scope` TEXT not null default `''` (space-separated `service:action`); `created_at` TIMESTAMPTZ not null default `now()`; `expires_at`, `last_used_at`, `revoked_at` TIMESTAMPTZ null. Index `app_tokens_username` on `username`.

## Cache store

Prefixes passed to the store: sessions `forge-session`, mail tokens `forge-verify`, rate limits `forge-ratelimit` (how the prefix joins the key is `quench-cache`'s concern; *Unverified:* the separator).

| Key | Value | TTL |
|---|---|---|
| `session:{sid}` | `{username, refresh_hash}` | refresh lifetime |
| `refresh:{sha256 of token}` | sid | refresh lifetime |
| `user:{username}` | set of sids | refresh lifetime |
| `reuse:{old hash}` | `{sid, username, token}` for the rotation grace | 30 s |
| `{purpose}:{uuid}` | JSON string: the username (`verify-email`, `reset-password`, `invite`), a `{username, new_email}` ticket (`email-change`) or a `{username, template}` ticket (`unsubscribe`) | 24 h, 1 h, 7 days, 24 h, 90 days |
| `ratelimit:{scope}:{hex16 of sha256(lowercase subject)}:{window index}` | set of unique hit ids | twice the window |

Redeeming a mail token is an atomic take (`GETDEL`); `peek` reads without spending (confirm-email and unsubscribe pages). Rate-limit scopes in use: `register-ip`, `register-email`, `reset-ip`, `reset-user`, `resend-ip`, `resend-user`, `resend-cooldown`, `email-change-user`, `email-change-ip`, `email-change-address`, `notify-user`, `notify-user-template`, `notify-dedupe`, `mail-budget`.

## Files

The service writes no files besides logs. `service/dist/assets/` (CSS, `translations.js`, favicon) is left over from the earlier server-rendered UI; no route reads it (the `/ui/assets/*` routes serve embedded bytes and `web/app.wasm`).
