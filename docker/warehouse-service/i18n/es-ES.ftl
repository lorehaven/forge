# required
header_label = Warehouse
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Idioma
theme_label = Tema

# ── Inicio ───────────────────────────────────────────────────────────────────

ui_header_home = Warehouse
ui_home_button = Inicio
ui_home_title = Servicios
ui_home_no_services = No hay servicios habilitados actualmente.
ui_home_group_services = Servicios de registro
ui_home_group_files = Almacenes de archivos

ui_service_docker_title = Registro Docker
ui_service_docker_desc = Explora imágenes, etiquetas y manifiestos.

ui_service_crates_title = Registro de crates
ui_service_crates_desc = Explora los crates publicados y sus versiones.

ui_service_files_title = Almacén de archivos
ui_service_files_desc = Explora y gestiona archivos.

# ── Docker ───────────────────────────────────────────────────────────────────

ui_header_docker = Warehouse — Explorador de repositorios Docker
ui_repositories = Repositorios
ui_tags = Etiquetas
ui_tags_for = Etiquetas de
ui_metadata = Metadatos
ui_metadata_for = Metadatos de
ui_col_tag = Etiqueta
ui_col_digest = Resumen
ui_col_media_type = Tipo de medio
ui_empty_select_repo = Selecciona un repositorio en el árbol.
ui_empty_no_tags = No se encontraron etiquetas.
ui_empty_select_tag = Selecciona una etiqueta para inspeccionar los metadatos.
ui_meta_tag = Etiqueta
ui_meta_digest = Resumen
ui_meta_media_type = Tipo de medio
ui_meta_manifest_size = Tamaño del manifiesto
ui_meta_unknown = desconocido

# ── Archivos ─────────────────────────────────────────────────────────────────

ui_header_files = Warehouse — Explorador del almacén de archivos
ui_files_storages = Almacenes
ui_files_entries = Entradas
ui_files_entries_for = Entradas de
ui_files_metadata = Metadatos
ui_files_col_name = Nombre
ui_files_col_type = Tipo
ui_files_col_size = Tamaño
ui_files_col_actions = Acciones
ui_files_upload = Subir
ui_files_download_folder = Descargar carpeta
ui_files_add_folder = Añadir carpeta
ui_files_bulk_download = Descarga masiva
ui_files_bulk_delete = Eliminación masiva
ui_files_up = Subir un nivel
ui_files_empty_storages = No hay almacenes configurados.
ui_files_empty_dir = El directorio está vacío.

# ── Crates ───────────────────────────────────────────────────────────────────

ui_header_crates = Warehouse — Explorador del registro de crates
ui_crates = Crates
ui_crates_empty = Aún no se ha publicado ningún crate.

ui_versions = Versiones
ui_versions_for = Versiones de
ui_col_version = Versión
ui_col_status = Estado
ui_col_checksum = Suma de verificación

ui_status_active = activa
ui_status_yanked = retirada
ui_yank = Retirar
ui_unyank = Restaurar

ui_empty_select_crate = Selecciona un crate de la lista.
ui_empty_no_versions = No se encontraron versiones.
ui_empty_select_version = Selecciona una versión para inspeccionar los metadatos.

ui_meta_version = Versión
ui_meta_status = Estado
ui_meta_checksum = Suma de verificación
ui_meta_rust_version = Versión de Rust
ui_meta_links = Enlaces
ui_meta_features = Características
ui_meta_deps = Dependencias

ui_deps_normal = dependencias
ui_deps_build = dependencias de compilación
ui_deps_dev = dependencias de desarrollo

# ── Común ────────────────────────────────────────────────────────────────────

ui_common_cancel = Cancelar
ui_common_delete = Eliminar
ui_modal_delete_title = Confirmar eliminación

# ── Docker (dinámico) ────────────────────────────────────────────────────────

ui_docker_delete_confirm_text = ¿Seguro que quieres eliminar esta imagen?
ui_delete_image = Eliminar imagen
ui_meta_bytes = {$size} bytes

# ── Crates (dinámico) ────────────────────────────────────────────────────────

ui_yank_version = Retirar versión
ui_unyank_version = Restaurar versión

# ── Códigos de error de la API ───────────────────────────────────────────────

api_error_internal = Algo salió mal. Inténtalo de nuevo.
api_error_digest_required = La eliminación del manifiesto requiere una referencia por resumen
api_error_invalid_repository = Nombre de repositorio no válido
api_error_invalid_digest = Resumen no válido
api_error_manifest_unknown = Manifiesto desconocido
api_error_crate_version_not_found = Versión del crate no encontrada

# ── Inicio de sesión ─────────────────────────────────────────────────────────

ui_login_sign_in = Iniciar sesión
ui_login_username = Nombre de usuario
ui_login_password = Contraseña
ui_login_submit = Acceder
ui_login_invalid_credentials = Credenciales no válidas
ui_logout = Cerrar sesión
