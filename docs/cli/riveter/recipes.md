---
title: Recipes
parent: riveter
grand_parent: Command-line tools
nav_order: 6
---

Cluster and registry steps are *Unverified* (not run for the book); everything else was run in a scratch project.

## Try an overlay with no cluster

```bash
riveter env list
riveter -e dev list
riveter -e dev render --scope all     # writes manifests/dev-manifests.yaml
riveter -e dev validate               # offline CRD check
```

## Day-to-day deploy

```bash
riveter env set prod
riveter diff                          # kubectl diff of the mutable resources
riveter apply deployment/api          # one deployment, waits for the rollout
riveter apply --scope all namespace   # bootstrap a fresh environment's namespace
riveter prune --dry-run               # what the overlay no longer declares
```

## Roll a change through two clusters

```bash
riveter --env canary,prod diff
riveter --env canary,prod apply       # stops at the first failure; one report
```

Both overlays must pin `kube_context`.

## What changed since a release

```bash
riveter -e prod diff --against v1.4   # no cluster contacted; needs git
```

## Keep secrets in git

```bash
riveter secrets keygen --out ~/.config/riveter/age.key     # prints the public recipient
printf '%s' "$PASSWORD" | riveter secrets set overlays/prod/secrets.yaml DB_PASSWORD -r age1...
riveter secrets list overlays/prod/secrets.yaml
riveter secrets rekey overlays/prod/secrets.yaml -r age1new...
```

## Ship an overlay as a package

```bash
riveter login --scope warehouse:write
riveter -e forge pack --version-suffix '{timestamp}.{sha}'
riveter -e forge publish --skip-unchanged --version-suffix '{timestamp}.{sha}'
riveter install forge --env-file overlays/forge/.env --replicas deployment/sage=0
riveter remote versions forge
```

## Keep image tags current

```bash
riveter images                         # report
riveter images --update                # rewrite templates in place
```
