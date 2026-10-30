---
title: Recipes
parent: conveyor-cli
grand_parent: Command-line tools
nav_order: 3
---

Verified: `validate`. The rest talks to a Conveyor service and is *Unverified* (not run for the book).

## Check a pipeline before pushing

```bash
conveyor validate            # .conveyor.toml in the current directory
```

## Connect

```bash
export CONVEYOR_URL=https://host/conveyor
export GATEHOUSE_URL=https://host/gatehouse
export CONVEYOR_USERNAME=me
read -rs CONVEYOR_PASSWORD; export CONVEYOR_PASSWORD
```

## Register a repository and run it

```bash
conveyor project add forge
conveyor project list                                   # find the project id
conveyor repo add owner/name https://git.example/owner/name.git --project <project-id>
conveyor run owner/name --wait                          # exits non-zero if the run fails
conveyor runs --repo owner/name --limit 1               # run id is the last column
conveyor show <run-id>
conveyor logs <run-id>
```

## Run one stage by hand

```bash
conveyor run owner/name --stage security
conveyor run owner/name --git-ref v1.2 --stage build,test
```

## Follow a job live

```bash
conveyor show <run-id>        # job ids are printed under each job
conveyor logs <job-id> -f
```

## Secrets and checkout credentials

```bash
printf '%s' "$TOKEN" | conveyor secret set REGISTRY_TOKEN --repo owner/name
conveyor secret list --repo owner/name
printf '%s' "$GIT_TOKEN" | conveyor credential set deploy --git-username x-access-token --project <project-id> 
```
