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
ui_sort_library = Orden de la biblioteca
ui_sort_top = Lo más popular
ui_sort_artists = Mejores artistas
ui_best = Mejores canciones: { $songs }
ui_sort_suggested = Sugeridas (sin descargar)
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
ui_genres_label = Solo estos géneros
ui_genres_hint = Conservar solo los álbumes de estos géneros de Deezer, separados por ; (p. ej. Rap/Hip Hop). Para un artista cuyo perfil de Deezer mezcla homónimos. Vacío conserva todos los álbumes. Se aplica en la próxima sincronización, que también quita los álbumes que ya no coinciden.
ui_cat_saved = Categoría guardada
ui_catrename_title = Renombrar categoría
ui_catrename_aria = Renombrar la categoría «{ $name }»
ui_catrename_label = Nombre
ui_catrename_hint = Cambia las etiquetas de las canciones de sus artistas; los archivos no se mueven.
ui_catrename_done = Categoría renombrada a { $name }

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
ui_step_images = Imágenes
ui_step_images_text = Busca imágenes de artistas, álbumes y sencillos en Deezer. Una canción muestra la portada de su álbum (la de un sencillo es la suya, si no, la imagen del artista). Solo se obtiene lo que falta.
ui_find_images = Buscar imágenes
ui_retry_images = Reintentar faltantes
ui_retry_images_title = Volver a pedir las imágenes que Deezer no tenía
ui_run_all_title = Ejecutar todo
ui_run_all_text = Sincroniza la biblioteca, busca enlaces (vuelve a probar las canciones sin resultado), descarga, convierte y busca imágenes: los pasos de abajo, uno tras otro. Se detiene en el primer paso que falla. Se aplican las opciones de abajo (sencillos, el artista elegido en el lateral, los límites).
ui_run_all = Ejecutar todo
ui_run_all_hint = Sincronizar, buscar enlaces, descargar, convertir y buscar imágenes de una vez
ui_stat_artists = Artistas
ui_stat_artists_sub = { $n } acreditados en total
ui_stat_albums = Álbumes
ui_stat_songs = Canciones
ui_stat_downloaded = Descargadas
ui_stat_downloaded_sub = { $pct } % de las canciones
ui_stat_missing = Faltan
ui_stat_missing_sub = { $linked } con enlace · { $unlinked } sin
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

# ── Harmonogram ──────────────────────────────────────────────────────────────
ui_nav_schedule = Programación
ui_sched_title = Ejecución nocturna
ui_sched_intro = Mantiene la biblioteca cada día a una hora fija: sincronización con valoraciones al día, búsqueda de enlaces que faltan, descarga, conversión y búsqueda de imágenes.
ui_sched_saved = Programación guardada.
ui_sched_off = La ejecución nocturna está desactivada.
ui_sched_next = Próxima ejecución: { $when }
ui_sched_never = Aún no se ha ejecutado.
ui_sched_last = Última ejecución: { $when } - { $result }
ui_sched_enabled = Ejecutar cada día
ui_sched_enabled_title = Desactivado: nada se ejecuta solo
ui_sched_time = Hora
ui_sched_zone = Zona horaria
ui_sched_zone_title = Nombre IANA como Europe/Warsaw; vacío: zona del servidor
ui_sched_when = Cuándo
ui_sched_stages = Etapas
ui_sched_options = Opciones
ui_sched_sync = Sincronizar
ui_sched_sync_title = Obtener las discografías de los artistas seguidos
ui_sched_links = Buscar enlaces
ui_sched_links_title = Buscar enlaces de YouTube, de nuevo para canciones sin coincidencia
ui_sched_download = Descargar
ui_sched_download_title = Descargar canciones con enlace y sin archivo; los fallos anteriores se reintentan
ui_sched_convert = Convertir
ui_sched_convert_title = Convertir las descargas a mp3
ui_sched_suggestions = Sugerencias
ui_sched_suggestions_title = Actualizar los artistas sugeridos y sus mejores canciones (Descubrir)
ui_sched_images = Imágenes
ui_sched_images_title = Buscar las imágenes de artistas, álbumes y sencillos nuevos
ui_sched_fresh = Valoraciones al día
ui_sched_fresh_title = Pedir a Deezer cada canción de nuevo para tener valoraciones actuales (más lento)
ui_sched_save = Guardar
ui_sched_run_now = Ejecutar ahora
ui_sched_run_now_title = Iniciar estas etapas ahora, como la ejecución nocturna
ui_purge_title = Borrar archivos de la lista negra
ui_purge_text = Borra los archivos de audio de los álbumes en la lista negra. La lista negra solo quita el álbum de la biblioteca; sus archivos descargados se quedan hasta que hagas esto. Las canciones que también están en un álbum que conservas y los archivos que añadiste tú nunca se borran. Primero la vista previa: lista lo que se iría.
ui_purge_preview = Vista previa
ui_purge_preview_title = Listar los archivos que se borrarían; no borra nada
ui_purge_delete = Borrar archivos de la lista negra
ui_purge_delete_title = Borrar para siempre los archivos de la vista previa
ui_fixlinks_title = Corregir enlaces erróneos
ui_fixlinks_text = Busca canciones cuyo enlace de YouTube no coincide (el título del vídeo no contiene el de la canción), aparta sus archivos de audio en .mismatch-old y busca un enlace nuevo, descarga y convierte. No forma parte del pipeline. Primero la vista previa: lista las canciones y no cambia nada. Se aplica el límite de buscar enlaces.
ui_fixlinks_preview = Vista previa
ui_fixlinks_preview_title = Listar las canciones cuyo enlace no coincide; no cambia nada
ui_fixlinks_run = Corregir enlaces
ui_fixlinks_run_title = Reemplazar los enlaces y archivos de las canciones de la vista previa

# La lista negra y el reemplazo de una canción
ui_nav_blacklist = Lista negra
ui_bls_title = Lista negra
ui_bls_intro = Álbumes que se omiten en cada sincronización. Añade uno por nombre, cambia una entrada o devuelve un álbum.
ui_bls_empty = No hay nada en la lista negra.
ui_bls_add = Añadir un álbum
ui_bls_edit = Editar
ui_bls_remove = Devolver
ui_bls_artist = Artista
ui_bls_album = Álbum
ui_bls_dialog_add = Añadir un álbum a la lista negra
ui_bls_dialog_edit = Cambiar la entrada
ui_bls_dialog_hint = El álbum, con sus otras ediciones, sale del índice ahora y se omite en las próximas sincronizaciones.
ui_bls_saved = Guardado; { $n } pistas salieron del índice
ui_bls_let_go = Devuelto: la próxima sincronización lo traerá
ui_replace = Reemplazar el archivo
ui_replace_title = Descargar este enlace y ponerlo en lugar del archivo de la canción
ui_replace_hint = «Reemplazar el archivo» descarga este enlace y cambia con él el audio de la canción (para una canción o versión equivocada). El archivo anterior se conserva si la descarga falla.
ui_replace_need_link = Primero escribe la URL del vídeo
ui_replace_started = Reemplazando la canción: mira Actividad
