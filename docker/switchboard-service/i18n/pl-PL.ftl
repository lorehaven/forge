# required
header_label = Switchboard
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Język
theme_label = Motyw

# ── Strona główna ────────────────────────────────────────────────────────────

ui_header_home = Switchboard
ui_home_button = Strona główna
ui_home_title = Usługi
ui_home_no_services = Żadne usługi nie są obecnie włączone.
ui_home_group_services = Dostępne usługi

ui_service_models_dashboard_title = Rejestr modeli AI
ui_service_models_dashboard_desc = Przeglądaj dostępne modele.

ui_service_vllm_management_title = Zarządzanie vLLM
ui_service_vllm_management_desc = Zarządzaj uruchomionymi instancjami vLLM i uruchamiaj nowe.

# ── Panel modeli ─────────────────────────────────────────────────────────────

ui_common_cancel = Anuluj
ui_header_dashboard = Panel
ui_header_models = Modele
ui_models_search_placeholder = Szukaj modeli...
ui_models_gpu_total = Łącznie:
ui_models_gpu_free = Wolne:
ui_models_sort_name_asc = nazwa ▴
ui_models_sort_name_desc = nazwa ▾
ui_models_sort_params_asc = parametry ▴
ui_models_sort_params_desc = parametry ▾
ui_models_sort_vram_asc = vram ▴
ui_models_sort_vram_desc = vram ▾
ui_models_tab_all = Wszystkie
ui_models_tab_hf = HF
ui_models_tab_gguf = GGUF
ui_models_filter_all_quants = wszystkie kwantyzacje
ui_models_filter_all_contexts = wszystkie konteksty
ui_models_filter_vllm_only = tylko vLLM
ui_header_vllm = vLLM
ui_vllm_launch_new = Uruchom instancję vLLM
ui_vllm_running_instances = Uruchomione instancje
ui_vllm_launch_modal_title = Uruchom instancję vLLM
ui_vllm_form_model = Model
ui_vllm_form_host = Host
ui_vllm_form_port = Port
ui_vllm_form_namespace = Przestrzeń nazw
ui_vllm_form_quant = Kwantyzacja
ui_vllm_form_dtype = Dtype
ui_vllm_form_device = Urządzenie
ui_vllm_form_limit_mm = Limit multimodalny
ui_vllm_form_max_len = Maks. długość modelu
ui_vllm_form_gpu_util = Wykorzystanie pamięci GPU
ui_vllm_form_prefix_caching = Włącz buforowanie prefiksów
ui_vllm_form_task = Zadanie
ui_vllm_form_tool_calling = Wywoływanie narzędzi
ui_vllm_launch_confirm = Uruchom
ui_models_card_delete_tooltip = Usuń model
ui_models_card_params = Parametry
ui_models_card_context = Kontekst
ui_models_card_quant = Kwant
ui_models_card_layers = Warstwy
ui_models_card_hidden = Ukryte
ui_models_card_fits_yes = Mieści się: TAK
ui_models_card_fits_no = Mieści się: NIE
ui_models_card_best = Najlepsze
ui_models_card_minimum = Minimum
ui_models_card_vram = VRAM
ui_models_card_margin = Margines
ui_models_card_estimate_btn = Szacunki
ui_models_modal_estimates_title = Szacunki
ui_models_modal_estimates_filter_all = Wszystkie
ui_models_modal_estimates_filter_fits = Mieści się
ui_models_modal_estimates_filter_nofit = Nie mieści się
ui_models_modal_estimates_filter_all_contexts = Wszystkie konteksty
ui_models_modal_estimates_filter_all_quants = Wszystkie kwantyzacje
ui_models_modal_delete_title = Potwierdź usunięcie
ui_models_modal_delete_text = Czy na pewno chcesz trwale usunąć ten model z dysku?
ui_models_modal_delete_confirm = Usuń
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

# ── Modele (fragmenty dynamiczne) ────────────────────────────────────────────

ui_gpu_unavailable = GPU: b/d
ui_models_card_no_estimates = Brak szacunków

# ── Zarządzanie vLLM ─────────────────────────────────────────────────────────

ui_vllm_launch_modal_subtitle = Skonfiguruj punkt końcowy, budżet pamięci i opcjonalną kwantyzację środowiska uruchomieniowego.
ui_vllm_form_select_model = -- wybierz model --
ui_vllm_no_instances = Brak uruchomionych instancji
ui_vllm_stop_tooltip = Zatrzymaj instancję
ui_vllm_stop_modal_title = Zatrzymaj instancję vLLM
ui_vllm_stop_modal_text = Czy na pewno chcesz zatrzymać tę instancję?
ui_vllm_stop_modal_confirm = Zatrzymaj instancję
ui_vllm_unknown_model = Nieznany model
ui_vllm_meta_id = ID
ui_vllm_meta_namespace = Przestrzeń nazw
ui_vllm_meta_endpoint = Punkt końcowy
ui_vllm_meta_status = Status
ui_vllm_meta_started = Uruchomiono
ui_vllm_meta_gpu_util = Wykorzystanie GPU
ui_vllm_status_running = działa
ui_vllm_status_starting = uruchamianie
ui_vllm_status_pending = oczekuje
ui_vllm_status_failed = awaria
ui_vllm_status_terminating = zatrzymywanie

ui_vllm_fit_select_model = Wybierz model, aby oszacować wymaganą pamięć VRAM.
ui_vllm_fit_no_estimate = Brak pasującego oszacowania.
ui_vllm_fit_wont_fit_budget = Nie zmieści się: model potrzebuje ~{ $model } GB dla wybranej maksymalnej długości, ale wykorzystanie pamięci GPU pozwala tylko na { $budget } GB
ui_vllm_fit_wont_fit_free = Nie zmieści się teraz: vLLM zarezerwuje ~{ $required } GB, ale wolne jest tylko { $free } GB
ui_vllm_fit_tight = Ciasne dopasowanie: model potrzebuje ~{ $model } GB, a vLLM zarezerwuje ~{ $required } GB, pozostawiając { $remaining } GB wolnego
ui_vllm_fit_ok = Powinno się zmieścić: model potrzebuje ~{ $model } GB, a vLLM zarezerwuje ~{ $required } GB
ui_vllm_fit_note_cpu = Uruchomiono na CPU - dopasowanie VRAM GPU nie jest sprawdzane. Wymaga wersji vLLM z obsługą CPU.

# ── Kody błędów API ──────────────────────────────────────────────────────────

api_error_model_name_empty = Nazwa modelu nie może być pusta
api_error_vllm_launch_failed = Nie udało się uruchomić instancji vLLM
api_error_vllm_stop_failed = Nie udało się zatrzymać instancji vLLM
api_error_vllm_list_failed = Nie udało się pobrać listy instancji vLLM
api_error_instance_not_found = Nie znaleziono instancji
api_error_invalid_model_path = Nieprawidłowa ścieżka modelu
api_error_model_not_found = Nie znaleziono modelu na dysku
api_error_model_delete_failed = Nie udało się usunąć modelu

# ── Logowanie ────────────────────────────────────────────────────────────────

ui_login_sign_in = Zaloguj się
ui_login_username = Nazwa użytkownika
ui_login_password = Hasło
ui_login_submit = Zaloguj
ui_login_invalid_credentials = Nieprawidłowe dane logowania
ui_logout = Wyloguj
