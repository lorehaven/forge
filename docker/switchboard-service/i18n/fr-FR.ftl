# required
header_label = Switchboard
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Langue
theme_label = Thème

# ── Accueil ──────────────────────────────────────────────────────────────────

ui_header_home = Switchboard
ui_home_button = Accueil
ui_home_title = Services
ui_home_no_services = Aucun service n'est actuellement activé.
ui_home_group_services = Services disponibles

ui_service_models_dashboard_title = Registre des modèles d'IA
ui_service_models_dashboard_desc = Parcourir les modèles disponibles.

ui_service_vllm_management_title = Gestion vLLM
ui_service_vllm_management_desc = Gérer les instances vLLM en cours et en lancer de nouvelles.

# ── Tableau de bord des modèles ──────────────────────────────────────────────

ui_common_cancel = Annuler
ui_header_dashboard = Tableau de bord
ui_header_models = Modèles
ui_models_search_placeholder = Rechercher des modèles...
ui_models_gpu_total = Total :
ui_models_gpu_free = Libre :
ui_models_sort_name_asc = nom ▴
ui_models_sort_name_desc = nom ▾
ui_models_sort_params_asc = paramètres ▴
ui_models_sort_params_desc = paramètres ▾
ui_models_sort_vram_asc = vram ▴
ui_models_sort_vram_desc = vram ▾
ui_models_tab_all = Tous
ui_models_tab_hf = HF
ui_models_tab_gguf = GGUF
ui_models_filter_all_quants = toutes les quantifications
ui_models_filter_all_contexts = tous les contextes
ui_models_filter_vllm_only = vLLM uniquement
ui_header_vllm = vLLM
ui_vllm_launch_new = Lancer une instance vLLM
ui_vllm_running_instances = Instances en cours
ui_vllm_launch_modal_title = Lancer une instance vLLM
ui_vllm_form_model = Modèle
ui_vllm_form_host = Hôte
ui_vllm_form_port = Port
ui_vllm_form_namespace = Espace de noms
ui_vllm_form_quant = Quantification
ui_vllm_form_dtype = Dtype
ui_vllm_form_device = Périphérique
ui_vllm_form_limit_mm = Limite multimodale
ui_vllm_form_max_len = Longueur max. du modèle
ui_vllm_form_gpu_util = Utilisation mémoire GPU
ui_vllm_form_prefix_caching = Activer le cache de préfixes
ui_vllm_form_task = Tâche
ui_vllm_form_tool_calling = Appels d'outils
ui_vllm_launch_confirm = Lancer
ui_models_card_delete_tooltip = Supprimer le modèle
ui_models_card_params = Paramètres
ui_models_card_context = Contexte
ui_models_card_quant = Quant
ui_models_card_layers = Couches
ui_models_card_hidden = Caché
ui_models_card_fits_yes = Compatible : OUI
ui_models_card_fits_no = Compatible : NON
ui_models_card_best = Meilleur
ui_models_card_minimum = Minimum
ui_models_card_vram = VRAM
ui_models_card_margin = Marge
ui_models_card_estimate_btn = Estimations
ui_models_modal_estimates_title = Estimations
ui_models_modal_estimates_filter_all = Toutes
ui_models_modal_estimates_filter_fits = Compatible
ui_models_modal_estimates_filter_nofit = Incompatible
ui_models_modal_estimates_filter_all_contexts = Tous les contextes
ui_models_modal_estimates_filter_all_quants = Toutes les quantifications
ui_models_modal_delete_title = Confirmer la suppression
ui_models_modal_delete_text = Voulez-vous vraiment supprimer physiquement ce modèle du disque ?
ui_models_modal_delete_confirm = Supprimer
ui_models_quant_fp16 = fp16
ui_models_quant_bf16 = bf16
ui_models_quant_fp8 = fp8
ui_models_quant_int8 = int8
ui_models_quant_awq = awq
ui_models_quant_gptq = gptq
ui_models_quant_q8_0 = q8_0
ui_models_quant_q6_k = q6_k
ui_models_quant_q5_k_m = q5_k_m
ui_models_quant_q5_0 = q5_0
ui_models_quant_q4_k_m = q4_k_m
ui_models_quant_q4_0 = q4_0
ui_models_quant_q3_k_m = q3_k_m
ui_models_quant_q2_k = q2_k

# ── Modèles (fragments dynamiques) ───────────────────────────────────────────

ui_gpu_unavailable = GPU : n/d
ui_models_card_no_estimates = Aucune estimation

# ── Gestion vLLM ─────────────────────────────────────────────────────────────

ui_vllm_launch_modal_subtitle = Configurez un point de terminaison, le budget mémoire et une quantification optionnelle à l'exécution.
ui_vllm_form_select_model = -- sélectionner un modèle --
ui_vllm_no_instances = Aucune instance en cours
ui_vllm_stop_tooltip = Arrêter l'instance
ui_vllm_stop_modal_title = Arrêter l'instance vLLM
ui_vllm_stop_modal_text = Voulez-vous vraiment arrêter cette instance ?
ui_vllm_stop_modal_confirm = Arrêter l'instance
ui_vllm_unknown_model = Modèle inconnu
ui_vllm_meta_id = ID
ui_vllm_meta_namespace = Espace de noms
ui_vllm_meta_endpoint = Point de terminaison
ui_vllm_meta_status = État
ui_vllm_meta_started = Démarré
ui_vllm_meta_gpu_util = Utilisation GPU
ui_vllm_status_running = en cours
ui_vllm_status_starting = démarrage
ui_vllm_status_pending = en attente
ui_vllm_status_failed = en échec
ui_vllm_status_terminating = arrêt en cours

ui_vllm_fit_select_model = Sélectionnez un modèle pour estimer la VRAM requise.
ui_vllm_fit_no_estimate = Aucune estimation correspondante disponible.
ui_vllm_fit_wont_fit_budget = Ne tiendra pas : le modèle nécessite ~{ $model } Go pour la longueur maximale sélectionnée, mais l'utilisation mémoire GPU n'autorise que { $budget } Go
ui_vllm_fit_wont_fit_free = Ne tient pas actuellement : vLLM réservera ~{ $required } Go, mais seulement { $free } Go sont libres
ui_vllm_fit_tight = Ajustement serré : le modèle nécessite ~{ $model } Go et vLLM réservera ~{ $required } Go, laissant { $remaining } Go libres
ui_vllm_fit_ok = Devrait tenir : le modèle nécessite ~{ $model } Go et vLLM réservera ~{ $required } Go
ui_vllm_fit_note_cpu = Exécution sur CPU - l'ajustement de la VRAM GPU n'est pas évalué. Nécessite une version de vLLM compatible CPU.

# ── Codes d'erreur API ───────────────────────────────────────────────────────

api_error_model_name_empty = Le nom du modèle ne peut pas être vide
api_error_vllm_launch_failed = Échec du lancement de l'instance vLLM
api_error_vllm_stop_failed = Échec de l'arrêt de l'instance vLLM
api_error_vllm_list_failed = Échec de la récupération des instances vLLM
api_error_instance_not_found = Instance introuvable
api_error_invalid_model_path = Chemin de modèle non valide
api_error_model_not_found = Modèle introuvable sur le disque
api_error_model_delete_failed = Échec de la suppression du modèle

# ── Connexion ────────────────────────────────────────────────────────────────

ui_login_sign_in = Connexion
ui_login_username = Nom d'utilisateur
ui_login_password = Mot de passe
ui_login_submit = Se connecter
ui_login_invalid_credentials = Identifiants non valides
ui_logout = Se déconnecter
