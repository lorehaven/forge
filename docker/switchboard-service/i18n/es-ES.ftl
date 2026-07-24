# required
header_label = Switchboard
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Idioma
theme_label = Tema

# ── Inicio ───────────────────────────────────────────────────────────────────

ui_header_home = Switchboard
ui_home_button = Inicio
ui_home_title = Servicios
ui_home_no_services = No hay servicios habilitados actualmente.
ui_home_group_services = Servicios disponibles

ui_service_models_dashboard_title = Registro de modelos de IA
ui_service_models_dashboard_desc = Explora los modelos disponibles.

ui_service_vllm_management_title = Gestión de vLLM
ui_service_vllm_management_desc = Gestiona las instancias vLLM en ejecución y lanza nuevas.

# ── Panel de modelos ─────────────────────────────────────────────────────────

ui_common_cancel = Cancelar
ui_header_dashboard = Panel
ui_header_models = Modelos
ui_models_search_placeholder = Buscar modelos...
ui_models_gpu_total = Total:
ui_models_gpu_free = Libre:
ui_models_sort_name_asc = nombre ▴
ui_models_sort_name_desc = nombre ▾
ui_models_sort_params_asc = parámetros ▴
ui_models_sort_params_desc = parámetros ▾
ui_models_sort_vram_asc = vram ▴
ui_models_sort_vram_desc = vram ▾
ui_models_tab_all = Todos
ui_models_tab_hf = HF
ui_models_tab_gguf = GGUF
ui_models_filter_all_quants = todas las cuantizaciones
ui_models_filter_all_contexts = todos los contextos
ui_models_filter_vllm_only = solo vLLM
ui_header_vllm = vLLM
ui_vllm_launch_new = Lanzar instancia vLLM
ui_vllm_running_instances = Instancias en ejecución
ui_vllm_launch_modal_title = Lanzar instancia vLLM
ui_vllm_form_model = Modelo
ui_vllm_form_host = Host
ui_vllm_form_port = Puerto
ui_vllm_form_namespace = Espacio de nombres
ui_vllm_form_quant = Cuantización
ui_vllm_form_dtype = Dtype
ui_vllm_form_limit_mm = Límite multimodal
ui_vllm_form_max_len = Longitud máx. del modelo
ui_vllm_form_gpu_util = Uso de memoria GPU
ui_vllm_form_prefix_caching = Habilitar caché de prefijos
ui_vllm_form_task = Tarea
ui_vllm_form_tool_calling = Llamadas a herramientas
ui_vllm_launch_confirm = Lanzar
ui_models_card_delete_tooltip = Eliminar modelo
ui_models_card_params = Parámetros
ui_models_card_context = Contexto
ui_models_card_quant = Cuant
ui_models_card_layers = Capas
ui_models_card_hidden = Oculto
ui_models_card_fits_yes = Cabe: SÍ
ui_models_card_fits_no = Cabe: NO
ui_models_card_best = Mejor
ui_models_card_minimum = Mínimo
ui_models_card_vram = VRAM
ui_models_card_margin = Margen
ui_models_card_estimate_btn = Estimaciones
ui_models_modal_estimates_title = Estimaciones
ui_models_modal_estimates_filter_all = Todas
ui_models_modal_estimates_filter_fits = Cabe
ui_models_modal_estimates_filter_nofit = No cabe
ui_models_modal_estimates_filter_all_contexts = Todos los contextos
ui_models_modal_estimates_filter_all_quants = Todas las cuantizaciones
ui_models_modal_delete_title = Confirmar eliminación
ui_models_modal_delete_text = ¿Seguro que quieres eliminar físicamente este modelo del disco?
ui_models_modal_delete_confirm = Eliminar
ui_models_quant_fp16 = fp16
ui_models_quant_bf16 = bf16
ui_models_quant_fp8 = fp8
ui_models_quant_int8 = int8
ui_models_quant_awq = awq
ui_models_quant_gptq = gptq
ui_models_quant_q8_0 = q8_0
ui_models_quant_q6_k = q6_k
ui_models_quant_q5_k_m = q5_k_m
ui_models_quant_q5_0 = q5_0
ui_models_quant_q4_k_m = q4_k_m
ui_models_quant_q4_0 = q4_0
ui_models_quant_q3_k_m = q3_k_m
ui_models_quant_q2_k = q2_k

# ── Modelos (fragmentos dinámicos) ───────────────────────────────────────────

ui_gpu_unavailable = GPU: n/d
ui_models_card_no_estimates = Sin estimaciones

# ── Gestión de vLLM ──────────────────────────────────────────────────────────

ui_vllm_launch_modal_subtitle = Configura un punto de conexión, el presupuesto de memoria y una cuantización opcional en tiempo de ejecución.
ui_vllm_form_select_model = -- selecciona un modelo --
ui_vllm_no_instances = No hay instancias en ejecución
ui_vllm_stop_tooltip = Detener instancia
ui_vllm_stop_modal_title = Detener instancia vLLM
ui_vllm_stop_modal_text = ¿Seguro que quieres detener esta instancia?
ui_vllm_stop_modal_confirm = Detener instancia
ui_vllm_unknown_model = Modelo desconocido
ui_vllm_meta_id = ID
ui_vllm_meta_namespace = Espacio de nombres
ui_vllm_meta_endpoint = Punto de conexión
ui_vllm_meta_status = Estado
ui_vllm_meta_started = Iniciado
ui_vllm_meta_gpu_util = Uso de GPU
ui_vllm_status_running = en ejecución
ui_vllm_status_starting = iniciando
ui_vllm_status_pending = pendiente
ui_vllm_status_failed = fallido
ui_vllm_status_terminating = deteniéndose

ui_vllm_fit_select_model = Selecciona un modelo para estimar la VRAM necesaria.
ui_vllm_fit_no_estimate = No hay ninguna estimación coincidente.
ui_vllm_fit_wont_fit_budget = No cabrá: el modelo necesita ~{ $model } GB para la longitud máxima seleccionada, pero el uso de memoria GPU solo permite { $budget } GB
ui_vllm_fit_wont_fit_free = No cabe ahora mismo: vLLM reservará ~{ $required } GB, pero solo hay { $free } GB libres
ui_vllm_fit_tight = Ajuste justo: el modelo necesita ~{ $model } GB y vLLM reservará ~{ $required } GB, dejando { $remaining } GB libres
ui_vllm_fit_ok = Debería caber: el modelo necesita ~{ $model } GB y vLLM reservará ~{ $required } GB

# ── Códigos de error de la API ───────────────────────────────────────────────

api_error_model_name_empty = El nombre del modelo no puede estar vacío
api_error_vllm_launch_failed = No se pudo lanzar la instancia vLLM
api_error_vllm_stop_failed = No se pudo detener la instancia vLLM
api_error_vllm_list_failed = No se pudo obtener la lista de instancias vLLM
api_error_instance_not_found = Instancia no encontrada
api_error_invalid_model_path = Ruta de modelo no válida
api_error_model_not_found = Modelo no encontrado en el disco
api_error_model_delete_failed = No se pudo eliminar el modelo

# ── Inicio de sesión ─────────────────────────────────────────────────────────

ui_login_sign_in = Iniciar sesión
ui_login_username = Nombre de usuario
ui_login_password = Contraseña
ui_login_submit = Acceder
ui_login_invalid_credentials = Credenciales no válidas
ui_logout = Cerrar sesión
