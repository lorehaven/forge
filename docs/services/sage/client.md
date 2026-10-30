---
title: Client
parent: Sage
grand_parent: Services
nav_order: 5
---

The browser client is `sage-app` compiled with `sage-web` to WebAssembly, served at `{base}/ui/assets/app.wasm`. Pattern: [The browser-client pattern](../../guides/browser-client.md). Sage and Conveyor are the two clients that use server-sent events.

## Routes (owned by the app)

| Path | Screen |
|---|---|
| `{base}/ui`, `{base}/ui/`, `{base}/ui/home`, `{base}/ui/home/` | Chat. Query parameters: `conversation_id` (open that conversation; absent means a new chat) and `project_id` (the chat belongs to that project). A new chat is given its address with `conversation_id` once stored (history `replace`). |
| `{base}/ui/initializing` | Initializing |
| anything else | not found (`ui_not_found`, link to home) |

The service serves page routes for `/ui`, `/ui/`, `/ui/home`, `/ui/home/` and `/ui/initializing`. On the chat screen, `models.ready == false` redirects (history `replace`) to `/ui/initializing`; on the initializing screen `Models` is asked again every 3 seconds until `ready`, then it redirects to `/ui/home`. At start the app asks `Sidebar`. Opening the chat asks `Models`, and `Conversation(id)` or `NewId` (plus `ProjectFiles(project)` for a project). If the opened conversation ends with the person's message and no turn runs, `Resume` is asked once.

## Request and Msg

`Client::execute(Request) -> Vec<Msg>` (JSON bodies, `Accept: application/json`, session cookie, paths under the service prefix):

| Request | Calls | Messages |
|---|---|---|
| `Models` | `GET /api/v1/models` | `Models` |
| `Sidebar` | `GET /api/v1/projects`, `GET /api/v1/conversations` | `Projects`, `Conversations` |
| `ProjectFiles(id)` | `GET /api/v1/files?project_id={id}` (404/403 give an empty list) | `ProjectFiles` |
| `NewId` | `GET /api/v1/conversation-id` | `NewId` |
| `Conversation(id)` | `GET /api/v1/conversations/{id}` | `Conversation`; `NoConversation(id)` on 404 |
| `Send(body)` | `POST /api/v1/turns` | `TurnStarted(id)`, then the host opens the stream |
| `Regenerate(message_id)` | `POST /api/v1/turns/regenerate` | `TurnStarted` |
| `Resume(id)` | `POST /api/v1/conversations/{id}/resume` | `TurnStarted` when `id` is not null |
| `Edit{conversation_id, message_id, text}` | `POST /api/v1/messages/{id}/edit`, then `GET` the conversation, then resume | `Conversation`, `TurnStarted` |
| `Switch{conversation_id, target}` | `POST /api/v1/conversations/{id}/switch`, then `GET` the conversation | `Conversation` |
| `DeleteConversation(id)` | `DELETE /api/v1/conversations/{id}`, then the conversation list | `ConversationDeleted`, `Conversations` (a 404 counts as deleted) |
| `DeleteFile(id)` | `DELETE /api/v1/files/{id}` | `FileGone` (404 too) |
| `CreateProject(body)` | `POST /api/v1/projects` | `ProjectCreated` |
| `FileStatus(id)` | `GET /api/v1/files/{id}` | `File`; `FileGone` on 404 |
| `Reprocess(id)` | `POST /api/v1/files/{id}/reprocess` | `File` (status shown as processing); a `409`/`422` re-reads the file |
| `StopStream` | none: closes the open `EventSource` | none |
| `Language(code)` | none (`localStorage["language"]`) | none |

Streaming: when `TurnStarted(id)` is delivered the host opens `EventSource {base}/api/v1/turns/{id}/stream` listening to `status`, `delta`, `replace`, `done`, `failure`, delivers each parsed event as `Msg::Stream(event)`, closes the source after `done` or `failure`, and delivers `StreamEnded` if the source closed without either. The app shows the person's message at once, appends `delta` text, replaces it on `replace`, shows `status` as a line under the answer (`ui_chat_status_<stage>`), and on `done` re-reads the conversation. See [Events](events.md).

Uploads do not go through `Client`: the composer's `<input type="file" id="composer-file-input">` carries `data-upload-url` (`{base}/api/v1/files/staged`), `data-upload-fields` (`{"conversation_id": ...}`) and `data-upload-result` (`#upload-result`); `forge-shell`'s `shell.js` posts each chosen file as multipart with the page cookies and writes `{name, status, body}` into that input, which the app parses (`upload_result`, `parse_file`). A staged chip whose status is `uploaded` or `processing` is polled with `FileStatus`.

Failures: reads: 401 `SignedOut`; 404 `Failed("the service has nothing there")`; 403 `Failed("this account may not see that")`; other `Failed("the service answered <n>")`; offline `Failed("cannot reach the service: ...")`. Writes: 401 `SignedOut`; 404 `Rejected("it is gone: someone removed it")`; 403 `Rejected("this account may not do that")`; a body that is a code (`api_error_*`, under 80 characters, no whitespace) is passed on as the rejection and translated by key; other statuses `Rejected("the service answered <n>")`. Form errors use keys: `ui_chat_error_empty`, `ui_chat_no_model_available`, `ui_projects_error_name`.

## Screens

* Chat: the thread `#thread` (messages with a user or Sage label, rendered Markdown with copy buttons on code blocks, sources list `ui_chat_sources` with `ui_chat_source_chunk`, attachment chips, branch controls `branch-<id>` with the position among alternatives and previous/next arrows, `#edit` and `#regenerate` buttons on the last messages, the edit form `#edit-form` with `#edit-cancel`/`#edit-save`); while a turn runs `#thinking`/`#streaming` and the status line, `#turn-error` on failure; `#welcome` for an empty chat; a navigator `#chat-nav` with one dot per message (tooltip = the first 30 characters); the composer `form#chat-form` with the model select (a field named by the draft), the search-provider select, `button#attach`, pending chips `#pending-attachments` (`chip-<file id>` with retry and remove), `button#send`, `#no-model` when no model is offered.
* Sidebar: `#new-chat`, Projects (`#new-project`, `#projects` with `project-<id>` items that open a project chat), the open project's files (`#project-files`, `file-item-<id>`, `#files-empty`; download, delete), History (`#history`, `history-item-<id>`, delete).
* Dialogs (`#dialog`): delete conversation (`ui_modal_delete_title`, `ui_chat_delete_confirm_text`), delete file (`ui_files_delete_confirm_text`), new project (`#new-project-form`, field name, `#project-error`, `#dialog-create`, `#dialog-cancel`, `#dialog-confirm`). The project name is trimmed, 1 to 120 characters.
* Initializing (`#init-card`): the list of default models `#model-rows` with state `Running`, `Starting…`, `Queued`, `Failed`, `Connecting…` (`data-state`) and the `(embedding)` tag; `#init-waiting` ("Waiting for the model service to respond…") while Switchboard does not answer.
* Banners: `p#problem` (`Failed`, and `SignedOut` with `a#sign-in`), `p#notice` (rejections).

## Languages and i18n keys

Locales `en-US`, `de-DE`, `es-ES`, `fr-FR`, `pl-PL` from `services/sage/service/i18n/<locale>.ftl`. Key groups in `en-US.ftl`: shell (`header_label`, `footer_label`, `locale_label`, `theme_label`, `ui_header_home`, `ui_home_button`, `ui_dashboard_button`, `ui_theme_*`, `ui_palette_*`, `ui_user_menu`, `ui_profile_edit`, `ui_logout`, `ui_menu_open`, `ui_menu_close`, `ui_close`, `ui_cancel`, `ui_nav_chat`, `ui_chat_history`); chat (`ui_chat_input_placeholder`, `ui_chat_send_button`, `ui_chat_welcome_message`, `ui_chat_welcome_tooltip`, `ui_chat_no_model_available`, `ui_chat_user_label`, `ui_chat_ai_label`, `ui_chat_thinking`, `ui_chat_regenerating`, `ui_chat_edit`, `ui_chat_regenerate`, `ui_chat_save_submit`, `ui_chat_error`, `ui_chat_sources`, `ui_chat_source_chunk`, `ui_chat_status_planning|searching|reading|checking`, `ui_chat_no_models`, `ui_chat_switchboard_unavailable`, `ui_chat_attach_tooltip`, `ui_chat_untitled`, `ui_chat_delete_confirm_text`, `ui_chat_this_conversation`, `ui_chat_error_empty`, `ui_chat_stream_lost`, `ui_code_copy`, `ui_code_copied`, `ui_code_copy_error`); sidebar (`ui_sidebar_new_chat`, `ui_sidebar_projects`, `ui_sidebar_new`, `ui_sidebar_history`, `ui_sidebar_files`); common (`ui_common_cancel`, `ui_common_delete`, `ui_modal_delete_title`); projects (`ui_projects_new_title`, `ui_projects_name_label`, `ui_projects_create`, `ui_projects_error_name`); files (`ui_file_status_ready|processing|uploaded|failed`, `ui_files_retry_tooltip`, `ui_files_remove_tooltip`, `ui_files_download_tooltip`, `ui_files_empty_project`, `ui_files_delete_confirm_text`); initializing (`ui_init_title`, `ui_init_subtitle`, `ui_init_waiting`, `ui_init_status_running|starting|queued|failed|unknown`, `ui_init_embedding_tag`); misc (`ui_home_title`, `ui_home_no_services`, `ui_home_group_services`, `ui_loading`, `ui_not_found`, `ui_session_ended`, `ui_sign_in`, `ui_login_*` left from the server-rendered login); API error codes (`api_error_internal`, `api_error_instance_not_found`, `api_error_embedding_model_chat`, `api_error_switchboard_unavailable`, `api_error_stream_failed`, `api_error_regenerate_non_assistant`, `api_error_no_parent_message`, `api_error_parent_not_found`, `api_error_no_models_available`, `api_error_metrics_not_found`, `api_error_costs_not_found`, `api_error_missing_conversation_id`, `api_error_conversation_create_failed`, `api_error_conversation_not_found`, `api_error_project_not_found`, `api_error_file_not_found`, `api_error_file_content_not_found`, `api_error_file_scope_required`, `api_error_missing_file_name`, `api_error_unsupported_file_type`, `api_error_image_not_processable`, `api_error_file_too_large`, `api_error_file_empty`, `api_error_file_limit_reached`, `api_error_file_already_processing`, `api_error_postgres_required`, `api_error_empty_message`, `api_error_message_not_found`, `api_error_edit_non_user`, `api_error_project_name`). The screen resolves any text starting with `api_error_` or `ui_` through the translations. The chat-status keys use the stage name from the `status` event.

## Styling hooks

Stylesheet `app/src/style.css`. Classes include the `sg-` prefix (for example `sg-section-body`, `sg-model-rows`). The Markdown of messages is rendered by `sage-core::markdown` into UI elements (no raw HTML is inserted); the `<div class="tool-result ...">` cards the service stores for tool calls are recognised and drawn as tool blocks ([Tools](tools.md)).
