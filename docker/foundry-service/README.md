# foundry-service

Database initialization for the Forge platform. One place for every migration,
one command to install a set of services at chosen versions.

You declare *what* should exist:

```toml
[[install]]
module = "sage"
version = "0.2.0"
```

and the service resolves the dependency tree (`sage` → `auth`, `quench-core`,
`pgvector`), topologically sorts it, and applies every
outstanding migration in order. It is a run-to-completion job: it exits `0` when
the database matches the plan, non-zero otherwise.

## The catalog

`migrations/<module>/` holds one independently versioned module:

```
migrations/
  quench-core/     module.toml, 0001-create-schema.toml
  auth/            module.toml, 0001-realm-schema.toml
  pgvector/        module.toml, 0001-install-vector.toml
  gatehouse/       module.toml
  sage/            module.toml, 0001-workspace.toml, 0002-files-and-rag.toml
  switchboard/     module.toml, 0001-model-cache.toml
  warehouse/       module.toml
```

`auth` is `scope = "database"` with a fixed `auth` schema: the shared identity
realm is one instance no matter how many services depend on it. It owns a single
table, `auth.users` - sessions live in the cache store (Redis), where expiry is
a TTL and revocation is a delete.

`module.toml` describes the module:

```toml
name = "sage"
version = "0.2.0"                          # tracks the service crate version
default_schema = "sage"
requires = ["auth@^0.1", "quench-core@^0.1", "pgvector@^0.1"]

[variables]
embedding_dim = "1024"                     # substituted as ${embedding_dim}
```

Migrations carry the version they were introduced in:

```toml
[[migrations]]
id = "0002-files-and-rag"
author = "sage-service"
since = "0.2.0"

[[migrations.changes]]
sql = "CREATE TABLE IF NOT EXISTS ${schema}.files (...);"
```

Installing `sage@0.2.0` applies every `sage` migration with `since <= 0.2.0`;
pinning `sage@0.1.0` would stop before the `0.2.0` ones. Upgrading later applies
only the gap.

The catalog starts at 0.2.0: it is not compatible with databases built by the
0.1.x per-service migration loaders, and there is no adoption path from them.
Install into a fresh database.

### Modules are installed per schema

A module instance is `module@schema`, so one module definition can serve several
schemas - `${schema}` renders per instance. Dependencies inherit the schema of
whatever required them unless the requirement names one:

```toml
requires = [{ module = "quench-core", version = "^0.1", schema = "shared" }]
```

Modules with `scope = "database"` (like `pgvector`) resolve to one instance per
database no matter how many schemas depend on them.

## State

Bookkeeping lives in its own schema, `foundry`, so it never collides with - or
gets dropped alongside - the schemas it tracks. Two tables,
`foundry.forge_migrations` and `foundry.forge_modules`, record applied
migrations (with a checksum of their rendered SQL) and installed module
versions. A session-level advisory lock serializes concurrent runs, so a Job
racing an init container is safe.

Point `--ledger-schema` elsewhere to keep the bookkeeping somewhere else.

If a migration's SQL changes after it was applied the run fails with a drift
error; `--allow-drift` downgrades that to a warning.

## Usage

```bash
foundry-service apply       # resolve and apply (default)
foundry-service plan        # what apply would do; writes nothing
foundry-service status      # installed module versions vs the catalog
foundry-service validate    # check catalog + plan without a database
foundry-service reset --yes # drop the planned schemas and their ledger rows
```

`reset` is a development convenience - it refuses to run without `--yes`, and it
never drops `public` or the ledger schema itself.

Configuration precedence is flags, then environment, then `config/install.toml`:

| Flag | Environment | Default |
| --- | --- | --- |
| `--database-url` | `DATABASE_URL`, `POSTGRES_URL` | required |
| `--install module[@version][:schema]` | `FOUNDRY_INSTALL` (comma separated) | `[[install]]` entries |
| `--catalog` | `FOUNDRY_CATALOG` | `migrations` |
| `--config` | `FOUNDRY_CONFIG` | `config/install.toml` |
| `--ledger-schema` | `FOUNDRY_LEDGER_SCHEMA` | `foundry` |
| `--ledger-table` | `FOUNDRY_LEDGER_TABLE` | `forge_migrations` |
| `--module-table` | `FOUNDRY_MODULE_TABLE` | `forge_modules` |

## Kubernetes

`k8s/job.yaml` is a ready-to-edit manifest. The image runs `apply` by default:

```yaml
containers:
  - name: foundry
    image: ennor.ddns.net/forge/foundry:latest
    args: ["apply"]
    env:
      - name: DATABASE_URL
        valueFrom:
          secretKeyRef: { name: foundry, key: url }
      - name: FOUNDRY_INSTALL
        value: "sage,switchboard,warehouse"
```

Run it as a `Job` before rolling out services, or as an init container on a
single service (`FOUNDRY_INSTALL=sage`). Services no longer migrate at startup.

## Adding a migration

1. Add `NNNN-name.toml` to the module directory.
2. Tag it with the `since` version that will ship it.
3. Bump `version` in `module.toml` **and the service crate** to that version -
   the two are meant to match, so `status` compares like with like. Under
   Cargo's 0.x rules the minor slot is the breaking one: a migration that
   existing deployments must coordinate around (renamed column, moved table,
   changed constraint) is `0.1.x → 0.2.0`, not a patch bump.
4. `foundry-service validate` to check the catalog, `plan` to preview.

`since` is not part of a migration's checksum - only its rendered SQL is - so
retagging one during a version renumber does not cause drift on databases that
already applied it.
