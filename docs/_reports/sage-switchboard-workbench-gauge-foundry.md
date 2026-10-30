# Gap report: sage, switchboard, workbench, gauge, foundry

> **Fixed 2026-10-30** (the pages of the book describe the fixed behaviour; the text below is the original finding):
> * Switchboard model delete: `..` components, the root itself and paths resolving outside the roots are refused.
> * Sage turns: a requested `capability_profile` may only narrow the configured one.
> * Sage `GET /ui/chat/stats/{id}` and `GET /api/v1/turns/{id}/stream` check that the conversation belongs to the caller.


Written with `docs/services/{sage,switchboard,workbench,gauge,foundry}`. Routes: the checker passes for all five (`python3 scripts/docs-check.py sage switchboard workbench gauge foundry`). Counts of operations in each `openapi.yaml` against route macros in `service/src`: sage 28 API operations and 17 `/ui` operations (pages, assets, SSO routes and `/ui/chat/stats/{conversation_id}`); switchboard 11 API + 19 UI; workbench 26 API + 17 UI; gauge 5 API + 22 UI; foundry 0 (no HTTP). The routes added by `quench-starter` (`/health`, `/health/live`, `/health/ready`, `/swagger-ui`, `/swagger-ui/`, base-path redirects) are `#[get]` macros in the starter crate, not in the services; they are documented once in `docs/services/common-runtime.md`.

## 1. Stale or wrong statements in the old notes

Old pages (`docs/docker/*-service.md`, now deleted) and `sage/service/docs/*.md` (still in the tree, outside the book).

Foundry (`docs/docker/foundry-service.md`)
* "The catalog today holds ... nine modules": the catalog has 13 (`daybook`, `gantry`, `gauge`, `hoist` missing from the list).
* "`config/install.toml` ... currently `conveyor`, `gatehouse`, `sage`, `switchboard`, `warehouse`, `workbench`": the file also lists `daybook`, `gantry`, `hoist`; it does not list `gauge`.
* Example `requires = ["auth@^0.1", ...]`: the real modules require `auth@^0.2`.
* "No dedicated tests/ directory": `services/foundry/service/tests/{cli_tests.rs,unit/config_tests.rs}` exist.

Sage (`docs/docker/sage-service.md`, `sage/service/docs/*`)
* UI routes `/ui/projects/*`, `/ui/files/*`, `/ui/chat/*`, "HTMX-rendered", `routers/ui/chat.rs`, `routers/ui/pages/files.rs`: gone. The UI is a WebAssembly client; the only remaining `/ui` data route is `GET /ui/chat/stats/{conversation_id}`. `sage/service/docs/{REQUEST_FEATURES,QUICK_TEST_REFERENCE,TESTING_GUIDE}.md` still instruct `POST /sage/ui/chat/send`, which does not exist (the flow is `POST /api/v1/turns` then `GET /api/v1/turns/{id}/stream`).
* The route list omits `GET /api/v1/models`, `/conversation-id`, `/conversations*`, `/projects`, `/turns*`, `/messages/{id}/edit`, `/files/staged`.
* `SAGE_SUPPORTED_MODELS` was described as the filter for default-model launch only; it also filters the picker (`instances`) and the readiness (`ready`, `defaults`).
* `DB_MIGRATION_TABLE`, `DB_RECREATE`, `DB_POOL_MAX_SIZE` listed under Sage: the first two are not read anywhere (`quench-starter` no longer migrates); `DB_POOL_MAX_SIZE` is read by `quench-db` (default 5).
* "`domain/` ... a `TokenCounter` used by the chunker/context builder": the chunker does not use it (`domain/context.rs`).
* "`evals/run_eval.py` uses it; see `evals/README.md`": no such directory under `services/sage` was found.
* `sage/service/migrations/` (9 files, `00001-initial-users-schema` ...) is a leftover; the catalog module `sage` has two migrations and no users/sessions tables.

Switchboard (`docs/docker/switchboard-service.md`)
* HTMX grid, modals, SSE (`GET /api/v1/gpu/status/sse`, `/api/v1/vllm/status/sse`, `/api/v1/models/grid`, `.../estimates-modal`, `.../delete-modal`, `.../launch-modal`, `.../stop-modal`): none exist. The GPU is polled via `GET /api/v1/gpu/status`.
* "`/ui/models`", "`/ui/vllm`": the routes are `/ui/models/dashboard` and `/ui/vllm/manage`.
* "`switchboard.users`/`switchboard.sessions`": not in the catalog (the module has `vllm_architectures` and `model_cache` only); `DB_SCHEMA`, `DB_MIGRATION_TABLE`, `DB_RECREATE` are not read by the service (table names are hard-coded `switchboard.*`).
* "Reads just the GGUF header/metadata (first 1 MB)": the code reads the header with `quench_convert::gguf::read`; no 1 MB limit appears in the code.
* "paginate" in `list.rs`: `POST /api/v1/models/list` filters and sorts only.

Workbench (`docs/docker/workbench-service.md`)
* HTMX UI routes (`POST /ui/projects`, `GET /ui/projects/{id}/board` fragments, `POST .../transition`, `/ui/issues/{id}` POST, `/ui/issues/{id}/links...`, `/ui/assets/{path:.*}`): gone. Page routes are `/ui/home`, `/ui/home/`, `/ui/projects/{id}/board`, `/ui/issues/{id}` and five fixed asset routes.
* The API list omits `GET /api/v1/me`, `GET /api/v1/users`, `GET /api/v1/issues?key=`.
* "`SERVICE_NAME` used in startup log lines": it is the audience the token must carry; "`AUTH_DB_SCHEMA` ... to verify tokens": it is the schema read for the assignee list.
* "Board view ... HTMX-driven": the board is a client screen using the status select and drag and drop.

Gauge (`docs/docker/gauge-service.md`): accurate against the code; checked point by point (targets file, ranges and step sizes, forecast limits, alert rules, retention). Exceptions: `docs/README.md` described Gauge as "no database" (the file was removed by another author); `HostSummary.kind` is documented in the code as `pcp` or `node_exporter`, but serialises as `pcp` or `node`.

## 2. In the code, undocumented before

* Foundry: variables `FOUNDRY_*`, `[[install]] variables`, the `reset` semantics for database-scoped modules, ledger tables and the advisory lock key, checksum rule ("since" excluded).
* Sage: the turn event stream (`status`, `delta`, `replace`, `done`, `failure`) and its shapes; `SAGE_*` with plain-name fallback; `capability_profile` and `tool_confirmations` in the turn body; `ChatRequest.evidence` and the grounded response metadata; rate-limit defaults per profile; the project/message-tree model; `GET /api/v1/files/{id}/chunks`.
* Switchboard: the instance id forms and status values per engine; restorer limits (5 restarts, 600 s stable run, 1200 s loading cutoff); Kubernetes pod spec and every `VLLM_*` / `HF_*` variable; `models/running` needs a wildcard role; the fit arithmetic constants.
* Workbench: per-project grants and `GET /projects` filtering; link semantics (`blocks`/`blocked_by`/`relates_to`); cascades; `seq` allocation with an advisory lock; error texts.
* Gauge: all metric names read from PCP/node_exporter, alert thresholds and keys, pruning cadence, the 30 s writer loop, `HISTORY_DAYS`, `DB_SCHEMA`, `GAUGE_TARGETS`.
* Common: the six SSO routes' behaviour, cookies, `AUTH_REDIRECT_HOSTS`, security headers, `x-correlation-id`, `LOG_SKIP_PREFIXES`.

## 3. Open questions and suspected bugs (not fixed)

Security-relevant
* `switchboard/service/src/routers/models/delete.rs:46-47`: the delete path check is `Path::starts_with(root)` on the raw string. A path like `/mnt/dev/huggingface/hub/../../../etc/x` starts with the root and passes; `remove_dir_all` then follows `..`. Needs the `switchboard:delete-model` grant, but the scope is the whole host. Canonicalise before the check.
* `sage/service/src/routers/turns.rs:41-50,1348`: `POST /api/v1/turns` deserialises `capability_profile` and `tool_confirmations` from the client, so any `sage:write` caller can run `cli_agent` and confirm `command` / `file_ops` / `code_executor` for the turn (the old `REQUEST_FEATURES.md` presents the profile override as a feature). Confirmations exist to be given by the caller, but the profile is then not an operator decision any more.
* `sage/service/src/tools/command.rs:129-146`: the allow-list contains `find`, `awk`, `sed`, `env`, `type`; `find -exec`, `awk 'BEGIN{system(...)}'` and `sed 'e'` run arbitrary commands, and the blacklist is substring based (`dd` also blocks `add`). `tools/code_executor.rs`: the "sandbox" is a substring blacklist plus a 10 s timeout around the host `python3`/`node`.
* `sage/service/src/routers/turns.rs:1219-1260`: `GET /ui/chat/stats/{conversation_id}` does not check that the conversation belongs to the caller, and sits outside the `Auth` prefixes (cookie only).
* `sage/service/src/routers/turns.rs:1092-1114`: `GET /api/v1/turns/{id}/stream` starts the pending request of any id for any logged-in user (ids are random UUIDs).
* `sage/service/src/routers/chat.rs:309-350`: metrics and costs (all users) readable by any authenticated caller.
* `gauge/service/src/routers/api/hosts.rs`: no grant check; the catalog action `gauge:read` is never enforced (any token with audience `gauge` passes). Same for `switchboard:read` (listing, `POST /models/list`, `GET /gpu/status`).
* `workbench`: `GET /api/v1/issues?key=` answers 404 for unknown keys and 403 for known ones the caller cannot read (`issues.rs` `find_by_key`), which reveals existence; `POST /issues/{id}/labels/{label_id}` and `parent_id` accept ids from other projects (no same-project check); `POST /issues/{id}/links` checks the grant on the owning issue only; kind, priority and `status` filter values are not validated (`status` is validated only by `transition`); label colour is free text.

Correctness
* `sage/service/src/routers/models.rs:47` compares the instance status with lowercase `failed`, while Switchboard's Kubernetes engine reports a crashed pod as `Failed` (`switchboard/.../kubernetes.rs:227`, handled by its reaper, which removes it within 30 s). A failed default model therefore shows as `pending`, and the monitor launches it again, which works only after the reaper removed the pod. Switchboard's client treats both `failed` and `Failed` as failures.
* `sage/service/src/startup/state.rs:109` reads `SEARCH_PROVIDER` (unprefixed) for the registry default while `config/settings.rs:165` reads `SAGE_SEARCH_PROVIDER` first: setting only the prefixed name changes the picker's default but not the registry's.
* `sage/service/src/startup/validate.rs:65`: warns about `BRAVE_API_KEY`, the provider reads `BRAVE_SEARCH_API_KEY`.
* `switchboard` reaper and sage monitor share a 10/30 s cadence; a model that fails 3 times is skipped until it has been seen running once (`default_models.rs`), which cannot happen for a model that never starts. Intended? (Not changed.)
* `switchboard/service/src/routers/vllm/launch.rs`: the form variant does not reject a blank `model`; the JSON variant does.
* `switchboard/.../kubernetes.rs` ignores the request's `host` (binds 0.0.0.0) and reports `<pod>.<ns>.svc.cluster.local`; for native the reported `port` may differ from the requested one.
* `switchboard` estimates: `OVERHEAD_GB = 3.0` times `FRAGMENTATION_MARGIN = 1.15` (3.45 GB); the "3.15 GiB launch reserve" from the working notes is not present in the code. The KV estimate ignores grouped-query attention and so overestimates; GGUF `vllm_supported` is always false.
* `gauge`: `config/install.toml` of foundry does not list `gauge`, and `.conveyor.toml` installs neither `gauge` nor `hoist`; a Job built from the shipped config creates no `gauge` schema, and gauge's history writes then fail with a warning per minute.`foreman` passes `--install <service>` for each selected service, so a foreman start of gauge does install it.
* `gauge/app` never sends `stat=max` (the API supports it). `gauge/core/src/api.rs` documents `HostSummary.kind` as `node_exporter`; it is `node`.
* `common`: `GET /swagger-ui/` redirects to `/swagger-ui/index.html`, which no route serves.
* `sage` pending turns (`ChatState.pending_messages`) have no expiry; a client that posts and never streams leaks an entry until restart.
* `workbench` comment delete lets any writer on the project delete any comment (author not compared). Intended?
* The READMEs `services/{sage,switchboard,workbench,gauge,foundry}/service/README.md` still link to the deleted `docs/docker/<name>-service.md` (outside `docs/`, left untouched as instructed).
* Unverified: Kubernetes behaviour was read from the code only; no cluster was contacted. Nothing was run against Postgres, vLLM or a GPU: the examples come from types, tests and code. The only binary executed was `foundry-service validate` (and `--help`), which needs no database.
