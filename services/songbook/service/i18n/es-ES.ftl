# required
header_label = Songbook
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Idioma
theme_label = Tema

# ── Navegación ───────────────────────────────────────────────────────────────
ui_nav_library = Biblioteca
ui_nav_pipeline = Canalización
ui_nav_discover = Descubrir
ui_nav_activity = Actividad
ui_followed = Artistas seguidos
ui_all_artists = Todos los artistas
ui_add_artist = Añadir artista
ui_not_synced = Aún sin sincronizar
ui_tracks_n = { $n ->
    [one] { $n } canción
   *[other] { $n } canciones
}
ui_artist_tooltip = { $name } · Deezer #{ $id } · { $fans } fans
ui_remove_artist_aria = Quitar { $name }
ui_change_category_aria = Cambiar la categoría de { $name }
ui_category_tooltip = Categoría: { $category }
ui_search_label = Buscar
ui_search_placeholder = Buscar canciones, álbumes, artistas
ui_job_running = En curso: { $name }…
ui_job_failed = { $name } falló
ui_waiting = Cargando…
ui_session_ended = Tu sesión ha terminado.
ui_sign_in = Iniciar sesión

# ── Biblioteca ───────────────────────────────────────────────────────────────
ui_library_title = Biblioteca
ui_library_intro = Explora los álbumes y canciones de tus artistas seguidos. Añade o corrige a mano los enlaces de YouTube; pon en la lista negra los álbumes de otro artista.
ui_all_categories = Todas las categorías
ui_all_genres = Todos los géneros
ui_links_all = Todas las canciones
ui_links_missing = Sin enlace
ui_links_has = Con enlace
ui_links_unofficial = Solo no oficiales
ui_summary = { $who } · { $total } canciones · { $linked } con enlace de YouTube
ui_summary_shown = se muestran las primeras { $shown }
ui_nothing = Nada coincide.
ui_blacklist = Lista negra
ui_blacklist_title = Omitir este álbum en las próximas sincronizaciones y quitarlo de la biblioteca
ui_manual = manual
ui_manual_title = Enlace puesto por ti
ui_official_title = Canal oficial / del artista
ui_unofficial_title = Subida no oficial
ui_no_link = sin enlace
ui_open_youtube_aria = Abrir en YouTube: { $title }
ui_edit_link = Editar enlace
ui_add_link = Añadir enlace
ui_edit_link_aria = Editar enlace de { $title }
ui_add_link_aria = Añadir enlace a { $title }

# ── Diálogos ─────────────────────────────────────────────────────────────────
ui_cancel = Cancelar
ui_save = Guardar
ui_add = Añadir
ui_remove = Quitar
ui_link_title = Enlace de YouTube
ui_link_url = URL del vídeo
ui_link_hint = Déjalo vacío y elige «Quitar enlace» para borrarlo.
ui_link_remove = Quitar enlace
ui_link_search = Buscar en YouTube ↗
ui_link_saved = Enlace guardado
ui_link_removed = Enlace quitado
ui_bl_title = ¿Poner el álbum en la lista negra?
ui_bl_text = «{ $album }» de { $artist } se quitará y se omitirá en las próximas sincronizaciones.
ui_bl_done = En la lista negra: { $album }
ui_rm_title = ¿Quitar artista?
ui_rm_text = { $name } dejará de seguirse. Sus canciones siguen en el índice hasta la próxima sincronización.
ui_rm_done = Quitado: { $name }
ui_add_title = Añadir artista
ui_add_name = Nombre del artista
ui_add_id = ID de artista de Deezer (opcional)
ui_add_id_hint = Fija una coincidencia exacta si el nombre es ambiguo.
ui_add_category = Categoría
ui_add_category_hint = Se puede cambiar después.
ui_add_done = Añadido: { $name }. Ejecuta la sincronización para obtenerlo.
ui_add_exists = { $name } ya está en la lista
ui_cat_title = Categoría
ui_cat_label = Categoría
ui_cat_new = O crea una categoría nueva
ui_cat_new_hint = Cambia las etiquetas de las canciones de este artista; los archivos no se mueven.
ui_cat_saved = Categoría guardada

# ── Canalización ─────────────────────────────────────────────────────────────
ui_pipeline_title = Canalización
ui_pipeline_intro = Construye la biblioteca paso a paso: obtén las discografías, busca enlaces de YouTube y descarga y convierte el audio.
ui_scope_one = Los pasos 2 y 3 se ejecutan solo para { $name }. Elige «Todos los artistas» en la barra lateral para ejecutarlos para todos.
ui_scope_all = Los pasos 2 y 3 se ejecutan para todos los artistas. Elige un artista en la barra lateral para limitarlos a él.
ui_step = Paso { $n }
ui_step_library = Biblioteca
ui_step_library_text = Obtén de Deezer las discografías de cada artista seguido.
ui_sync = Sincronizar biblioteca
ui_singles = Incluir sencillos
ui_singles_title = Obtener también sencillos, no solo álbumes y EP
ui_rebuild_tags = Rehacer etiquetas
ui_rebuild_tags_title = Recalcular las etiquetas de categoría y género del índice sin descargar
ui_step_links = Enlaces de YouTube
ui_step_links_text = Busca automáticamente un vídeo por cada canción que aún no tiene archivo. Edita cualquier enlace a mano en la Biblioteca.
ui_find_links = Buscar enlaces
ui_retry_missing = Reintentar faltantes
ui_retry_missing_title = Reintentar las canciones que antes no tuvieron coincidencia
ui_max_tracks = Máx. canciones
ui_max_all = todas
ui_step_download = Descargar y convertir
ui_step_download_text = Descarga los vídeos encontrados en la carpeta de música y conviértelos a audio en el mismo sitio.
ui_download = Descargar
ui_convert = Convertir

# ── Descubrir ────────────────────────────────────────────────────────────────
ui_discover_title = Descubrir
ui_discover_intro = Sugiere artistas relacionados con los que sigues, ordenados por con cuántos de tus artistas se relacionan.
ui_current_list = Lista actual: { $scope }.
ui_scope = Ámbito
ui_top = Máx.
ui_find_similar = Buscar artistas parecidos
ui_no_suggestions = Aún no hay sugerencias.
ui_via = { $n }× vía { $names }
ui_fab = { $n ->
    [one] Añadir { $n } artista y sincronizar
   *[other] Añadir { $n } artistas y sincronizar
}
ui_added_n = { $n ->
    [one] { $n } artista añadido
   *[other] { $n } artistas añadidos
}

# ── Actividad ────────────────────────────────────────────────────────────────
ui_activity_title = Actividad
ui_activity_intro = Progreso y registro de la tarea en curso (o la última). Solo se ejecuta una a la vez.
ui_idle = Inactivo
ui_status_running = En curso: { $name }…
ui_status_error = Error: { $why }
ui_status_finished = Terminado: { $name }
ui_no_job = Aún no se ha ejecutado ninguna tarea.
ui_snack_finished = Terminado: { $name }
ui_snack_failed = { $name } falló: { $why }
ui_job_busy = Ya hay una tarea en curso

# ── Shell ────────────────────────────────────────────────────────────────────
ui_home_button = Inicio
ui_dashboard_button = Panel de servicios
ui_theme_toggle = Cambiar tema
ui_theme_mode = Modo
ui_theme_light = Claro
ui_theme_dark = Oscuro
ui_theme_palette = Color
ui_palette_green = Verde
ui_palette_teal = Turquesa
ui_palette_blue = Azul
ui_palette_purple = Morado
ui_palette_rose = Rosa
ui_palette_orange = Naranja
ui_user_menu = Menú de usuario
ui_profile_edit = Editar perfil
ui_logout = Cerrar sesión
