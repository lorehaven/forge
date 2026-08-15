# required
header_label = Conveyor
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Langue
theme_label = Thème

# ── Shell ────────────────────────────────────────────────────────────────────

ui_header_home = Conveyor
ui_home_button = Accueil
ui_nav_credentials = Identifiants
ui_logout = Se déconnecter

# ── Home ─────────────────────────────────────────────────────────────────────

ui_home_title = Pipelines
ui_home_subtitle = Chaque build, depuis le commit qui l'a demandé.

ui_repos_title = Dépôts
ui_repos_empty = Aucun dépôt n'est encore enregistré.

ui_runs_title = Exécutions récentes
ui_runs_empty = Rien n'a encore été exécuté.
ui_runs_view_all = Voir tous les pipelines

ui_project_subtitle = Dépôts et pipelines sous ce nœud.
ui_project_not_found = Aucun projet de ce type.

# ── Pipelines ────────────────────────────────────────────────────────────────

ui_pipelines_title = Tous les pipelines
ui_pipelines_subtitle = Chaque exécution, la plus récente en premier.
ui_pager_prev = Précédent
ui_pager_next = Suivant
ui_pager_page = Page {$page} sur {$total}

# ── Runs ─────────────────────────────────────────────────────────────────────

ui_col_status = Statut
ui_col_repository = Dépôt
ui_col_ref = Référence
ui_col_commit = Commit
ui_col_trigger = Déclencheur
ui_col_when = Quand
ui_col_provider = Fournisseur
ui_col_branch = Branche par défaut
ui_col_state = État
ui_col_checks = Vérifications
ui_col_actions = Actions
ui_repo_enabled = Activé
ui_repo_disabled = Désactivé
ui_repo_run_now = Exécuter maintenant
ui_chip_not_run = Pas encore exécuté
ui_run_not_found = Aucune exécution de ce type.
ui_run_reason = Pourquoi elle s'est terminée
ui_meta_trigger = Déclencheur
ui_meta_queued = En file
ui_meta_duration = Durée
ui_meta_attempt = Tentative
ui_artifacts_title = Artefacts
ui_log_loading = Chargement…

# ── Scan ─────────────────────────────────────────────────────────────────────

ui_scan_subtitle = Lint, dépendances inutilisées et vulnérabilités connues, à partir de la dernière exécution.
ui_scan_lint_title = Lint
ui_scan_machete_title = Dépendances inutilisées
ui_scan_audit_title = Vulnérabilités
ui_scan_no_runs = Ce dépôt n'a pas encore été exécuté.
ui_scan_no_checks = La dernière exécution n'a exécuté ni lint, ni machete, ni audit. Ajoutez `anvil lint`, `anvil machete` ou `anvil audit` comme étapes dans .conveyor.toml pour les voir ici.
ui_scan_repo_not_found = Dépôt introuvable.
ui_scan_back = Retour à l'aperçu
ui_scan_clean = Rien trouvé.

# ── Status ───────────────────────────────────────────────────────────────────

ui_status_queued = En attente
ui_status_running = En cours
ui_status_success = Réussie
ui_status_failed = Échouée
ui_status_cancelled = Annulée
ui_status_skipped = Ignorée

# ── Administration des dépôts ────────────────────────────────────────────────

ui_repos_manage = Gérer les dépôts
ui_repos_back = Retour aux dépôts
ui_repos_back_home = Retour à l'accueil
ui_repos_add_title = Ajouter un dépôt
ui_repos_owner = Propriétaire
ui_repos_name = Nom
ui_repos_clone_url = URL de clonage
ui_repos_branch = Branche par défaut
ui_repos_project = Projet
ui_repos_provider = Fournisseur
ui_repos_create = Enregistrer le dépôt
ui_repos_save = Enregistrer les modifications
ui_repos_edit = Modifier
ui_repos_view_only = Vous n'avez pas d'accès en écriture au projet de ce dépôt ; ces champs sont en lecture seule.
ui_repos_delete_title = Zone de danger
ui_repos_delete_hint = Supprimer un dépôt efface son historique d'exécutions. Cette action est irréversible.
ui_repos_delete = Supprimer le dépôt
ui_repos_not_found = Aucun dépôt de ce type.
ui_repos_ok_created = Dépôt enregistré.
ui_repos_ok_saved = Modifications enregistrées.
ui_repos_ok_deleted = Dépôt supprimé.
ui_repos_err_owner_name_empty = Le propriétaire et le nom sont requis.
ui_repos_err_project_required = Choisissez un projet pour ce dépôt.
ui_repos_err_bad_clone_url = Cette URL de clonage n'est pas valide.
ui_repos_err_forbidden = Vous n'avez pas d'accès en écriture à ce projet.
ui_repos_err_not_found = Aucun dépôt de ce type.
ui_repos_err_write_failed = Impossible d'enregistrer. Il existe peut-être déjà.

# ── Credentials ──────────────────────────────────────────────────────────────

ui_credentials_title = Identifiants
ui_credentials_empty = Aucun identifiant visible pour vous.
