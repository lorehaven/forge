---
title: Client
parent: Workbench
grand_parent: Services
nav_order: 5
---

The browser client is `workbench-app` (screens, protocol) compiled with `workbench-web` to WebAssembly and served at `{base}/ui/assets/app.wasm`. Pattern: [The browser-client pattern](../../guides/browser-client.md). Workbench is that guide's reference service.

## Routes (owned by the app)

| Path | Screen | On entry the app asks for |
|---|---|---|
| `{base}/ui`, `{base}/ui/`, `{base}/ui/home`, `{base}/ui/home/` | Projects | `Projects` |
| `{base}/ui/projects/:id/board` | Board | `Board(id)` |
| `{base}/ui/issues/:id` | Issue | `Issue(id)` |
| anything else | not found (`ui_not_found`) | |

On start (once) the app asks `Startup`. The service serves page routes for `/ui`, `/ui/`, `/ui/home`, `/ui/home/`, `/ui/projects/{id}/board` and `/ui/issues/{id}`. There is no polling: a screen is refreshed by navigating to it, by a change the person makes, and by a refused move (the board is asked for again).

## Request and Msg

`workbench_web::client::Client::execute(Request) -> Vec<Msg>` (all calls `Accept: application/json`, `Content-Type: application/json` with a body, session cookie; paths are under the service prefix; ids are percent-encoded):

| Request | Calls | Messages |
|---|---|---|
| `Startup` | `GET /api/v1/me`, `GET /api/v1/users` (each failure other than 401 is silently skipped) | `Me(username)`, `Users(list)`; `SignedOut` on 401 |
| `Projects` | `GET /api/v1/projects` | `Projects(list)` |
| `Board(id)` | `GET /api/v1/projects/{id}`, `GET /api/v1/projects/{id}/issues` | `Board{project, issues}` |
| `Issue(id)` | `GET /api/v1/issues/{id}`, `GET /api/v1/projects/{project_id}`, `GET /api/v1/issues/{id}/comments`, `GET /api/v1/issues/{id}/links` | `Issue{issue, project, comments, links}` |
| `CreateProject(body)` | `POST /api/v1/projects` | `ProjectCreated(project)` (the app adds it to the sidebar and opens its board) |
| `CreateIssue{project, body}` | `POST /api/v1/projects/{id}/issues` | `IssueCreated(issue)` (added to the open board) |
| `Move{issue, status}` | `POST /api/v1/issues/{id}/transition` | none on success (the screen already shows the move) |
| `Save{issue, body, status}` | `PUT /api/v1/issues/{id}`; then, when the form changed the status, `POST .../transition` | the reopened `Issue` |
| `Comment{issue, body}` | `POST /api/v1/issues/{id}/comments` | the reopened `Issue` |
| `AddLink{issue, key, kind}` | `GET /api/v1/issues?key={key}`, then `POST /api/v1/issues/{id}/links` with `{linked_issue_id, kind}` | the reopened `Issue`; `Rejected("there is no issue <key>")` when the key is unknown |
| `RemoveLink{issue, link}` | `DELETE /api/v1/issue-links/{link}` | the reopened `Issue` |
| `Language(code)` | none (stored in `localStorage["language"]`) | none |

Reads that fail: 401 `SignedOut`; 404 `NotFound`; 403 `Failed("this account may not see Workbench: ask for access in gatehouse")`; other `Failed("the service answered <n>")`; no network `Failed("cannot reach the service: <why>")`. Writes that fail: 401 `SignedOut`; 404 `Rejected("it is gone: someone removed it")`; any other status `Rejected(<error text from the body's {"error"} or "the service answered <n>")`; no network `Failed(...)`. `Rejected` shows `p#notice` and, if a board is open, asks for the board again.

Not used by the client although the API has them: project update and delete, issue delete, labels (create, list, attach, detach, delete), comment delete, issue `parent_id`, the `status` filter.

## Screens

* Projects (`screens::home`): header with `button#new-project-button`; the form `form#new-project` (fields `project-key` with maxlength 10, `project-name`, `project-description`, buttons `#cancel-project`, `#create-project`) validated by `ProjectDraft` (error: "a key and a name are required"); cards per project (`a.wb-card.wb-project[data-project=KEY]`, text `KEY — Name`, description); `p#no-projects` when empty; `p#waiting` before the first answer.
* Board (`screens::board`): `h1#project-title`, `button#new-issue-button`, the form `form#new-issue` (title, kind, priority, assignee with "Me", estimate; a Cancel button without id and `#create-issue`), one column per status in the order blocked, todo, in-progress, done, rejected (`[data-status]`, `data-drop-value`), a card per issue (`[data-issue]`, number `#seq`, title link to the issue, kind and priority badges, estimate `N pt`, a status select `select#status-{issue id}`). Changing the select moves the card at once and sends `Move`. Cards are `draggable`; `forge-shell`'s script turns a drop on a column into the select's change.
* Issue (`screens::issue`): breadcrumb (`Projects / KEY — Name (a#crumb-project) / KEY-n`), `h1#issue-title`, details form (`#details`: title, description, status, kind, priority, assignee with `button#assign-me`, estimate; `button#save-issue`), dependencies (`#dependencies`: three lists Blocks, Blocked by, Relates to, each row a link to the issue with remove button; the add form `#add-link` with key and type and `button#add-link-button`), comments (`#comments`, `p#no-comments`, form `#add-comment`, `button#add-comment-button`).
* Not found: `p#not-found`.

Form validation messages (English, not translated): "a key and a name are required", "a title is required", "the estimate must be a whole number, 0 or more", "a comment cannot be empty", "an issue key looks like WB-4". The banners `p#problem` (`Failed`, and `SignedOut` with `a#sign-in` and the untranslated text "Your session ended. Sign in") and `p#notice` (`Rejected`) are untranslated too.

## Sidebar and shell

The sidebar has the Projects item (icon `columns`, `ui_nav_projects`) and, under it, one sub-item per project (`a.fui-nav-subitem[data-project=KEY]`, current one marked `aria-current=page` and `wb-current`). The top bar is `forge_shell::Bar` (brand `Workbench`, icon `list-check`, dashboard button, language picker, logout `{base}/ui/logout`).

## Languages and i18n keys

Locales `en-US`, `de-DE`, `es-ES`, `fr-FR`, `pl-PL` from `services/workbench/service/i18n/<locale>.ftl`. English keys: `header_label`, `footer_label`, `locale_label`, `theme_label`; home `ui_header_home`, `ui_home_button`, `ui_home_title`, `ui_home_no_projects`, `ui_home_new_project`, `ui_home_create_button`; board `ui_board_new_issue`, `ui_board_create_button`, `ui_board_back`, status column titles `ui_status_blocked|todo|in_progress|done|rejected` (key built by `status_key`: dashes become underscores); issue `ui_issue_save_button`, `ui_issue_comments`, `ui_issue_empty_comments`, `ui_issue_add_comment_button`, `ui_issue_dependencies`, `ui_issue_blocks`, `ui_issue_blocked_by`, `ui_issue_relates_to`, `ui_issue_no_links`, `ui_issue_add_link_button`, `ui_issue_details`; fields `ui_field_key|name|description|title|kind|priority|assignee|status|comment_body|unassigned|estimate|link_target|link_kind`, `ui_assign_to_me`; shell and misc `ui_login_sign_in`, `ui_login_username`, `ui_login_password`, `ui_login_submit`, `ui_login_invalid_credentials` (left from the server-rendered login, not used by the client), `ui_logout`, `ui_profile_edit`, `ui_user_menu`, `ui_menu_open`, `ui_menu_close`, `ui_theme_toggle`, `ui_close`, `ui_cancel`, `ui_nav_projects`, `ui_dashboard_button`, `ui_theme_mode`, `ui_theme_light`, `ui_theme_dark`, `ui_theme_palette`, `ui_palette_green|teal|blue|purple|rose|orange`, `ui_loading`, `ui_not_found`, `ui_sign_out`, `ui_remove`. The kind and priority names shown in selects and badges (`task`, `bug`, `story`, `low`, `medium`, `high`) and the user label "Me" are not translated.

## Styling hooks

Root `div#workbench.fui-app.fui-client`, content `main.wb-main`. Classes: `wb-page`, `wb-head`, `wb-grid`, `wb-card`, `wb-project`, `wb-form`, `wb-input`, `wb-small`, `wb-btn`, `wb-primary`, `wb-actions`, `wb-ticket`, `wb-ticket-title`, `wb-key`, `wb-meta`, `wb-muted`, `wb-crumbs`, `wb-current`, and badges with tones from `badge(text, tone)`.
