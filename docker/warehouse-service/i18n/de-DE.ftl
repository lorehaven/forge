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

# ── API-Fehlercodes ──────────────────────────────────────────────────────────

api_error_internal = Etwas ist schiefgelaufen. Bitte versuche es erneut.
api_error_digest_required = Das Löschen eines Manifests erfordert eine Digest-Referenz
api_error_invalid_repository = Ungültiger Repository-Name
api_error_invalid_digest = Ungültiger Digest
api_error_manifest_unknown = Unbekanntes Manifest
api_error_crate_version_not_found = Crate-Version nicht gefunden

# ── Anmeldung ────────────────────────────────────────────────────────────────

ui_login_sign_in = Anmelden
ui_login_username = Benutzername
ui_login_password = Passwort
ui_login_submit = Anmelden
ui_login_invalid_credentials = Ungültige Anmeldedaten
ui_logout = Abmelden
