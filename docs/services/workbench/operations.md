---
title: Operations
parent: Workbench
grand_parent: Services
nav_order: 4
---

## Run locally

`foreman start workbench` builds `workbench-service` and the browser module (`scripts/web.sh`), runs foundry for the `workbench` module and its dependencies, waits for Gatehouse and serves HTTPS with `BASE_PATH=/workbench`. Open `{origin}/workbench/ui/home` and sign in through Gatehouse. Test data: foreman's `seed` task loads demo workbench projects and issues (plain SQL). See [Running the estate locally](../../guides/running-locally.md).

## Cluster

Image `workbench` from `images/Dockerfile.alpine` (see `.anvil/docker.toml`, module `workbench`: build args `RESOURCE_DIR=web`, `WASM_PACKAGE=workbench-web`). Run foundry first (`--install workbench`). Needs Gatehouse reachable, Postgres, optionally Redis. Health: `/health`, `/health/live`, `/health/ready` ([Common service runtime](../common-runtime.md)); ready after Gatehouse's `/health/ready` answers.

## Failure modes

| Symptom | Cause |
|---|---|
| Panic `no database configured` | no `DATABASE_URL`/`POSTGRES_URL`. |
| `503` `workbench needs Postgres; ...` | in-memory database (`ALLOW_IN_MEMORY_DB=true`). |
| `400` `the id given for a related record (project, issue, ...) does not exist` | foreign key violation (unknown project, parent issue, assignee, label or linked issue). |
| `409` `a record with that identity already exists` | duplicate project key, label name per project, or identical link. |
| `500` with the database error text | any other SQL error; the error text is returned to the caller. |
| `401` on every API call | token expired and no refresh cookie, wrong audience, or inactive session. |
| Relation `workbench.projects` does not exist | the `workbench` module was not installed by foundry. |

Limits: none beyond the pool size; lists are not paginated; no rate limiting.

## Tests

`cargo test -p workbench-core -p workbench-app -p workbench-web -p workbench`. Unit: `service/tests/unit/{authz_tests,authz_request_tests,client_tests}.rs`, `core/tests/unit`, `web/tests/unit.rs` (stand-in transport), `app/tests/screens.rs` (`TestRenderer`). Integration (`service/tests/integration/{project,issue,comment_and_label,realm_users,routers_api}_tests.rs`) need Postgres with the module installed and `WORKBENCH_TEST_DATABASE_URL`; they skip without it. `scripts/docs-check.py workbench` checks `openapi.yaml` against the route macros.
