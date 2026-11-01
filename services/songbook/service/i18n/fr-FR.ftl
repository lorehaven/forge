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
