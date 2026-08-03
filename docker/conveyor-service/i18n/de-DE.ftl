# required
header_label = Conveyor
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Sprache
theme_label = Design

# ── Shell ────────────────────────────────────────────────────────────────────

ui_header_home = Conveyor
ui_home_button = Startseite
ui_logout = Abmelden

# ── Home ─────────────────────────────────────────────────────────────────────

ui_home_title = Pipelines
ui_home_subtitle = Jeder Build, vom Commit der ihn angefordert hat.

ui_repos_title = Repositorys
ui_repos_empty = Es sind noch keine Repositorys registriert.

ui_runs_title = Letzte Durchläufe
ui_runs_empty = Es wurde noch nichts ausgeführt.
ui_runs_view_all = Alle Pipelines anzeigen

ui_project_subtitle = Repositorys und Pipelines unter diesem Knoten.
ui_project_not_found = Kein solches Projekt.

# ── Pipelines ────────────────────────────────────────────────────────────────

ui_pipelines_title = Alle Pipelines
ui_pipelines_subtitle = Jeder Durchlauf, neueste zuerst.
ui_pager_prev = Zurück
ui_pager_next = Weiter
ui_pager_page = Seite {$page} von {$total}

# ── Runs ─────────────────────────────────────────────────────────────────────

ui_col_status = Status
ui_col_repository = Repository
ui_col_ref = Ref
ui_col_commit = Commit
ui_col_trigger = Auslöser
ui_col_when = Wann
ui_col_provider = Anbieter
ui_col_branch = Standard-Branch
ui_col_state = Zustand
ui_col_checks = Prüfungen
ui_col_actions = Aktionen
ui_repo_enabled = Aktiviert
ui_repo_disabled = Deaktiviert
ui_repo_run_now = Jetzt ausführen
ui_chip_not_run = Noch nicht ausgeführt
ui_run_not_found = Kein solcher Durchlauf.
ui_run_reason = Warum er endete
ui_meta_trigger = Auslöser
ui_meta_queued = Eingereiht
ui_meta_duration = Dauer
ui_meta_attempt = Versuch
ui_artifacts_title = Artefakte
ui_log_loading = Wird geladen…

# ── Scan ─────────────────────────────────────────────────────────────────────

ui_scan_subtitle = Lint, ungenutzte Abhängigkeiten und bekannte Schwachstellen, aus dem letzten Lauf.
ui_scan_lint_title = Lint
ui_scan_machete_title = Ungenutzte Abhängigkeiten
ui_scan_audit_title = Schwachstellen
ui_scan_no_runs = Dieses Repository wurde noch nicht ausgeführt.
ui_scan_no_checks = Der letzte Lauf hat weder lint, machete noch audit ausgeführt. Füge `anvil lint`, `anvil machete` oder `anvil audit` als Schritte in .conveyor.toml hinzu, um sie hier zu sehen.
ui_scan_repo_not_found = Kein solches Repository.
ui_scan_back = Zurück zur Übersicht
ui_scan_clean = Nichts gefunden.

# ── Status ───────────────────────────────────────────────────────────────────

ui_status_queued = In Warteschlange
ui_status_running = Läuft
ui_status_success = Erfolgreich
ui_status_failed = Fehlgeschlagen
ui_status_cancelled = Abgebrochen
ui_status_skipped = Übersprungen

# ── Repository-Verwaltung ────────────────────────────────────────────────────

ui_repos_manage = Repositorys verwalten
ui_repos_back = Zurück zu den Repositorys
ui_repos_back_home = Zurück zur Startseite
ui_repos_add_title = Repository hinzufügen
ui_repos_owner = Besitzer
ui_repos_name = Name
ui_repos_clone_url = Clone-URL
ui_repos_branch = Standard-Branch
ui_repos_project = Projekt
ui_repos_provider = Anbieter
ui_repos_create = Repository registrieren
ui_repos_save = Änderungen speichern
ui_repos_edit = Bearbeiten
ui_repos_view_only = Du hast keinen Schreibzugriff auf das Projekt dieses Repositorys; diese Felder sind schreibgeschützt.
ui_repos_delete_title = Gefahrenzone
ui_repos_delete_hint = Das Löschen eines Repositorys entfernt auch dessen Lauf-Historie. Dies kann nicht rückgängig gemacht werden.
ui_repos_delete = Repository löschen
ui_repos_not_found = Kein solches Repository.
ui_repos_ok_created = Repository registriert.
ui_repos_ok_saved = Änderungen gespeichert.
ui_repos_ok_deleted = Repository gelöscht.
ui_repos_err_owner_name_empty = Besitzer und Name sind erforderlich.
ui_repos_err_project_required = Wähle ein Projekt für dieses Repository.
ui_repos_err_bad_clone_url = Diese Clone-URL ist ungültig.
ui_repos_err_forbidden = Du hast keinen Schreibzugriff auf dieses Projekt.
ui_repos_err_not_found = Kein solches Repository.
ui_repos_err_write_failed = Das konnte nicht gespeichert werden. Es existiert möglicherweise bereits.
