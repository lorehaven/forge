---
title: Operations
parent: Foundry
grand_parent: Services
nav_order: 4
---

## Locally (foreman)

`foreman.toml` defines `[tasks.foundry]` (role `migrate`, container `postgres`): builds `foundry-service`, then runs

```text
target/debug/foundry-service --catalog services/foundry/service/migrations \
  --config services/foundry/service/config/install.toml <selection> apply
```

with `DATABASE_URL` from foreman. `each_selected = ["--install", "${service}"]` turns the started services into `--install` flags (`selection = "subset-only"`: a full start passes none and the config file's list is used). `foreman reset` (`[tasks.reset]`) runs `... reset --yes` after stopping services, with a warning "dropping every schema foundry owns". `foreman db` starts Postgres and the catalog only. Seed data (`[tasks.seed]`) is plain SQL applied after foundry.

## In the cluster

Run the `foundry` image (`ossiriand.arda:8080/forge/foundry`, `ennor.ddns.net/forge/foundry`) as a Job or init container with `DATABASE_URL` (and optionally `FOUNDRY_INSTALL`). The image entrypoint is `./service` (the binary copied as `service`) with no default arguments, so `apply` runs; it runs as uid/gid 999:999 and ships the catalog in `migrations` (build arg `RESOURCE_DIR=migrations`). There is no probe, port, metric or health endpoint. Deployment manifests are not in this repository.

Exit status 0 means the database matches the plan; non-zero fails the Job.

## Pipeline

`.conveyor.toml` coverage job: a Postgres sidecar, then `cargo run -q -p foundry -- apply --catalog services/foundry/service/migrations --database-url postgres://...@127.0.0.1:5432/postgres --install conveyor --install daybook --install gantry --install gatehouse --install sage --install switchboard --install warehouse --install workbench`. `gauge` and `hoist` are not in that list.

## Logging

`quench_log::init()`; migration application logs `applying migration <id>` at info, drift at warn, `dropping schema <s>` at warn. User-facing progress is the status lines on stdout.

## Failure modes

| Symptom | Cause |
|---|---|
| `no database configured` | no URL from flag, `DATABASE_URL`, `POSTGRES_URL`, file. |
| `nothing to install` | no `--install`, no `FOUNDRY_INSTALL`, no `[[install]]`. |
| `unknown module '<m>'` | not in the catalog (optionally `required_by`). |
| `migration '<id>' changed after it was applied` | edited applied migration; use `--allow-drift` only knowingly. |
| `migration '<id>': <pg error>` | SQL failure; the migration's transaction was rolled back. |
| Blocks on start | another apply holds the advisory lock; it waits (no timeout). |
| `reset` ... exit 1 | missing `--yes`. |

## Testing

`tests/cli_tests.rs` (assert_cmd against the built binary: `validate` on the real catalog, unknown module, missing database URL, and - with `FOUNDRY_TEST_DATABASE_URL` - `reset` refusal, `apply`, `status`, `plan`) and `tests/unit/config_tests.rs` (config resolution). Run from `services/foundry/service`: `cargo test -p foundry`.
