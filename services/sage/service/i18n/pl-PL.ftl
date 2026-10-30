# required
header_label = Sage
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Język
theme_label = Motyw

# ── Strona główna ────────────────────────────────────────────────────────────

ui_header_home = Sage
ui_home_button = Strona główna
ui_home_title = Asystent
ui_home_no_services = Przestrzeń robocza Sage AI jest gotowa.
ui_home_group_services = Dostępne usługi

# ── Czat ─────────────────────────────────────────────────────────────────────

ui_chat_input_placeholder = Zapytaj Sage o cokolwiek...
ui_chat_send_button = Wyślij
ui_chat_welcome_message = Cześć! Jestem Sage, Twój asystent AI. Jak mogę Ci dziś pomóc?
ui_chat_no_model_available = Żaden model nie jest obecnie wybrany ani dostępny.
ui_chat_user_label = Ty
ui_chat_ai_label = Sage

# ── Czat (dynamiczne) ────────────────────────────────────────────────────────

ui_chat_thinking = Sage myśli...
ui_chat_regenerating = Sage generuje ponownie...
ui_chat_edit = Edytuj
ui_chat_regenerate = Wygeneruj ponownie
ui_chat_save_submit = Zapisz i wyślij
ui_chat_error = Błąd
ui_chat_sources = Źródła
ui_chat_source_chunk = fragment {$index}
ui_chat_status_planning = Ustalam, czego szukać…
ui_chat_status_searching = Przeszukuję internet…
ui_chat_status_reading = Czytam źródła…
ui_chat_status_checking = Sprawdzam odpowiedź ze źródłami…
ui_chat_no_models = Brak dostępnych modeli
ui_chat_switchboard_unavailable = Switchboard niedostępny
ui_chat_welcome_tooltip = Cześć! Jestem Sage...
ui_chat_attach_tooltip = Załącz plik (obrazy, pdf, txt, csv, md, html, json, yaml, kod źródłowy…)
ui_chat_untitled = Nowy czat
ui_chat_delete_confirm_text = Czy na pewno chcesz usunąć tę rozmowę?
ui_chat_this_conversation = ta rozmowa

ui_code_copy = Kopiuj
ui_code_copied = Skopiowano!
ui_code_copy_error = Błąd

# ── Panel boczny ─────────────────────────────────────────────────────────────

ui_sidebar_new_chat = Nowy czat
ui_sidebar_projects = Projekty
ui_sidebar_new = Nowy
ui_sidebar_history = Historia
ui_sidebar_files = Pliki

# ── Wspólne ──────────────────────────────────────────────────────────────────

ui_common_cancel = Anuluj
ui_common_delete = Usuń
ui_modal_delete_title = Potwierdź usunięcie

# ── Projekty ─────────────────────────────────────────────────────────────────

ui_projects_new_title = Utwórz nowy projekt
ui_projects_name_label = Nazwa projektu
ui_projects_create = Utwórz

# ── Pliki ────────────────────────────────────────────────────────────────────

ui_file_status_ready = gotowy
ui_file_status_processing = przetwarzanie
ui_file_status_uploaded = w kolejce
ui_file_status_failed = błąd
ui_files_retry_tooltip = Ponów przetwarzanie
ui_files_remove_tooltip = Anuluj / usuń
ui_files_download_tooltip = Pobierz
ui_files_empty_project = Brak plików przesłanych do tego projektu.
ui_files_delete_confirm_text = Czy na pewno chcesz usunąć ten plik?

# ── Inicjalizacja ────────────────────────────────────────────────────────────

ui_init_title = Przygotowywanie Sage
ui_init_subtitle = Uruchamianie modeli, których Sage potrzebuje, zanim zaczniesz rozmowę.
ui_init_waiting = Oczekiwanie na odpowiedź usługi modeli…
ui_init_status_running = Działa
ui_init_status_starting = Uruchamianie…
ui_init_status_queued = W kolejce
ui_init_status_failed = Błąd
ui_init_status_unknown = Łączenie…
ui_init_embedding_tag = (embedding)

# ── Kody błędów API ──────────────────────────────────────────────────────────

api_error_internal = Coś poszło nie tak. Spróbuj ponownie.
api_error_instance_not_found = Nie znaleziono instancji modelu
api_error_embedding_model_chat = Wybrany model jest modelem embedding i nie może służyć do czatu
api_error_switchboard_unavailable = Usługa modeli jest niedostępna
api_error_stream_failed = Nie udało się rozpocząć strumienia czatu
api_error_regenerate_non_assistant = Ponownie generować można tylko odpowiedzi asystenta
api_error_no_parent_message = Wiadomość nie ma rodzica, od którego można wygenerować ponownie
api_error_parent_not_found = Nie znaleziono wiadomości nadrzędnej
api_error_no_models_available = Brak dostępnych modeli AI do ponownego generowania
api_error_metrics_not_found = Nie znaleziono metryk dla tego profilu
api_error_costs_not_found = Nie znaleziono kosztów dla tego użytkownika
api_error_missing_conversation_id = Brak identyfikatora rozmowy
api_error_conversation_create_failed = Nie udało się rozpocząć rozmowy
api_error_conversation_not_found = Nie znaleziono rozmowy
api_error_project_not_found = Nie znaleziono projektu
api_error_file_not_found = Nie znaleziono pliku
api_error_file_content_not_found = Nie znaleziono zawartości pliku
api_error_file_scope_required = Należy podać dokładnie jedno: rozmowę albo projekt
api_error_missing_file_name = Brak nazwy pliku
api_error_unsupported_file_type = Nieobsługiwany typ pliku. Dozwolone: obrazy (png, jpg, webp, gif), pdf, txt, csv, md, html, json, yaml, toml, xml oraz popularne pliki z kodem źródłowym
api_error_image_not_processable = Obrazy nie są indeksowane do wyszukiwania; są wysyłane bezpośrednio do modelu wraz z wiadomością
api_error_file_too_large = Plik przekracza maksymalny dozwolony rozmiar
api_error_file_empty = Plik jest pusty
api_error_file_limit_reached = Osiągnięto limit plików dla tej rozmowy/projektu
api_error_file_already_processing = Plik jest już przetwarzany
api_error_postgres_required = Przechowywanie plików wymaga bazy danych Postgres

# ── Logowanie ────────────────────────────────────────────────────────────────

ui_login_sign_in = Zaloguj się
ui_login_username = Nazwa użytkownika
ui_login_password = Hasło
ui_login_submit = Zaloguj
ui_login_invalid_credentials = Nieprawidłowe dane logowania
ui_logout = Wyloguj
ui_profile_edit = Edytuj profil
ui_user_menu = Menu użytkownika
ui_menu_open = Otwórz menu
ui_menu_close = Zamknij menu
ui_theme_toggle = Zmień motyw
ui_close = Zamknij
ui_cancel = Anuluj
ui_dashboard_button = Panel usług
ui_nav_chat = Czat
ui_chat_history = Rozmowy
ui_theme_mode = Tryb
ui_theme_light = Jasny
ui_theme_dark = Ciemny
ui_theme_palette = Kolor
ui_palette_green = Zielony
ui_palette_teal = Morski
ui_palette_blue = Niebieski
ui_palette_purple = Fioletowy
ui_palette_rose = Różowy
ui_palette_orange = Pomarańczowy

# ── Browser client ───────────────────────────────────────────────────────────

ui_loading = Ładowanie…
ui_not_found = Nic tu nie ma.
ui_session_ended = Twoja sesja wygasła.
ui_sign_in = Zaloguj się
ui_chat_error_empty = Najpierw napisz wiadomość.
ui_chat_stream_lost = Utracono połączenie z odpowiedzią.
ui_projects_error_name = Projekt potrzebuje nazwy (do 120 znaków).
api_error_empty_message = Wiadomość jest pusta
api_error_message_not_found = Nie znaleziono wiadomości
api_error_edit_non_user = Można edytować tylko własne wiadomości
api_error_project_name = Projekt potrzebuje nazwy (do 120 znaków)
