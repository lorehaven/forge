# required
header_label = Songbook
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Language
theme_label = Theme

# ── Navigation ───────────────────────────────────────────────────────────────
ui_nav_library = Library
ui_nav_pipeline = Pipeline
ui_nav_discover = Discover
ui_nav_activity = Activity
ui_followed = Followed artists
ui_all_artists = All artists
ui_add_artist = Add artist
ui_not_synced = Not synced yet
ui_tracks_n = { $n ->
    [one] { $n } song
   *[other] { $n } songs
}
ui_artist_tooltip = { $name } · Deezer #{ $id } · { $fans } fans
ui_remove_artist_aria = Remove { $name }
ui_change_category_aria = Change category of { $name }
ui_category_tooltip = Category: { $category }
ui_search_label = Search
ui_search_placeholder = Search songs, albums, artists
ui_job_running = Running { $name }…
ui_job_failed = { $name } failed
ui_waiting = Loading…
ui_session_ended = Your session ended.
ui_sign_in = Sign in

# ── Library ──────────────────────────────────────────────────────────────────
ui_library_title = Library
ui_library_intro = Browse the albums and songs of your followed artists. Add or fix YouTube links by hand; blacklist albums that belong to another artist.
ui_all_categories = All categories
ui_all_genres = All genres
ui_links_all = All songs
ui_links_missing = Missing link
ui_links_has = Has link
ui_links_unofficial = Unofficial only
ui_sort_library = Library order
ui_sort_top = Top
ui_sort_artists = Top artists
ui_best = Best songs: { $songs }
ui_sort_suggested = Suggested (not downloaded)
ui_summary = { $who } · { $total } songs · { $linked } with a YouTube link
ui_summary_shown = showing first { $shown }
ui_nothing = Nothing matches.
ui_blacklist = Blacklist
ui_blacklist_title = Skip this album in future syncs and remove it from the library
ui_manual = manual
ui_manual_title = Link set by you
ui_official_title = Official / artist channel
ui_unofficial_title = Unofficial upload
ui_no_link = no link
ui_open_youtube_aria = Open on YouTube: { $title }
ui_edit_link = Edit link
ui_add_link = Add link
ui_edit_link_aria = Edit link for { $title }
ui_add_link_aria = Add link for { $title }

# ── Dialogs ──────────────────────────────────────────────────────────────────
ui_cancel = Cancel
ui_save = Save
ui_add = Add
ui_remove = Remove
ui_link_title = YouTube link
ui_link_url = Video URL
ui_link_hint = Leave empty and choose “Remove link” to clear it.
ui_link_remove = Remove link
ui_link_search = Search on YouTube ↗
ui_link_saved = Link saved
ui_link_removed = Link removed
ui_bl_title = Blacklist album?
ui_bl_text = “{ $album }” by { $artist } will be removed and skipped by future syncs.
ui_bl_done = Blacklisted { $album }
ui_rm_title = Remove artist?
ui_rm_text = { $name } will stop being followed. Their songs stay in the index until the next sync.
ui_rm_done = Removed { $name }
ui_add_title = Add artist
ui_add_name = Artist name
ui_add_id = Deezer artist id (optional)
ui_add_id_hint = Pin an exact match if the name is ambiguous.
ui_add_category = Category
ui_add_category_hint = Can be changed later.
ui_add_done = Added { $name }. Run Sync to fetch it.
ui_add_exists = { $name } is already in the list
ui_cat_title = Category
ui_cat_label = Category
ui_cat_new = Or create a new category
ui_cat_new_hint = Retags this artist's songs; files stay where they are.
ui_genres_label = Only these genres
ui_genres_hint = Keep only albums in these Deezer genres, separated by ; (e.g. Rap/Hip Hop). For an artist whose Deezer profile mixes in people of the same name. Empty keeps every album. Applied at the next sync, which also drops albums that no longer match.
ui_cat_saved = Category saved
ui_catrename_title = Rename category
ui_catrename_aria = Rename category “{ $name }”
ui_catrename_label = Name
ui_catrename_hint = Retags the songs of its artists; files stay where they are.
ui_catrename_done = Category renamed to { $name }

# ── Pipeline ─────────────────────────────────────────────────────────────────
ui_pipeline_title = Pipeline
ui_pipeline_intro = Build the library step by step: fetch discographies, find YouTube links, then download and convert the audio.
ui_scope_one = Steps 2 and 3 run for { $name } only. Pick “All artists” in the sidebar to run for everyone.
ui_scope_all = Steps 2 and 3 run for all artists. Pick an artist in the sidebar to limit them to that artist.
ui_step = Step { $n }
ui_step_library = Library
ui_step_library_text = Fetch discographies from Deezer for every followed artist.
ui_sync = Sync library
ui_singles = Include singles
ui_singles_title = Also fetch singles, not just albums and EPs
ui_rebuild_tags = Rebuild tags
ui_rebuild_tags_title = Recompute the category and genre tags in the index without fetching
ui_step_links = YouTube links
ui_step_links_text = Auto-find a video per song that has no file yet. Edit any link by hand in the Library.
ui_find_links = Find links
ui_retry_missing = Retry missing
ui_retry_missing_title = Retry songs that previously had no match
ui_max_tracks = Max songs
ui_max_all = all
ui_step_download = Download & convert
ui_step_download_text = Download the matched videos into the music folder, then convert them to audio in place.
ui_step_images = Images
ui_step_images_text = Find pictures for artists, albums and singles on Deezer. A song shows its album's cover (a single's own cover, else its artist's picture). Only what is missing is fetched.
ui_find_images = Find images
ui_retry_images = Retry missing
ui_retry_images_title = Ask again for the pictures Deezer had none for
ui_run_all_title = Run everything
ui_run_all_text = Sync the library, find links (trying again the songs that had no match), download, convert and find images: the steps below, one after the other. It stops at the first step that fails. The options below (singles, the artist picked in the sidebar, the limits) apply.
ui_run_all = Run everything
ui_run_all_hint = Sync, find links, download, convert and find images in one go
ui_stat_artists = Artists
ui_stat_artists_sub = { $n } credited in all
ui_stat_albums = Albums
ui_stat_songs = Songs
ui_stat_downloaded = Downloaded
ui_stat_downloaded_sub = { $pct }% of the songs
ui_stat_missing = Missing
ui_stat_missing_sub = { $linked } with a link · { $unlinked } without
ui_download = Download
ui_convert = Convert

# ── Discover ─────────────────────────────────────────────────────────────────
ui_discover_title = Discover
ui_discover_intro = Suggest artists related to the ones you follow, ranked by how many of your artists they are related to.
ui_current_list = Current list: { $scope }.
ui_scope = Scope
ui_top = Top
ui_find_similar = Find similar artists
ui_no_suggestions = No suggestions yet.
ui_via = { $n }× via { $names }
ui_fab = { $n ->
    [one] Add { $n } artist & sync
   *[other] Add { $n } artists & sync
}
ui_added_n = { $n ->
    [one] Added { $n } artist
   *[other] Added { $n } artists
}

# ── Activity ─────────────────────────────────────────────────────────────────
ui_activity_title = Activity
ui_activity_intro = Progress and log of the running (or last) job. One job runs at a time.
ui_idle = Idle
ui_status_running = Running { $name }…
ui_status_error = Error: { $why }
ui_status_finished = Finished { $name }
ui_no_job = No job has run yet.
ui_snack_finished = Finished { $name }
ui_snack_failed = { $name } failed: { $why }
ui_job_busy = A job is already running

# ── Shell ────────────────────────────────────────────────────────────────────
ui_home_button = Home
ui_dashboard_button = Services dashboard
ui_theme_toggle = Toggle theme
ui_theme_mode = Mode
ui_theme_light = Light
ui_theme_dark = Dark
ui_theme_palette = Colour
ui_palette_green = Green
ui_palette_teal = Teal
ui_palette_blue = Blue
ui_palette_purple = Purple
ui_palette_rose = Rose
ui_palette_orange = Orange
ui_user_menu = User menu
ui_profile_edit = Edit profile
ui_logout = Log out

# ── Harmonogram ──────────────────────────────────────────────────────────────
ui_nav_schedule = Schedule
ui_sched_title = Nightly run
ui_sched_intro = Run the library upkeep by itself every day at a set time: sync with fresh ratings, find missing links, download, convert and find images.
ui_sched_saved = Schedule saved.
ui_sched_off = The nightly run is off.
ui_sched_next = Next run: { $when }
ui_sched_never = It has not run yet.
ui_sched_last = Last run: { $when } - { $result }
ui_sched_enabled = Run every day
ui_sched_enabled_title = Off: nothing runs by itself
ui_sched_time = Time
ui_sched_zone = Time zone
ui_sched_zone_title = IANA name such as Europe/Warsaw; empty for the server's own zone
ui_sched_when = When
ui_sched_stages = Stages
ui_sched_options = Options
ui_sched_sync = Sync
ui_sched_sync_title = Fetch the discographies of the followed artists
ui_sched_links = Find links
ui_sched_links_title = Look for YouTube links, again for songs that had no match
ui_sched_download = Download
ui_sched_download_title = Download songs that have a link and no file; earlier failures are tried again
ui_sched_convert = Convert
ui_sched_convert_title = Convert the downloads to mp3
ui_sched_suggestions = Suggestions
ui_sched_suggestions_title = Refresh the suggested artists and their best songs (Discover)
ui_sched_images = Images
ui_sched_images_title = Find the pictures of new artists, albums and singles
ui_sched_fresh = Fresh ratings
ui_sched_fresh_title = Ask Deezer for every track again so the ratings are current (slower)
ui_sched_save = Save
ui_sched_run_now = Run now
ui_sched_run_now_title = Start these stages now, as the nightly run would
ui_purge_title = Delete blacklisted files
ui_purge_text = Delete the audio files of the albums you blacklisted. Blacklisting only removes an album from the library; its downloaded files stay until you do this. Songs that are also on an album you keep, and files you added yourself, are never deleted. Preview first: it lists what would go.
ui_purge_preview = Preview
ui_purge_preview_title = List the files that would be deleted; deletes nothing
ui_purge_delete = Delete blacklisted files
ui_purge_delete_title = Delete the files the preview lists, for good
