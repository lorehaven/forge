---
title: Recipes
parent: toolbox
grand_parent: Command-line tools
nav_order: 3
---

Not run for the book (the interface queries the registry); `--help` and `--version` were verified.

## Check what is out of date

```bash
forge-toolbox        # look at the "updatable" column; press q to leave
```

## Update everything that is behind

Select each row marked `update` with the arrow keys and press `Enter`; wait for `status: <package> updated` before moving on (only one action runs at a time).

## Update Toolbox itself

The `note:` line suggests `forge-toolbox self-update`, which does not exist. Use:

```bash
cargo install forge-toolbox --registry ennor --force
```

## Install from a checkout instead

```bash
anvil install -p anvil
anvil install --all          # every package listed in [install].packages
```
