# conveyor-cli
Terminal client of the Conveyor CI/CD service.

## What it is
`conveyor-cli` installs the `conveyor` binary. It registers repositories, starts and inspects runs, streams logs, and manages projects, secrets and git credentials through Conveyor's API. It signs in to Gatehouse when a username is given.

## Use cases
- Register a repository and trigger a run by hand.
- Follow a run's logs from a terminal.
- Manage project secrets and git credentials.
- Validate a `.conveyor.toml` offline (`conveyor validate`).

## Example
```sh
export CONVEYOR_URL=https://localhost:9443/conveyor
conveyor runs --limit 5
conveyor run my-repo --wait
```

## Documentation
Full documentation: [cli/conveyor-cli](../../docs/cli/conveyor-cli/index.md)
