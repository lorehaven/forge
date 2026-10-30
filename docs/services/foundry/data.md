---
title: Data
parent: Foundry
grand_parent: Services
nav_order: 3
---

Foundry owns the ledger and, through the catalog, creates the schemas of every service. This page lists the ledger and the shared modules; each service's tables are in that service's `data.md` (sage, switchboard, workbench, gauge in this part of the book). Redis and files: none.

## Ledger (schema `foundry`)

Created by `apply` (`CREATE SCHEMA IF NOT EXISTS`, `CREATE TABLE IF NOT EXISTS`). Never dropped by `reset`.

`foundry.forge_migrations`:

| Column | Type | Notes |
|---|---|---|
| `id` | TEXT PRIMARY KEY | `module@schema:migration-id` |
| `module` | TEXT NOT NULL | |
| `target_schema` | TEXT NOT NULL | |
| `migration_id` | TEXT NOT NULL | |
| `since` | TEXT NOT NULL | |
| `module_version` | TEXT NOT NULL | version being installed when applied |
| `author` | TEXT NOT NULL | |
| `checksum` | TEXT NOT NULL | SHA-256 hex |
| `applied_at` | TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP | |

`foundry.forge_modules`:

| Column | Type | Notes |
|---|---|---|
| `module` | TEXT NOT NULL | |
| `target_schema` | TEXT NOT NULL | `*`-scoped modules store their instance schema (`public` for `pgvector`, `auth` for `auth`) |
| `version` | TEXT NOT NULL | |
| `updated_at` | TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP | |
| | PRIMARY KEY (`module`, `target_schema`) | upsert on every apply |

## Shared modules

`quench-core` 0.1.0: migration `0001-create-schema` (author `quench-db`): `createSchema { name = "${schema}" }` - creates the target schema.

`pgvector` 0.1.0 (scope `database`): `0001-install-vector` (author `quench-srv`): `CREATE EXTENSION IF NOT EXISTS vector`.

`gatehouse` 0.1.0: no migrations.

`auth` 0.2.3 (scope `database`, schema `auth`), owned by gatehouse, twelve migrations. Workbench, sage and switchboard reference `auth.users(username)`:

| Migration | since | Effect |
|---|---|---|
| 0001-realm-schema | 0.1.0 | schema; `users(username TEXT PK, password TEXT NOT NULL, roles JSONB NOT NULL)` |
| 0002-permissions | 0.1.1 | `users.permissions JSONB NOT NULL DEFAULT '{}'` - the grants, `{service: level}` |
| 0003-user-email | 0.1.2 | `users.email TEXT`, `email_verified_at TIMESTAMPTZ` |
| 0004-signing-keys | 0.2.0 | `signing_keys(kid PK, algorithm, private_key, public_key, created_at, not_after)` |
| 0005-clients | 0.2.0 | `clients(client_id PK, secret_hash, redirect_uris TEXT[], allowed_scopes TEXT[], created_at)` |
| 0006-authorization-codes | 0.2.0 | `authorization_codes(code_hash PK, client_id FK, username FK, redirect_uri, scope, pkce_challenge, created_at, expires_at, consumed_at)` |
| 0007-user-profile | 0.2.1 | `users.display_name`, `avatar_url`, `title` |
| 0008-user-lifecycle | 0.2.1 | `users.created_at` (NOT NULL DEFAULT NOW()), `last_login_at`, `disabled_at`, `password_changed_at` |
| 0009-user-security | 0.2.1 | `users.mfa_enabled BOOLEAN NOT NULL DEFAULT FALSE`, `mfa_secret`, `failed_login_attempts INTEGER NOT NULL DEFAULT 0`, `locked_until` |
| 0010-user-locale | 0.2.1 | `users.timezone`, `preferred_locale` |
| 0011-notification-prefs | 0.2.2 | `notification_prefs(key PK, username, template, enabled, updated_at)` + index on `username` |
| 0012-app-tokens | 0.2.3 | `app_tokens(token_hash PK, id UNIQUE, username FK, name, scope DEFAULT '', created_at, expires_at, last_used_at, revoked_at)` + index on `username` |

Gatehouse's own pages in the Forge book describe how these are used.
