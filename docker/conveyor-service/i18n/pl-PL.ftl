# required
header_label = Conveyor
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Język
theme_label = Motyw

# ── Shell ────────────────────────────────────────────────────────────────────

ui_header_home = Conveyor
ui_home_button = Strona główna
ui_logout = Wyloguj się

# ── Home ─────────────────────────────────────────────────────────────────────

ui_home_title = Potoki
ui_home_subtitle = Każda kompilacja, z commita który o nią poprosił.

ui_repos_title = Repozytoria
ui_repos_empty = Nie zarejestrowano jeszcze żadnych repozytoriów.

ui_runs_title = Ostatnie przebiegi
ui_runs_empty = Nic jeszcze nie zostało uruchomione.
ui_runs_view_all = Zobacz wszystkie potoki

ui_project_subtitle = Repozytoria i potoki w tym węźle.
ui_project_not_found = Nie znaleziono takiego projektu.

# ── Pipelines ────────────────────────────────────────────────────────────────

ui_pipelines_title = Wszystkie potoki
ui_pipelines_subtitle = Każdy przebieg, od najnowszego.
ui_pager_prev = Poprzednia
ui_pager_next = Następna
ui_pager_page = Strona {$page} z {$total}

# ── Runs ─────────────────────────────────────────────────────────────────────

ui_col_status = Status
ui_col_repository = Repozytorium
ui_col_ref = Referencja
ui_col_commit = Commit
ui_col_trigger = Wyzwalacz
ui_col_when = Kiedy
ui_col_provider = Dostawca
ui_col_branch = Gałąź domyślna
ui_col_state = Stan
ui_col_checks = Kontrole
ui_col_actions = Akcje
ui_repo_enabled = Włączone
ui_repo_disabled = Wyłączone
ui_repo_run_now = Uruchom teraz
ui_chip_not_run = Jeszcze nie uruchomiono
ui_run_not_found = Nie ma takiego przebiegu.
ui_run_reason = Dlaczego się zakończył
ui_meta_trigger = Wyzwalacz
ui_meta_queued = W kolejce
ui_meta_duration = Czas trwania
ui_meta_attempt = Próba
ui_artifacts_title = Artefakty
ui_log_loading = Ładowanie…

# ── Scan ─────────────────────────────────────────────────────────────────────

ui_scan_subtitle = Lint, nieużywane zależności i znane podatności, z ostatniego uruchomienia.
ui_scan_lint_title = Lint
ui_scan_machete_title = Nieużywane zależności
ui_scan_audit_title = Podatności
ui_scan_no_runs = To repozytorium jeszcze się nie uruchomiło.
ui_scan_no_checks = Ostatnie uruchomienie nie uruchomiło lint, machete ani audit. Dodaj `anvil lint`, `anvil machete` lub `anvil audit` jako kroki w .conveyor.toml, aby zobaczyć je tutaj.
ui_scan_repo_not_found = Nie znaleziono takiego repozytorium.
ui_scan_back = Powrót do przeglądu
ui_scan_clean = Niczego nie znaleziono.

# ── Status ───────────────────────────────────────────────────────────────────

ui_status_queued = W kolejce
ui_status_running = W trakcie
ui_status_success = Zaliczony
ui_status_failed = Niepowodzenie
ui_status_cancelled = Anulowany
ui_status_skipped = Pominięty

# ── Zarządzanie repozytoriami ───────────────────────────────────────────────

ui_repos_manage = Zarządzaj repozytoriami
ui_repos_back = Powrót do repozytoriów
ui_repos_back_home = Powrót do strony głównej
ui_repos_add_title = Dodaj repozytorium
ui_repos_owner = Właściciel
ui_repos_name = Nazwa
ui_repos_clone_url = Adres URL klonowania
ui_repos_branch = Gałąź domyślna
ui_repos_project = Projekt
ui_repos_provider = Dostawca
ui_repos_create = Zarejestruj repozytorium
ui_repos_save = Zapisz zmiany
ui_repos_edit = Edytuj
ui_repos_view_only = Nie masz uprawnień do zapisu w projekcie tego repozytorium; te pola są tylko do odczytu.
ui_repos_delete_title = Strefa zagrożenia
ui_repos_delete_hint = Usunięcie repozytorium kasuje historię jego przebiegów. Tej operacji nie można cofnąć.
ui_repos_delete = Usuń repozytorium
ui_repos_not_found = Nie ma takiego repozytorium.
ui_repos_ok_created = Repozytorium zarejestrowane.
ui_repos_ok_saved = Zmiany zapisane.
ui_repos_ok_deleted = Repozytorium usunięte.
ui_repos_err_owner_name_empty = Właściciel i nazwa są wymagane.
ui_repos_err_project_required = Wybierz projekt dla tego repozytorium.
ui_repos_err_bad_clone_url = Ten adres URL klonowania jest nieprawidłowy.
ui_repos_err_forbidden = Nie masz uprawnień do zapisu w tym projekcie.
ui_repos_err_not_found = Nie ma takiego repozytorium.
ui_repos_err_write_failed = Nie udało się tego zapisać. Może już istnieć.
