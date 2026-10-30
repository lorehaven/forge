---
title: Commands
parent: conveyor-cli
grand_parent: Command-line tools
nav_order: 1
---

Status lines are `conveyor <label> <message>` style (`print_status`), tables are plain `println!` rows. All requests go to `{url}/api/v1{path}`; the paths below are relative to that. A *repo reference* is `owner/name` (resolved with `GET /repos`; `<ref> is not registered with conveyor` if unknown) or a repository id (anything without `/`). A *project reference* is a project id, taken as given.

## repo

| Command | Arguments/flags | Request | Output |
|---|---|---|---|
| `repo add <SLUG> <CLONE_URL>` | `--project <PROJECT>` (required), `--provider` (default `github`; `github` or `generic`), `--default-branch` (default `master`) | `POST /repos` `{provider, owner, name, clone_url, default_branch, project_id}` | `registered owner/name (<id>)`. `SLUG` must be `owner/name` (`expected owner/name, got x`). |
| `repo list` | | `GET /repos` | `repos none registered`, or per repo `owner/name` (40 cols), provider (9), `enabled`/`disabled` (8), id. |
| `repo enable <REPO>` | | `POST /repos/{id}/enabled` `{enabled:true}` | status line |
| `repo disable <REPO>` | | same with `false` | Turns it off, keeping history. |
| `repo move <REPO> --project <PROJECT>` | `--project` required | `PATCH /repos/{id}` `{project_id}` | `moved <repo>` |
| `repo set-branch <REPO> <BRANCH>` | | `PATCH /repos/{id}` `{default_branch}` | `default branch <branch>`. The branch need not exist yet. |
| `repo remove <REPO>` | | `DELETE /repos/{id}` | `removed <repo>`. Removes everything it built. |

## run

```text
conveyor run <REPO> [--git-ref <REF>] [--sha <SHA>] [--stage <STAGE>]... [--wait]
```

| Flag | Meaning |
|---|---|
| `--git-ref <REF>` | Branch or tag; default the repository's default branch. |
| `--sha <SHA>` | Commit; default what the ref points at. |
| `--stage <STAGE>` | Run only this stage whatever its `when` says. Repeat, or comma-separate (`--stage a,b`); empty entries are dropped. |
| `--wait` | Poll the run every 750 ms until it rests; print each new status. |

`POST /repos/{id}/runs` with `{git_ref?, sha?, stages?}`. Prints the status line `queued <ref> at <sha7>` and then the run id on its own line. Status lines are written with `print_status` (standard output as far as the source shows), so the whole output, not just the id, is on stdout: take the last line (`tail -n1`) when scripting without `--wait`. With `--wait`: `run <status>` lines, exit 0 for `success` or `skipped`, exit 1 with `run failed[: reason]` / `run cancelled[: reason]`.

## runs

```text
conveyor runs [--repo <REPO>] [--limit <N>]
```

`GET /runs?limit=<N>[&repo_id=<id>]`, `--limit` default `20`. Rows: status (10), git ref (28), sha7 (10), run id; `runs none yet` when empty.

## show

```text
conveyor show <RUN_ID>
```

`GET /runs/{id}`. Prints `<status> <ref> at <sha7>`, the run's error if any, `jobs:` with rows `status  stage/name  error` and the job id underneath, and `artifacts:` with `name  uri` when present.

## logs

```text
conveyor logs <ID> [-f | --follow]
```

`<ID>` is a run id (each job printed in order, with a `job <stage/name>` header) or a single job id (tried as a run first; if `GET /runs/{id}` fails it is treated as a job). Without `--follow`: `GET /jobs/{id}/logs`, printing each stored line; `stderr` lines go to the CLI's stderr, everything else to stdout. With `--follow`: `GET /jobs/{id}/stream` (server-sent events) printed as they arrive until an `event: done`.

## cancel

```text
conveyor cancel <RUN_ID>
```

`POST /runs/{id}/cancel`; prints `cancelling <id>`.

## project

| Command | Arguments/flags | Request |
|---|---|---|
| `project add <NAME>` | `--parent <PARENT>` (omit for a root project) | `POST /projects` → `<name> (<id>)` |
| `project list` | `--parent <PARENT>` | `GET /projects[?parent_id=<id>]`; rows `name (30 cols)  id`; `projects none` |
| `project show <PROJECT>` | | `GET /projects/{id}`; prints the full path, `id:`, `parent:` (`(root)` if none) |
| `project rename <PROJECT> <NAME>` | | `PATCH /projects/{id}` |
| `project move <PROJECT>` | `--parent <PARENT>` or `--to-root` (conflict with each other; neither: `pass --parent or --to-root`) | `PATCH /projects/{id}` |
| `project remove <PROJECT>` | | `DELETE /projects/{id}`; the project must have no children and no repository |

Repositories must live under a project: `repo add` requires `--project`, so create one with `project add` first.

## secret

| Command | Arguments/flags | Request |
|---|---|---|
| `secret set <NAME> [VALUE]` | `--repo <REPO>` scope; without `VALUE` it is read from standard input (all of stdin, trailing `\n`s removed) | `PUT /secrets/{name}` or `PUT /repos/{id}/secrets/{name}` with `{"value": ...}`; `set <name>` |
| `secret list` | `--repo <REPO>` | `GET /secrets` or `GET /repos/{id}/secrets`; rows `name (30)  set by <user>`; `secrets none set`. Values are never returned. |
| `secret remove <NAME>` | `--repo <REPO>` | `DELETE` on the same path; `removed <name>` |

Without `--repo` a secret is estate-wide.

## credential

The git credential a project or repository checks out with.

| Command | Arguments/flags | Request |
|---|---|---|
| `credential set <NAME> [TOKEN]` | `--git-username <USER>` (required: `x-access-token` for GitHub fine-grained tokens, `oauth2` for GitLab), `--repo <REPO>` or `--project <PROJECT>` (exactly one); token from stdin if omitted | `PUT /repos/{id}/credentials` or `PUT /projects/{id}/credentials` `{name, username, token}`; `set <name> (<preview>)` |
| `credential show` | `--repo` or `--project` | `GET` same path; `<name>  <preview> as <user>, set by <who>`, or `credential none set`. The token is never returned. |
| `credential remove` | `--repo` or `--project` | `DELETE`; `removed the credential` |

Errors: `a credential needs a scope: pass --repo or --project`; `pass --repo or --project, not both`. The username flag is `--git-username` because the global `--username` is the Conveyor login account.

## validate

```text
conveyor validate [PATH]
```

`PATH` defaults to `.conveyor.toml`. Parses the file with the same parser the service uses (`conveyor_pipeline::parse`), without any network access or configuration (no URL needed). Verified:

```text
$ conveyor validate forge/.conveyor.toml
valid 4 stage(s), 7 job(s)
  check
    format (1 step(s))
    lint (1 step(s))
    deps (3 step(s))
  build  needs check
    workspace (1 step(s))
  test  needs build
    unit (1 step(s))
    coverage (4 step(s))
  security
    nightly (1 step(s))
$ conveyor validate nope.toml
conveyor could not read nope.toml
$ conveyor validate b.toml        # file containing "[x]"
invalid b.toml
conveyor unknown field `x`, expected one of `on`, `stage`
```

Exit 0 when valid, 1 otherwise. The pipeline format (top-level `on` and `[[stage]]` tables with `[[stage.job]]`) belongs to the `conveyor-pipeline` library.
