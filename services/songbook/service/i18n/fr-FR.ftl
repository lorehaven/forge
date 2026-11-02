# required
header_label = Songbook
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Langue
theme_label = Thème

# ── Navigation ───────────────────────────────────────────────────────────────
ui_nav_library = Bibliothèque
ui_nav_pipeline = Pipeline
ui_nav_discover = Découvrir
ui_nav_activity = Activité
ui_followed = Artistes suivis
ui_all_artists = Tous les artistes
ui_add_artist = Ajouter un artiste
ui_not_synced = Pas encore synchronisé
ui_tracks_n = { $n ->
    [one] { $n } titre
   *[other] { $n } titres
}
ui_artist_tooltip = { $name } · Deezer #{ $id } · { $fans } fans
ui_remove_artist_aria = Retirer { $name }
ui_change_category_aria = Changer la catégorie de { $name }
ui_category_tooltip = Catégorie : { $category }
ui_search_label = Rechercher
ui_search_placeholder = Rechercher titres, albums, artistes
ui_job_running = En cours : { $name }…
ui_job_failed = Échec de { $name }
ui_waiting = Chargement…
ui_session_ended = Votre session a expiré.
ui_sign_in = Se connecter

# ── Bibliothèque ─────────────────────────────────────────────────────────────
ui_library_title = Bibliothèque
ui_library_intro = Parcourez les albums et titres des artistes suivis. Ajoutez ou corrigez à la main les liens YouTube ; mettez sur liste noire les albums d'un autre artiste.
ui_all_categories = Toutes les catégories
ui_all_genres = Tous les genres
ui_links_all = Tous les titres
ui_links_missing = Lien manquant
ui_links_has = Avec lien
ui_links_unofficial = Non officiels seulement
ui_sort_library = Ordre de la bibliothèque
ui_sort_top = Les plus écoutés
ui_sort_artists = Meilleurs artistes
ui_best = Meilleurs titres : { $songs }
ui_sort_suggested = Suggestions (non téléchargés)
ui_summary = { $who } · { $total } titres · { $linked } avec un lien YouTube
ui_summary_shown = affichage des { $shown } premiers
ui_nothing = Aucun résultat.
ui_blacklist = Liste noire
ui_blacklist_title = Ignorer cet album lors des prochaines synchronisations et le retirer de la bibliothèque
ui_manual = manuel
ui_manual_title = Lien défini par vous
ui_official_title = Chaîne officielle / de l'artiste
ui_unofficial_title = Mise en ligne non officielle
ui_no_link = pas de lien
ui_open_youtube_aria = Ouvrir sur YouTube : { $title }
ui_edit_link = Modifier le lien
ui_add_link = Ajouter un lien
ui_edit_link_aria = Modifier le lien de { $title }
ui_add_link_aria = Ajouter un lien pour { $title }

# ── Boîtes de dialogue ───────────────────────────────────────────────────────
ui_cancel = Annuler
ui_save = Enregistrer
ui_add = Ajouter
ui_remove = Retirer
ui_link_title = Lien YouTube
ui_link_url = URL de la vidéo
ui_link_hint = Laissez vide et choisissez « Retirer le lien » pour l'effacer.
ui_link_remove = Retirer le lien
ui_link_search = Chercher sur YouTube ↗
ui_link_saved = Lien enregistré
ui_link_removed = Lien retiré
ui_bl_title = Mettre l'album sur liste noire ?
ui_bl_text = « { $album } » de { $artist } sera retiré et ignoré lors des prochaines synchronisations.
ui_bl_done = Sur liste noire : { $album }
ui_rm_title = Retirer l'artiste ?
ui_rm_text = { $name } ne sera plus suivi. Ses titres restent dans l'index jusqu'à la prochaine synchronisation.
ui_rm_done = Retiré : { $name }
ui_add_title = Ajouter un artiste
ui_add_name = Nom de l'artiste
ui_add_id = Identifiant d'artiste Deezer (facultatif)
ui_add_id_hint = Imposez une correspondance exacte si le nom est ambigu.
ui_add_category = Catégorie
ui_add_category_hint = Modifiable plus tard.
ui_add_done = { $name } ajouté. Lancez la synchronisation pour le récupérer.
ui_add_exists = { $name } est déjà dans la liste
ui_cat_title = Catégorie
ui_cat_label = Catégorie
ui_cat_new = Ou créez une nouvelle catégorie
ui_cat_new_hint = Met à jour les étiquettes des titres de cet artiste ; les fichiers ne bougent pas.
ui_genres_label = Seulement ces genres
ui_genres_hint = Ne garder que les albums de ces genres Deezer, séparés par ; (ex. Rap/Hip Hop). Pour un artiste dont le profil Deezer mélange des homonymes. Vide : tous les albums. Appliqué à la prochaine synchro, qui retire aussi les albums qui ne correspondent plus.
ui_cat_saved = Catégorie enregistrée
ui_catrename_title = Renommer la catégorie
ui_catrename_aria = Renommer la catégorie « { $name } »
ui_catrename_label = Nom
ui_catrename_hint = Met à jour les étiquettes des titres de ses artistes ; les fichiers ne bougent pas.
ui_catrename_done = Catégorie renommée en { $name }

# ── Pipeline ─────────────────────────────────────────────────────────────────
ui_pipeline_title = Pipeline
ui_pipeline_intro = Construisez la bibliothèque pas à pas : récupérer les discographies, trouver les liens YouTube, puis télécharger et convertir l'audio.
ui_scope_one = Les étapes 2 et 3 ne s'appliquent qu'à { $name }. Choisissez « Tous les artistes » dans la barre latérale pour les lancer pour tous.
ui_scope_all = Les étapes 2 et 3 s'appliquent à tous les artistes. Choisissez un artiste dans la barre latérale pour les limiter à lui.
ui_step = Étape { $n }
ui_step_library = Bibliothèque
ui_step_library_text = Récupérer sur Deezer les discographies de chaque artiste suivi.
ui_sync = Synchroniser la bibliothèque
ui_singles = Inclure les singles
ui_singles_title = Récupérer aussi les singles, pas seulement albums et EP
ui_rebuild_tags = Recalculer les étiquettes
ui_rebuild_tags_title = Recalculer les étiquettes de catégorie et de genre de l'index sans rien télécharger
ui_step_links = Liens YouTube
ui_step_links_text = Trouve automatiquement une vidéo pour chaque titre qui n'a pas encore de fichier. Modifiez un lien à la main dans la Bibliothèque.
ui_find_links = Trouver les liens
ui_retry_missing = Réessayer les manquants
ui_retry_missing_title = Réessayer les titres sans correspondance jusqu'ici
ui_max_tracks = Titres max.
ui_max_all = tous
ui_step_download = Télécharger et convertir
ui_step_download_text = Télécharge les vidéos trouvées dans le dossier de musique, puis les convertit en audio sur place.
ui_step_images = Images
ui_step_images_text = Trouve sur Deezer les images des artistes, albums et singles. Un titre affiche la pochette de son album (celle du single lui-même, sinon l'image de l'artiste). Seul ce qui manque est récupéré.
ui_find_images = Trouver les images
ui_retry_images = Réessayer les manquants
ui_retry_images_title = Redemander les images que Deezer n'avait pas
ui_run_all_title = Tout lancer
ui_run_all_text = Synchronise la bibliothèque, cherche les liens (en réessayant les titres sans résultat), télécharge, convertit et trouve les images : les étapes ci-dessous, l’une après l’autre. Elle s’arrête à la première étape en échec. Les options ci-dessous (singles, l’artiste choisi à gauche, les limites) s’appliquent.
ui_run_all = Tout lancer
ui_run_all_hint = Synchroniser, chercher les liens, télécharger, convertir et trouver les images d’un coup
ui_stat_artists = Artistes
ui_stat_artists_sub = { $n } crédités au total
ui_stat_albums = Albums
ui_stat_songs = Titres
ui_stat_downloaded = Téléchargés
ui_stat_downloaded_sub = { $pct } % des titres
ui_stat_missing = Manquants
ui_stat_missing_sub = { $linked } avec un lien · { $unlinked } sans
ui_download = Télécharger
ui_convert = Convertir

# ── Découvrir ────────────────────────────────────────────────────────────────
ui_discover_title = Découvrir
ui_discover_intro = Suggère des artistes liés à ceux que vous suivez, classés selon le nombre de vos artistes auxquels ils sont liés.
ui_current_list = Liste actuelle : { $scope }.
ui_scope = Portée
ui_top = Top
ui_find_similar = Trouver des artistes similaires
ui_no_suggestions = Pas encore de suggestions.
ui_via = { $n }× via { $names }
ui_fab = { $n ->
    [one] Ajouter { $n } artiste et synchroniser
   *[other] Ajouter { $n } artistes et synchroniser
}
ui_added_n = { $n ->
    [one] { $n } artiste ajouté
   *[other] { $n } artistes ajoutés
}

# ── Activité ─────────────────────────────────────────────────────────────────
ui_activity_title = Activité
ui_activity_intro = Progression et journal de la tâche en cours (ou la dernière). Une seule tâche à la fois.
ui_idle = Inactif
ui_status_running = En cours : { $name }…
ui_status_error = Erreur : { $why }
ui_status_finished = Terminé : { $name }
ui_no_job = Aucune tâche n'a encore été lancée.
ui_snack_finished = Terminé : { $name }
ui_snack_failed = Échec de { $name } : { $why }
ui_job_busy = Une tâche est déjà en cours

# ── Shell ────────────────────────────────────────────────────────────────────
ui_home_button = Accueil
ui_dashboard_button = Tableau des services
ui_theme_toggle = Changer de thème
ui_theme_mode = Mode
ui_theme_light = Clair
ui_theme_dark = Sombre
ui_theme_palette = Couleur
ui_palette_green = Vert
ui_palette_teal = Sarcelle
ui_palette_blue = Bleu
ui_palette_purple = Violet
ui_palette_rose = Rose
ui_palette_orange = Orange
ui_user_menu = Menu utilisateur
ui_profile_edit = Modifier le profil
ui_logout = Déconnexion

# ── Harmonogram ──────────────────────────────────────────────────────────────
ui_nav_schedule = Planification
ui_sched_title = Exécution nocturne
ui_sched_intro = Entretient la bibliothèque chaque jour à heure fixe : synchronisation avec notes à jour, recherche des liens manquants, téléchargement, conversion et recherche des images.
ui_sched_saved = Planification enregistrée.
ui_sched_off = L'exécution nocturne est désactivée.
ui_sched_next = Prochaine exécution : { $when }
ui_sched_never = Pas encore exécutée.
ui_sched_last = Dernière exécution : { $when } - { $result }
ui_sched_enabled = Exécuter chaque jour
ui_sched_enabled_title = Désactivé : rien ne démarre seul
ui_sched_time = Heure
ui_sched_zone = Fuseau horaire
ui_sched_zone_title = Nom IANA comme Europe/Warsaw ; vide : fuseau du serveur
ui_sched_when = Quand
ui_sched_stages = Étapes
ui_sched_options = Options
ui_sched_sync = Synchro
ui_sched_sync_title = Récupérer les discographies des artistes suivis
ui_sched_links = Chercher les liens
ui_sched_links_title = Chercher des liens YouTube, de nouveau pour les titres sans résultat
ui_sched_download = Télécharger
ui_sched_download_title = Télécharger les titres avec lien et sans fichier ; les échecs précédents sont retentés
ui_sched_convert = Convertir
ui_sched_convert_title = Convertir les téléchargements en mp3
ui_sched_suggestions = Suggestions
ui_sched_suggestions_title = Actualiser les artistes suggérés et leurs meilleurs titres (Découvrir)
ui_sched_images = Images
ui_sched_images_title = Trouver les images des nouveaux artistes, albums et singles
ui_sched_fresh = Notes à jour
ui_sched_fresh_title = Redemander chaque titre à Deezer pour des notes actuelles (plus lent)
ui_sched_save = Enregistrer
ui_sched_run_now = Lancer maintenant
ui_sched_run_now_title = Lancer ces étapes maintenant, comme l'exécution nocturne
ui_purge_title = Supprimer les fichiers de la liste noire
ui_purge_text = Supprime les fichiers audio des albums en liste noire. La liste noire retire seulement l'album de la bibliothèque ; ses fichiers téléchargés restent jusqu'à ce que vous le fassiez. Les titres présents aussi sur un album conservé et les fichiers ajoutés par vous ne sont jamais supprimés. D'abord l'aperçu : il liste ce qui disparaîtra.
ui_purge_preview = Aperçu
ui_purge_preview_title = Lister les fichiers qui seraient supprimés ; ne supprime rien
ui_purge_delete = Supprimer les fichiers de la liste noire
ui_purge_delete_title = Supprimer définitivement les fichiers de l'aperçu
