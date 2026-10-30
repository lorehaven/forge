---
title: Client
parent: Gantry
grand_parent: Services
nav_order: 5
---

The browser client: the `gantry-app` screens compiled to WebAssembly with `gantry-web`, served at `/ui`. The pattern is in [The browser-client pattern](../../guides/browser-client.md). Sources: `services/gantry/app/src/{live.rs,screens.rs}`, `web/src/{browser.rs,client.rs}`, `core/src`.

## Host and app

`gantry_web::app()` derives the service prefix from the page path, reads the language from `localStorage` key `language` (default `en-US`), builds the app with `Setup { login_url: {base}/ui/login, .. }` and sends each `Request` to `Client::execute`. Calls are `fetch` with `Accept: application/json` and the session cookie. There is no streaming and **no periodic polling except one**: a running operation's page asks for the operation again every **3000 ms** until it is over (`POLL_MS`). All other screens load once when opened (a reload or navigation refreshes them). On start the app sends `Request::Startup` (`GET /api/v1/info`) once; a `401` there shows the "session ended" banner, other failures are ignored.

Error mapping: `401` becomes `SignedOut` (banner "Your session ended." with a "Sign in" link); `404` on a read is `NotFound` (page "Nothing here."); other read failures show the service's own `error` text (so a `502` from a cluster or registry that cannot be reached says why) or `the service answered <status>`; `cannot reach the service: <why>` when the request did not complete; a failed change shows the service's `error` text as a banner (`Rejected`). Starting a change navigates to its operation page.

## Routes

`{base}` is the prefix.

| Route | Screen | Opens with | Notes |
|---|---|---|---|
| `{base}/ui`, `/ui/`, `/ui/home`, `/ui/home/` | Packages (applications) | `Request::Packages` | no polling |
| `{base}/ui/apps/{name}` | One application: resources and deployments | `Request::Package(name)` | |
| `{base}/ui/targets/{name}` | A package's versions and what runs | `Request::Target(name)` | |
| `{base}/ui/operations`, `/ui/operations/` | History | `Request::Operations` | the server registers only the version without the trailing slash; loading the slash form directly is a server `404`, in-app navigation works |
| `{base}/ui/operations/{id}` | One operation | `Request::Operation(id)` | polls every 3 s until terminal |
| `{base}/ui/plans/{id}` | A stored plan | `Request::Plan(id)` | |
| `{base}/ui/resource?package=&kind=&name=&namespace=&api_version=` | The YAML editor | `Request::Yaml(resource)` | without package, kind and name: "Nothing here." |

The sidebar has two items: "Packages" (`{base}/ui/home`, `#nav-packages`) and "History" (`{base}/ui/operations`, `#nav-history`). The top bar is the shared shell (brand, dashboard link, account menu, log out `{base}/ui/logout`, language picker with the five locales). The banner shows, in order of precedence, the problem (`#problem`), the notice (`#notice`) or a pending question (`#confirm`: the question text `#confirm-question`, buttons `#confirm-yes` "Confirm" and `#confirm-no` "Cancel").

## Screens

**Packages (`#packages`).** One card per application, linking to `/ui/apps/<package>`: name, a sync badge (`in sync`, `out of sync`, `missing`, `not installed`, `unpublished`), the running version with `→ <latest>` when an update is on offer, the description, the resource count and notes for what is missing, edited, not in package and not ready. Above the cards: filter chips (`#chip-all` and one per sync state that has packages, each with its count; `#chip-<state>`) and a name filter (`#home-q`). Order: applications that are not `synced` first, then by name. A banner `#busy` links to the operation in progress ("`<title>` …"), taken from the newest 10 operations (a caller without blanket read does not get it). Empty: "Nothing published yet." (`#no-packages`); "Nothing matches." (`#no-match`).

**Application (`/ui/apps/{name}`).** Header with the package, description, `<installed> → <latest>`, sync badge, and the package actions (shown by permission): **Upgrade** (`#upgrade`, `deploy`, only when an update is offered; asks "Upgrade <package> to <version>?"), **Apply missing** (`#apply-missing`, `scale`, only when something is missing), **Refresh** (`#refresh`, only when there is no inventory yet and the package is installed), **Versions** (`#versions`, link to the target page). State counts (`#count-<state>`) that act as filters. A **deployments** strip (`#deployments`) for the package's *declared* deployments only (a package with no `[[deployment]]` shows no strip): a dot for the observed state (`running`, `stopped`, `partial`, `not installed`), the name, `⇄ conflicts`, and one button: **Stop** (`#stop-<name>`, asks "Stop <name>? Its workloads are deleted.") when something is up, **Start** (`#start-<name>`) when installed and not all up, both only with `scale` on the package. The resource table (`#resources`, rows `data-resource=<Kind>-<name>`): Kind, Name, Namespace, State badge (`synced`, `edited`, `missing`, `not in package`, `secret`) with the ready count, and buttons by permission and state: **Apply** (`#apply-<tag>`, `scale`, only `missing`), **Edit** (`#edit-<tag>`, `deploy`, only `editable`; a link to the editor), **Delete** (`#delete-<tag>`, `scale`, not for `missing` or `hidden`; asks "Delete <kind>/<name>?"). Filters: kind select (`#app-kind`), state select (`#app-state`), name box (`#app-q`) and **Clear** (`#clear`), with a tally (`#tally`). Empty states `#no-units`, `#no-match`.

**Target (`/ui/targets/{name}`).** The versions line (`#versions-line`), a form `#sync-form` with a select (Latest, or a version) and **Sync** (`#sync`): puts the package at that version (plan and confirm in one go; `deploy`, or `rollback` for an older version), the running units (`#units`: Kind, Name, Version, Ready; `#no-units` "Nothing running."), and the published versions (`#published`: Version, Description, `yanked`).

**History (`#operations`).** Table of the newest 100 operations: Operation (link), State badge, By, When. Empty: "No operations yet." (`#no-operations`).

**Operation.** Title (`#operation-title`), state badge, `<requested by> · <time>`, the error (`#operation-error`), a collapsed list of the steps in words (`#steps`), the log (`#log`: the runner's live output while running, the stored log after; `—` when empty) and, while not over and with blanket `deploy`, a **Cancel** button (`#cancel`).

**Plan.** Plan action (`#plan-action`), the summary lines (`#summary`), the steps in words (`#steps`) and, depending on the plan and the caller: a link "View the operation →" when already confirmed (`#confirmed`), a **Run** button (`#confirm-plan`) when `can_confirm`, else "You can view this plan but not run it." (`#not-allowed`). Plans are reached by link (the UI itself creates and confirms plans in one step for start, stop and sync, then opens the operation); they are an API feature the UI can display.

**Resource editor.** Title `<Kind>/<name>` (`#resource-title`), a text area with the live YAML (`#edit-form`), a note ("Applied to the cluster as written. The next install of the package overwrites it."), and **Apply** (`#save`) when `editable` (read-only otherwise). The draft is checked locally (`EditDraft`) before it is sent.

## Request to HTTP

| `Request` | Calls | Messages |
|---|---|---|
| `Startup` | `GET /api/v1/info` | `Info` |
| `Packages` | `GET /api/v1/resources`; `GET /api/v1/operations?limit=10` (failure ignored; the first non-terminal one becomes the busy banner) | `Groups`, `Busy` |
| `Package(name)` | `GET /api/v1/resources`; `GET /api/v1/deployments` (failure gives an empty list); no group named so: `NotFound` | `Groups`, `Deployments` |
| `Target(name)` | `GET /api/v1/targets/{name}` | `Target` |
| `Operations` | `GET /api/v1/operations?limit=100` | `Operations` |
| `Operation(id)` | `GET /api/v1/operations/{id}` | `Operation` |
| `Plan(id)` | `GET /api/v1/plans/{id}` | `Plan` |
| `Yaml(resource)` | `GET /api/v1/resources/yaml?package=&apiVersion=&kind=&namespace=&name=` | `Yaml` |
| `Start(name)`, `Stop(name)` | `POST /api/v1/deployments/{name}/plan` `{action, stop: []}` then `POST /api/v1/plans/{id}/confirm` | `Started(operation id)`: the app opens it |
| `Sync{package,version}` | `POST /api/v1/targets/{package}/plan` `{version}` then `POST /api/v1/plans/{id}/confirm` | `Started` |
| `ApplyResource(r)` | `POST /api/v1/resources/apply` | `Started` |
| `ApplyMissing(p)` | `POST /api/v1/resources/apply` `{package}` | `Started` |
| `Refresh(p)` | `POST /api/v1/resources/refresh` `{package}` | `Started` |
| `DeleteResource(r)` | `POST /api/v1/resources/delete` | `Started` |
| `Edit{resource,yaml}` | `POST /api/v1/resources/edit` | `Started` |
| `Cancel(id)` | `POST /api/v1/operations/{id}/cancel`, then `GET /api/v1/operations/{id}` | `Operation`, `Done("cancelling")` |
| `Confirm(id)` | `POST /api/v1/plans/{id}/confirm` | `Started` |
| `Language(code)` | `localStorage.language = code` | |

Because a start with something to stop first is planned and confirmed in one go, the UI's **Start** runs a swap without a separate question (the package manifest's documentation says a start "asks for confirmation"; the confirm step there is the plan, which the UI auto-confirms).

## Translations

`services/gantry/service/i18n/{en-US,de-DE,es-ES,fr-FR,pl-PL}.ftl` (Fluent), compiled into the wasm; all five files have the same 118 keys (checked). Groups: shell and theme keys (`ui_header_home`, `ui_nav_targets` "Packages", `ui_nav_operations` "History", `ui_menu_*`, `ui_theme_*`, `ui_palette_*`, `ui_logout`, ...); home (`ui_home_no_targets`, `ui_filter_*`, `ui_sync_{synced,out_of_sync,missing,not_installed,unlisted}`); columns (`ui_col_*`); target status (`ui_status_{not_installed,current,update_available,ahead,mixed,unlisted}`); operation states (`ui_state_{queued,running,succeeded,failed,rolled_back,cancelled}`: `succeeded` reads "done"); observed (`ui_observed_*`); actions (`ui_action_*`: start, stop, swap, install, upgrade, reinstall, downgrade, edit, delete, apply, apply_missing, sync, refresh, filter, clear, versions); resource states (`ui_rstate_{synced,edited,missing,extra,hidden}`: `extra` reads "not in package", `hidden` reads "secret"); plan (`ui_plan_*`); operation (`ui_op_log`, `ui_op_cancel`); editor (`ui_edit_save`, `ui_edit_note`); confirmations (`ui_confirm_yes`, `ui_confirm_upgrade` with `$package` and `$version`, `ui_confirm_stop` with `$name`, `ui_confirm_delete` with `$kind` and `$name`); session (`ui_loading`, `ui_not_found`, `ui_session_ended`, `ui_sign_in`).

Keys with no literal reference in the app code: the target-status keys `ui_status_*`, the operation-state keys `ui_state_*`, `ui_observed_*`, `ui_sync_*` and `ui_rstate_*` are built with `format!` from the API's values (the badge function), so they are used; the ones that appear unused are `ui_action_install`, `ui_action_reinstall`, `ui_action_downgrade`, `ui_action_swap`, `ui_action_filter`, `ui_col_conflicts`, `ui_col_installed`, `ui_col_latest`, `ui_expected`, `ui_home_title`, `ui_ops_title`, `ui_target_plan_button` (planned screens that the one-step flow replaced), plus keys consumed by `forge-shell`. Strings built by the client itself are not translated: `cannot reach the service: ...`, `—`, the `⇄` and `→` marks, and `cancelling`.

## Styling hooks

The stylesheet is `gantry-app/src/style.css`, classes prefixed `gt-` (`gt-page`, `gt-grid`, `gt-app`, `gt-chip`, `gt-dep`, `gt-log`, `gt-list`, `gt-btn`, `gt-state`) beside the shell's `fui-*`; a badge is `<span class="gt-state gt-state-<value>" data-state="<value>">` with a dot and the translated word (key `ui_<kind>_<value>`). The ids above are the test hooks of `gantry-app/tests/screens.rs`.
