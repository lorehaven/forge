# required
header_label = Warehouse
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Sprache
theme_label = Design

# ── Startseite ───────────────────────────────────────────────────────────────

ui_header_home = Warehouse
ui_home_button = Startseite
ui_home_title = Dienste
ui_home_no_services = Derzeit sind keine Dienste aktiviert.
ui_home_group_services = Registry-Dienste
ui_home_group_files = Dateispeicher

ui_service_docker_title = Docker-Registry
ui_service_docker_desc = Images, Tags und Manifeste durchsuchen.

ui_service_crates_title = Crates-Registry
ui_service_crates_desc = Veröffentlichte Crates und Versionen durchsuchen.

ui_service_files_title = Dateispeicher
ui_service_files_desc = Dateien durchsuchen und verwalten.

ui_service_apk_title = APK-Registry
ui_service_apk_desc = Veröffentlichte Android-Pakete und Versionen durchsuchen.

# ── Docker ───────────────────────────────────────────────────────────────────

ui_header_docker = Warehouse — Docker-Repository-Explorer
ui_repositories = Repositories
ui_tags = Tags
ui_tags_for = Tags für
ui_metadata = Metadaten
ui_metadata_for = Metadaten für
ui_col_tag = Tag
ui_col_digest = Digest
ui_col_media_type = Medientyp
ui_empty_select_repo = Wähle ein Repository aus dem Baum.
ui_empty_no_tags = Keine Tags gefunden.
ui_empty_select_tag = Wähle einen Tag, um die Metadaten anzusehen.
ui_meta_tag = Tag
ui_meta_digest = Digest
ui_meta_media_type = Medientyp
ui_meta_manifest_size = Manifestgröße
ui_meta_unknown = unbekannt

# ── Dateien ──────────────────────────────────────────────────────────────────

ui_header_files = Warehouse — Dateispeicher-Explorer
ui_files_storages = Speicher
ui_files_entries = Einträge
ui_files_entries_for = Einträge für
ui_files_metadata = Metadaten
ui_files_col_name = Name
ui_files_col_type = Typ
ui_files_col_size = Größe
ui_files_col_actions = Aktionen
ui_files_upload = Hochladen
ui_files_download_folder = Ordner herunterladen
ui_files_add_folder = Ordner hinzufügen
ui_files_bulk_download = Massen-Download
ui_files_bulk_delete = Massen-Löschung
ui_files_up = Nach oben
ui_files_empty_storages = Keine Speicher konfiguriert.
ui_files_empty_dir = Das Verzeichnis ist leer.

# ── Crates ───────────────────────────────────────────────────────────────────

ui_header_crates = Warehouse — Crates-Registry-Explorer
ui_crates = Crates
ui_crates_empty = Es wurden noch keine Crates veröffentlicht.

ui_versions = Versionen
ui_versions_for = Versionen für
ui_col_version = Version
ui_col_status = Status
ui_col_checksum = Prüfsumme

ui_status_active = aktiv
ui_status_yanked = zurückgezogen
ui_yank = Zurückziehen
ui_unyank = Wiederherstellen

ui_empty_select_crate = Wähle einen Crate aus der Liste.
ui_empty_no_versions = Keine Versionen gefunden.
ui_empty_select_version = Wähle eine Version, um die Metadaten anzusehen.

ui_meta_version = Version
ui_meta_status = Status
ui_meta_checksum = Prüfsumme
ui_meta_rust_version = Rust-Version
ui_meta_links = Links
ui_meta_features = Features
ui_meta_deps = Abhängigkeiten

ui_deps_normal = Abhängigkeiten
ui_deps_build = Build-Abhängigkeiten
ui_deps_dev = Entwicklungs-Abhängigkeiten

# ── Allgemein ────────────────────────────────────────────────────────────────

ui_common_cancel = Abbrechen
ui_common_delete = Löschen
ui_modal_delete_title = Löschen bestätigen

# ── Docker (dynamisch) ───────────────────────────────────────────────────────

ui_docker_delete_confirm_text = Möchtest du dieses Image wirklich löschen?
ui_delete_image = Image löschen
ui_meta_bytes = {$size} Bytes

# ── Crates (dynamisch) ───────────────────────────────────────────────────────

ui_yank_version = Version zurückziehen
ui_unyank_version = Version wiederherstellen

# ── Dateiverwaltung ────────────────────────────────────────────────────────

ui_storages_title = Speicher
ui_storages_empty = Es ist kein Speicher konfiguriert.
ui_storages_detail_title = Speicher
ui_storages_select = Wähle einen Speicher aus der Liste.
ui_storage_static_badge = statisch
ui_storage_kind = Art
ui_storage_owner = Eigentümer
ui_storage_usage = Nutzung
ui_storage_max_file = Max. Dateigröße
ui_storage_sync = Synchronisierung
ui_storage_sync_on = aktiviert
ui_storage_sync_off = deaktiviert
ui_storage_created = Erstellt
ui_storage_root = Wurzelverzeichnis
ui_storage_files_title = Dateien
ui_storage_files_empty = Dieser Speicher enthält keine Dateien.
ui_storage_files_truncated = Es wird nur die erste Seite der Dateien angezeigt.
ui_storage_not_found = Kein solcher Speicher.
ui_storage_root_unreadable = Das Wurzelverzeichnis des Speichers konnte nicht gelesen werden.
ui_file_download = herunterladen
ui_file_delete = Löschen
ui_storage_edit_title = Speicher bearbeiten
ui_storage_quota_gib = Kontingent (GiB)
ui_storage_max_file_mib = Max. Dateigröße (MiB)
ui_storage_clear_max_file = Max. Dateigröße auf Standard zurücksetzen
ui_storage_save = Änderungen speichern
ui_storage_new_title = Neuer Speicher
ui_storage_name = Name
ui_storage_create = Speicher erstellen
ui_storage_delete = Speicher löschen
ui_storage_delete_title = Speicher löschen
ui_storage_delete_confirm_text = Diesen Speicher und seinen gesamten Inhalt löschen? Dies kann nicht rückgängig gemacht werden.

# ── APK-Verwaltung ─────────────────────────────────────────────────────────

ui_header_apk = Warehouse - APK-Registry-Explorer
ui_apk_packages = Pakete
ui_apk_empty = Es wurden noch keine APK-Pakete veröffentlicht.
ui_apk_empty_select_version = Wähle eine Version, um die Metadaten anzuzeigen.
ui_apk_meta_package = Paket
ui_apk_meta_version_name = Versionsname
ui_apk_meta_version_code = Versionscode
ui_apk_meta_label = Bezeichnung
ui_apk_meta_min_sdk = Min-SDK
ui_apk_meta_target_sdk = Ziel-SDK
ui_apk_meta_size = Größe
ui_apk_meta_uploaded_by = Hochgeladen von
ui_apk_meta_permissions = Berechtigungen
ui_apk_yank = Zurückziehen
ui_apk_unyank = Wiederherstellen

# ── API-Fehlercodes ──────────────────────────────────────────────────────────

api_error_internal = Etwas ist schiefgelaufen. Bitte versuche es erneut.
api_error_digest_required = Das Löschen eines Manifests erfordert eine Digest-Referenz
api_error_invalid_repository = Ungültiger Repository-Name
api_error_invalid_digest = Ungültiger Digest
api_error_manifest_unknown = Unbekanntes Manifest
api_error_crate_version_not_found = Crate-Version nicht gefunden
api_error_forbidden = Du hast keine Berechtigung dafür.
api_error_files_disabled = Der Dateispeicher ist in dieser Bereitstellung nicht aktiviert.
api_error_apk_disabled = Die APK-Registry ist in dieser Bereitstellung nicht aktiviert.
api_error_invalid_storage_name = Speichernamen dürfen nur Buchstaben, Ziffern, - und _ enthalten.
api_error_storage_owner_required = Ein Eigentümer ist erforderlich.
api_error_storage_owner_unknown = Kein solcher Benutzer als Eigentümer dieses Speichers.
api_error_storage_name_static_clash = Ein statischer Speicher verwendet diesen Namen bereits.
api_error_storage_exists = Ein Speicher mit diesem Namen existiert bereits.
api_error_storage_not_found = Kein solcher dynamischer Speicher.
api_error_invalid_quota = Das Kontingent muss eine nicht negative Zahl sein.
api_error_invalid_max_file = Die max. Dateigröße muss eine nicht negative Zahl sein.
api_error_invalid_path = Ungültiger Dateipfad.
api_error_path_escapes_storage = Dieser Pfad führt aus dem Speicher heraus.
api_error_no_dynamic_root = Für diese Bereitstellung ist kein Wurzelverzeichnis für dynamischen Speicher konfiguriert.
api_error_file_not_found = Keine solche Datei.

# ── Anmeldung ────────────────────────────────────────────────────────────────

ui_login_sign_in = Anmelden
ui_login_username = Benutzername
ui_login_password = Passwort
ui_login_submit = Anmelden
ui_login_invalid_credentials = Ungültige Anmeldedaten
ui_logout = Abmelden
