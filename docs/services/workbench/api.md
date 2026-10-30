---
title: API
parent: Workbench
grand_parent: Services
nav_order: 1
---

The HTTP API of Workbench 0.3.2, rendered from [openapi.yaml](openapi.yaml) (OpenAPI 3.1.0). Projects, issues, comments, labels and issue links. All of /api/v1 is behind the realm Auth middleware (bearer or session cookie, audience workbench); each route checks the grant workbench:<action> or the project-scoped workbench:project:<id>:<action> itself.

Paths below are relative to the base path (`BASE_PATH`, server variable `base`).

## projects

Projects: the flat container issues live in.

### `GET /api/v1/projects`

List projects. Operation id `listProjects`.

Authentication: bearer token or session cookie.

Permission: `workbench:read (blanket) or project:{id}:read per project`.

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | array of [`Project`](#project) | Every project ordered by key; with only project-scoped read grants, just the granted ones (no 403). |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `200`:

```json
[
  {
    "id": "0b3a4c1e-6d7f-4b8a-9e21-5c2d8f3a1b90",
    "key": "WB",
    "name": "Workbench",
    "description": null,
    "created_at": "2026-10-01T09:30:00.123Z",
    "updated_at": "2026-10-01T09:30:00.123Z"
  }
]
```

### `POST /api/v1/projects`

Create a project. Operation id `createProject`.

`key` and `name` are trimmed. Only the unscoped grant `workbench:write` works: there is no project id to scope a grant to yet.

Authentication: bearer token or session cookie.

Permission: `workbench:write`.

Request body (`application/json`), required: [`CreateProject`](#createproject)

| Field | Type | Required | Description |
|---|---|---|---|
| key | string | yes |  |
| name | string | yes |  |
| description | string or null | no |  |

```json
{
  "key": "WB",
  "name": "Workbench",
  "description": "The tracker"
}
```

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 201 | application/json | [`Project`](#project) | The new project. |
| 400 | application/json | [`Error`](#error) | Blank key or name, or the project could not be created. |
| 403 | application/json | [`Error`](#error) | The caller lacks the grant. |
| 409 | application/json | [`Error`](#error) | The key is taken. |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `201`:

```json
{
  "id": "0b3a4c1e-6d7f-4b8a-9e21-5c2d8f3a1b90",
  "key": "WB",
  "name": "Workbench",
  "description": null,
  "created_at": "2026-10-01T09:30:00.123Z",
  "updated_at": "2026-10-01T09:30:00.123Z"
}
```

Example `400`:

```json
{
  "error": "key and name are required"
}
```

Example `403`:

```json
{
  "error": "no write access here"
}
```

Example `409`:

```json
{
  "error": "a record with that identity already exists"
}
```

### `GET /api/v1/projects/{id}`

Read a project. Operation id `getProject`.

Authentication: bearer token or session cookie.

Permission: `workbench:read or workbench:project:{id}:read`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Project id (uuid text). |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`Project`](#project) | The project. |
| 403 | application/json | [`Error`](#error) | The caller lacks the grant. |
| 404 | application/json | [`Error`](#error) |  |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `200`:

```json
{
  "id": "0b3a4c1e-6d7f-4b8a-9e21-5c2d8f3a1b90",
  "key": "WB",
  "name": "Workbench",
  "description": null,
  "created_at": "2026-10-01T09:30:00.123Z",
  "updated_at": "2026-10-01T09:30:00.123Z"
}
```

Example `403`:

```json
{
  "error": "no read access here"
}
```

Example `404`:

```json
{
  "error": "no such project"
}
```

### `PUT /api/v1/projects/{id}`

Rename a project. Operation id `updateProject`.

The key cannot change. `description` replaces the stored value (omitted means null).

Authentication: bearer token or session cookie.

Permission: `workbench:write or workbench:project:{id}:write`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Project id (uuid text). |

Request body (`application/json`), required: [`UpdateProject`](#updateproject)

| Field | Type | Required | Description |
|---|---|---|---|
| name | string | yes |  |
| description | string or null | no |  |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`Project`](#project) | The updated project. |
| 400 | application/json | [`Error`](#error) |  |
| 403 | application/json | [`Error`](#error) | The caller lacks the grant. |
| 404 | application/json | [`Error`](#error) |  |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `400`:

```json
{
  "error": "name cannot be empty"
}
```

Example `403`:

```json
{
  "error": "no write access here"
}
```

Example `404`:

```json
{
  "error": "no such project"
}
```

### `DELETE /api/v1/projects/{id}`

Delete a project. Operation id `deleteProject`.

Authentication: bearer token or session cookie.

Permission: `workbench:write or workbench:project:{id}:write`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Project id (uuid text). |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 204 |  |  | Deleted; its issues, labels, comments and links go with it (ON DELETE CASCADE). |
| 403 | application/json | [`Error`](#error) | The caller lacks the grant. |
| 404 | application/json | [`Error`](#error) |  |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `403`:

```json
{
  "error": "no write access here"
}
```

Example `404`:

```json
{
  "error": "no such project"
}
```

## issues

Issues and their workflow status.

### `GET /api/v1/projects/{id}/issues`

List a project's issues. Operation id `listIssues`.

Authentication: bearer token or session cookie.

Permission: `workbench:read or workbench:project:{id}:read`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Project id (uuid text). |
| status | query | no | string | Only issues with this status (any text; not validated). |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | array of [`Issue`](#issue) | Issues ordered by `seq`. An unknown project id gives an empty list. |
| 403 | application/json | [`Error`](#error) | The caller lacks the grant. |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `200`:

```json
[
  {
    "id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
    "project_id": "0b3a4c1e-6d7f-4b8a-9e21-5c2d8f3a1b90",
    "parent_id": null,
    "seq": 4,
    "kind": "task",
    "title": "Write the docs",
    "description": null,
    "status": "todo",
    "priority": "medium",
    "assignee": null,
    "reporter": "admin",
    "estimate": null,
    "created_at": "2026-10-01T09:31:00.000Z",
    "updated_at": "2026-10-01T09:31:00.000Z"
  }
]
```

Example `403`:

```json
{
  "error": "no read access here"
}
```

### `POST /api/v1/projects/{id}/issues`

Create an issue in a project. Operation id `createIssue`.

`seq` is MAX(seq)+1 for the project, assigned in a transaction holding a per-project advisory lock. `reporter` is the caller. `kind` and `priority` are stored as given: the service does not check them against the lists in the core crate.

Authentication: bearer token or session cookie.

Permission: `workbench:write or workbench:project:{id}:write`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Project id (uuid text). |

Request body (`application/json`), required: [`CreateIssue`](#createissue)

| Field | Type | Required | Description |
|---|---|---|---|
| parent_id | string or null | no |  |
| kind | string | no |  Default `"task"`. |
| title | string | yes |  |
| description | string or null | no |  |
| priority | string | no |  Default `"medium"`. |
| assignee | string or null | no | A username in the realm (foreign key to auth.users). |
| estimate | integer or null `int32` | no |  |

```json
{
  "title": "Write the docs",
  "kind": "task",
  "priority": "medium"
}
```

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 201 | application/json | [`Issue`](#issue) | The issue with its number `seq`. |
| 400 | application/json | [`Error`](#error) | Blank title, or a related id does not exist (parent, assignee, project). |
| 403 | application/json | [`Error`](#error) | The caller lacks the grant. |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `201`:

```json
{
  "id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
  "project_id": "0b3a4c1e-6d7f-4b8a-9e21-5c2d8f3a1b90",
  "parent_id": null,
  "seq": 4,
  "kind": "task",
  "title": "Write the docs",
  "description": null,
  "status": "todo",
  "priority": "medium",
  "assignee": null,
  "reporter": "admin",
  "estimate": null,
  "created_at": "2026-10-01T09:31:00.000Z",
  "updated_at": "2026-10-01T09:31:00.000Z"
}
```

Example `400`:

```json
{
  "error": "title is required"
}
```

Example `403`:

```json
{
  "error": "no write access here"
}
```

### `GET /api/v1/issues`

Find an issue by its key. Operation id `findIssueByKey`.

Resolves the project by key, then the issue by `seq`. Existence is checked before the grant, so a 404 versus 403 tells the caller whether the key exists.

Authentication: bearer token or session cookie.

Permission: `workbench:read or workbench:project:{id}:read`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| key | query | yes | string | `KEY-number`, split at the last `-`. |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`Issue`](#issue) | The issue. |
| 400 | application/json | [`Error`](#error) |  |
| 403 | application/json | [`Error`](#error) | The caller lacks the grant. |
| 404 | application/json | [`Error`](#error) |  |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `400`:

```json
{
  "error": "an issue key looks like WB-4"
}
```

Example `403`:

```json
{
  "error": "no read access to this issue"
}
```

Example `404`:

```json
{
  "error": "no such issue"
}
```

### `GET /api/v1/issues/{id}`

Read an issue. Operation id `getIssue`.

Authentication: bearer token or session cookie.

Permission: `workbench:read or workbench:project:<issue.project_id>:read`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Issue id (uuid text). |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`Issue`](#issue) | The issue. |
| 403 | application/json | [`Error`](#error) | The caller lacks the grant. |
| 404 | application/json | [`Error`](#error) |  |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `200`:

```json
{
  "id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
  "project_id": "0b3a4c1e-6d7f-4b8a-9e21-5c2d8f3a1b90",
  "parent_id": null,
  "seq": 4,
  "kind": "task",
  "title": "Write the docs",
  "description": null,
  "status": "todo",
  "priority": "medium",
  "assignee": null,
  "reporter": "admin",
  "estimate": null,
  "created_at": "2026-10-01T09:31:00.000Z",
  "updated_at": "2026-10-01T09:31:00.000Z"
}
```

Example `403`:

```json
{
  "error": "no read access to this issue"
}
```

Example `404`:

```json
{
  "error": "no such issue"
}
```

### `PUT /api/v1/issues/{id}`

Replace an issue's editable fields. Operation id `updateIssue`.

Every field is replaced (omitted `description`, `assignee`, `estimate` become null). Status is not changed here; see transition.

Authentication: bearer token or session cookie.

Permission: `workbench:write or workbench:project:<issue.project_id>:write`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Issue id (uuid text). |

Request body (`application/json`), required: [`UpdateIssue`](#updateissue)

| Field | Type | Required | Description |
|---|---|---|---|
| title | string | yes |  |
| description | string or null | no |  |
| kind | string | yes |  |
| priority | string | yes |  |
| assignee | string or null | no |  |
| estimate | integer or null `int32` | no |  |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`Issue`](#issue) | The updated issue. |
| 400 | application/json | [`Error`](#error) | Blank title, or the assignee is not a realm user. |
| 403 | application/json | [`Error`](#error) | The caller lacks the grant. |
| 404 | application/json | [`Error`](#error) |  |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `400`:

```json
{
  "error": "title cannot be empty"
}
```

Example `403`:

```json
{
  "error": "no write access to this issue"
}
```

Example `404`:

```json
{
  "error": "no such issue"
}
```

### `DELETE /api/v1/issues/{id}`

Delete an issue. Operation id `deleteIssue`.

Authentication: bearer token or session cookie.

Permission: `workbench:write or workbench:project:<issue.project_id>:write`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Issue id (uuid text). |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 204 |  |  | Deleted with its comments, links, label attachments and sub-issues (CASCADE). |
| 403 | application/json | [`Error`](#error) | The caller lacks the grant. |
| 404 | application/json | [`Error`](#error) |  |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `403`:

```json
{
  "error": "no write access to this issue"
}
```

Example `404`:

```json
{
  "error": "no such issue"
}
```

### `POST /api/v1/issues/{id}/transition`

Set an issue's status. Operation id `transitionIssue`.

Any status may follow any status; there are no transition rules.

Authentication: bearer token or session cookie.

Permission: `workbench:write or workbench:project:<issue.project_id>:write`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Issue id (uuid text). |

Request body (`application/json`), required: [`Transition`](#transition)

| Field | Type | Required | Description |
|---|---|---|---|
| status | string (`blocked`, `todo`, `in-progress`, `done`, `rejected`) | yes |  |

```json
{
  "status": "in-progress"
}
```

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`Issue`](#issue) | The updated issue. |
| 400 | application/json | [`Error`](#error) | Unknown status. |
| 403 | application/json | [`Error`](#error) | The caller lacks the grant. |
| 404 | application/json | [`Error`](#error) |  |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `400`:

```json
{
  "error": "unknown status 'x'; must be one of [\"blocked\", \"todo\", \"in-progress\", \"done\", \"rejected\"]"
}
```

Example `403`:

```json
{
  "error": "no write access to this issue"
}
```

Example `404`:

```json
{
  "error": "no such issue"
}
```

## comments

Comments on issues.

### `GET /api/v1/issues/{id}/comments`

List an issue's comments. Operation id `listComments`.

Authentication: bearer token or session cookie.

Permission: `workbench:read or workbench:project:<issue.project_id>:read`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Issue id (uuid text). |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | array of [`Comment`](#comment) | Oldest first. |
| 403 | application/json | [`Error`](#error) | The caller lacks the grant. |
| 404 | application/json | [`Error`](#error) |  |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `403`:

```json
{
  "error": "no read access to this issue"
}
```

Example `404`:

```json
{
  "error": "no such issue"
}
```

### `POST /api/v1/issues/{id}/comments`

Add a comment. Operation id `createComment`.

Authentication: bearer token or session cookie.

Permission: `workbench:write or workbench:project:<issue.project_id>:write`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Issue id (uuid text). |

Request body (`application/json`), required: [`NewComment`](#newcomment)

| Field | Type | Required | Description |
|---|---|---|---|
| body | string | yes |  |

```json
{
  "body": "Looks good."
}
```

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 201 | application/json | [`Comment`](#comment) | The comment (author is the caller). |
| 400 | application/json | [`Error`](#error) |  |
| 403 | application/json | [`Error`](#error) | The caller lacks the grant. |
| 404 | application/json | [`Error`](#error) |  |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `400`:

```json
{
  "error": "body is required"
}
```

Example `403`:

```json
{
  "error": "no write access to this issue"
}
```

Example `404`:

```json
{
  "error": "no such issue"
}
```

### `DELETE /api/v1/comments/{id}`

Delete a comment. Operation id `deleteComment`.

Any holder of write on the project may delete any comment; the author is not compared.

Authentication: bearer token or session cookie.

Permission: `workbench:write or workbench:project:<issue.project_id>:write`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Comment id. |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 204 |  |  | Deleted. |
| 403 | application/json | [`Error`](#error) | The caller lacks the grant. |
| 404 | application/json | [`Error`](#error) | No such comment (or its issue). |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `403`:

```json
{
  "error": "no write access to this comment"
}
```

Example `404`:

```json
{
  "error": "no such comment"
}
```

## labels

Project labels and their attachment to issues.

### `GET /api/v1/issues/{id}/labels`

Labels attached to an issue. Operation id `listIssueLabels`.

Authentication: bearer token or session cookie.

Permission: `workbench:read or workbench:project:<issue.project_id>:read`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Issue id (uuid text). |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | array of [`Label`](#label) | Ordered by label name. |
| 403 | application/json | [`Error`](#error) | The caller lacks the grant. |
| 404 | application/json | [`Error`](#error) |  |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `403`:

```json
{
  "error": "no read access to this issue"
}
```

Example `404`:

```json
{
  "error": "no such issue"
}
```

### `POST /api/v1/issues/{id}/labels/{label_id}`

Attach a label to an issue. Operation id `attachLabel`.

The label is not checked to belong to the issue's project.

Authentication: bearer token or session cookie.

Permission: `workbench:write or workbench:project:<issue.project_id>:write`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Issue id (uuid text). |
| label_id | path | yes | string |  |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 204 |  |  | Attached (idempotent: ON CONFLICT DO NOTHING). |
| 400 | application/json | [`Error`](#error) | The label id does not exist. |
| 403 | application/json | [`Error`](#error) | The caller lacks the grant. |
| 404 | application/json | [`Error`](#error) |  |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `400`:

```json
{
  "error": "the id given for a related record (project, issue, ...) does not exist"
}
```

Example `403`:

```json
{
  "error": "no write access to this issue"
}
```

Example `404`:

```json
{
  "error": "no such issue"
}
```

### `DELETE /api/v1/issues/{id}/labels/{label_id}`

Detach a label from an issue. Operation id `detachLabel`.

Authentication: bearer token or session cookie.

Permission: `workbench:write or workbench:project:<issue.project_id>:write`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Issue id (uuid text). |
| label_id | path | yes | string |  |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 204 |  |  | Detached (also when it was not attached). |
| 403 | application/json | [`Error`](#error) | The caller lacks the grant. |
| 404 | application/json | [`Error`](#error) |  |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `403`:

```json
{
  "error": "no write access to this issue"
}
```

Example `404`:

```json
{
  "error": "no such issue"
}
```

### `GET /api/v1/projects/{id}/labels`

List a project's labels. Operation id `listLabels`.

Authentication: bearer token or session cookie.

Permission: `workbench:read or workbench:project:{id}:read`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Project id (uuid text). |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | array of [`Label`](#label) | Ordered by name. |
| 403 | application/json | [`Error`](#error) | The caller lacks the grant. |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `403`:

```json
{
  "error": "no read access here"
}
```

### `POST /api/v1/projects/{id}/labels`

Create a label in a project. Operation id `createLabel`.

Authentication: bearer token or session cookie.

Permission: `workbench:write or workbench:project:{id}:write`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Project id (uuid text). |

Request body (`application/json`), required: [`CreateLabel`](#createlabel)

| Field | Type | Required | Description |
|---|---|---|---|
| name | string | yes |  |
| color | string | no | Not validated. Default `"#888888"`. |

```json
{
  "name": "urgent",
  "color": "#ff0000"
}
```

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 201 | application/json | [`Label`](#label) | The label. |
| 400 | application/json | [`Error`](#error) |  |
| 403 | application/json | [`Error`](#error) | The caller lacks the grant. |
| 409 | application/json | [`Error`](#error) | The name exists in the project. |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `400`:

```json
{
  "error": "name is required"
}
```

Example `403`:

```json
{
  "error": "no write access here"
}
```

Example `409`:

```json
{
  "error": "a record with that identity already exists"
}
```

### `DELETE /api/v1/labels/{id}`

Delete a label. Operation id `deleteLabel`.

Authentication: bearer token or session cookie.

Permission: `workbench:write or workbench:project:<label.project_id>:write`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Label id. |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 204 |  |  | Deleted; attachments go with it. |
| 403 | application/json | [`Error`](#error) | The caller lacks the grant. |
| 404 | application/json | [`Error`](#error) |  |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `403`:

```json
{
  "error": "no write access to this label"
}
```

Example `404`:

```json
{
  "error": "no such label"
}
```

## links

Typed links between issues.

### `GET /api/v1/issues/{id}/links`

The links of an issue, resolved. Operation id `listIssueLinks`.

Authentication: bearer token or session cookie.

Permission: `workbench:read or workbench:project:<issue.project_id>:read`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Issue id (uuid text). |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`RelatedIssues`](#relatedissues) | `blocks` (this issue blocks them), `blocked_by` (they block this one), `relates_to` (both directions), each ordered by project key and number. |
| 403 | application/json | [`Error`](#error) | The caller lacks the grant. |
| 404 | application/json | [`Error`](#error) |  |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `403`:

```json
{
  "error": "no read access to this issue"
}
```

Example `404`:

```json
{
  "error": "no such issue"
}
```

### `POST /api/v1/issues/{id}/links`

Link an issue to another. Operation id `createIssueLink`.

Only the owning issue's project is checked for the grant; the linked issue may be in another project.

Authentication: bearer token or session cookie.

Permission: `workbench:write or workbench:project:<issue.project_id>:write`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Issue id (uuid text). |

Request body (`application/json`), required: [`NewLink`](#newlink)

| Field | Type | Required | Description |
|---|---|---|---|
| linked_issue_id | string | yes |  |
| kind | string (`blocks`, `relates_to`) | yes |  |

```json
{
  "linked_issue_id": "8d1f2c4a-3b5e-4f60-a7b8-9c0d1e2f3a4b",
  "kind": "blocks"
}
```

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 201 | application/json | [`IssueLink`](#issuelink) | The stored link. |
| 400 | application/json | [`Error`](#error) | Unknown kind, self link, or the other issue does not exist. |
| 403 | application/json | [`Error`](#error) | The caller lacks the grant. |
| 404 | application/json | [`Error`](#error) |  |
| 409 | application/json | [`Error`](#error) | The same (issue, linked issue, kind) exists. |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `400`:

```json
{
  "error": "unknown kind 'x'; must be one of [\"blocks\", \"relates_to\"]"
}
```

Example `403`:

```json
{
  "error": "no write access to this issue"
}
```

Example `404`:

```json
{
  "error": "no such issue"
}
```

Example `409`:

```json
{
  "error": "a record with that identity already exists"
}
```

### `DELETE /api/v1/issue-links/{id}`

Remove a link. Operation id `deleteIssueLink`.

Authentication: bearer token or session cookie.

Permission: `workbench:write or workbench:project:<owning issue.project_id>:write`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Link id. |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 204 |  |  | Removed. |
| 403 | application/json | [`Error`](#error) | The caller lacks the grant. |
| 404 | application/json | [`Error`](#error) | No such link (or its issue). |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `403`:

```json
{
  "error": "no write access to this link"
}
```

Example `404`:

```json
{
  "error": "no such link"
}
```

## people

Who is asking and who can be assigned.

### `GET /api/v1/me`

The caller's user name. Operation id `me`.

Authentication: bearer token or session cookie.

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`Me`](#me) | `username` is the token `sub`, or `dev` when authentication is off. |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |

Example `200`:

```json
{
  "username": "admin"
}
```

### `GET /api/v1/users`

Every realm user, for the assignee picker. Operation id `listUsers`.

Needs only a valid identity; no workbench grant is checked.

Authentication: bearer token or session cookie.

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | array of [`User`](#user) | Rows of `auth.users` ordered by username (username and display name only). |
| 401 |  |  | No valid token or session (Auth middleware; also unmapped /api/v1 paths). Empty body, or a redirect for a browser. |
| 500 | application/json | [`Error`](#error) | Query failed (error text of the database error). |
| 503 | application/json | [`Error`](#error) | The service runs against an in-memory database. |

Example `200`:

```json
[
  {
    "username": "admin",
    "display_name": null
  }
]
```

## ui

Browser pages, assets and SSO routes.

### `GET /ui`

Redirect to the home page, or to login without a session. Operation id `uiRoot`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | To {base}/ui/home, or {base}/ui/login when not signed in. |

### `GET /ui/`

Same as /ui. Operation id `uiRootSlash`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | To {base}/ui/home, or {base}/ui/login when not signed in. |

### `GET /ui/home`

Client page, projects screen. Operation id `uiHome`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | The HTML page that starts the browser client (cache-control no-store, nonce CSP). |
| 302 |  |  | To {base}/ui/login without a valid session cookie. |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/home/`

Same as /ui/home. Operation id `uiHomeSlash`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | The HTML page that starts the browser client (cache-control no-store, nonce CSP). |
| 302 |  |  | To {base}/ui/login without a valid session cookie. |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/projects/{id}/board`

Client page, board screen. Operation id `uiBoard`.

Authentication: none (the route checks the session itself where noted).

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string |  |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | The HTML page that starts the browser client (cache-control no-store, nonce CSP). |
| 302 |  |  | To {base}/ui/login without a valid session cookie. |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/issues/{id}`

Client page, issue screen. Operation id `uiIssue`.

Authentication: none (the route checks the session itself where noted).

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string |  |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | The HTML page that starts the browser client (cache-control no-store, nonce CSP). |
| 302 |  |  | To {base}/ui/login without a valid session cookie. |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/assets/quench.js`

The quench-ui loader script. Operation id `uiAssetScript`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/javascript | string | The file (cache-control: no-cache). |

### `GET /ui/assets/app.css`

Shell stylesheet followed by the client stylesheet. Operation id `uiAssetStyle`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/css | string | The file (cache-control: no-cache). |

### `GET /ui/assets/shell.js`

The forge-shell script. Operation id `uiAssetShell`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/javascript | string | The file (cache-control: no-cache). |

### `GET /ui/assets/favicon.png`

The service icon. Operation id `uiAssetFavicon`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | image/png | string `binary` | The file (cache-control: no-cache). |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/assets/app.wasm`

The browser client module. Operation id `uiAssetModule`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/wasm | string `binary` | The file (cache-control: no-cache). |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/login`

Start sign-in (SSO redirect to Gatehouse). Operation id `uiLogin`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | To {base}/ui/home (refresh cookie accepted, new cookies set) or to Gatehouse's /api/v1/authorize; sets the cookie forge_authorize_state. |
| 503 |  |  | Text `gatehouse is not configured` (GATEHOUSE_URL, GATEHOUSE_CLIENT_ID or GATEHOUSE_CLIENT_SECRET missing). |

### `GET /ui/login/`

Same as /ui/login. Operation id `uiLoginSlash`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | See /ui/login. |
| 503 |  |  | Text `gatehouse is not configured` (GATEHOUSE_URL, GATEHOUSE_CLIENT_ID or GATEHOUSE_CLIENT_SECRET missing). |

### `GET /ui/auth/callback`

Complete the authorization-code exchange. Operation id `uiAuthCallback`.

Authentication: none (the route checks the session itself where noted).

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| code | query | no | string |  |
| state | query | no | string |  |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | To {base}/ui/home with session cookies, or to {base}/ui/login on any failure. |
| 503 |  |  | Text `gatehouse is not configured` (GATEHOUSE_URL, GATEHOUSE_CLIENT_ID or GATEHOUSE_CLIENT_SECRET missing). |

### `GET /ui/logout`

Realm logout through Gatehouse. Operation id `uiLogout`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | To Gatehouse's /ui/logout with a redirect back to {base}/ui/login. |
| 503 |  |  | Text `gatehouse is not configured` (GATEHOUSE_URL, GATEHOUSE_CLIENT_ID or GATEHOUSE_CLIENT_SECRET missing). |

### `GET /ui/status`

Session status for the page shell. Operation id `uiStatus`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`AuthStatus`](#authstatus) | Always 200. |

### `POST /ui/refresh`

Exchange the refresh cookie for new cookies. Operation id `uiRefresh`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`AuthStatus`](#authstatus) | New session and refresh cookies set. |
| 401 |  |  | No refresh cookie, or Gatehouse refused it. Empty body. |

Example `200`:

```json
{
  "authenticated": true,
  "username": null,
  "roles": []
}
```

## Schemas

### Me

| Field | Type | Required | Description |
|---|---|---|---|
| username | string | yes |  |

### User

| Field | Type | Required | Description |
|---|---|---|---|
| username | string | yes |  |
| display_name | string or null | yes |  |

### Project

| Field | Type | Required | Description |
|---|---|---|---|
| id | string | yes |  |
| key | string | yes | Unique short code used in issue keys (WB-4). |
| name | string | yes |  |
| description | string or null | yes |  |
| created_at | string | yes | RFC 3339 timestamp. |
| updated_at | string | yes |  |

### CreateProject

| Field | Type | Required | Description |
|---|---|---|---|
| key | string | yes |  |
| name | string | yes |  |
| description | string or null | no |  |

### UpdateProject

| Field | Type | Required | Description |
|---|---|---|---|
| name | string | yes |  |
| description | string or null | no |  |

### Issue

| Field | Type | Required | Description |
|---|---|---|---|
| id | string | yes |  |
| project_id | string | yes |  |
| parent_id | string or null | yes |  |
| seq | integer `int32` | yes | Number within the project; the key is `{project.key}-{seq}`. |
| kind | string | yes | The client offers task, bug, story. Not validated by the service. |
| title | string | yes |  |
| description | string or null | yes |  |
| status | string (`blocked`, `todo`, `in-progress`, `done`, `rejected`) | yes |  |
| priority | string | yes | The client offers low, medium, high. Not validated. |
| assignee | string or null | yes |  |
| reporter | string | yes |  |
| estimate | integer or null `int32` | yes | Story points. |
| created_at | string | yes |  |
| updated_at | string | yes |  |

### CreateIssue

| Field | Type | Required | Description |
|---|---|---|---|
| parent_id | string or null | no |  |
| kind | string | no |  Default `"task"`. |
| title | string | yes |  |
| description | string or null | no |  |
| priority | string | no |  Default `"medium"`. |
| assignee | string or null | no | A username in the realm (foreign key to auth.users). |
| estimate | integer or null `int32` | no |  |

### UpdateIssue

| Field | Type | Required | Description |
|---|---|---|---|
| title | string | yes |  |
| description | string or null | no |  |
| kind | string | yes |  |
| priority | string | yes |  |
| assignee | string or null | no |  |
| estimate | integer or null `int32` | no |  |

### Transition

| Field | Type | Required | Description |
|---|---|---|---|
| status | string (`blocked`, `todo`, `in-progress`, `done`, `rejected`) | yes |  |

### Comment

| Field | Type | Required | Description |
|---|---|---|---|
| id | string | yes |  |
| issue_id | string | yes |  |
| author | string | yes |  |
| body | string | yes |  |
| created_at | string | yes |  |

### NewComment

| Field | Type | Required | Description |
|---|---|---|---|
| body | string | yes |  |

### Label

| Field | Type | Required | Description |
|---|---|---|---|
| id | string | yes |  |
| project_id | string | yes |  |
| name | string | yes |  |
| color | string | yes |  |
| created_at | string | yes |  |

### CreateLabel

| Field | Type | Required | Description |
|---|---|---|---|
| name | string | yes |  |
| color | string | no | Not validated. Default `"#888888"`. |

### IssueLink

| Field | Type | Required | Description |
|---|---|---|---|
| id | string | yes |  |
| issue_id | string | yes |  |
| linked_issue_id | string | yes |  |
| kind | string (`blocks`, `relates_to`) | yes |  |
| created_at | string | yes |  |

### NewLink

| Field | Type | Required | Description |
|---|---|---|---|
| linked_issue_id | string | yes |  |
| kind | string (`blocks`, `relates_to`) | yes |  |

### LinkedIssue

| Field | Type | Required | Description |
|---|---|---|---|
| link_id | string | yes |  |
| issue_id | string | yes |  |
| project_key | string | yes |  |
| seq | integer | yes |  |
| title | string | yes |  |
| status | string | yes |  |

### RelatedIssues

| Field | Type | Required | Description |
|---|---|---|---|
| blocks | array of [`LinkedIssue`](#linkedissue) | yes |  |
| blocked_by | array of [`LinkedIssue`](#linkedissue) | yes |  |
| relates_to | array of [`LinkedIssue`](#linkedissue) | yes |  |

### Error

| Field | Type | Required | Description |
|---|---|---|---|
| error | string | yes |  |

### AuthStatus

| Field | Type | Required | Description |
|---|---|---|---|
| authenticated | boolean | yes |  |
| username | string or null | yes |  |
| roles | array of string | yes |  |

