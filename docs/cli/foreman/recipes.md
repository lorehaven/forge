---
title: Recipes
parent: foreman
grand_parent: Command-line tools
nav_order: 4
---

Verified in a scratch directory: the first block. The rest start processes and are *Unverified* (not run).

## Start a new estate file

```bash
mkdir demo && cd demo
foreman init
foreman list
foreman env web
foreman status
```

## Daily use

```bash
foreman                   # everything (containers, migrations, services)
foreman start web         # web and what it needs
foreman status
foreman logs web
foreman stop              # services only; the database container stays
foreman stop all          # services and containers
```

## Schema work without services

```bash
foreman db                # database up, catalog installed
foreman reset             # drop and reinstall (needs a role = "reset" task)
```

## Run the tests

```bash
foreman test              # stops the estate if it is up, runs [test].command
foreman test web          # adds [test].service_arg for web
foreman test --no-fail-fast      # flags go to the suite command unchanged
```

## Pick services by hand

```bash
foreman repl web          # opens with web ticked; type `up`
```

## See what a service gets

```bash
foreman env api           # merged env, build command, health URL (values in clear text)
```
