# required
header_label = Switchboard
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Language
theme_label = Theme

# ── Home ─────────────────────────────────────────────────────────────────────

ui_header_home = Switchboard
ui_home_button = Home
ui_home_title = Services
ui_home_no_services = No services are currently enabled.
ui_home_group_services = Available Services

ui_service_models_dashboard_title = AI Models Registry
ui_service_models_dashboard_desc = Browse available models.

ui_service_vllm_management_title = vLLM Management
ui_service_vllm_management_desc = Manage running vLLM instances and launch new ones.

# ── Models Dashboard ─────────────────────────────────────────────────────────

ui_common_cancel = Cancel
ui_header_dashboard = Dashboard
ui_header_models = Models
ui_models_search_placeholder = Search models...
ui_models_gpu_total = Total:
ui_models_gpu_free = Free:
ui_models_sort_name_asc = name ▴
ui_models_sort_name_desc = name ▾
ui_models_sort_params_asc = params ▴
ui_models_sort_params_desc = params ▾
ui_models_sort_vram_asc = vram ▴
ui_models_sort_vram_desc = vram ▾
ui_models_tab_all = All
ui_models_tab_hf = HF
ui_models_tab_gguf = GGUF
ui_models_filter_all_quants = all quants
ui_models_filter_all_contexts = all contexts
ui_models_filter_vllm_only = vLLM only
ui_header_vllm = vLLM
ui_vllm_launch_new = Launch vLLM Instance
ui_vllm_running_instances = Running Instances
ui_vllm_launch_modal_title = Launch vLLM Instance
ui_vllm_form_model = Model
ui_vllm_form_host = Host
ui_vllm_form_port = Port
ui_vllm_form_namespace = Namespace
ui_vllm_form_quant = Quantization
ui_vllm_form_dtype = Dtype
ui_vllm_form_device = Device
ui_vllm_form_limit_mm = Multimodal Limit
ui_vllm_form_max_len = Max Model Len
ui_vllm_form_gpu_util = GPU Memory Utilization
ui_vllm_form_prefix_caching = Enable Prefix Caching
ui_vllm_form_task = Task
ui_vllm_form_tool_calling = Tool Calling
ui_vllm_launch_confirm = Launch
ui_models_card_delete_tooltip = Delete Model
ui_models_card_params = Params
ui_models_card_context = Context
ui_models_card_quant = Quant
ui_models_card_layers = Layers
ui_models_card_hidden = Hidden
ui_models_card_fits_yes = Fits: YES
ui_models_card_fits_no = Fits: NO
ui_models_card_best = Best
ui_models_card_minimum = Minimum
ui_models_card_vram = VRAM
ui_models_card_margin = Margin
ui_models_card_estimate_btn = Estimates
ui_models_modal_estimates_title = Estimates
ui_models_modal_estimates_filter_all = All
ui_models_modal_estimates_filter_fits = Fits
ui_models_modal_estimates_filter_nofit = Does Not Fit
ui_models_modal_estimates_filter_all_contexts = All Contexts
ui_models_modal_estimates_filter_all_quants = All Quants
ui_models_modal_delete_title = Confirm Delete
ui_models_modal_delete_text = Are you sure you want to physically delete this model from drive?
ui_models_modal_delete_confirm = Delete
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

# ── Models (dynamic fragments) ───────────────────────────────────────────────

ui_gpu_unavailable = GPU: n/a
ui_models_card_no_estimates = No estimates

# ── vLLM Management ──────────────────────────────────────────────────────────

ui_vllm_launch_modal_subtitle = Configure an endpoint, memory budget, and optional runtime quantization.
ui_vllm_form_select_model = -- select model --
ui_vllm_no_instances = No running instances
ui_vllm_stop_tooltip = Stop instance
ui_vllm_stop_modal_title = Stop vLLM Instance
ui_vllm_stop_modal_text = Are you sure you want to stop this instance?
ui_vllm_stop_modal_confirm = Stop Instance
ui_vllm_unknown_model = Unknown Model
ui_vllm_meta_id = ID
ui_vllm_meta_namespace = Namespace
ui_vllm_meta_endpoint = Endpoint
ui_vllm_meta_status = Status
ui_vllm_meta_started = Started
ui_vllm_meta_gpu_util = GPU Util
ui_vllm_status_running = running
ui_vllm_status_starting = starting
ui_vllm_status_pending = pending
ui_vllm_status_failed = failed
ui_vllm_status_terminating = terminating

ui_vllm_fit_select_model = Select a model to estimate required VRAM.
ui_vllm_fit_no_estimate = No matching estimate available.
ui_vllm_fit_wont_fit_budget = Won't fit: model needs ~{ $model } GB for the selected max length, but gpu memory utilization allows only { $budget } GB
ui_vllm_fit_wont_fit_free = Won't fit right now: vLLM will reserve ~{ $required } GB, but only { $free } GB is free
ui_vllm_fit_tight = Tight fit: model needs ~{ $model } GB and vLLM will reserve ~{ $required } GB, leaving { $remaining } GB free
ui_vllm_fit_ok = Should fit: model needs ~{ $model } GB and vLLM will reserve ~{ $required } GB
ui_vllm_fit_note_cpu = Running on CPU - GPU VRAM fit is not evaluated. Needs a CPU-capable vLLM build.

# ── API error codes ──────────────────────────────────────────────────────────

api_error_model_name_empty = Model name cannot be empty
api_error_vllm_launch_failed = Failed to launch vLLM instance
api_error_vllm_stop_failed = Failed to stop vLLM instance
api_error_vllm_list_failed = Failed to list vLLM instances
api_error_instance_not_found = Instance not found
api_error_invalid_model_path = Invalid model path
api_error_model_not_found = Model not found on disk
api_error_model_delete_failed = Failed to delete model

# ── Auth ─────────────────────────────────────────────────────────────────────

ui_login_sign_in = Sign in
ui_login_username = Username
ui_login_password = Password
ui_login_submit = Log in
ui_login_invalid_credentials = Invalid credentials
ui_logout = Log out
