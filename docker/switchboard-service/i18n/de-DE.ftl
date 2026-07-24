# required
header_label = Switchboard
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Sprache
theme_label = Design

# ── Startseite ───────────────────────────────────────────────────────────────

ui_header_home = Switchboard
ui_home_button = Startseite
ui_home_title = Dienste
ui_home_no_services = Derzeit sind keine Dienste aktiviert.
ui_home_group_services = Verfügbare Dienste

ui_service_models_dashboard_title = KI-Modellregister
ui_service_models_dashboard_desc = Verfügbare Modelle durchsuchen.

ui_service_vllm_management_title = vLLM-Verwaltung
ui_service_vllm_management_desc = Laufende vLLM-Instanzen verwalten und neue starten.

# ── Modell-Dashboard ─────────────────────────────────────────────────────────

ui_common_cancel = Abbrechen
ui_header_dashboard = Dashboard
ui_header_models = Modelle
ui_models_search_placeholder = Modelle suchen...
ui_models_gpu_total = Gesamt:
ui_models_gpu_free = Frei:
ui_models_sort_name_asc = Name ▴
ui_models_sort_name_desc = Name ▾
ui_models_sort_params_asc = Parameter ▴
ui_models_sort_params_desc = Parameter ▾
ui_models_sort_vram_asc = VRAM ▴
ui_models_sort_vram_desc = VRAM ▾
ui_models_tab_all = Alle
ui_models_tab_hf = HF
ui_models_tab_gguf = GGUF
ui_models_filter_all_quants = alle Quantisierungen
ui_models_filter_all_contexts = alle Kontexte
ui_models_filter_vllm_only = nur vLLM
ui_header_vllm = vLLM
ui_vllm_launch_new = vLLM-Instanz starten
ui_vllm_running_instances = Laufende Instanzen
ui_vllm_launch_modal_title = vLLM-Instanz starten
ui_vllm_form_model = Modell
ui_vllm_form_host = Host
ui_vllm_form_port = Port
ui_vllm_form_namespace = Namespace
ui_vllm_form_quant = Quantisierung
ui_vllm_form_dtype = Dtype
ui_vllm_form_limit_mm = Multimodal-Limit
ui_vllm_form_max_len = Max. Modelllänge
ui_vllm_form_gpu_util = GPU-Speicherauslastung
ui_vllm_form_prefix_caching = Prefix-Caching aktivieren
ui_vllm_form_task = Aufgabe
ui_vllm_form_tool_calling = Tool-Aufrufe
ui_vllm_launch_confirm = Starten
ui_models_card_delete_tooltip = Modell löschen
ui_models_card_params = Parameter
ui_models_card_context = Kontext
ui_models_card_quant = Quant
ui_models_card_layers = Schichten
ui_models_card_hidden = Verborgen
ui_models_card_fits_yes = Passt: JA
ui_models_card_fits_no = Passt: NEIN
ui_models_card_best = Beste
ui_models_card_minimum = Minimum
ui_models_card_vram = VRAM
ui_models_card_margin = Spielraum
ui_models_card_estimate_btn = Schätzungen
ui_models_modal_estimates_title = Schätzungen
ui_models_modal_estimates_filter_all = Alle
ui_models_modal_estimates_filter_fits = Passt
ui_models_modal_estimates_filter_nofit = Passt nicht
ui_models_modal_estimates_filter_all_contexts = Alle Kontexte
ui_models_modal_estimates_filter_all_quants = Alle Quantisierungen
ui_models_modal_delete_title = Löschen bestätigen
ui_models_modal_delete_text = Möchtest du dieses Modell wirklich physisch von der Festplatte löschen?
ui_models_modal_delete_confirm = Löschen
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

# ── Modelle (dynamische Fragmente) ───────────────────────────────────────────

ui_gpu_unavailable = GPU: k. A.
ui_models_card_no_estimates = Keine Schätzungen

# ── vLLM-Verwaltung ──────────────────────────────────────────────────────────

ui_vllm_launch_modal_subtitle = Konfiguriere einen Endpunkt, das Speicherbudget und eine optionale Laufzeit-Quantisierung.
ui_vllm_form_select_model = -- Modell auswählen --
ui_vllm_no_instances = Keine laufenden Instanzen
ui_vllm_stop_tooltip = Instanz stoppen
ui_vllm_stop_modal_title = vLLM-Instanz stoppen
ui_vllm_stop_modal_text = Möchtest du diese Instanz wirklich stoppen?
ui_vllm_stop_modal_confirm = Instanz stoppen
ui_vllm_unknown_model = Unbekanntes Modell
ui_vllm_meta_id = ID
ui_vllm_meta_namespace = Namespace
ui_vllm_meta_endpoint = Endpunkt
ui_vllm_meta_status = Status
ui_vllm_meta_started = Gestartet
ui_vllm_meta_gpu_util = GPU-Auslastung
ui_vllm_status_running = läuft
ui_vllm_status_starting = startet
ui_vllm_status_pending = wartend
ui_vllm_status_failed = fehlgeschlagen
ui_vllm_status_terminating = wird beendet

ui_vllm_fit_select_model = Wähle ein Modell, um den benötigten VRAM zu schätzen.
ui_vllm_fit_no_estimate = Keine passende Schätzung verfügbar.
ui_vllm_fit_wont_fit_budget = Passt nicht: Das Modell benötigt ~{ $model } GB für die gewählte maximale Länge, die GPU-Speicherauslastung erlaubt aber nur { $budget } GB
ui_vllm_fit_wont_fit_free = Passt derzeit nicht: vLLM reserviert ~{ $required } GB, aber nur { $free } GB sind frei
ui_vllm_fit_tight = Knapper Spielraum: Das Modell benötigt ~{ $model } GB und vLLM reserviert ~{ $required } GB, es bleiben { $remaining } GB frei
ui_vllm_fit_ok = Sollte passen: Das Modell benötigt ~{ $model } GB und vLLM reserviert ~{ $required } GB

# ── API-Fehlercodes ──────────────────────────────────────────────────────────

api_error_model_name_empty = Der Modellname darf nicht leer sein
api_error_vllm_launch_failed = vLLM-Instanz konnte nicht gestartet werden
api_error_vllm_stop_failed = vLLM-Instanz konnte nicht gestoppt werden
api_error_vllm_list_failed = vLLM-Instanzen konnten nicht aufgelistet werden
api_error_instance_not_found = Instanz nicht gefunden
api_error_invalid_model_path = Ungültiger Modellpfad
api_error_model_not_found = Modell nicht auf der Festplatte gefunden
api_error_model_delete_failed = Modell konnte nicht gelöscht werden

# ── Anmeldung ────────────────────────────────────────────────────────────────

ui_login_sign_in = Anmelden
ui_login_username = Benutzername
ui_login_password = Passwort
ui_login_submit = Anmelden
ui_login_invalid_credentials = Ungültige Anmeldedaten
ui_logout = Abmelden
