# required
header_label = Songbook
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Sprache
theme_label = Design

# ── Navigation ───────────────────────────────────────────────────────────────
ui_nav_library = Bibliothek
ui_nav_pipeline = Pipeline
ui_nav_discover = Entdecken
ui_nav_activity = Aktivität
ui_followed = Gefolgte Künstler
ui_all_artists = Alle Künstler
ui_add_artist = Künstler hinzufügen
ui_not_synced = Noch nicht synchronisiert
ui_tracks_n = { $n ->
    [one] { $n } Titel
   *[other] { $n } Titel
}
ui_artist_tooltip = { $name } · Deezer #{ $id } · { $fans } Fans
ui_remove_artist_aria = { $name } entfernen
ui_change_category_aria = Kategorie von { $name } ändern
ui_category_tooltip = Kategorie: { $category }
ui_search_label = Suchen
ui_search_placeholder = Titel, Alben, Künstler suchen
ui_job_running = Läuft: { $name }…
ui_job_failed = { $name } fehlgeschlagen
ui_waiting = Wird geladen…
ui_session_ended = Deine Sitzung ist abgelaufen.
ui_sign_in = Anmelden

# ── Bibliothek ───────────────────────────────────────────────────────────────
ui_library_title = Bibliothek
ui_library_intro = Durchsuche Alben und Titel der gefolgten Künstler. YouTube-Links von Hand ergänzen oder korrigieren; Alben anderer Künstler auf die schwarze Liste setzen.
ui_all_categories = Alle Kategorien
ui_all_genres = Alle Genres
ui_links_all = Alle Titel
ui_links_missing = Link fehlt
ui_links_has = Mit Link
ui_links_unofficial = Nur inoffiziell
ui_sort_library = Bibliotheksreihenfolge
ui_sort_top = Top
ui_sort_artists = Top-Künstler
ui_best = Beste Titel: { $songs }
ui_sort_suggested = Vorgeschlagen (nicht geladen)
ui_summary = { $who } · { $total } Titel · { $linked } mit YouTube-Link
ui_summary_shown = die ersten { $shown } werden angezeigt
ui_nothing = Nichts passt.
ui_blacklist = Sperren
ui_blacklist_title = Dieses Album bei künftigen Synchronisierungen überspringen und aus der Bibliothek entfernen
ui_manual = manuell
ui_manual_title = Von dir gesetzter Link
ui_official_title = Offizieller Kanal / Künstlerkanal
ui_unofficial_title = Inoffizieller Upload
ui_no_link = kein Link
ui_open_youtube_aria = Auf YouTube öffnen: { $title }
ui_edit_link = Link bearbeiten
ui_add_link = Link hinzufügen
ui_edit_link_aria = Link bearbeiten: { $title }
ui_add_link_aria = Link hinzufügen: { $title }

# ── Dialoge ──────────────────────────────────────────────────────────────────
ui_cancel = Abbrechen
ui_save = Speichern
ui_add = Hinzufügen
ui_remove = Entfernen
ui_link_title = YouTube-Link
ui_link_url = Video-URL
ui_link_hint = Leer lassen und „Link entfernen“ wählen, um ihn zu löschen.
ui_link_remove = Link entfernen
ui_link_search = Auf YouTube suchen ↗
ui_link_saved = Link gespeichert
ui_link_removed = Link entfernt
ui_bl_title = Album sperren?
ui_bl_text = „{ $album }“ von { $artist } wird entfernt und bei künftigen Synchronisierungen übersprungen.
ui_bl_done = Gesperrt: { $album }
ui_rm_title = Künstler entfernen?
ui_rm_text = { $name } wird nicht mehr gefolgt. Die Titel bleiben bis zur nächsten Synchronisierung im Index.
ui_rm_done = Entfernt: { $name }
ui_add_title = Künstler hinzufügen
ui_add_name = Name des Künstlers
ui_add_id = Deezer-Künstler-ID (optional)
ui_add_id_hint = Eine genaue ID festlegen, wenn der Name mehrdeutig ist.
ui_add_category = Kategorie
ui_add_category_hint = Kann später geändert werden.
ui_add_done = { $name } hinzugefügt. Starte die Synchronisierung, um ihn abzurufen.
ui_add_exists = { $name } ist schon in der Liste
ui_cat_title = Kategorie
ui_cat_label = Kategorie
ui_cat_new = Oder eine neue Kategorie anlegen
ui_cat_new_hint = Setzt die Tags der Titel dieses Künstlers neu; Dateien bleiben, wo sie sind.
ui_genres_label = Nur diese Genres
ui_genres_hint = Nur Alben in diesen Deezer-Genres behalten, getrennt durch ; (z. B. Rap/Hip Hop). Für Künstler, deren Deezer-Profil Personen gleichen Namens vermischt. Leer behält alle Alben. Gilt ab dem nächsten Sync, der auch nicht mehr passende Alben entfernt.
ui_cat_saved = Kategorie gespeichert
ui_catrename_title = Kategorie umbenennen
ui_catrename_aria = Kategorie „{ $name }“ umbenennen
ui_catrename_label = Name
ui_catrename_hint = Setzt die Tags der Titel ihrer Künstler neu; die Dateien bleiben, wo sie sind.
ui_catrename_done = Kategorie umbenannt in { $name }

# ── Pipeline ─────────────────────────────────────────────────────────────────
ui_pipeline_title = Pipeline
ui_pipeline_intro = Baue die Bibliothek Schritt für Schritt: Diskografien holen, YouTube-Links finden, dann Audio herunterladen und konvertieren.
ui_scope_one = Schritt 2 und 3 laufen nur für { $name }. Wähle in der Seitenleiste „Alle Künstler“, um sie für alle auszuführen.
ui_scope_all = Schritt 2 und 3 laufen für alle Künstler. Wähle einen Künstler in der Seitenleiste, um sie darauf zu beschränken.
ui_step = Schritt { $n }
ui_step_library = Bibliothek
ui_step_library_text = Diskografien aller gefolgten Künstler von Deezer holen.
ui_sync = Bibliothek synchronisieren
ui_singles = Singles einbeziehen
ui_singles_title = Auch Singles holen, nicht nur Alben und EPs
ui_rebuild_tags = Tags neu berechnen
ui_rebuild_tags_title = Kategorie- und Genre-Tags im Index neu berechnen, ohne abzurufen
ui_step_links = YouTube-Links
ui_step_links_text = Findet automatisch ein Video für jeden Titel, der noch keine Datei hat. Links kannst du in der Bibliothek von Hand ändern.
ui_find_links = Links finden
ui_retry_missing = Fehlende wiederholen
ui_retry_missing_title = Titel erneut versuchen, für die es bisher keinen Treffer gab
ui_max_tracks = Max. Titel
ui_max_all = alle
ui_step_download = Herunterladen & konvertieren
ui_step_download_text = Die gefundenen Videos in den Musikordner laden und dort in Audio umwandeln.
ui_step_images = Bilder
ui_step_images_text = Bilder für Künstler, Alben und Singles bei Deezer suchen. Ein Titel zeigt das Cover seines Albums (bei einer Single deren eigenes Cover, sonst das Bild des Künstlers). Geholt wird nur, was fehlt.
ui_find_images = Bilder suchen
ui_retry_images = Fehlende wiederholen
ui_retry_images_title = Erneut nach den Bildern fragen, die Deezer nicht hatte
ui_run_all_title = Alles ausführen
ui_run_all_text = Bibliothek synchronisieren, Links suchen (Titel ohne Treffer werden erneut versucht), herunterladen, konvertieren und Bilder suchen: die Schritte unten, nacheinander. Beim ersten fehlgeschlagenen Schritt wird gestoppt. Die Optionen unten (Singles, der links gewählte Künstler, die Limits) gelten.
ui_run_all = Alles ausführen
ui_run_all_hint = Synchronisieren, Links suchen, herunterladen, konvertieren und Bilder suchen in einem Durchgang
ui_stat_artists = Künstler
ui_stat_artists_sub = { $n } insgesamt genannt
ui_stat_albums = Alben
ui_stat_songs = Titel
ui_stat_downloaded = Heruntergeladen
ui_stat_downloaded_sub = { $pct } % der Titel
ui_stat_missing = Fehlend
ui_stat_missing_sub = { $linked } mit Link · { $unlinked } ohne
ui_download = Herunterladen
ui_convert = Konvertieren

# ── Entdecken ────────────────────────────────────────────────────────────────
ui_discover_title = Entdecken
ui_discover_intro = Schlägt Künstler vor, die mit den gefolgten verwandt sind, sortiert danach, mit wie vielen deiner Künstler sie verwandt sind.
ui_current_list = Aktuelle Liste: { $scope }.
ui_scope = Bereich
ui_top = Top
ui_find_similar = Ähnliche Künstler finden
ui_no_suggestions = Noch keine Vorschläge.
ui_via = { $n }× über { $names }
ui_fab = { $n ->
    [one] { $n } Künstler hinzufügen & synchronisieren
   *[other] { $n } Künstler hinzufügen & synchronisieren
}
ui_added_n = { $n ->
    [one] { $n } Künstler hinzugefügt
   *[other] { $n } Künstler hinzugefügt
}

# ── Aktivität ────────────────────────────────────────────────────────────────
ui_activity_title = Aktivität
ui_activity_intro = Fortschritt und Protokoll des laufenden (oder letzten) Auftrags. Es läuft immer nur einer.
ui_idle = Leerlauf
ui_status_running = Läuft: { $name }…
ui_status_error = Fehler: { $why }
ui_status_finished = Fertig: { $name }
ui_no_job = Noch kein Auftrag gelaufen.
ui_snack_finished = Fertig: { $name }
ui_snack_failed = { $name } fehlgeschlagen: { $why }
ui_job_busy = Es läuft bereits ein Auftrag

# ── Shell ────────────────────────────────────────────────────────────────────
ui_home_button = Start
ui_dashboard_button = Dienste-Übersicht
ui_theme_toggle = Design wechseln
ui_theme_mode = Modus
ui_theme_light = Hell
ui_theme_dark = Dunkel
ui_theme_palette = Farbe
ui_palette_green = Grün
ui_palette_teal = Petrol
ui_palette_blue = Blau
ui_palette_purple = Violett
ui_palette_rose = Rosé
ui_palette_orange = Orange
ui_user_menu = Benutzermenü
ui_profile_edit = Profil bearbeiten
ui_logout = Abmelden

# ── Harmonogram ──────────────────────────────────────────────────────────────
ui_nav_schedule = Zeitplan
ui_sched_title = Nächtlicher Lauf
ui_sched_intro = Pflegt die Bibliothek täglich zu einer festen Zeit selbst: Sync mit frischen Bewertungen, fehlende Links suchen, laden, konvertieren und Bilder suchen.
ui_sched_saved = Zeitplan gespeichert.
ui_sched_off = Der nächtliche Lauf ist aus.
ui_sched_next = Nächster Lauf: { $when }
ui_sched_never = Noch nicht gelaufen.
ui_sched_last = Letzter Lauf: { $when } - { $result }
ui_sched_enabled = Täglich ausführen
ui_sched_enabled_title = Aus: nichts läuft von selbst
ui_sched_time = Uhrzeit
ui_sched_zone = Zeitzone
ui_sched_zone_title = IANA-Name wie Europe/Warsaw; leer: Zone des Servers
ui_sched_when = Wann
ui_sched_stages = Schritte
ui_sched_options = Optionen
ui_sched_sync = Sync
ui_sched_sync_title = Diskografien der gefolgten Künstler holen
ui_sched_links = Links suchen
ui_sched_links_title = YouTube-Links suchen, Titel ohne Treffer erneut
ui_sched_download = Herunterladen
ui_sched_download_title = Titel mit Link und ohne Datei laden; frühere Fehlschläge werden wiederholt
ui_sched_convert = Konvertieren
ui_sched_convert_title = Downloads in mp3 umwandeln
ui_sched_suggestions = Vorschläge
ui_sched_suggestions_title = Vorgeschlagene Künstler und ihre besten Titel erneuern (Entdecken)
ui_sched_images = Bilder
ui_sched_images_title = Bilder neuer Künstler, Alben und Singles suchen
ui_sched_fresh = Frische Bewertungen
ui_sched_fresh_title = Deezer für jeden Titel neu fragen, damit die Bewertungen aktuell sind (langsamer)
ui_sched_save = Speichern
ui_sched_run_now = Jetzt ausführen
ui_sched_run_now_title = Diese Schritte jetzt starten, wie der nächtliche Lauf
ui_purge_title = Gesperrte Dateien löschen
ui_purge_text = Löscht die Audiodateien der gesperrten Alben. Sperren nimmt ein Album nur aus der Bibliothek; seine heruntergeladenen Dateien bleiben, bis Sie das hier tun. Titel, die auch auf einem behaltenen Album sind, und selbst abgelegte Dateien werden nie gelöscht. Erst die Vorschau: sie listet auf, was verschwindet.
ui_purge_preview = Vorschau
ui_purge_preview_title = Dateien auflisten, die gelöscht würden; löscht nichts
ui_purge_delete = Gesperrte Dateien löschen
ui_purge_delete_title = Die Dateien aus der Vorschau endgültig löschen
ui_fixlinks_title = Falsche Links korrigieren
ui_fixlinks_text = Findet Titel, deren YouTube-Link nicht passt (der Videotitel enthält den Titel nicht), legt ihre Audiodateien in .mismatch-old beiseite und sucht neu: neuer Link, Download und Konvertierung. Nicht Teil der Pipeline. Erst die Vorschau: sie listet die Titel auf und ändert nichts. Das Limit von „Links finden“ gilt.
ui_fixlinks_preview = Vorschau
ui_fixlinks_preview_title = Titel mit nicht passendem Link auflisten; ändert nichts
ui_fixlinks_run = Links korrigieren
ui_fixlinks_run_title = Links und Dateien der Titel aus der Vorschau ersetzen

# Die Sperrliste und das Ersetzen eines Titels
ui_nav_blacklist = Sperrliste
ui_bls_title = Sperrliste
ui_bls_intro = Alben, die bei jeder Synchronisierung übersprungen werden. Ein Album per Name hinzufügen, einen Eintrag ändern oder ein Album wieder zulassen.
ui_bls_empty = Nichts ist gesperrt.
ui_bls_add = Album hinzufügen
ui_bls_edit = Bearbeiten
ui_bls_remove = Wieder zulassen
ui_bls_artist = Künstler
ui_bls_album = Album
ui_bls_dialog_add = Album sperren
ui_bls_dialog_edit = Eintrag ändern
ui_bls_dialog_hint = Das Album mit seinen anderen Ausgaben verlässt den Index sofort und wird bei späteren Synchronisierungen übersprungen.
ui_bls_saved = Gespeichert; { $n } Titel haben den Index verlassen
ui_bls_let_go = Wieder zugelassen: die nächste Synchronisierung bringt es zurück
ui_replace = Datei ersetzen
ui_replace_title = Diesen Link herunterladen und an die Stelle der Datei des Titels setzen
ui_replace_hint = „Datei ersetzen“ lädt diesen Link herunter und tauscht damit das Audio des Titels aus (für einen falschen Titel oder eine falsche Version). Die alte Datei bleibt, wenn der Download scheitert.
ui_replace_need_link = Zuerst die Video-URL eingeben
ui_replace_started = Titel wird ersetzt: siehe Aktivität
