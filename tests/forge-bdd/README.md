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
