# Architecture

How the pieces documented in the [Home](./README.md) page fit together.

## The estate

Forge runs six Actix Web services (`docker/*`), each owning its own Postgres schema, plus a set of CLI tools that either drive those services or operate independently on the workspace/cluster. [Foreman](./cli/foreman.md) is what brings a chosen subset of the estate up locally, in dependency order, from one `foreman.toml`.

```
                     ┌───────────────────┐
                     │  Gatehouse (auth) │  issues realm tokens
                     └─────────┬─────────┘
                               │ every service verifies tokens locally
        ┌──────────┬──────────┼──────────┬───────────┐
        ▼          ▼          ▼          ▼           ▼
   Conveyor    Warehouse   Switchboard  Sage      (Foundry: not a
   (CI/CD)     (storage)   (vLLM mgmt) (chat/RAG)  peer — runs once
        │          ▲            ▲          │        ahead of the others,
        │          │            └──────────┘        installs every
        └──────────┘         Sage calls Switchboard  service's schema)
   artifacts/images           to launch/find models
```

## Identity: Gatehouse is the one source of truth

[Gatehouse](./docker/gatehouse-service.md) holds the only user table in the realm (`auth.users`) and the only login page. It issues Ed25519-signed JWTs; every other service verifies those tokens **locally**, against Gatehouse's published JWKS — there is no per-request call back to Gatehouse on the hot path. [Quench Auth](https://github.com/lorehaven/quench/blob/master/docs/quench-auth.md) is the library that gives each relying-party service that verification logic, its actix middleware, and the permission-check helpers (`Claims::can(service, action)`); Gatehouse itself depends on it too, to verify its own tokens.

Two related points worth knowing if you're integrating a new service:
- **Actions, not read/write levels.** There is no `Access::Read`/`Access::Write` ordering — a `"write"` grant does not imply `"read"`. Permissions are checked per action string (`can(service, "write")`, or a service-specific action like `switchboard`'s `"launch"`/`"stop"`/`"delete-model"`).
- **The permission catalog drives the audience ceiling.** Gatehouse derives the realm's JWT audience list and its admin access-matrix from `config/permissions.toml`'s per-action catalog now, not from a `SERVICE_AUDIENCES` env var (every other service still reads `SERVICE_AUDIENCES` normally — Gatehouse is the one exception, since it's the issuer).

## Schema: Foundry runs once, ahead of everything

[Foundry](./docker/foundry-service.md) is not a long-running peer of the other five services — it's a run-to-completion job that owns the migration files for **all** of them (`conveyor/`, `gatehouse/`, `warehouse/`, `sage/`, `switchboard/`, plus shared `pgvector/` and `quench-core/` modules), each versioned independently with its own dependency graph. It runs as a Kubernetes `Job` (or init container) ahead of a rollout, or as a `foreman` task ahead of starting services locally, and exits non-zero if the database doesn't match the declared plan. [Quench DB](https://github.com/lorehaven/quench/blob/master/docs/quench-db.md) is the library underneath both Foundry's migration runner and every service's own CRUD access.

## The AI stack: Switchboard serves, Sage talks to users

[Switchboard](./docker/switchboard-service.md) is the only thing in the estate that starts or stops a vLLM process. It scans model files, estimates VRAM fit per quantization, and exposes `/api/v1/vllm/*` behind Gatehouse-issued client-credentials tokens. [Sage](./docker/sage-service.md) — the chat/RAG application users actually talk to — never launches vLLM itself; it asks Switchboard, including keeping its own configured default models warm on a startup/10s-loop cycle, and blocks its home page behind an "initializing" screen until they're up. This indirection means Sage (and anything else that needs an LLM) can target a shared, GPU-aware fleet instead of managing its own model processes.

[Welder](./cli/welder.md), the multi-agent CLI, is a separate consumer of the same idea: it can target a local Ollama daemon, a locally-spawned vLLM process, or the shared Switchboard-managed fleet, selected per workflow.

## CI/CD and storage: Conveyor drives, Warehouse holds

[Conveyor](./docker/conveyor-service.md) is the estate's CI/CD service: a webhook triggers it, it checks out the commit, reads the `.conveyor.toml` **that commit declares** (so a branch can change its own build, reviewably), and runs the pipeline — reusing [Anvil](./cli/anvil.md) for builds/tests/images, [Riveter](./cli/riveter.md) for manifest application, and [Warehouse](./docker/warehouse-service.md) as the artifact/image registry, rather than reinventing any of them. Pipeline parsing/planning itself lives in [Conveyor Pipeline](./libs/conveyor-pipeline.md), a separate crate so Conveyor Service (which runs pipelines) and [Conveyor CLI](./cli/conveyor-cli.md)'s `conveyor validate` (which checks them without a running service) share one parser.

Warehouse itself mounts three addressing schemes side by side behind one service — a Cargo registry, a Docker Registry HTTP API v2, and plain named-directory file storage — because Conveyor is the first real caller of both the files API (build artifacts) and the Docker registry (release images).

## Shared plumbing

Every `docker/*` service is built on [Quench Starter](https://github.com/lorehaven/quench/blob/master/docs/quench-starter.md) (TLS/plain HTTP setup, base-path scoping, health checks, DB bootstrap) and renders its UI with [Quench Web](https://github.com/lorehaven/quench/blob/master/docs/quench-web.md) (a templating-engine-free `Element`/`PageBuilder` HTML builder). [Quench Cache](https://github.com/lorehaven/quench/blob/master/docs/quench-cache.md), [Quench Client](https://github.com/lorehaven/quench/blob/master/docs/quench-client.md), and [Quench Config](https://github.com/lorehaven/quench/blob/master/docs/quench-config.md) round out the shared layer — caching, authenticated HTTP calls between services, and config/env loading, respectively. [Quench Web Components](https://github.com/lorehaven/quench/blob/master/docs/quench-web-components.md) exists in that workspace but currently has no consumers; see its page for details. These libraries all live in the sibling [quench](https://github.com/lorehaven/quench) repository, pulled in from the `ennor` cargo registry.

## See also

- [Foreman](./cli/foreman.md) for how the local dev estate is actually brought up and torn down
- [Forge BDD](./tests/forge-bdd.md) for how the services are tested together, end to end
