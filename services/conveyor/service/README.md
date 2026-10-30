# Conveyor

The CI/CD service for the Forge estate. A webhook arrives, conveyor checks out the commit that triggered it, reads the `.conveyor.toml` that commit declares, and runs it — reusing the tooling the estate already has: `anvil` for builds, tests and images, `riveter` for manifests, `warehouse` as the registry, `gatehouse` for identity.

Full documentation, including the `.conveyor.toml` reference, executors, secrets, webhooks, the API and configuration, lives at [`docs/services/conveyor`](../../../docs/services/conveyor/index.md).
