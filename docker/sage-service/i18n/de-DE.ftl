# required
header_label = Sage
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Sprache
theme_label = Design

# ── Startseite ───────────────────────────────────────────────────────────────

ui_header_home = Sage
ui_home_button = Startseite
ui_home_title = Assistent
ui_home_no_services = Der Sage-KI-Arbeitsbereich ist bereit.
ui_home_group_services = Verfügbare Dienste

# ── Chat ─────────────────────────────────────────────────────────────────────

ui_chat_input_placeholder = Frag Sage, was du möchtest...
ui_chat_send_button = Senden
ui_chat_welcome_message = Hallo! Ich bin Sage, dein KI-Assistent. Wie kann ich dir heute helfen?
ui_chat_no_model_available = Derzeit ist kein Modell ausgewählt oder verfügbar.
ui_chat_user_label = Du
ui_chat_ai_label = Sage

# ── Chat (dynamisch) ─────────────────────────────────────────────────────────

ui_chat_thinking = Sage denkt nach...
ui_chat_regenerating = Sage generiert neu...
ui_chat_edit = Bearbeiten
ui_chat_regenerate = Neu generieren
ui_chat_save_submit = Speichern und senden
ui_chat_error = Fehler
ui_chat_sources = Quellen
ui_chat_source_chunk = Abschnitt {$index}
ui_chat_no_models = Keine Modelle verfügbar
ui_chat_switchboard_unavailable = Switchboard nicht erreichbar
ui_chat_welcome_tooltip = Hallo! Ich bin Sage...
ui_chat_attach_tooltip = Datei anhängen (Bilder, pdf, txt, csv, md, html, json, yaml, Quellcode…)
ui_chat_untitled = Neuer Chat
ui_chat_delete_confirm_text = Möchtest du diese Unterhaltung wirklich löschen?
ui_chat_this_conversation = diese Unterhaltung

ui_code_copy = Kopieren
ui_code_copied = Kopiert!
ui_code_copy_error = Fehler

# ── Seitenleiste ─────────────────────────────────────────────────────────────

ui_sidebar_new_chat = Neuer Chat
ui_sidebar_projects = Projekte
ui_sidebar_new = Neu
ui_sidebar_history = Verlauf
ui_sidebar_files = Dateien

# ── Allgemein ────────────────────────────────────────────────────────────────

ui_common_cancel = Abbrechen
ui_common_delete = Löschen
ui_modal_delete_title = Löschen bestätigen

# ── Projekte ─────────────────────────────────────────────────────────────────

ui_projects_new_title = Neues Projekt erstellen
ui_projects_name_label = Projektname
ui_projects_create = Erstellen

# ── Dateien ──────────────────────────────────────────────────────────────────

ui_file_status_ready = bereit
ui_file_status_processing = in Verarbeitung
ui_file_status_uploaded = in Warteschlange
ui_file_status_failed = fehlgeschlagen
ui_files_retry_tooltip = Verarbeitung erneut versuchen
ui_files_remove_tooltip = Abbrechen / entfernen
ui_files_download_tooltip = Herunterladen
ui_files_empty_project = Für dieses Projekt wurden keine Dateien hochgeladen.
ui_files_delete_confirm_text = Möchtest du diese Datei wirklich löschen?

# ── Initialisierung ──────────────────────────────────────────────────────────

ui_init_title = Sage wird vorbereitet
ui_init_subtitle = Die Modelle, die Sage benötigt, werden gestartet, bevor du chatten kannst.
ui_init_waiting = Warten auf Antwort des Modelldienstes…
ui_init_status_running = Läuft
ui_init_status_starting = Startet…
ui_init_status_queued = In Warteschlange
ui_init_status_failed = Fehlgeschlagen
ui_init_status_unknown = Verbinden…
ui_init_embedding_tag = (Embedding)

# ── API-Fehlercodes ──────────────────────────────────────────────────────────

api_error_internal = Etwas ist schiefgelaufen. Bitte versuche es erneut.
api_error_instance_not_found = Modellinstanz nicht gefunden
api_error_embedding_model_chat = Das ausgewählte Modell ist ein Embedding-Modell und kann nicht zum Chatten verwendet werden
api_error_switchboard_unavailable = Der Modelldienst ist nicht erreichbar
api_error_stream_failed = Der Chat-Stream konnte nicht gestartet werden
api_error_regenerate_non_assistant = Nur Assistentennachrichten können neu generiert werden
api_error_no_parent_message = Die Nachricht hat keine übergeordnete Nachricht zum Neugenerieren
api_error_parent_not_found = Übergeordnete Nachricht nicht gefunden
api_error_no_models_available = Keine KI-Modelle zum Neugenerieren verfügbar
api_error_metrics_not_found = Keine Metriken für dieses Profil gefunden
api_error_costs_not_found = Keine Kosten für diesen Benutzer gefunden
api_error_missing_conversation_id = Unterhaltungs-ID fehlt
api_error_conversation_create_failed = Die Unterhaltung konnte nicht gestartet werden
api_error_conversation_not_found = Unterhaltung nicht gefunden
api_error_project_not_found = Projekt nicht gefunden
api_error_file_not_found = Datei nicht gefunden
api_error_file_content_not_found = Dateiinhalt nicht gefunden
api_error_file_scope_required = Es muss genau eine Unterhaltung oder ein Projekt angegeben werden
api_error_missing_file_name = Dateiname fehlt
api_error_unsupported_file_type = Nicht unterstützter Dateityp. Erlaubt: Bilder (png, jpg, webp, gif), pdf, txt, csv, md, html, json, yaml, toml, xml und gängige Quellcode-Dateien
api_error_image_not_processable = Bilder werden nicht für die Suche indexiert; sie werden mit Ihrer Nachricht direkt an das Modell gesendet
api_error_file_too_large = Die Datei überschreitet die maximal zulässige Größe
api_error_file_empty = Die Datei ist leer
api_error_file_limit_reached = Dateilimit für diese Unterhaltung/dieses Projekt erreicht
api_error_file_already_processing = Die Datei wird bereits verarbeitet
api_error_postgres_required = Die Dateispeicherung erfordert eine Postgres-Datenbank

# ── Anmeldung ────────────────────────────────────────────────────────────────

ui_login_sign_in = Anmelden
ui_login_username = Benutzername
ui_login_password = Passwort
ui_login_submit = Anmelden
ui_login_invalid_credentials = Ungültige Anmeldedaten
ui_logout = Abmelden
