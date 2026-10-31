# foreman
Runs a project's local development estate from one TOML file.

## What it is
`foreman` reads `foreman.toml` and starts what the project needs: Docker containers the services sit on, tasks that must run first (schema install), and the services themselves from their built binaries, with health checks, pid files and logs.

## Use cases
- Start, stop and inspect a multi-service estate with one command.
- Follow one service's log.
- Run named tasks (database install, tests) against the estate.
- Pick which services to run in an interactive selector.

## Example
```sh
foreman            # start everything
foreman status
foreman logs gatehouse
foreman stop all
```

## Documentation
Full documentation: [cli/foreman](../../docs/cli/foreman/index.md)
