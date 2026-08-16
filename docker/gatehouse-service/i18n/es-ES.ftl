header_label = Gatehouse
ui_home_button = Inicio
ui_account_button = Mi cuenta
ui_logout = Cerrar sesión

ui_login_sign_in = Iniciar sesión
ui_login_username = Nombre de usuario
ui_login_password = Contraseña
ui_login_submit = Acceder
ui_login_invalid_credentials = Credenciales no válidas
ui_login_forgot_password = ¿Olvidaste tu contraseña?
ui_login_register = Crear una cuenta
ui_login_registered_ok = Cuenta creada. Revisa tu correo para verificar la dirección y luego inicia sesión.
ui_login_verified_ok = Tu dirección de correo está verificada. Inicia sesión para continuar.
ui_login_verify_invalid = Ese enlace de verificación no es válido o ha caducado.
ui_login_reset_requested_ok = Si esa cuenta existe, un enlace para restablecer la contraseña está en camino.
ui_login_reset_ok = Tu contraseña se ha restablecido. Inicia sesión con la nueva contraseña.
ui_login_reset_invalid = Ese enlace de restablecimiento no es válido o ha caducado.
ui_login_account_disabled = Esta cuenta ha sido deshabilitada.
ui_login_account_locked = Demasiados intentos fallidos. Vuelve a intentarlo en unos minutos.
ui_login_mfa_title = Código de verificación
ui_login_mfa_code = Código
ui_login_mfa_hint = Introduce el código de 6 dígitos de tu aplicación de autenticación.
ui_login_mfa_submit = Verificar
ui_login_mfa_invalid = Ese código no coincide. Inténtalo de nuevo.

ui_register_title = Crear una cuenta
ui_register_email = Correo electrónico
ui_register_submit = Crear cuenta
ui_register_have_account = ¿Ya tienes una cuenta? Inicia sesión
ui_register_error_email_invalid = Introduce una dirección de correo válida.

ui_forgot_password_title = Restablece tu contraseña
ui_forgot_password_hint = Enviaremos un enlace de restablecimiento a la dirección de correo registrada, si existe.
ui_forgot_password_submit = Enviar enlace de restablecimiento

ui_reset_title = Elige una nueva contraseña
ui_reset_new_password = Nueva contraseña
ui_reset_submit = Guardar nueva contraseña
ui_reset_error_password_empty = Se requiere una contraseña.

ui_home_title = Servicios
ui_home_subtitle = Un solo inicio de sesión para todo lo siguiente.
ui_home_group_services = Servicios disponibles
ui_home_no_services = No hay servicios habilitados actualmente.

ui_service_conveyor_title = Conveyor
ui_service_conveyor_desc = Canalizaciones, compilaciones y despliegues.
ui_service_sage_title = Sage
ui_service_sage_desc = Espacio de trabajo de IA: conversaciones, proyectos y archivos.
ui_service_switchboard_title = Switchboard
ui_service_switchboard_desc = Orquestación de modelos e instancias de GPU.
ui_service_warehouse_title = Warehouse
ui_service_warehouse_desc = Registros de crates, imágenes y archivos.

ui_home_group_realm = Dominio

ui_admin_title = Usuarios
ui_admin_users_title = Usuarios
ui_admin_users_desc = Cuentas, roles y a qué puede acceder cada una.
ui_admin_no_users = El dominio no tiene usuarios.
ui_admin_you = tú
ui_admin_edit = Editar
ui_admin_back = Volver a usuarios
ui_admin_grants_all = todos los servicios
ui_admin_grants_none = sin acceso

ui_admin_create_title = Añadir un usuario
ui_admin_new_username = Nombre de usuario
ui_admin_new_password = Contraseña
ui_admin_new_hint = Un usuario nuevo empieza sin acceso. Concédelo en la pantalla siguiente.
ui_admin_create = Crear

ui_admin_role = Rol
ui_admin_role_user = Usuario
ui_admin_role_admin = Administrador
ui_admin_role_service = Cuenta de servicio

ui_admin_permissions = Acceso
ui_admin_wildcard_note = Este rol ya concede todos los servicios; las opciones de arriba son fijas.
ui_admin_new_password_optional = Nueva contraseña (déjala vacía para mantener la actual)
ui_admin_save = Guardar

ui_admin_template_title = Aplicar una plantilla
ui_admin_template = Plantilla
ui_admin_template_hint = Sustituye cada permiso de abajo por los de la plantilla.
ui_admin_apply_template = Aplicar

ui_admin_delete_title = Eliminar
ui_admin_delete_hint = Eliminar la cuenta cierra sus sesiones de inmediato.
ui_admin_delete = Eliminar este usuario

ui_admin_status_title = Estado
ui_admin_status_created = Creado
ui_admin_status_last_login = Último inicio de sesión
ui_admin_status_never = Nunca
ui_admin_status_disabled = Deshabilitado
ui_admin_status_locked = Bloqueado
ui_admin_status_mfa = Autenticación en dos pasos
ui_admin_status_yes = Sí
ui_admin_status_no = No
ui_admin_action_disable = Deshabilitar
ui_admin_action_enable = Habilitar
ui_admin_action_unlock = Desbloquear
ui_admin_action_mfa_disable = Forzar desactivación

ui_admin_ok_created = Usuario creado.
ui_admin_ok_saved = Cambios guardados.
ui_admin_ok_deleted = Usuario eliminado.

ui_admin_forbidden_title = No permitido
ui_admin_forbidden = Gestionar usuarios requiere el rol de administrador.

ui_admin_error_not_found = No existe ese usuario.
ui_admin_error_username_empty = Se requiere un nombre de usuario.
ui_admin_error_password_empty = Se requiere una contraseña.
ui_admin_error_exists = Ese nombre de usuario ya está en uso.
ui_admin_error_unknown_service = Ese servicio no forma parte de esta instalación.
ui_admin_error_last_admin = El dominio debe conservar al menos un administrador.
ui_admin_error_self_demote = No puedes quitarte tu propio rol de administrador.
ui_admin_error_self_delete = No puedes eliminar tu propia cuenta.
ui_admin_error_self_disable = No puedes desactivar tu propia cuenta.
ui_admin_error_unknown_template = No existe esa plantilla de permisos.
ui_admin_error_roles_require_admin = Solo un administrador puede asignar el rol admin o service.
ui_admin_error_mfa_code_invalid = Ese código no coincide - inténtalo de nuevo.
ui_admin_error_internal = No se pudo guardar el cambio.

ui_account_title = Mi cuenta
ui_account_profile_title = Perfil
ui_account_display_name = Nombre para mostrar
ui_account_avatar_url = URL del avatar
ui_account_title_field = Título
ui_account_timezone = Zona horaria
ui_account_preferred_locale = Idioma preferido
ui_account_new_password = Nueva contraseña
ui_account_password_hint = Déjalo en blanco para mantener tu contraseña actual.
ui_account_save = Guardar cambios
ui_account_ok_saved = Tu cuenta se ha actualizado.
ui_account_ok_mfa_enabled = La autenticación en dos pasos ya está activada.
ui_account_ok_mfa_disabled = La autenticación en dos pasos se ha desactivado.

ui_account_mfa_title = Autenticación en dos pasos
ui_account_mfa_enabled = La autenticación en dos pasos está activada en tu cuenta.
ui_account_mfa_disabled = La autenticación en dos pasos no está activada.
ui_account_mfa_enable = Configurar autenticación en dos pasos
ui_account_mfa_disable = Desactivar autenticación en dos pasos

ui_account_mfa_enroll_title = Configurar autenticación en dos pasos
ui_account_mfa_enroll_hint = Escanea esto con tu aplicación de autenticación, o introduce el secreto manualmente.
ui_account_mfa_secret = Secreto
ui_account_mfa_code = Código
ui_account_mfa_verify = Verificar y activar
