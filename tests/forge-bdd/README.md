# forge-bdd

Every service's BDD suite, behind one entry point.

```bash
cargo run -p forge-bdd                          # all services
cargo run -p forge-bdd -- --tags @gatehouse     # one service
cargo run -p forge-bdd -- --service warehouse   # same, without a tag expression
cargo run -p forge-bdd -- --tags '@sage and not @slow'
cargo run -p forge-bdd -- --name 'refresh'      # filter by scenario name
```

Or through foreman, which takes the local environment down first — the suite
starts its own services on the same ports:

```bash
foreman test                 # every suite
foreman test gatehouse       # one suite; a bare name means --service
foreman test --tags @sage    # anything starting with `-` goes to cucumber
```

The runner starts only the services a run needs, waits for them to answer, runs
the matching scenarios, and stops them again — including when a scenario fails.

Each service is a separate pass, and the run closes with a consolidated report:

```
═══ Forge BDD summary ═══════════════════════════════════════════════
suite                      scenarios                   steps   result
sage                       48 passed              171 passed   ok
sage (shutdown)              1 passed                3 passed   ok
switchboard                24 passed              116 passed   ok
warehouse                  24 passed              111 passed   ok
gatehouse                  17 passed               76 passed   ok
─────────────────────────────────────────────────────────────────────
TOTAL                     114 passed              477 passed   PASSED
═════════════════════════════════════════════════════════════════════
```

The exit code follows that table: `0` only when nothing failed, so CI can just
run the binary.

## Tags

Every feature carries its service's tag, and that tag drives both scenario
filtering and which services get started:

| Tag | Service | Port |
|---|---|---|
| `@sage` | sage-service (+ mock switchboard and vLLM) | 7777 |
| `@switchboard` | switchboard-service | 8554 |
| `@warehouse` | warehouse-service | 8443 |
| `@gatehouse` | gatehouse-service | 5443 |

Scenario-level tags compose with these, so `--tags '@warehouse and @docker'`
works once you tag the scenarios you care about.

`--service` is the direct way to say what to boot; without it the runner reads
the service names out of the `--tags` expression, and with neither it runs
everything.

## Layout

```
features/<service>/*.feature   tagged @<service>
src/world.rs                   one World for every suite
src/steps/common.rs            steps shared by all services
src/steps/<service>/           service-specific steps
src/services.rs                how each service is built, started and stopped
src/mocks.rs                   mock switchboard and vLLM, for sage
```

Cucumber allows one `World` per run, so the three previous crates' state is
unioned in `ForgeWorld`. Generic steps ("GET request is sent to …") address
whichever service the feature's `Given <service> API is available` background
step selected, which is why the same phrasing works everywhere.

## Notes for adding suites

- Tag the feature `@<service>`, and give it a `Given <service> API is available`
  background so the generic steps know where to go.
- Put steps only that service uses under `src/steps/<service>/`; if two services
  would define the same phrase, it belongs in `src/steps/common.rs` instead —
  duplicate patterns are an ambiguous-match error at runtime, not a compile
  error.
- Services run with an in-memory database and seed their own admin
  (`AUTH_BOOTSTRAP=true`), so the suite needs no Postgres. That differs from
  production, where gatehouse is the only service that seeds users.
- gatehouse ships no test certificates, so it is reached over plain HTTP here
  while the other three use HTTPS with their committed self-signed certs.
