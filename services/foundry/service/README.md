# Foundry Service
Database initialisation from the Forge migration catalog.

## What it is
`foundry-service` runs as a Kubernetes Job. It creates and migrates the databases of the other services from the Forge migration catalog, so each service finds its schema in place on start.

## Use cases
- Initialise a fresh estate's databases.
- Apply migrations before a service upgrade.

## Documentation
Full documentation: [services/foundry](../../../docs/services/foundry/index.md)
