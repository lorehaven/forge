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
