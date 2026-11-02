# required
header_label = Songbook
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Język
theme_label = Motyw

# ── Nawigacja ────────────────────────────────────────────────────────────────
ui_nav_library = Biblioteka
ui_nav_pipeline = Potok
ui_nav_discover = Odkrywaj
ui_nav_activity = Aktywność
ui_followed = Obserwowani artyści
ui_all_artists = Wszyscy artyści
ui_add_artist = Dodaj artystę
ui_not_synced = Jeszcze nie zsynchronizowano
ui_tracks_n = { $n ->
    [one] { $n } utwór
    [few] { $n } utwory
    [many] { $n } utworów
   *[other] { $n } utworu
}
ui_artist_tooltip = { $name } · Deezer #{ $id } · { $fans } fanów
ui_remove_artist_aria = Usuń { $name }
ui_change_category_aria = Zmień kategorię: { $name }
ui_category_tooltip = Kategoria: { $category }
ui_search_label = Szukaj
ui_search_placeholder = Szukaj utworów, albumów, artystów
ui_job_running = Trwa: { $name }…
ui_job_failed = Niepowodzenie: { $name }
ui_waiting = Ładowanie…
ui_session_ended = Sesja wygasła.
ui_sign_in = Zaloguj się

# ── Biblioteka ───────────────────────────────────────────────────────────────
ui_library_title = Biblioteka
ui_library_intro = Przeglądaj albumy i utwory obserwowanych artystów. Dodawaj lub poprawiaj linki YouTube ręcznie; dodawaj do czarnej listy albumy innego artysty.
ui_all_categories = Wszystkie kategorie
ui_all_genres = Wszystkie gatunki
ui_links_all = Wszystkie utwory
ui_links_missing = Bez linku
ui_links_has = Z linkiem
ui_links_unofficial = Tylko nieoficjalne
ui_sort_library = Kolejność biblioteki
ui_sort_top = Najpopularniejsze
ui_sort_artists = Najlepsi artyści
ui_best = Najlepsze utwory: { $songs }
ui_sort_suggested = Sugerowane (nie pobrane)
ui_summary = { $who } · utworów: { $total } · z linkiem YouTube: { $linked }
ui_summary_shown = pokazano pierwsze { $shown }
ui_nothing = Nic nie pasuje.
ui_blacklist = Czarna lista
ui_blacklist_title = Pomijaj ten album przy kolejnych synchronizacjach i usuń go z biblioteki
ui_manual = ręcznie
ui_manual_title = Link ustawiony przez Ciebie
ui_official_title = Oficjalny kanał / kanał artysty
ui_unofficial_title = Nieoficjalne przesłanie
ui_no_link = brak linku
ui_open_youtube_aria = Otwórz w YouTube: { $title }
ui_edit_link = Edytuj link
ui_add_link = Dodaj link
ui_edit_link_aria = Edytuj link: { $title }
ui_add_link_aria = Dodaj link: { $title }

# ── Okna ─────────────────────────────────────────────────────────────────────
ui_cancel = Anuluj
ui_save = Zapisz
ui_add = Dodaj
ui_remove = Usuń
ui_link_title = Link YouTube
ui_link_url = Adres wideo
ui_link_hint = Zostaw puste i wybierz „Usuń link”, aby wyczyścić.
ui_link_remove = Usuń link
ui_link_search = Szukaj w YouTube ↗
ui_link_saved = Zapisano link
ui_link_removed = Usunięto link
ui_bl_title = Dodać album do czarnej listy?
ui_bl_text = Album „{ $album }” artysty { $artist } zostanie usunięty i pominięty przy kolejnych synchronizacjach.
ui_bl_done = Dodano do czarnej listy: { $album }
ui_rm_title = Usunąć artystę?
ui_rm_text = { $name } przestanie być obserwowany. Jego utwory zostaną w indeksie do następnej synchronizacji.
ui_rm_done = Usunięto: { $name }
ui_add_title = Dodaj artystę
ui_add_name = Nazwa artysty
ui_add_id = Identyfikator artysty w Deezer (opcjonalnie)
ui_add_id_hint = Podaj dokładny identyfikator, jeśli nazwa jest niejednoznaczna.
ui_add_category = Kategoria
ui_add_category_hint = Można zmienić później.
ui_add_done = Dodano: { $name }. Uruchom synchronizację, aby go pobrać.
ui_add_exists = { $name } jest już na liście
ui_cat_title = Kategoria
ui_cat_label = Kategoria
ui_cat_new = Albo utwórz nową kategorię
ui_cat_new_hint = Zmienia tagi utworów tego artysty; pliki zostają na miejscu.
ui_genres_label = Tylko te gatunki
ui_genres_hint = Zachowaj tylko albumy w tych gatunkach Deezera, rozdzielone znakiem ; (np. Rap/Hip Hop). Dla artysty, którego profil w Deezerze miesza osoby o tym samym imieniu. Puste zachowuje wszystkie albumy. Działa od następnej synchronizacji, która usuwa też niepasujące albumy.
ui_cat_saved = Zapisano kategorię
ui_catrename_title = Zmień nazwę kategorii
ui_catrename_aria = Zmień nazwę kategorii „{ $name }”
ui_catrename_label = Nazwa
ui_catrename_hint = Zmienia tagi utworów jej artystów; pliki zostają na miejscu.
ui_catrename_done = Zmieniono nazwę kategorii na { $name }

# ── Potok ────────────────────────────────────────────────────────────────────
ui_pipeline_title = Potok
ui_pipeline_intro = Buduj bibliotekę krok po kroku: pobierz dyskografie, znajdź linki YouTube, potem pobierz i przekonwertuj dźwięk.
ui_scope_one = Kroki 2 i 3 dotyczą tylko: { $name }. Wybierz „Wszyscy artyści” na pasku bocznym, aby uruchomić dla wszystkich.
ui_scope_all = Kroki 2 i 3 dotyczą wszystkich artystów. Wybierz artystę na pasku bocznym, aby ograniczyć je do niego.
ui_step = Krok { $n }
ui_step_library = Biblioteka
ui_step_library_text = Pobierz dyskografie z Deezera dla każdego obserwowanego artysty.
ui_sync = Synchronizuj bibliotekę
ui_singles = Dołącz single
ui_singles_title = Pobierz także single, nie tylko albumy i EP-ki
ui_rebuild_tags = Przebuduj tagi
ui_rebuild_tags_title = Przelicz tagi kategorii i gatunków w indeksie bez pobierania
ui_step_links = Linki YouTube
ui_step_links_text = Automatycznie znajdź film dla każdego utworu, którego nie ma jeszcze na dysku. Każdy link możesz poprawić ręcznie w Bibliotece.
ui_find_links = Znajdź linki
ui_retry_missing = Ponów brakujące
ui_retry_missing_title = Ponów utwory, dla których wcześniej nie było dopasowania
ui_max_tracks = Maks. utworów
ui_max_all = wszystkie
ui_step_download = Pobieranie i konwersja
ui_step_download_text = Pobierz dopasowane filmy do folderu z muzyką, potem przekonwertuj je na dźwięk w miejscu.
ui_run_all_title = Uruchom wszystko
ui_run_all_text = Synchronizuje bibliotekę, szuka linków (ponawiając utwory bez dopasowania), pobiera i konwertuje: kroki poniżej, jeden po drugim. Zatrzymuje się na pierwszym kroku, który się nie powiedzie. Obowiązują opcje poniżej (single, artysta wybrany z lewej, limity).
ui_run_all = Uruchom wszystko
ui_run_all_hint = Synchronizacja, szukanie linków, pobieranie i konwersja za jednym razem
ui_stat_artists = Artyści
ui_stat_artists_sub = { $n } wszystkich w utworach
ui_stat_albums = Albumy
ui_stat_songs = Utwory
ui_stat_downloaded = Pobrane
ui_stat_downloaded_sub = { $pct }% utworów
ui_stat_missing = Brakujące
ui_stat_missing_sub = { $linked } z linkiem · { $unlinked } bez
ui_download = Pobierz
ui_convert = Konwertuj

# ── Odkrywaj ─────────────────────────────────────────────────────────────────
ui_discover_title = Odkrywaj
ui_discover_intro = Proponuj artystów powiązanych z obserwowanymi, uszeregowanych według liczby Twoich artystów, z którymi są powiązani.
ui_current_list = Obecna lista: { $scope }.
ui_scope = Zakres
ui_top = Najlepsze
ui_find_similar = Znajdź podobnych artystów
ui_no_suggestions = Brak propozycji.
ui_via = { $n }× przez { $names }
ui_fab = { $n ->
    [one] Dodaj { $n } artystę i synchronizuj
    [few] Dodaj { $n } artystów i synchronizuj
   *[other] Dodaj { $n } artystów i synchronizuj
}
ui_added_n = { $n ->
    [one] Dodano { $n } artystę
    [few] Dodano { $n } artystów
   *[other] Dodano { $n } artystów
}

# ── Aktywność ────────────────────────────────────────────────────────────────
ui_activity_title = Aktywność
ui_activity_intro = Postęp i dziennik trwającego (lub ostatniego) zadania. Naraz działa jedno zadanie.
ui_idle = Bezczynny
ui_status_running = Trwa: { $name }…
ui_status_error = Błąd: { $why }
ui_status_finished = Zakończono: { $name }
ui_no_job = Żadne zadanie jeszcze nie działało.
ui_snack_finished = Zakończono: { $name }
ui_snack_failed = Niepowodzenie ({ $name }): { $why }
ui_job_busy = Zadanie już trwa

# ── Shell ────────────────────────────────────────────────────────────────────
ui_home_button = Start
ui_dashboard_button = Panel usług
ui_theme_toggle = Zmień motyw
ui_theme_mode = Tryb
ui_theme_light = Jasny
ui_theme_dark = Ciemny
ui_theme_palette = Kolor
ui_palette_green = Zielony
ui_palette_teal = Morski
ui_palette_blue = Niebieski
ui_palette_purple = Fioletowy
ui_palette_rose = Różowy
ui_palette_orange = Pomarańczowy
ui_user_menu = Menu użytkownika
ui_profile_edit = Edytuj profil
ui_logout = Wyloguj

# ── Harmonogram ──────────────────────────────────────────────────────────────
ui_nav_schedule = Harmonogram
ui_sched_title = Nocne uruchomienie
ui_sched_intro = Codziennie o ustalonej godzinie samo utrzymuje bibliotekę: synchronizacja ze świeżymi ocenami, szukanie brakujących linków, pobieranie i konwersja.
ui_sched_saved = Zapisano harmonogram.
ui_sched_off = Nocne uruchomienie jest wyłączone.
ui_sched_next = Następne uruchomienie: { $when }
ui_sched_never = Jeszcze się nie uruchomiło.
ui_sched_last = Ostatnie uruchomienie: { $when } - { $result }
ui_sched_enabled = Uruchamiaj codziennie
ui_sched_enabled_title = Wyłączone: nic nie uruchamia się samo
ui_sched_time = Godzina
ui_sched_zone = Strefa czasowa
ui_sched_zone_title = Nazwa IANA, np. Europe/Warsaw; puste: strefa serwera
ui_sched_when = Kiedy
ui_sched_stages = Etapy
ui_sched_options = Opcje
ui_sched_sync = Synchronizacja
ui_sched_sync_title = Pobierz dyskografie obserwowanych artystów
ui_sched_links = Szukanie linków
ui_sched_links_title = Szukaj linków YouTube, ponownie dla utworów bez dopasowania
ui_sched_download = Pobieranie
ui_sched_download_title = Pobierz utwory z linkiem i bez pliku; wcześniejsze niepowodzenia są ponawiane
ui_sched_convert = Konwersja
ui_sched_convert_title = Zamień pobrane pliki na mp3
ui_sched_suggestions = Sugestie
ui_sched_suggestions_title = Odśwież sugerowanych artystów i ich najlepsze utwory (Odkrywaj)
ui_sched_fresh = Świeże oceny
ui_sched_fresh_title = Zapytaj Deezer o każdy utwór od nowa, by oceny były aktualne (wolniej)
ui_sched_save = Zapisz
ui_sched_run_now = Uruchom teraz
ui_sched_run_now_title = Uruchom te etapy teraz, tak jak nocne uruchomienie
ui_purge_title = Usuń pliki z czarnej listy
ui_purge_text = Usuń pliki audio albumów z czarnej listy. Dodanie albumu do czarnej listy usuwa go tylko z biblioteki; pobrane pliki zostają, dopóki tego nie zrobisz. Utwory, które są też na zachowanym albumie, oraz pliki dodane przez Ciebie nigdy nie są usuwane. Najpierw podgląd: pokazuje, co zniknie.
ui_purge_preview = Podgląd
ui_purge_preview_title = Wypisz pliki, które zostałyby usunięte; niczego nie usuwa
ui_purge_delete = Usuń pliki z czarnej listy
ui_purge_delete_title = Usuń na stałe pliki z podglądu
