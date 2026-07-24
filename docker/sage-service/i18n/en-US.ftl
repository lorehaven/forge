# required
header_label = Sage
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Language
theme_label = Theme

# ── Home ─────────────────────────────────────────────────────────────────────

ui_header_home = Sage
ui_home_button = Home
ui_home_title = Assistant
ui_home_no_services = Sage AI workspace is ready.
ui_home_group_services = Available Services

# ── Chat ─────────────────────────────────────────────────────────────────────

ui_chat_input_placeholder = Ask Sage anything...
ui_chat_send_button = Send
ui_chat_welcome_message = Hello! I am Sage, your AI assistant. How can I help you today?
ui_chat_no_model_available = No model is currently selected or available.
ui_chat_user_label = You
ui_chat_ai_label = Sage

# ── Chat (dynamic) ───────────────────────────────────────────────────────────

ui_chat_thinking = Sage is thinking...
ui_chat_regenerating = Sage is regenerating...
ui_chat_edit = Edit
ui_chat_regenerate = Regenerate
ui_chat_save_submit = Save & Submit
ui_chat_error = Error
ui_chat_sources = Sources
ui_chat_source_chunk = chunk {$index}
ui_chat_no_models = No models available
ui_chat_switchboard_unavailable = Switchboard unavailable
ui_chat_welcome_tooltip = Hello! I am Sage...
ui_chat_attach_tooltip = Attach a file (images, pdf, txt, csv, md, html, json, yaml, source code…)
ui_chat_untitled = New chat
ui_chat_delete_confirm_text = Are you sure you want to delete this conversation?
ui_chat_this_conversation = this conversation

ui_code_copy = Copy
ui_code_copied = Copied!
ui_code_copy_error = Error

# ── Sidebar ──────────────────────────────────────────────────────────────────

ui_sidebar_new_chat = New Chat
ui_sidebar_projects = Projects
ui_sidebar_new = New
ui_sidebar_history = History
ui_sidebar_files = Files

# ── Common ───────────────────────────────────────────────────────────────────

ui_common_cancel = Cancel
ui_common_delete = Delete
ui_modal_delete_title = Confirm Delete

# ── Projects ─────────────────────────────────────────────────────────────────

ui_projects_new_title = Create New Project
ui_projects_name_label = Project Name
ui_projects_create = Create

# ── Files ────────────────────────────────────────────────────────────────────

ui_file_status_ready = ready
ui_file_status_processing = processing
ui_file_status_uploaded = queued
ui_file_status_failed = failed
ui_files_retry_tooltip = Retry processing
ui_files_remove_tooltip = Cancel / remove
ui_files_download_tooltip = Download
ui_files_empty_project = No files uploaded for this project.
ui_files_delete_confirm_text = Are you sure you want to delete this file?

# ── Initializing ─────────────────────────────────────────────────────────────

ui_init_title = Preparing Sage
ui_init_subtitle = Launching the models Sage needs before you can start chatting.
ui_init_waiting = Waiting for the model service to respond…
ui_init_status_running = Running
ui_init_status_starting = Starting…
ui_init_status_queued = Queued
ui_init_status_failed = Failed
ui_init_status_unknown = Connecting…
ui_init_embedding_tag = (embedding)

# ── API error codes ──────────────────────────────────────────────────────────

api_error_internal = Something went wrong. Please try again.
api_error_instance_not_found = Model instance not found
api_error_embedding_model_chat = Selected model is an embedding model and cannot be used for chat
api_error_switchboard_unavailable = The model service is unavailable
api_error_stream_failed = Failed to start the chat stream
api_error_regenerate_non_assistant = Only assistant messages can be regenerated
api_error_no_parent_message = Message has no parent to regenerate from
api_error_parent_not_found = Parent message not found
api_error_no_models_available = No AI models available for regeneration
api_error_metrics_not_found = No metrics found for this profile
api_error_costs_not_found = No costs found for this user
api_error_missing_conversation_id = Missing conversation id
api_error_conversation_create_failed = Could not start the conversation
api_error_conversation_not_found = Conversation not found
api_error_project_not_found = Project not found
api_error_file_not_found = File not found
api_error_file_content_not_found = File content not found
api_error_file_scope_required = Exactly one of conversation or project must be provided
api_error_missing_file_name = Missing file name
api_error_unsupported_file_type = Unsupported file type. Allowed: images (png, jpg, webp, gif), pdf, txt, csv, md, html, json, yaml, toml, xml, and common source code files
api_error_image_not_processable = Images are not indexed for search; they are sent directly to the model with your message
api_error_file_too_large = File exceeds the maximum allowed size
api_error_file_empty = File is empty
api_error_file_limit_reached = File limit reached for this conversation/project
api_error_file_already_processing = File is already being processed
api_error_postgres_required = File storage requires a Postgres database

# ── Auth ─────────────────────────────────────────────────────────────────────

ui_login_sign_in = Sign in
ui_login_username = Username
ui_login_password = Password
ui_login_submit = Log in
ui_login_invalid_credentials = Invalid credentials
ui_logout = Log out
