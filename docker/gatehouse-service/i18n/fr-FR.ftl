header_label = Gatehouse
ui_home_button = Accueil
ui_account_button = Mon compte
ui_logout = Se déconnecter

ui_login_sign_in = Connexion
ui_login_username = Nom d'utilisateur
ui_login_password = Mot de passe
ui_login_submit = Se connecter
ui_login_invalid_credentials = Identifiants non valides
ui_login_forgot_password = Mot de passe oublié ?
ui_login_register = Créer un compte
ui_login_registered_ok = Compte créé. Vérifiez votre e-mail pour confirmer votre adresse, puis connectez-vous.
ui_login_verified_ok = Votre adresse e-mail est vérifiée. Connectez-vous pour continuer.
ui_login_verify_invalid = Ce lien de vérification est invalide ou a expiré.
ui_login_reset_requested_ok = Si ce compte existe, un lien de réinitialisation est en route.
ui_login_reset_ok = Votre mot de passe a été réinitialisé. Connectez-vous avec le nouveau.
ui_login_reset_invalid = Ce lien de réinitialisation est invalide ou a expiré.
ui_login_account_disabled = Ce compte a été désactivé.
ui_login_account_locked = Trop de tentatives échouées. Réessayez dans quelques minutes.
ui_login_mfa_title = Code de vérification
ui_login_mfa_code = Code
ui_login_mfa_hint = Saisissez le code à 6 chiffres de votre application d'authentification.
ui_login_mfa_submit = Vérifier
ui_login_mfa_invalid = Ce code est incorrect. Réessayez.

ui_register_title = Créer un compte
ui_register_email = E-mail
ui_register_submit = Créer le compte
ui_register_have_account = Déjà un compte ? Connectez-vous
ui_register_error_email_invalid = Saisissez une adresse e-mail valide.

ui_forgot_password_title = Réinitialiser votre mot de passe
ui_forgot_password_hint = Nous enverrons un lien de réinitialisation à l'adresse e-mail enregistrée, si elle existe.
ui_forgot_password_submit = Envoyer le lien

ui_reset_title = Choisissez un nouveau mot de passe
ui_reset_new_password = Nouveau mot de passe
ui_reset_submit = Enregistrer le nouveau mot de passe
ui_reset_error_password_empty = Un mot de passe est requis.

ui_home_title = Services
ui_home_subtitle = Une seule connexion pour tout ce qui suit.
ui_home_group_services = Services disponibles
ui_home_no_services = Aucun service n'est actuellement activé.

ui_service_conveyor_title = Conveyor
ui_service_conveyor_desc = Pipelines, builds et déploiements.
ui_service_sage_title = Sage
ui_service_sage_desc = Espace de travail IA : conversations, projets et fichiers.
ui_service_switchboard_title = Switchboard
ui_service_switchboard_desc = Orchestration des modèles et instances GPU.
ui_service_warehouse_title = Warehouse
ui_service_warehouse_desc = Registres de crates, d'images et de fichiers.

ui_home_group_realm = Domaine

ui_admin_title = Utilisateurs
ui_admin_users_title = Utilisateurs
ui_admin_users_desc = Comptes, rôles et ce que chacun peut atteindre.
ui_admin_no_users = Le domaine n'a aucun utilisateur.
ui_admin_you = vous
ui_admin_edit = Modifier
ui_admin_back = Retour aux utilisateurs
ui_admin_grants_all = tous les services
ui_admin_grants_none = aucun accès

ui_admin_create_title = Ajouter un utilisateur
ui_admin_new_username = Nom d'utilisateur
ui_admin_new_password = Mot de passe
ui_admin_new_hint = Un nouvel utilisateur n'a aucun accès. Accordez-le à l'écran suivant.
ui_admin_create = Créer

ui_admin_role = Rôle
ui_admin_role_user = Utilisateur
ui_admin_role_admin = Administrateur
ui_admin_role_service = Compte de service

ui_admin_permissions = Accès
ui_admin_wildcard_note = Ce rôle accorde déjà tous les services ; les choix ci-dessus sont fixés.
ui_admin_new_password_optional = Nouveau mot de passe (laisser vide pour conserver l'actuel)
ui_admin_save = Enregistrer

ui_admin_template_title = Appliquer un modèle
ui_admin_template = Modèle
ui_admin_template_hint = Remplace chaque permission ci-dessous par celles du modèle.
ui_admin_apply_template = Appliquer

ui_admin_delete_title = Supprimer
ui_admin_delete_hint = Supprimer le compte met fin à ses sessions immédiatement.
ui_admin_delete = Supprimer cet utilisateur

ui_admin_status_title = Statut
ui_admin_status_created = Créé
ui_admin_status_last_login = Dernière connexion
ui_admin_status_never = Jamais
ui_admin_status_disabled = Désactivé
ui_admin_status_locked = Verrouillé
ui_admin_status_mfa = Authentification à deux facteurs
ui_admin_status_yes = Oui
ui_admin_status_no = Non
ui_admin_action_disable = Désactiver
ui_admin_action_enable = Activer
ui_admin_action_unlock = Déverrouiller
ui_admin_action_mfa_disable = Forcer la désactivation

ui_admin_ok_created = Utilisateur créé.
ui_admin_ok_saved = Modifications enregistrées.
ui_admin_ok_deleted = Utilisateur supprimé.

ui_admin_forbidden_title = Non autorisé
ui_admin_forbidden = La gestion des utilisateurs requiert le rôle d'administrateur.

ui_admin_error_not_found = Cet utilisateur n'existe pas.
ui_admin_error_username_empty = Un nom d'utilisateur est requis.
ui_admin_error_password_empty = Un mot de passe est requis.
ui_admin_error_exists = Ce nom d'utilisateur est déjà pris.
ui_admin_error_unknown_service = Ce service ne fait pas partie de ce déploiement.
ui_admin_error_last_admin = Le domaine doit conserver au moins un administrateur.
ui_admin_error_self_demote = Vous ne pouvez pas retirer votre propre rôle d'administrateur.
ui_admin_error_self_delete = Vous ne pouvez pas supprimer votre propre compte.
ui_admin_error_self_disable = Vous ne pouvez pas désactiver votre propre compte.
ui_admin_error_unknown_template = Ce modèle de permissions n'existe pas.
ui_admin_error_roles_require_admin = Seul un administrateur peut attribuer le rôle admin ou service.
ui_admin_error_mfa_code_invalid = Ce code est incorrect - réessayez.
ui_admin_error_internal = La modification n'a pas pu être enregistrée.

ui_account_title = Mon compte
ui_account_profile_title = Profil
ui_account_display_name = Nom affiché
ui_account_avatar_url = URL de l'avatar
ui_account_title_field = Titre
ui_account_timezone = Fuseau horaire
ui_account_preferred_locale = Langue préférée
ui_account_new_password = Nouveau mot de passe
ui_account_password_hint = Laissez vide pour conserver votre mot de passe actuel.
ui_account_save = Enregistrer les modifications
ui_account_ok_saved = Votre compte a été mis à jour.
ui_account_ok_mfa_enabled = L'authentification à deux facteurs est maintenant activée.
ui_account_ok_mfa_disabled = L'authentification à deux facteurs a été désactivée.

ui_account_mfa_title = Authentification à deux facteurs
ui_account_mfa_enabled = L'authentification à deux facteurs est activée sur votre compte.
ui_account_mfa_disabled = L'authentification à deux facteurs n'est pas activée.
ui_account_mfa_enable = Configurer l'authentification à deux facteurs
ui_account_mfa_disable = Désactiver l'authentification à deux facteurs

ui_account_mfa_enroll_title = Configurer l'authentification à deux facteurs
ui_account_mfa_enroll_hint = Scannez ceci avec votre application d'authentification, ou saisissez le secret manuellement ci-dessous.
ui_account_mfa_secret = Secret
ui_account_mfa_code = Code
ui_account_mfa_verify = Vérifier et activer
