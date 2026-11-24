# forge-bdd
One BDD runner for every service.

## What it is
The workspace's single Cucumber-based BDD runner, covering sage, switchboard, warehouse, gatehouse, conveyor, workbench, Daybook, Mailroom and Gauge from one binary and one `World`. It starts the services a run needs, runs the matching scenarios against real HTTP, and tears them back down.

## Use cases
- Run one service's scenarios, or all of them.
- Produce the JSON and HTML result pages Conveyor shows.

## Daily run
Conveyor runs the whole suite every day at 6 am Warsaw time (`bdd` stage of `.conveyor.toml`) in a throwaway job pod, with a Postgres sidecar for the Mailroom suite (`MAILROOM_BDD_DATABASE_URL`). The runner writes one report per suite to `QUENCH_TEST_REPORT_DIR` (`target/bdd` there, merged into `summary.json` and `index.html`); Conveyor keeps them as `bdd-summary.json` and `bdd.html` and shows the BDD card on the repository's scan page.

## Documentation
Full documentation: [guides/testing.md](../../docs/guides/testing.md)

## Security scenarios

`features/<service>/authz.feature` (`@authz`) runs forge-sectest's authorisation check against each service (identities minted from gatehouse's
test endpoint; spec in `tests/forge-sectest/authz.toml`), and `features/gatehouse/attacks.feature` (`@security`) attacks gatehouse's
sign-in flows. Both pass when the defence holds. See `tests/forge-sectest/README.md`.

`features/<service>/fuzz.feature` (`@fuzz`, `@serial`) runs forge-sectest's fuzzer on the service's own OpenAPI document and fails on a 500, a leaked
error, a stall or a client-settable privileged field.

`features/<service>/scan.feature` (`@scan`, `@serial`) runs the rest of forge-sectest's checks (anonymous access, forged tokens, traversal, headers, CORS, hostile
input, request smuggling, password-guess throttling) against the service. The authz and fuzz features need `FORGE_BDD_DATABASE_URL` for the
recipes that touch tables (`foundry apply --install conveyor --install daybook --install sage --install warehouse --install workbench`): with it
conveyor, sage and warehouse run on it too, so their whole APIs are real.
