# required
header_label = Warehouse
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Język
theme_label = Motyw

# ── Strona główna ────────────────────────────────────────────────────────────

ui_header_home = Warehouse
ui_home_button = Strona główna
ui_home_title = Usługi
ui_home_no_services = Żadne usługi nie są obecnie włączone.
ui_home_group_services = Usługi rejestrów
ui_home_group_files = Magazyny plików

ui_service_docker_title = Rejestr Docker
ui_service_docker_desc = Przeglądaj obrazy, tagi i manifesty.

ui_service_crates_title = Rejestr crate'ów
ui_service_crates_desc = Przeglądaj opublikowane crate'y i wersje.

ui_service_files_title = Magazyn plików
ui_service_files_desc = Przeglądaj i zarządzaj plikami.

# ── Docker ───────────────────────────────────────────────────────────────────

ui_header_docker = Warehouse — przeglądarka repozytoriów Docker
ui_repositories = Repozytoria
ui_tags = Tagi
ui_tags_for = Tagi dla
ui_metadata = Metadane
ui_metadata_for = Metadane dla
ui_col_tag = Tag
ui_col_digest = Skrót
ui_col_media_type = Typ mediów
ui_empty_select_repo = Wybierz repozytorium z drzewa.
ui_empty_no_tags = Nie znaleziono tagów.
ui_empty_select_tag = Wybierz tag, aby zobaczyć metadane.
ui_meta_tag = Tag
ui_meta_digest = Skrót
ui_meta_media_type = Typ mediów
ui_meta_manifest_size = Rozmiar manifestu
ui_meta_unknown = nieznany

# ── Pliki ────────────────────────────────────────────────────────────────────

ui_header_files = Warehouse — przeglądarka magazynu plików
ui_files_storages = Magazyny
ui_files_entries = Wpisy
ui_files_entries_for = Wpisy dla
ui_files_metadata = Metadane
ui_files_col_name = Nazwa
ui_files_col_type = Typ
ui_files_col_size = Rozmiar
ui_files_col_actions = Akcje
ui_files_upload = Prześlij
ui_files_download_folder = Pobierz folder
ui_files_add_folder = Dodaj folder
ui_files_bulk_download = Pobierz zbiorczo
ui_files_bulk_delete = Usuń zbiorczo
ui_files_up = W górę
ui_files_empty_storages = Brak skonfigurowanych magazynów.
ui_files_empty_dir = Katalog jest pusty.

# ── Crate'y ──────────────────────────────────────────────────────────────────

ui_header_crates = Warehouse — przeglądarka rejestru crate'ów
ui_crates = Crate'y
ui_crates_empty = Nie opublikowano jeszcze żadnych crate'ów.

ui_versions = Wersje
ui_versions_for = Wersje dla
ui_col_version = Wersja
ui_col_status = Status
ui_col_checksum = Suma kontrolna

ui_status_active = aktywna
ui_status_yanked = wycofana
ui_yank = Wycofaj
ui_unyank = Przywróć

ui_empty_select_crate = Wybierz crate z listy.
ui_empty_no_versions = Nie znaleziono wersji.
ui_empty_select_version = Wybierz wersję, aby zobaczyć metadane.

ui_meta_version = Wersja
ui_meta_status = Status
ui_meta_checksum = Suma kontrolna
ui_meta_rust_version = Wersja Rusta
ui_meta_links = Linki
ui_meta_features = Funkcje
ui_meta_deps = Zależności

ui_deps_normal = zależności
ui_deps_build = zależności budowania
ui_deps_dev = zależności deweloperskie

# ── Wspólne ──────────────────────────────────────────────────────────────────

ui_common_cancel = Anuluj
ui_common_delete = Usuń
ui_modal_delete_title = Potwierdź usunięcie

# ── Docker (dynamiczne) ──────────────────────────────────────────────────────

ui_docker_delete_confirm_text = Czy na pewno chcesz usunąć ten obraz?
ui_delete_image = Usuń obraz
ui_meta_bytes = {$size} bajtów

# ── Crate'y (dynamiczne) ─────────────────────────────────────────────────────

ui_yank_version = Wycofaj wersję
ui_unyank_version = Przywróć wersję

# ── Kody błędów API ──────────────────────────────────────────────────────────

api_error_internal = Coś poszło nie tak. Spróbuj ponownie.
api_error_digest_required = Usunięcie manifestu wymaga odwołania przez skrót
api_error_invalid_repository = Nieprawidłowa nazwa repozytorium
api_error_invalid_digest = Nieprawidłowy skrót
api_error_manifest_unknown = Nieznany manifest
api_error_crate_version_not_found = Nie znaleziono wersji crate'a

# ── Logowanie ────────────────────────────────────────────────────────────────

ui_login_sign_in = Zaloguj się
ui_login_username = Nazwa użytkownika
ui_login_password = Hasło
ui_login_submit = Zaloguj
ui_login_invalid_credentials = Nieprawidłowe dane logowania
ui_logout = Wyloguj
