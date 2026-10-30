---
title: Permissions
parent: Gatehouse
grand_parent: Services
nav_order: 6
---

## Roles

| Role | Wildcard | Meaning |
|---|---|---|
| `admin` | yes | every permission on every service; the only role that may assign `admin`/`service` |
| `service` | yes | machine identity; every permission on every service access, but not user administration |
| `user` | no | only what is granted |

A wildcard role has no stored grants (`permissions` is `{}`) and its tokens carry the role alone in `scope`. User administration is **not** implied by a wildcard: it needs the `gatehouse:*` actions below, which a wildcard role satisfies through `Claims::can` (a wildcard answers true for any action). Rules enforced in `realm` for both the API and the pages: the realm keeps at least one `admin`; nobody deletes or demotes themselves; nobody disables themselves; only a literal `admin` role assigns `admin` or `service`; grants must exist in the catalog.

## The permission catalog (`config/permissions.toml`)

Path: `PERMISSIONS_CONFIG` (default `config/permissions.toml`). A missing or invalid file stops the process. Validation at start: at least one `[services.*]`; every template grant must be known; `registration.default_template` must name a template.

```toml
[services.<name>]
label = "Shown name"            # optional; default the key
home = true                     # optional: a card on the dashboard
description = "English text"    # optional; card text until a translation exists
actions = ["read", "write"]     # grantable action names, no implication between them
resource_types = ["project"]    # optional: allows <type>:<id>:<action> grants

[templates.<name>]              # named grant bundles
<service> = ["action", ...]

[registration]
default_template = "viewer"     # grants of a self-registered account
```

The service keys are also the realm's **audience list**. A service absent from the catalog is in no token's audience and answers every request with 401.

### Shipped catalog (`services/gatehouse/service/config/permissions.toml`)

| Service | Label | `home` | Actions | `resource_types` |
|---|---|---|---|---|
| `sage` | Sage | yes | `read`, `write` | |
| `warehouse` | Warehouse | yes | `read`, `write` | `storage` |
| `conveyor` | Conveyor | yes | `read`, `write` | `project` |
| `switchboard` | Switchboard | yes | `read`, `launch`, `stop`, `delete-model` | |
| `workbench` | Workbench | yes | `read`, `write` | `project` |
| `daybook` | Daybook | yes | `read`, `write` | |
| `gantry` | Gantry | yes | `read`, `deploy`, `scale`, `activate`, `rollback` | `target` |
| `gauge` | Gauge | yes | `read` | |
| `hoist` | Hoist | no | `read` | |
| `gatehouse` | Gatehouse | no | `read-users`, `create-user`, `edit-user`, `delete-user`, `manage-permissions`, `manage-signing-keys`, `notify` | |

Templates: `viewer` (`read` on sage, warehouse, switchboard, conveyor, workbench, daybook, gantry, gauge, hoist), `editor` (`read`+`write` on sage, warehouse, conveyor, workbench, daybook; switchboard `read`, `launch`, `stop`; `read` on gantry, gauge, hoist), `user-manager` (gatehouse `read-users`, `create-user`, `edit-user`, `delete-user`, `manage-permissions`). Registration default: `viewer`. The cluster overlay mounts its own copy (`gatehouse-permissions-config`, which adds palantir); an overlay copy and the repository copy can differ.

### Gatehouse's own actions

| Action | Guards |
|---|---|
| `read-users` | `GET /api/v1/users[/{username}]`, `GET /api/v1/ui/admin/users[/{username}]` |
| `create-user` | `POST /api/v1/users`, `POST /ui/admin/users` |
| `edit-user` | `PATCH /api/v1/users/{username}`; `POST /ui/admin/users/{username}` and `.../invite`, `/disable`, `/enable`, `/unlock`, `/mfa/disable` |
| `delete-user` | `DELETE /api/v1/users/{username}`, `POST /ui/admin/users/{username}/delete` |
| `manage-permissions` | `PUT /api/v1/users/{username}/permissions`, `POST .../template` (API and page) |
| `manage-signing-keys` | `POST /api/v1/admin/keys/rotate` |
| `notify` | `POST /api/v1/notify` |

## Grant syntax

A grant is `service:action`. In tokens it is a `scope` entry; in storage it is `{service: [action, ...]}`. An action is either a plain catalog name or, for a service with `resource_types`, `<resource_type>:<resource_id>:<action>` where the id is not checked and the action must be in the service's `actions` (for example `conveyor:project:7:write`, `warehouse:storage:media:read`, `gantry:target:media:deploy`). How a relying party interprets scoped grants (an ancestor walk for conveyor projects, a flat match for workbench) is up to that service.

The admin form's checkboxes cover only the plain catalog actions (`perm_<service>_<action>`); resource-scoped grants made through the API are preserved when a form is saved.

## Dashboard visibility

A catalog service with `home = true` gets a card when `<NAME>_UI_URL` (or `<NAME>_URL`) is set and `FEATURE_<NAME>_ENABLED` is not `0/false/no/off` (default on). The person sees the card if they can `read` it, hold any grant on it, or hold a wildcard role. Card text keys: `ui_service_<name>_title`, `ui_service_<name>_desc`.

## OAuth clients

`CLIENTS_CONFIG` (default `config/clients.toml`) is read at every start; each entry is upserted into `auth.clients`:

```toml
[[client]]
client_id = "warehouse"
redirect_path = "/ui/auth/callback"     # optional; absent = client_credentials only
allowed_scopes = ["warehouse"]          # audience of tokens issued to this client
secret_env = "CLIENT_SECRET_WAREHOUSE"  # name of the env var holding the secret
```

A client whose `secret_env` variable is unset or blank is skipped with a warning. The secret is stored as SHA-256 hex. The redirect URI is `<base><redirect_path>` where `<base>` is `<PREFIX>_UI_URL` or `<PREFIX>_URL` (`PREFIX` = upper-cased client id with `-` as `_`), trimmed of a trailing `/` and of a trailing `/ui/home`; with neither set the client has no redirect URI and the authorization-code flow rejects it.

Shipped clients: `sage`, `switchboard`, `warehouse`, `conveyor`, `workbench`, `gantry`, `gauge` (authorization code, redirect `/ui/auth/callback`, audience their own name) and the machine identities `sage-switchboard` (audience `switchboard`), `conveyor-gatehouse` (`gatehouse`), `conveyor-publish` (`warehouse`), `gantry-warehouse` (`warehouse`), each with `secret_env` `CLIENT_SECRET_<ID upper-cased>`.
