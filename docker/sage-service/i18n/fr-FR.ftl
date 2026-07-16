# required
header_label = Sage
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Langue
theme_label = Thème

# ── Accueil ──────────────────────────────────────────────────────────────────

ui_header_home = Sage
ui_home_button = Accueil
ui_home_title = Assistant
ui_home_no_services = L'espace de travail Sage AI est prêt.
ui_home_group_services = Services disponibles

# ── Chat ─────────────────────────────────────────────────────────────────────

ui_chat_input_placeholder = Demandez ce que vous voulez à Sage...
ui_chat_send_button = Envoyer
ui_chat_welcome_message = Bonjour ! Je suis Sage, votre assistant IA. Comment puis-je vous aider aujourd'hui ?
ui_chat_no_model_available = Aucun modèle n'est actuellement sélectionné ou disponible.
ui_chat_user_label = Vous
ui_chat_ai_label = Sage

# ── Chat (dynamique) ─────────────────────────────────────────────────────────

ui_chat_thinking = Sage réfléchit...
ui_chat_regenerating = Sage régénère...
ui_chat_edit = Modifier
ui_chat_regenerate = Régénérer
ui_chat_save_submit = Enregistrer et envoyer
ui_chat_error = Erreur
ui_chat_sources = Sources
ui_chat_source_chunk = fragment {$index}
ui_chat_no_models = Aucun modèle disponible
ui_chat_switchboard_unavailable = Switchboard indisponible
ui_chat_welcome_tooltip = Bonjour ! Je suis Sage...
ui_chat_attach_tooltip = Joindre un fichier (pdf, txt, csv, md)
ui_chat_untitled = Nouvelle discussion
ui_chat_delete_confirm_text = Voulez-vous vraiment supprimer cette conversation ?
ui_chat_this_conversation = cette conversation

ui_code_copy = Copier
ui_code_copied = Copié !
ui_code_copy_error = Erreur

# ── Barre latérale ───────────────────────────────────────────────────────────

ui_sidebar_new_chat = Nouvelle discussion
ui_sidebar_projects = Projets
ui_sidebar_new = Nouveau
ui_sidebar_history = Historique
ui_sidebar_files = Fichiers

# ── Commun ───────────────────────────────────────────────────────────────────

ui_common_cancel = Annuler
ui_common_delete = Supprimer
ui_modal_delete_title = Confirmer la suppression

# ── Projets ──────────────────────────────────────────────────────────────────

ui_projects_new_title = Créer un nouveau projet
ui_projects_name_label = Nom du projet
ui_projects_create = Créer

# ── Fichiers ─────────────────────────────────────────────────────────────────

ui_file_status_ready = prêt
ui_file_status_processing = en traitement
ui_file_status_uploaded = en file d'attente
ui_file_status_failed = échec
ui_files_retry_tooltip = Relancer le traitement
ui_files_remove_tooltip = Annuler / retirer
ui_files_download_tooltip = Télécharger
ui_files_empty_project = Aucun fichier téléversé pour ce projet.
ui_files_delete_confirm_text = Voulez-vous vraiment supprimer ce fichier ?

# ── Initialisation ───────────────────────────────────────────────────────────

ui_init_title = Préparation de Sage
ui_init_subtitle = Lancement des modèles dont Sage a besoin avant de pouvoir discuter.
ui_init_waiting = En attente de la réponse du service de modèles…
ui_init_status_running = En cours
ui_init_status_starting = Démarrage…
ui_init_status_queued = En file d'attente
ui_init_status_failed = Échec
ui_init_status_unknown = Connexion…
ui_init_embedding_tag = (embedding)

# ── Codes d'erreur API ───────────────────────────────────────────────────────

api_error_internal = Une erreur s'est produite. Veuillez réessayer.
api_error_instance_not_found = Instance de modèle introuvable
api_error_embedding_model_chat = Le modèle sélectionné est un modèle d'embedding et ne peut pas être utilisé pour discuter
api_error_switchboard_unavailable = Le service de modèles est indisponible
api_error_stream_failed = Impossible de démarrer le flux de discussion
api_error_regenerate_non_assistant = Seuls les messages de l'assistant peuvent être régénérés
api_error_no_parent_message = Le message n'a pas de parent à partir duquel régénérer
api_error_parent_not_found = Message parent introuvable
api_error_no_models_available = Aucun modèle d'IA disponible pour la régénération
api_error_metrics_not_found = Aucune métrique trouvée pour ce profil
api_error_costs_not_found = Aucun coût trouvé pour cet utilisateur
api_error_missing_conversation_id = Identifiant de conversation manquant
api_error_conversation_create_failed = Impossible de démarrer la conversation
api_error_conversation_not_found = Conversation introuvable
api_error_project_not_found = Projet introuvable
api_error_file_not_found = Fichier introuvable
api_error_file_content_not_found = Contenu du fichier introuvable
api_error_file_scope_required = Vous devez indiquer exactement une conversation ou un projet
api_error_missing_file_name = Nom de fichier manquant
api_error_unsupported_file_type = Type de fichier non pris en charge. Autorisés : pdf, txt, csv, md
api_error_file_too_large = Le fichier dépasse la taille maximale autorisée
api_error_file_empty = Le fichier est vide
api_error_file_limit_reached = Limite de fichiers atteinte pour cette conversation/ce projet
api_error_file_already_processing = Le fichier est déjà en cours de traitement
api_error_postgres_required = Le stockage de fichiers nécessite une base de données Postgres

# ── Connexion ────────────────────────────────────────────────────────────────

ui_login_sign_in = Connexion
ui_login_username = Nom d'utilisateur
ui_login_password = Mot de passe
ui_login_submit = Se connecter
ui_login_invalid_credentials = Identifiants non valides
ui_logout = Se déconnecter
