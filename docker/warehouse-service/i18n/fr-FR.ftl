# required
header_label = Warehouse
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Langue
theme_label = Thème

# ── Accueil ──────────────────────────────────────────────────────────────────

ui_header_home = Warehouse
ui_home_button = Accueil
ui_home_title = Services
ui_home_no_services = Aucun service n'est actuellement activé.
ui_home_group_services = Services de registre
ui_home_group_files = Stockages de fichiers

ui_service_docker_title = Registre Docker
ui_service_docker_desc = Parcourir les images, les étiquettes et les manifestes.

ui_service_crates_title = Registre de crates
ui_service_crates_desc = Parcourir les crates publiés et leurs versions.

ui_service_files_title = Stockage de fichiers
ui_service_files_desc = Parcourir et gérer les fichiers.

ui_service_apk_title = Registre APK
ui_service_apk_desc = Parcourir les paquets Android publiés et leurs versions.

# ── Docker ───────────────────────────────────────────────────────────────────

ui_header_docker = Warehouse — Explorateur de dépôts Docker
ui_repositories = Dépôts
ui_tags = Étiquettes
ui_tags_for = Étiquettes de
ui_metadata = Métadonnées
ui_metadata_for = Métadonnées de
ui_col_tag = Étiquette
ui_col_digest = Empreinte
ui_col_media_type = Type de média
ui_empty_select_repo = Sélectionnez un dépôt dans l'arborescence.
ui_empty_no_tags = Aucune étiquette trouvée.
ui_empty_select_tag = Sélectionnez une étiquette pour inspecter les métadonnées.
ui_meta_tag = Étiquette
ui_meta_digest = Empreinte
ui_meta_media_type = Type de média
ui_meta_manifest_size = Taille du manifeste
ui_meta_unknown = inconnu

# ── Fichiers ─────────────────────────────────────────────────────────────────

ui_header_files = Warehouse — Explorateur du stockage de fichiers
ui_files_storages = Stockages
ui_files_entries = Entrées
ui_files_entries_for = Entrées de
ui_files_metadata = Métadonnées
ui_files_col_name = Nom
ui_files_col_type = Type
ui_files_col_size = Taille
ui_files_col_actions = Actions
ui_files_upload = Téléverser
ui_files_download_folder = Télécharger le dossier
ui_files_add_folder = Ajouter un dossier
ui_files_bulk_download = Téléchargement groupé
ui_files_bulk_delete = Suppression groupée
ui_files_up = Remonter
ui_files_empty_storages = Aucun stockage configuré.
ui_files_empty_dir = Le répertoire est vide.

# ── Crates ───────────────────────────────────────────────────────────────────

ui_header_crates = Warehouse — Explorateur du registre de crates
ui_crates = Crates
ui_crates_empty = Aucun crate publié pour le moment.

ui_versions = Versions
ui_versions_for = Versions de
ui_col_version = Version
ui_col_status = État
ui_col_checksum = Somme de contrôle

ui_status_active = active
ui_status_yanked = retirée
ui_yank = Retirer
ui_unyank = Restaurer

ui_empty_select_crate = Sélectionnez un crate dans la liste.
ui_empty_no_versions = Aucune version trouvée.
ui_empty_select_version = Sélectionnez une version pour inspecter les métadonnées.

ui_meta_version = Version
ui_meta_status = État
ui_meta_checksum = Somme de contrôle
ui_meta_rust_version = Version de Rust
ui_meta_links = Liens
ui_meta_features = Fonctionnalités
ui_meta_deps = Dépendances

ui_deps_normal = dépendances
ui_deps_build = dépendances de compilation
ui_deps_dev = dépendances de développement

# ── Commun ───────────────────────────────────────────────────────────────────

ui_common_cancel = Annuler
ui_common_delete = Supprimer
ui_modal_delete_title = Confirmer la suppression

# ── Docker (dynamique) ───────────────────────────────────────────────────────

ui_docker_delete_confirm_text = Voulez-vous vraiment supprimer cette image ?
ui_delete_image = Supprimer l'image
ui_meta_bytes = {$size} octets

# ── Crates (dynamique) ───────────────────────────────────────────────────────

ui_yank_version = Retirer la version
ui_unyank_version = Restaurer la version

# ── Gestion des fichiers ───────────────────────────────────────────────────

ui_storages_title = Stockages
ui_storages_empty = Aucun stockage n'est configuré.
ui_storages_detail_title = Stockage
ui_storages_select = Sélectionnez un stockage dans la liste.
ui_storage_static_badge = statique
ui_storage_kind = Type
ui_storage_owner = Propriétaire
ui_storage_usage = Utilisation
ui_storage_max_file = Taille max. de fichier
ui_storage_sync = Synchronisation
ui_storage_sync_on = activée
ui_storage_sync_off = désactivée
ui_storage_created = Créé le
ui_storage_root = Racine
ui_storage_files_title = Fichiers
ui_storage_files_empty = Ce stockage ne contient aucun fichier.
ui_storage_files_truncated = Affichage de la première page de fichiers uniquement.
ui_storage_not_found = Aucun stockage de ce nom.
ui_storage_root_unreadable = Impossible de lire la racine du stockage.
ui_file_download = télécharger
ui_file_delete = Supprimer
ui_storage_edit_title = Modifier le stockage
ui_storage_quota_gib = Quota (Gio)
ui_storage_max_file_mib = Taille max. de fichier (Mio)
ui_storage_clear_max_file = Réinitialiser la taille max. de fichier par défaut
ui_storage_save = Enregistrer les modifications
ui_storage_new_title = Nouveau stockage
ui_storage_name = Nom
ui_storage_create = Créer le stockage
ui_storage_delete = Supprimer le stockage
ui_storage_delete_title = Supprimer le stockage
ui_storage_delete_confirm_text = Supprimer ce stockage et tout son contenu ? Cette action est irréversible.

# ── Gestion des APK ────────────────────────────────────────────────────────

ui_header_apk = Warehouse - Explorateur du registre APK
ui_apk_packages = Paquets
ui_apk_empty = Aucun paquet APK n'a encore été publié.
ui_apk_empty_select_version = Sélectionnez une version pour voir ses métadonnées.
ui_apk_meta_package = Paquet
ui_apk_meta_version_name = Nom de version
ui_apk_meta_version_code = Code de version
ui_apk_meta_label = Libellé
ui_apk_meta_min_sdk = SDK minimal
ui_apk_meta_target_sdk = SDK cible
ui_apk_meta_size = Taille
ui_apk_meta_uploaded_by = Envoyé par
ui_apk_meta_permissions = Autorisations
ui_apk_yank = Retirer
ui_apk_unyank = Restaurer

# ── Codes d'erreur API ───────────────────────────────────────────────────────

api_error_internal = Une erreur s'est produite. Veuillez réessayer.
api_error_digest_required = La suppression d'un manifeste nécessite une référence par empreinte
api_error_invalid_repository = Nom de dépôt non valide
api_error_invalid_digest = Empreinte non valide
api_error_manifest_unknown = Manifeste inconnu
api_error_crate_version_not_found = Version du crate introuvable
api_error_forbidden = Vous n'avez pas l'autorisation de faire cela.
api_error_files_disabled = Le stockage de fichiers n'est pas activé sur ce déploiement.
api_error_apk_disabled = Le registre APK n'est pas activé sur ce déploiement.
api_error_invalid_storage_name = Les noms de stockage ne peuvent contenir que des lettres, des chiffres, - et _.
api_error_storage_owner_required = Un propriétaire est requis.
api_error_storage_owner_unknown = Aucun utilisateur de ce nom pour être propriétaire de ce stockage.
api_error_storage_name_static_clash = Un stockage statique utilise déjà ce nom.
api_error_storage_exists = Un stockage portant ce nom existe déjà.
api_error_storage_not_found = Aucun stockage dynamique de ce nom.
api_error_invalid_quota = Le quota doit être un nombre positif ou nul.
api_error_invalid_max_file = La taille max. de fichier doit être un nombre positif ou nul.
api_error_invalid_path = Chemin de fichier non valide.
api_error_path_escapes_storage = Ce chemin pointe hors du stockage.
api_error_no_dynamic_root = Ce déploiement n'a pas de racine de stockage dynamique configurée.
api_error_file_not_found = Aucun fichier de ce nom.

# ── Connexion ────────────────────────────────────────────────────────────────

ui_login_sign_in = Connexion
ui_login_username = Nom d'utilisateur
ui_login_password = Mot de passe
ui_login_submit = Se connecter
ui_login_invalid_credentials = Identifiants non valides
ui_logout = Se déconnecter
