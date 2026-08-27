#[path = "support.rs"]
mod support;

#[path = "unit/apk_manifest_tests.rs"]
mod apk_manifest_tests;
#[path = "unit/docker_token_tests.rs"]
mod docker_token_tests;
#[path = "unit/domain_apk_tests.rs"]
mod domain_apk_tests;
#[path = "unit/files_confinement_tests.rs"]
mod files_confinement_tests;
#[path = "unit/files_path_tests.rs"]
mod files_path_tests;
#[path = "unit/files_storage_tests.rs"]
mod files_storage_tests;
#[path = "unit/middleware_auth_tests.rs"]
mod middleware_auth_tests;
#[path = "unit/middleware_limits_tests.rs"]
mod middleware_limits_tests;
#[path = "unit/routers_admin_crates_gc_tests.rs"]
mod routers_admin_crates_gc_tests;
#[path = "unit/routers_admin_docker_gc_tests.rs"]
mod routers_admin_docker_gc_tests;
#[path = "unit/routers_apk_download_tests.rs"]
mod routers_apk_download_tests;
#[path = "unit/routers_apk_latest_tests.rs"]
mod routers_apk_latest_tests;
#[path = "unit/routers_apk_list_tests.rs"]
mod routers_apk_list_tests;
#[path = "unit/routers_apk_metadata_tests.rs"]
mod routers_apk_metadata_tests;
#[path = "unit/routers_apk_mod_tests.rs"]
mod routers_apk_mod_tests;
#[path = "unit/routers_apk_ops_mod_tests.rs"]
mod routers_apk_ops_mod_tests;
#[path = "unit/routers_apk_publish_tests.rs"]
mod routers_apk_publish_tests;
#[path = "unit/routers_apk_unyank_tests.rs"]
mod routers_apk_unyank_tests;
#[path = "unit/routers_apk_yank_tests.rs"]
mod routers_apk_yank_tests;
#[path = "unit/routers_crates_mod_tests.rs"]
mod routers_crates_mod_tests;
#[path = "unit/routers_crates_owners_tests.rs"]
mod routers_crates_owners_tests;
#[path = "unit/routers_crates_search_tests.rs"]
mod routers_crates_search_tests;
#[path = "unit/routers_docker_blob_cancel_upload_tests.rs"]
mod routers_docker_blob_cancel_upload_tests;
#[path = "unit/routers_docker_blob_check_exists_tests.rs"]
mod routers_docker_blob_check_exists_tests;
#[path = "unit/routers_docker_blob_complete_upload_tests.rs"]
mod routers_docker_blob_complete_upload_tests;
#[path = "unit/routers_docker_blob_get_upload_status_tests.rs"]
mod routers_docker_blob_get_upload_status_tests;
#[path = "unit/routers_docker_blob_retrieve_tests.rs"]
mod routers_docker_blob_retrieve_tests;
#[path = "unit/routers_docker_blob_start_upload_tests.rs"]
mod routers_docker_blob_start_upload_tests;
#[path = "unit/routers_docker_blob_upload_chunk_tests.rs"]
mod routers_docker_blob_upload_chunk_tests;
#[path = "unit/routers_docker_manifest_check_exists_tests.rs"]
mod routers_docker_manifest_check_exists_tests;
#[path = "unit/routers_docker_manifest_delete_image_tests.rs"]
mod routers_docker_manifest_delete_image_tests;
#[path = "unit/routers_docker_manifest_get_image_tests.rs"]
mod routers_docker_manifest_get_image_tests;
#[path = "unit/routers_docker_manifest_put_image_tests.rs"]
mod routers_docker_manifest_put_image_tests;
#[path = "unit/routers_docker_mod_tests.rs"]
mod routers_docker_mod_tests;
#[path = "unit/routers_docker_registry_catalog_tests.rs"]
mod routers_docker_registry_catalog_tests;
#[path = "unit/routers_docker_registry_check_tests.rs"]
mod routers_docker_registry_check_tests;
#[path = "unit/routers_docker_registry_storage_tests.rs"]
mod routers_docker_registry_storage_tests;
#[path = "unit/routers_docker_registry_tags_tests.rs"]
mod routers_docker_registry_tags_tests;
#[path = "unit/routers_docker_token_tests.rs"]
mod routers_docker_token_tests;
#[path = "unit/routers_files_authz_tests.rs"]
mod routers_files_authz_tests;
#[path = "unit/routers_files_dynamic_tests.rs"]
mod routers_files_dynamic_tests;
#[path = "unit/routers_files_mod_tests.rs"]
mod routers_files_mod_tests;
#[path = "unit/routers_files_ops_delete_tests.rs"]
mod routers_files_ops_delete_tests;
#[path = "unit/routers_files_ops_download_tests.rs"]
mod routers_files_ops_download_tests;
#[path = "unit/routers_files_ops_list_tests.rs"]
mod routers_files_ops_list_tests;
#[path = "unit/routers_files_ops_upload_tests.rs"]
mod routers_files_ops_upload_tests;
#[path = "unit/routers_files_pagination_tests.rs"]
mod routers_files_pagination_tests;
#[path = "unit/routers_mod_tests.rs"]
mod routers_mod_tests;
#[path = "unit/routers_ui_authz_tests.rs"]
mod routers_ui_authz_tests;
#[path = "unit/routers_ui_common_css_mod_tests.rs"]
mod routers_ui_common_css_mod_tests;
#[path = "unit/routers_ui_common_css_rules_tests.rs"]
mod routers_ui_common_css_rules_tests;
#[path = "unit/routers_ui_common_mod_tests.rs"]
mod routers_ui_common_mod_tests;
#[path = "unit/routers_ui_mod_tests.rs"]
mod routers_ui_mod_tests;
#[path = "unit/routers_ui_pages_apk_catalog_tests.rs"]
mod routers_ui_pages_apk_catalog_tests;
#[path = "unit/routers_ui_pages_auth_tests.rs"]
mod routers_ui_pages_auth_tests;
#[path = "unit/routers_ui_pages_crates_catalog_tests.rs"]
mod routers_ui_pages_crates_catalog_tests;
#[path = "unit/routers_ui_pages_crates_storage_tests.rs"]
mod routers_ui_pages_crates_storage_tests;
#[path = "unit/routers_ui_pages_docker_catalog_tests.rs"]
mod routers_ui_pages_docker_catalog_tests;
#[path = "unit/routers_ui_pages_docker_tags_tests.rs"]
mod routers_ui_pages_docker_tags_tests;
#[path = "unit/routers_ui_pages_files_storages_tests.rs"]
mod routers_ui_pages_files_storages_tests;
#[path = "unit/routers_ui_pages_home_tests.rs"]
mod routers_ui_pages_home_tests;
#[path = "unit/utils_sha256_tests.rs"]
mod utils_sha256_tests;
