# forge-bdd
One BDD runner for every service.

## What it is
The workspace's single Cucumber-based BDD runner, covering sage, switchboard, warehouse, gatehouse, conveyor, workbench and Daybook from one binary and one `World`. It starts the services a run needs, runs the matching scenarios against real HTTP, and tears them back down.

## Use cases
- Run one service's scenarios, or all of them.
- Produce the JSON and HTML result pages Conveyor shows.

## Documentation
Full documentation: [guides/testing.md](../../docs/guides/testing.md)
