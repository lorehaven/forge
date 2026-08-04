# forge-bdd

The workspace's single Cucumber-based BDD runner, covering every Forge service's behavior — sage, switchboard, warehouse, gatehouse, and conveyor — from one binary and one `World`. It starts the services a run needs, runs the matching scenarios against real HTTP, and tears them back down.

See [docs/tests/forge-bdd.md](../../docs/tests/forge-bdd.md) for full documentation.
