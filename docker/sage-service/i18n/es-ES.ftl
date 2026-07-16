# required
header_label = Sage
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Idioma
theme_label = Tema

# ── Inicio ───────────────────────────────────────────────────────────────────

ui_header_home = Sage
ui_home_button = Inicio
ui_home_title = Asistente
ui_home_no_services = El espacio de trabajo de Sage AI está listo.
ui_home_group_services = Servicios disponibles

# ── Chat ─────────────────────────────────────────────────────────────────────

ui_chat_input_placeholder = Pregunta lo que quieras a Sage...
ui_chat_send_button = Enviar
ui_chat_welcome_message = ¡Hola! Soy Sage, tu asistente de IA. ¿En qué puedo ayudarte hoy?
ui_chat_no_model_available = No hay ningún modelo seleccionado o disponible actualmente.
ui_chat_user_label = Tú
ui_chat_ai_label = Sage

# ── Chat (dinámico) ──────────────────────────────────────────────────────────

ui_chat_thinking = Sage está pensando...
ui_chat_regenerating = Sage está regenerando...
ui_chat_edit = Editar
ui_chat_regenerate = Regenerar
ui_chat_save_submit = Guardar y enviar
ui_chat_error = Error
ui_chat_sources = Fuentes
ui_chat_source_chunk = fragmento {$index}
ui_chat_no_models = No hay modelos disponibles
ui_chat_switchboard_unavailable = Switchboard no disponible
ui_chat_welcome_tooltip = ¡Hola! Soy Sage...
ui_chat_attach_tooltip = Adjuntar un archivo (pdf, txt, csv, md)
ui_chat_untitled = Nuevo chat
ui_chat_delete_confirm_text = ¿Seguro que quieres eliminar esta conversación?
ui_chat_this_conversation = esta conversación

ui_code_copy = Copiar
ui_code_copied = ¡Copiado!
ui_code_copy_error = Error

# ── Barra lateral ────────────────────────────────────────────────────────────

ui_sidebar_new_chat = Nuevo chat
ui_sidebar_projects = Proyectos
ui_sidebar_new = Nuevo
ui_sidebar_history = Historial
ui_sidebar_files = Archivos

# ── Común ────────────────────────────────────────────────────────────────────

ui_common_cancel = Cancelar
ui_common_delete = Eliminar
ui_modal_delete_title = Confirmar eliminación

# ── Proyectos ────────────────────────────────────────────────────────────────

ui_projects_new_title = Crear nuevo proyecto
ui_projects_name_label = Nombre del proyecto
ui_projects_create = Crear

# ── Archivos ─────────────────────────────────────────────────────────────────

ui_file_status_ready = listo
ui_file_status_processing = procesando
ui_file_status_uploaded = en cola
ui_file_status_failed = fallido
ui_files_retry_tooltip = Reintentar el procesamiento
ui_files_remove_tooltip = Cancelar / quitar
ui_files_download_tooltip = Descargar
ui_files_empty_project = No hay archivos subidos para este proyecto.
ui_files_delete_confirm_text = ¿Seguro que quieres eliminar este archivo?

# ── Inicialización ───────────────────────────────────────────────────────────

ui_init_title = Preparando Sage
ui_init_subtitle = Lanzando los modelos que Sage necesita antes de que puedas empezar a chatear.
ui_init_waiting = Esperando la respuesta del servicio de modelos…
ui_init_status_running = En ejecución
ui_init_status_starting = Iniciando…
ui_init_status_queued = En cola
ui_init_status_failed = Fallido
ui_init_status_unknown = Conectando…
ui_init_embedding_tag = (embedding)

# ── Códigos de error de la API ───────────────────────────────────────────────

api_error_internal = Algo salió mal. Inténtalo de nuevo.
api_error_instance_not_found = Instancia del modelo no encontrada
api_error_embedding_model_chat = El modelo seleccionado es un modelo de embeddings y no puede usarse para chatear
api_error_switchboard_unavailable = El servicio de modelos no está disponible
api_error_stream_failed = No se pudo iniciar el flujo del chat
api_error_regenerate_non_assistant = Solo se pueden regenerar los mensajes del asistente
api_error_no_parent_message = El mensaje no tiene un padre desde el que regenerar
api_error_parent_not_found = Mensaje padre no encontrado
api_error_no_models_available = No hay modelos de IA disponibles para regenerar
api_error_metrics_not_found = No se encontraron métricas para este perfil
api_error_costs_not_found = No se encontraron costes para este usuario
api_error_missing_conversation_id = Falta el identificador de la conversación
api_error_conversation_create_failed = No se pudo iniciar la conversación
api_error_conversation_not_found = Conversación no encontrada
api_error_project_not_found = Proyecto no encontrado
api_error_file_not_found = Archivo no encontrado
api_error_file_content_not_found = Contenido del archivo no encontrado
api_error_file_scope_required = Debe indicarse exactamente una conversación o un proyecto
api_error_missing_file_name = Falta el nombre del archivo
api_error_unsupported_file_type = Tipo de archivo no compatible. Permitidos: pdf, txt, csv, md
api_error_file_too_large = El archivo supera el tamaño máximo permitido
api_error_file_empty = El archivo está vacío
api_error_file_limit_reached = Se alcanzó el límite de archivos para esta conversación/proyecto
api_error_file_already_processing = El archivo ya se está procesando
api_error_postgres_required = El almacenamiento de archivos requiere una base de datos Postgres

# ── Inicio de sesión ─────────────────────────────────────────────────────────

ui_login_sign_in = Iniciar sesión
ui_login_username = Nombre de usuario
ui_login_password = Contraseña
ui_login_submit = Acceder
ui_login_invalid_credentials = Credenciales no válidas
ui_logout = Cerrar sesión
