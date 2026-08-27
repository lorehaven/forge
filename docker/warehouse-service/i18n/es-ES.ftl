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

ui_service_apk_title = Registro de APK
ui_service_apk_desc = Explora los paquetes de Android publicados y sus versiones.

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

# ── Gestión de archivos ─────────────────────────────────────────────────────

ui_storages_title = Almacenes
ui_storages_empty = No hay ningún almacén configurado.
ui_storages_detail_title = Almacén
ui_storages_select = Selecciona un almacén de la lista.
ui_storage_static_badge = estático
ui_storage_kind = Tipo
ui_storage_owner = Propietario
ui_storage_usage = Uso
ui_storage_max_file = Tamaño máx. de archivo
ui_storage_sync = Sincronización
ui_storage_sync_on = activada
ui_storage_sync_off = desactivada
ui_storage_created = Creado
ui_storage_root = Raíz
ui_storage_files_title = Archivos
ui_storage_files_empty = Este almacén no tiene archivos.
ui_storage_files_truncated = Mostrando solo la primera página de archivos.
ui_storage_not_found = No existe ese almacén.
ui_storage_root_unreadable = No se pudo leer la raíz del almacén.
ui_file_download = descargar
ui_file_delete = Eliminar
ui_storage_edit_title = Editar almacén
ui_storage_quota_gib = Cuota (GiB)
ui_storage_max_file_mib = Tamaño máx. de archivo (MiB)
ui_storage_clear_max_file = Restablecer el tamaño máx. de archivo predeterminado
ui_storage_save = Guardar cambios
ui_storage_new_title = Nuevo almacén
ui_storage_name = Nombre
ui_storage_create = Crear almacén
ui_storage_delete = Eliminar almacén
ui_storage_delete_title = Eliminar almacén
ui_storage_delete_confirm_text = ¿Eliminar este almacén y todo su contenido? Esta acción no se puede deshacer.

# ── Gestión de APK ─────────────────────────────────────────────────────────

ui_header_apk = Warehouse - Explorador del registro de APK
ui_apk_packages = Paquetes
ui_apk_empty = Aún no se ha publicado ningún paquete APK.
ui_apk_empty_select_version = Selecciona una versión para ver sus metadatos.
ui_apk_meta_package = Paquete
ui_apk_meta_version_name = Nombre de versión
ui_apk_meta_version_code = Código de versión
ui_apk_meta_label = Etiqueta
ui_apk_meta_min_sdk = SDK mínimo
ui_apk_meta_target_sdk = SDK objetivo
ui_apk_meta_size = Tamaño
ui_apk_meta_uploaded_by = Subido por
ui_apk_meta_permissions = Permisos
ui_apk_yank = Retirar
ui_apk_unyank = Restaurar

# ── Códigos de error de la API ───────────────────────────────────────────────

api_error_internal = Algo salió mal. Inténtalo de nuevo.
api_error_digest_required = La eliminación del manifiesto requiere una referencia por resumen
api_error_invalid_repository = Nombre de repositorio no válido
api_error_invalid_digest = Resumen no válido
api_error_manifest_unknown = Manifiesto desconocido
api_error_crate_version_not_found = Versión del crate no encontrada
api_error_forbidden = No tienes permiso para hacer eso.
api_error_files_disabled = El almacenamiento de archivos no está habilitado en este despliegue.
api_error_apk_disabled = El registro de APK no está habilitado en este despliegue.
api_error_invalid_storage_name = Los nombres de almacén solo pueden usar letras, dígitos, - y _.
api_error_storage_owner_required = Se requiere un propietario.
api_error_storage_owner_unknown = No existe ese usuario para ser propietario del almacén.
api_error_storage_name_static_clash = Un almacén estático ya usa ese nombre.
api_error_storage_exists = Ya existe un almacén con ese nombre.
api_error_storage_not_found = No existe ese almacén dinámico.
api_error_invalid_quota = La cuota debe ser un número no negativo.
api_error_invalid_max_file = El tamaño máx. de archivo debe ser un número no negativo.
api_error_invalid_path = Ruta de archivo no válida.
api_error_path_escapes_storage = Esa ruta apunta fuera del almacén.
api_error_no_dynamic_root = Este despliegue no tiene configurada una raíz de almacenamiento dinámico.
api_error_file_not_found = No existe ese archivo.

# ── Inicio de sesión ─────────────────────────────────────────────────────────

ui_login_sign_in = Iniciar sesión
ui_login_username = Nombre de usuario
ui_login_password = Contraseña
ui_login_submit = Acceder
ui_login_invalid_credentials = Credenciales no válidas
ui_logout = Cerrar sesión
