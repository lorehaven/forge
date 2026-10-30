---
title: Client
parent: Conveyor
grand_parent: Services
nav_order: 5
---

The browser client: the `conveyor-app` screens compiled to WebAssembly with `conveyor-web`, served by the service at `/ui`. The pattern (crates, `Request`/`Msg`, page serving) is in [The browser-client pattern](../../guides/browser-client.md); this page is what is specific to conveyor. Sources: `services/conveyor/app/src/{live.rs,screens/*}`, `web/src/{browser.rs,client.rs}`, `core/src`.

## Host and app

`conveyor_web::app()` derives the service prefix from the page path (`/conveyor/ui/home` gives `/conveyor`), reads the language from `localStorage` key `language` (default `en-US`), builds the app with `Setup { login_url: {base}/ui/login, home_refresh_ms: 5000, run_refresh_ms: 2000, .. }` and sends every `Request` to `Client::execute`, delivering the resulting `Msg`s back. Calls are `fetch` with `Accept: application/json` (and `Content-Type: application/json` with a body) carrying the session cookie. The only streaming is the job log over `EventSource`.

Error mapping of a call: `401` becomes `Msg::SignedOut` (banner "Your session ended." with a "Sign in" link to `{base}/ui/login`); `404` on a read becomes `Msg::NotFound`; a failed read otherwise `Msg::Failed("this account may not see that: ask for access in gatehouse")` for `403`, `the service answered <status>` for other statuses, `cannot reach the service: <why>` when the request did not complete; a failed change shows the service's own `error` text (`Msg::Rejected`), or `the service answered <status>`.

## Routes

The app owns these paths (the service serves the same page for all of them; the app routes after loading). `{base}` is the prefix.

| Route | Screen | Opens with | Polling |
|---|---|---|---|
| `{base}/ui`, `/ui/`, `/ui/home`, `/ui/home/` | Pipelines: the project tree with repository tables, recent runs | `Request::Home{scope: None}` | `Request::Recent` every 5 s while the screen is shown |
| `{base}/ui/projects/{id}` | The same page rooted at project `{id}` (breadcrumb, only that subtree) | `Request::Home{scope: Some(id)}` | as above |
| `{base}/ui/runs` (query `project`, `page`) | All pipelines: paged history, optionally limited to a project | `Request::History` (re-asked when the query changes) | `Request::History` every 5 s |
| `{base}/ui/runs/{id}` | One run: header, meta, reason, restart, job graph, logs, artifacts | `Request::Run(id)` | `Request::Run(id)` every 2 s **until the run is terminal**, then stops |
| `{base}/ui/credentials` | Credentials visible to the caller | `Request::Credentials` | none |
| `{base}/ui/repos` | Manage repositories: list, follow projects, add form | `Request::Repos` | `Request::Repos` every 5 s |
| `{base}/ui/repos/{owner}/{name}/edit` | Edit or delete a repository | `Request::Repos` | none |
| `{base}/ui/repos/{owner}/{name}/run` | Run with parameters | `Request::Launch` | none |
| `{base}/ui/repos/{owner}/{name}/scan` | Scan overview: one card per check | `Request::Scan` | `Request::Scan` every 5 s |
| `{base}/ui/repos/{owner}/{name}/scan/{category}` | One check: findings, embedded report, scanner findings | `Request::Check` | none |
| anything else | "There is nothing here." with a link home | | |

Pages are rebuilt when only a parameter changes (`/runs/a` to `/runs/b`). Polls stop when the screen that started them is left. The sidebar has two items, "Pipelines" (`{base}/ui/home`, id `nav-pipelines`) and "Credentials" (`nav-credentials`), the current one marked `cv-current`; the top bar (from `forge-shell`) has the brand, a services-dashboard link, the account menu with log out (`{base}/ui/logout`) and a language picker with `en-US`, `de-DE`, `es-ES`, `fr-FR`, `pl-PL`. A banner above the page shows the current problem (`#problem`, class `fui-notice fui-notice-error`) or notice (`#notice`, `fui-notice-ok` or `-error`).

## Screens

**Pipelines (`#home`).** Header "Pipelines" (or the project's breadcrumb `#breadcrumb`: Home / ancestors / project). Panel "Recent runs" (`#recent-runs`, table `#runs`: Status pill, Repository link to the run, Ref, Commit (7 characters), Trigger, When as "3m ago") with a link "View all pipelines" (`#view-all`, to `/ui/runs` with `?project=` when scoped). Panel "Repositories" with a link "Manage repositories" (`#manage-repos`) and the tree (`#project-tree`) built by `conveyor_core::tree::build`: a node with children or repositories is a `<details open>`; each repository table has columns Repository (link to its scan page), Provider, Default branch, State (Enabled/Disabled), Checks (six chips `data-check`), Actions: **Run now** (`#run-<repo id>`, not shown for a disabled repository) and **Run with parameters** (`#launch-<repo id>`). Empty states: "No repositories are registered yet.", "Nothing has run yet.", "No such project.".

**All pipelines (`#history`).** Table of runs as above, pager (`#prev`, `#pager-status` "Page {page} of {total}", `#next`) whose links keep `project`.

**Run (`#run`).** Header (`#run-header`): status pill, repository slug (or "unknown repository"), ref, short sha, commit message. Meta (`#run-meta`): Trigger, Stages (only when the run was limited), Queued, Duration ("so far" while running), Attempt. If the run has `error`, a panel "Why it ended" (`#run-reason`). A **Restart** button (`#restart`) for `failed` or `cancelled` runs. The jobs (`#run-jobs`) are laid out by `conveyor_core::graph::stage_levels`: one row per dependency level, arrows between rows, jobs of one stage grouped in a card; each job is a `<details class="cv-job" data-job=id>` whose summary shows the pill, `stage/job`, "Reused from an earlier run" when `reused_from_run` is set, `exit <code>` when non-zero, and the duration. Opening a job that ran starts its log stream once (`Request::OpenLog`); a job that is `queued` or `skipped` shows its reason (or "this job did not run"). The log (`#log-<job id>`) has toolbar links "raw" (`#raw-<job id>`, to `{base}/api/v1/jobs/{id}/raw`) and "copy" (`#copy-<job id>`) and appends lines as they arrive, stderr lines marked; "Loading…" until the first line. Panel "Artifacts" (`#run-artifacts`) lists artifacts with a link to `href` and a short digest. There is **no cancel button**.

**Credentials (`#credentials`).** For each credential: name, scope (`project: <path>` or `repo: <owner>/<name>`), kind, username, preview, age. Empty: "No credentials visible to you.". Read-only: credentials are set through the API.

**Manage repositories (`#repos`).** Back link to home; panel "Repositories" with a table and an **Edit** link (`#edit-<id>`) and a follow button (`#follow-<id>`: "Follow" / "Following - click to stop"); panel "Follow projects" (`#follow-projects`) with a follow button per project and the explanation that results are mailed (failures and the first success after one; preferences are on the gatehouse account page); panel "Add a repository" (`#add-repo`, form `#new-repo`) with Owner, Name, Clone URL, Default branch, Project (a select of the projects the caller can write to) and Provider, and the button "Register repository" (`#create-repo`). The form is validated by `RepoDraft` before sending: owner and name required (`ui_repos_err_owner_name_empty`), a project chosen (`ui_repos_err_project_required`), a clone URL given (`ui_repos_err_bad_clone_url`). Success: banner "Repository registered." and the form is cleared.

**Edit repository (`#edit`).** The same fields for an existing repository plus an Enabled checkbox (`#enabled`), **Save changes** (`#save-repo`), and a "Danger zone" with **Delete repository** (`#delete-repo`; its text says removing a repository deletes its run history). Read-only with a note (`#view-only`) when the caller lacks write on the repository's project. After saving the screen stays on (or moves to) `/ui/repos/{owner}/{name}/edit` with the new names; after deleting it goes to the list with "Repository deleted.".

**Run with parameters (`#launch`).** Fields: branch or tag (empty means the default branch; a bare name becomes `refs/heads/<name>`), one checkbox per known stage (ticked when `on_by_default`; stages that an ordinary run leaves out are marked "normally left out of a run"), and a free-text field for other stage names (comma separated). Button "Run selected stages" (`#launch-submit`). With no known stage a hint (`#none-known`) says to name them below. Without write access the form is replaced by `#no-access`. Submit builds `TriggerRun{git_ref, sha: null, stages}` (error "Select at least one stage." when none) and opens the new run.

**Scan (`#scan`).** Header with the slug, a "Run with parameters" link (`#launch-link`), subtitle; the newest run (`#scan-run`: sha link, ref, age); a grid (`#scan-grid`) with one card per check present (`data-check`): the metric or finding count, the title, the headline and "from <sha>, <trigger>, <age>" for the run it came from; green (`cv-status-success`) when passed, red otherwise. Empty states: "This repository has not run yet." or the text telling which `anvil` steps to add.

**Check (`#check`).** Back link (`#back`), title, subtitle `<headline> - from <job> (<origin>)` (`#check-subtitle`). For `tests` and `security`: the stored report in a sandboxed iframe (`#report-frame`, `sandbox="allow-scripts"`) with "open" (`#report-open`) and "download" (`#report-download`, `?download=1`) links. Then the findings list (`#findings`: title, severity badge, location, id, date, extra) or "Nothing found.". For `security` a panel "Scanner findings" (`#scanner-findings`): level checkboxes (critical, high, medium, low, info, each with its count), **Apply** (`#apply-filter`), a count (`#findings-count`) and the list (`#scanner-list`) with severity, check, `service METHOD path` and detail; "Nothing at the selected levels." when the filter excludes everything; before Apply every level is shown.

## Request to HTTP

| `Request` | Calls | Messages |
|---|---|---|
| `Home{scope}` | `GET /api/v1/projects?all=true`, `GET /api/v1/repos`, `GET /api/v1/overview/runs[?project=]`, `GET /api/v1/scan` (a failure here is ignored) | `Projects`, `Repos`, `Recent`, `Chips` |
| `Recent{scope}` | `GET .../overview/runs`, `GET /api/v1/scan` | `Recent`, `Chips` |
| `History{scope,page}` | (with a scope) `GET /api/v1/projects?all=true`; `GET /api/v1/repos`; `GET /api/v1/overview/history?project=&page=` | `Projects`, `Repos`, `History` |
| `Run(id)` | `GET /api/v1/runs/{id}`, then `GET /api/v1/repos/{repo_id}` (failure ignored) | `Run` |
| `Restart(id)` | `POST /api/v1/runs/{id}/restart` | `Opened(new id)`: the app navigates to it |
| `RunNow{repo,scope}` | `POST /api/v1/repos/{repo}/runs` (no body), then `GET .../overview/runs` | `Recent` (a failure: `Rejected`) |
| `OpenLog(job)` / `CloseLog(job)` | `EventSource` on `{base}/api/v1/jobs/{job}/stream` / close it | `LogLine{job, stderr, line}`, `LogDone{job}` |
| `Repos` | `GET /api/v1/access` (optional), `.../projects?all=true`, `.../repos`, `.../subscriptions` (optional) | `Access`, `Projects`, `Repos`, `Subscriptions` |
| `CreateRepo(body)` | `POST /api/v1/repos`, then `GET /api/v1/repos` | `Done("ui_repos_ok_created")`, `Repos` |
| `SaveRepo{id,body}` | `PATCH /api/v1/repos/{id}`, then `GET /api/v1/repos` | `Saved{owner,name}` (banner `ui_repos_ok_saved`), `Repos` |
| `DeleteRepo(id)` | `DELETE /api/v1/repos/{id}` | `Deleted` (banner `ui_repos_ok_deleted`) |
| `Follow{target,on}` | `PUT` or `DELETE /api/v1/repos/{id}/subscription` or `/projects/{id}/subscription`, then `GET /api/v1/subscriptions` | `Subscriptions`, `Done("ui_follow_ok_followed"` or `"ui_follow_ok_unfollowed")` |
| `Launch{owner,name}` | `GET /api/v1/access` (optional), `GET /api/v1/repos`, `GET /api/v1/repos/{id}/stages` | `Access`, `Repos`, `Stages` |
| `RunStages{repo,body}` | `POST /api/v1/repos/{repo}/runs` with the body | `Opened(run id)` |
| `Scan{owner,name}` | `GET /api/v1/repos`, `GET /api/v1/repos/{id}/scan` | `Repos`, `Scan` |
| `Check{owner,name,kind}` | as `Scan`, plus for `security` `GET /api/v1/repos/{id}/scan/findings` (`404` gives `Findings{found: None}`) | `Scan`, `Findings` |
| `Credentials` | `GET /api/v1/projects?all=true`, `GET /api/v1/repos`, `GET /api/v1/credentials` | `Projects`, `Repos`, `Credentials` |
| `Language(code)` | `localStorage.language = code`; no call | |

The repository for `{owner}/{name}` is found by matching `owner` and `name` in `GET /api/v1/repos`, so the page only knows repositories the caller may read.

Not in the browser: creating, renaming, moving or deleting projects, secrets, git credentials, cancelling a run, reading schedules. Those are API-only.

## Translations

Keys in `services/conveyor/service/i18n/{en-US,de-DE,es-ES,fr-FR,pl-PL}.ftl` (Fluent), compiled into the wasm with `include_str!`. All five files have the same 145 keys (checked). Groups, with the prefix:

* Shell: `header_label`, `footer_label`, `locale_label`, `theme_label`, `ui_header_home`, `ui_home_button`, `ui_nav_pipelines`, `ui_nav_credentials`, `ui_logout`, `ui_profile_edit`, `ui_user_menu`, menu, theme and palette keys (`ui_menu_open`, `ui_menu_close`, `ui_theme_*`, `ui_palette_*`, `ui_dashboard_button`, `ui_close`, `ui_cancel`), and `ui_loading`, `ui_not_found`, `ui_session_ended`, `ui_sign_in`, `ui_unknown_repo`, `ui_job_did_not_run`.
* Home and history: `ui_home_title`, `ui_home_subtitle`, `ui_repos_title`, `ui_repos_empty`, `ui_runs_title`, `ui_runs_empty`, `ui_runs_view_all`, `ui_project_subtitle`, `ui_project_not_found`, `ui_pipelines_title`, `ui_pipelines_subtitle`, `ui_pager_prev`, `ui_pager_next`, `ui_pager_page` (arguments `$page`, `$total`).
* Runs: columns `ui_col_{status,repository,ref,commit,trigger,when,provider,branch,state,checks,actions}`, `ui_repo_enabled`, `ui_repo_disabled`, `ui_repo_run_now`, `ui_run_not_found`, `ui_run_reason`, `ui_meta_{trigger,queued,duration,attempt,stages}`, `ui_artifacts_title`, `ui_log_{loading,raw_tooltip,copy_tooltip}`, `ui_run_restart`, `ui_job_reused`, statuses `ui_status_{queued,running,success,failed,cancelled,skipped}` (success reads "Passed").
* Scan: `ui_scan_*` (subtitle, six titles, tests report open/download, no runs, no checks, repo not found, back, clean), `ui_findings_{title,apply,none}`, `ui_chip_not_run`.
* Launch: `ui_launch_*` (link, subtitle, ref, stages, normally skipped, none known, extra, submit, errors, no access).
* Repositories: `ui_repos_*` (manage, back, add, owner, name, clone url, branch, project, provider, create, save, edit, view only, delete title/hint/button, not found, ok messages, error messages), `ui_follow_*`.
* Credentials: `ui_credentials_title`, `ui_credentials_empty`.

Keys with no literal reference in the app code (some are used dynamically: the status pill builds `ui_status_<status>`, the scan cards use `ui_scan_<kind>_title`): `ui_chip_not_run` (used through `conveyor_core::scan::chip`), `ui_launch_err_failed`, `ui_follow_err_failed`, `ui_repos_err_forbidden`, `ui_repos_err_not_found`, `ui_repos_err_write_failed`, `ui_cancel`, `ui_close`, `ui_menu_*`, `ui_user_menu`, `ui_profile_edit`, `ui_header_home`, `ui_logout` and the theme and palette keys (the last ones are consumed by `forge-shell`). Strings the client composes itself are not translated: `just now`/`<n>m ago` times, `exit <code>`, and the error texts of `Client` (`cannot reach the service: ...`).

## Styling hooks

The app's stylesheet is `conveyor-app/src/style.css` (served inside `/ui/assets/app.css` after the shell styles); classes are prefixed `cv-` (`cv-page`, `cv-panel`, `cv-table`, `cv-status cv-status-<status>`, `cv-chip cv-chip-<none|clean|warning|danger>`, `cv-graph`, `cv-job`, `cv-log`, `cv-scan-card`, `cv-finding-severity-<level>`) and the shell's `fui-*`. Element ids listed above are stable test hooks (`conveyor-app/tests/screens.rs`).
