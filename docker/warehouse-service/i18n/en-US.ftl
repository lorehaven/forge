# required
header_label = Warehouse
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Language
theme_label = Theme

# ── Home ─────────────────────────────────────────────────────────────────────

ui_header_home = Warehouse
ui_home_button = Home
ui_home_title = Services
ui_home_no_services = No services are currently enabled.
ui_home_group_services = Registry Services
ui_home_group_files = File Storages

ui_service_docker_title = Docker Registry
ui_service_docker_desc = Browse images, tags, and manifests.

ui_service_crates_title = Crates Registry
ui_service_crates_desc = Browse published crates and versions.

ui_service_files_title = File Storage
ui_service_files_desc = Browse and manage plain files.

# ── Docker ───────────────────────────────────────────────────────────────────

ui_header_docker = Warehouse - Docker Repository Explorer
ui_repositories = Repositories
ui_tags = Tags
ui_tags_for = Tags for
ui_metadata = Metadata
ui_metadata_for = Metadata for
ui_col_tag = Tag
ui_col_digest = Digest
ui_col_media_type = Media Type
ui_empty_select_repo = Select a repository from the tree.
ui_empty_no_tags = No tags found.
ui_empty_select_tag = Select a tag to inspect metadata.
ui_meta_tag = Tag
ui_meta_digest = Digest
ui_meta_media_type = Media Type
ui_meta_manifest_size = Manifest Size
ui_meta_unknown = unknown

# ── Files ────────────────────────────────────────────────────────────────────

ui_header_files = Warehouse - File Storage Explorer
ui_files_storages = Storages
ui_files_entries = Entries
ui_files_entries_for = Entries for
ui_files_metadata = Metadata
ui_files_col_name = Name
ui_files_col_type = Type
ui_files_col_size = Size
ui_files_col_actions = Actions
ui_files_upload = Upload
ui_files_download_folder = Download folder
ui_files_add_folder = Add folder
ui_files_bulk_download = Bulk download
ui_files_bulk_delete = Bulk delete
ui_files_up = Up
ui_files_empty_storages = No storages configured.
ui_files_empty_dir = Directory is empty.

# ── Crates ───────────────────────────────────────────────────────────────────

ui_header_crates = Warehouse - Crates Registry Explorer
ui_crates = Crates
ui_crates_empty = No crates published yet.

ui_versions = Versions
ui_versions_for = Versions for
ui_col_version = Version
ui_col_status = Status
ui_col_checksum = Checksum

ui_status_active = active
ui_status_yanked = yanked
ui_yank = Yank
ui_unyank = Unyank

ui_empty_select_crate = Select a crate from the list.
ui_empty_no_versions = No versions found.
ui_empty_select_version = Select a version to inspect metadata.

ui_meta_version = Version
ui_meta_status = Status
ui_meta_checksum = Checksum
ui_meta_rust_version = Rust version
ui_meta_links = Links
ui_meta_features = Features
ui_meta_deps = Dependencies

ui_deps_normal = dependencies
ui_deps_build = build-dependencies
ui_deps_dev = dev-dependencies

# ── Common ───────────────────────────────────────────────────────────────────

ui_common_cancel = Cancel
ui_common_delete = Delete
ui_modal_delete_title = Confirm Delete

# ── Docker (dynamic) ─────────────────────────────────────────────────────────

ui_docker_delete_confirm_text = Are you sure you want to delete this image?
ui_delete_image = Delete Image
ui_meta_bytes = {$size} bytes

# ── Crates (dynamic) ─────────────────────────────────────────────────────────

ui_yank_version = Yank Version
ui_unyank_version = Unyank Version

# ── API error codes ──────────────────────────────────────────────────────────

api_error_internal = Something went wrong. Please try again.
api_error_digest_required = Manifest deletion requires a digest reference
api_error_invalid_repository = Invalid repository name
api_error_invalid_digest = Invalid digest
api_error_manifest_unknown = Manifest unknown
api_error_crate_version_not_found = Crate version not found

# ── Auth ─────────────────────────────────────────────────────────────────────

ui_login_sign_in = Sign in
ui_login_username = Username
ui_login_password = Password
ui_login_submit = Log in
ui_login_invalid_credentials = Invalid credentials
ui_logout = Log out
