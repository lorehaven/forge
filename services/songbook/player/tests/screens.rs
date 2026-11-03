//! The screens with no host and no engine: mounted into a test tree, driven by events and the clock, read back as text. The
//! engine's side is played by the test (the commands are read from `#sb-engine`, its answers are `input` events).
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::cell::RefCell;
use std::rc::Rc;

use quench_serde::{Value, json};
use quench_ui::Event;
use quench_ui::prelude::*;
use quench_ui::router::MemoryHistory;
use quench_ui::test_renderer::NodeId;
use songbook_core::{
    AlbumInfo, ArtistInfo, CategoryInfo, Overview, PlaylistInfo, Search, SongIds, SongInfo,
    SongList,
};
use songbook_player::live::Intent;
use songbook_player::{Msg, Player, Request, Setup, player};

type Asked = Rc<RefCell<Vec<Request>>>;

thread_local! {
    static CLOCK: RefCell<Option<Rc<quench_ui::platform::ManualPlatform>>> = const { RefCell::new(None) };
}

fn tick(ms: f64) {
    CLOCK
        .with(|c| c.borrow().clone())
        .expect("a test that ticks starts with `live`")
        .advance(ms);
}

struct Live {
    r: Rc<TestRenderer>,
    asked: Asked,
    deliver: Rc<dyn Fn(Msg)>,
    _mount: quench_ui::Mount<TestRenderer>,
}

impl Live {
    fn find_id(&self, id: &str) -> NodeId {
        self.r
            .document_order()
            .into_iter()
            .find(|n| self.r.attr(*n, "id").as_deref() == Some(id))
            .unwrap_or_else(|| panic!("no #{id}: {}", self.r.html()))
    }

    fn has(&self, id: &str) -> bool {
        self.r
            .document_order()
            .into_iter()
            .any(|n| self.r.attr(n, "id").as_deref() == Some(id))
    }

    fn click(&self, id: &str) {
        self.r.fire(self.find_id(id), &Event::Click);
    }

    fn text_of(&self, id: &str) -> String {
        self.r.text_of(self.find_id(id))
    }

    fn type_into(&self, id: &str, text: &str) {
        self.r.fire(self.find_id(id), &Event::Input(text.into()));
    }

    fn send(&self, msg: Msg) {
        (self.deliver)(msg);
    }

    fn take(&self) -> Vec<Request> {
        std::mem::take(&mut *self.asked.borrow_mut())
    }

    /// What the engine has been told, in order.
    fn commands(&self) -> Vec<Value> {
        let text = self.r.attr(self.find_id("sb-engine"), "data-cmd").unwrap();
        quench_serde::json::from_str::<Value>(&text)
            .unwrap()
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    fn last_command(&self) -> Value {
        self.commands().last().cloned().expect("a command was sent")
    }

    /// The engine says something.
    fn hear(&self, event: &Value) {
        self.r.fire(
            self.find_id("sb-engine-in"),
            &Event::Input(quench_serde::json::to_string(event)),
        );
    }

    /// Clicks the first element of `class` inside `#id`'s subtree.
    fn click_class(&self, class: &str) {
        let node = self
            .r
            .document_order()
            .into_iter()
            .find(|n| {
                self.r
                    .attr(*n, "class")
                    .is_some_and(|c| c.split_whitespace().any(|w| w == class))
            })
            .unwrap_or_else(|| panic!("no .{class}: {}", self.r.html()));
        self.r.fire(node, &Event::Click);
    }
}

fn live_on(path: &str, mobile: bool) -> Live {
    let fresh = Rc::new(quench_ui::platform::ManualPlatform::new());
    quench_ui::platform::install(fresh.clone());
    CLOCK.with(|c| *c.borrow_mut() = Some(fresh));
    let asked: Asked = Rc::default();
    let log = asked.clone();
    let Player { view, deliver, .. } = player(
        Rc::new(move |q| log.borrow_mut().push(q)),
        Setup {
            base: "/songbook".into(),
            history: Rc::new(MemoryHistory::new(path)),
            login_url: "/songbook/ui/login".into(),
            mobile,
            engine: None,
            sign_out: None,
        },
    );
    let r = Rc::new(TestRenderer::new());
    let mount = mount(&r, &r.root(), view);
    Live {
        r,
        asked,
        deliver,
        _mount: mount,
    }
}

fn live(path: &str) -> Live {
    live_on(path, false)
}

fn song(id: i64, title: &str, artists: &[&str], album: &str) -> SongInfo {
    SongInfo {
        id,
        title: title.into(),
        artists: artists.iter().map(|a| (*a).to_string()).collect(),
        album: album.into(),
        album_id: 7,
        year: "1999".into(),
        duration: 187,
    }
}

fn songs() -> SongList {
    SongList {
        total: 3,
        songs: vec![
            song(
                1,
                "Nobody Does It Better",
                &["Nate Dogg"],
                "Ghetto Preacher",
            ),
            song(
                2,
                "Hardest Man in Town",
                &["Nate Dogg", "Snoop Dogg"],
                "Ghetto Preacher",
            ),
            song(3, "Puppy Love", &["Nate Dogg"], "Ghetto Preacher"),
        ],
    }
}

fn overview() -> Overview {
    Overview {
        songs: 3,
        albums: 1,
        artists: vec![
            ArtistInfo {
                name: "Nate Dogg".into(),
                songs: 3,
                albums: 1,
            },
            ArtistInfo {
                name: "Snoop Dogg".into(),
                songs: 1,
                albums: 0,
            },
        ],
        categories: vec![CategoryInfo {
            name: "American Rap".into(),
            songs: 3,
        }],
    }
}

fn album() -> AlbumInfo {
    AlbumInfo {
        id: 7,
        title: "Ghetto Preacher".into(),
        artist: "Nate Dogg".into(),
        year: "1999-05-01".into(),
        kind: "album".into(),
        songs: 3,
    }
}

fn state(id: i64, index: i64, playing: bool) -> Value {
    json!({ "ev": "state", "playing": playing, "id": id, "index": index, "len": 3, "pos": 61.0, "dur": 187.0,
            "shuffle": false, "repeat": "off", "fade": 0.0, "vol": 1.0, "loading": false })
}

fn home() -> songbook_core::Home {
    songbook_core::Home {
        popular: songs().songs,
        mixes: vec![
            songbook_core::Mix {
                name: "Daily Mix".into(),
                kind: "mix".into(),
                category: String::new(),
                songs: 150,
            },
            songbook_core::Mix {
                name: "Best of American Rap".into(),
                kind: "best".into(),
                category: "American Rap".into(),
                songs: 50,
            },
        ],
        artists: overview().artists,
        discover: vec![song(9, "Forgotten Gem", &["Snoop Dogg"], "Tha Doggfather")],
        albums: vec![album()],
    }
}

#[quench_test::test]
fn the_home_page_offers_picks_not_the_whole_library() {
    let l = live("/songbook/ui");
    let asked = l.take();
    assert!(asked.contains(&Request::Overview));
    assert!(asked.contains(&Request::Playlists));
    assert!(asked.contains(&Request::Home));
    assert!(
        !asked.iter().any(|r| matches!(r, Request::Albums(..))),
        "no list of every album"
    );
    l.send(Msg::Overview(overview()));
    l.send(Msg::Home(home()));
    l.send(Msg::Playlists(vec![PlaylistInfo {
        id: 4,
        name: "Road trip".into(),
        songs: 12,
    }]));
    for id in [
        "shelf-mixes",
        "shelf-popular",
        "shelf-artists",
        "shelf-albums",
        "shelf-categories",
        "shelf-playlists",
    ] {
        assert!(l.has(id), "{id}");
    }
    assert!(l.has("mix-Daily%20Mix") && l.has("mix-Best%20of%20American%20Rap"));
    assert!(l.has("popular"));
    assert!(!l.has("shelf-discover"), "no \"worth hearing again\"");
    assert!(l.has("category-American%20Rap"), "a way to the category");
    assert!(l.has("playlist-4"));
    assert!(l.r.text_of(l.find_id("playlist-4")).contains("12 songs"));
    assert!(!l.has("recent"), "nothing was played yet");
}

#[quench_test::test]
fn a_mix_plays_the_popular_songs_of_its_category_in_its_order() {
    let l = live("/songbook/ui");
    l.take();
    l.send(Msg::Home(home()));
    l.click("mix-Best%20of%20American%20Rap");
    let asked = l.take();
    assert!(
        matches!(&asked[..], [Request::Ids(q, Intent::Play { shuffle: false })]
        if q.category == "American Rap" && q.sort == "popular" && q.limit == 50),
        "{asked:?}"
    );
    l.click("mix-Daily%20Mix");
    let asked = l.take();
    assert!(
        matches!(&asked[..], [Request::Ids(q, Intent::Play { shuffle: true })]
        if q.category.is_empty() && q.sort == "popular" && q.limit == 150),
        "a mix is shuffled: {asked:?}"
    );
}

#[quench_test::test]
fn what_was_played_last_is_on_the_home_page_and_plays_from_there() {
    let l = live("/songbook/ui");
    l.take();
    l.hear(&json!({ "ev": "recent", "ids": [2, 1] }));
    let asked = l.take();
    assert!(
        asked
            .iter()
            .any(|r| matches!(r, Request::Songs(q, key) if key == "meta" && q.ids.contains('2')))
    );
    l.send(Msg::Songs("meta".into(), songs()));
    assert!(l.has("recent"));
    l.click_class("pl-card-button");
    let c = l
        .commands()
        .into_iter()
        .find(|c| c.get("op").and_then(Value::as_str) == Some("load"))
        .unwrap();
    assert_eq!(
        c.get("queue")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .filter_map(Value::as_i64)
            .collect::<Vec<_>>(),
        vec![2, 1]
    );
}

#[quench_test::test]
fn the_artists_page_lists_all_of_them_and_finds_by_name() {
    let l = live("/songbook/ui/artists");
    l.take();
    l.send(Msg::Overview(overview()));
    assert!(l.has("artist-Nate%20Dogg") && l.has("artist-Snoop%20Dogg"));
    l.type_into("artist-filter", "snoop");
    assert!(!l.has("artist-Nate%20Dogg") && l.has("artist-Snoop%20Dogg"));
    l.type_into("artist-filter", "zzz");
    assert!(l.has("nothing"));
}

#[quench_test::test]
fn playing_everything_shuffled_asks_for_the_ids_and_then_commands_the_engine() {
    let l = live("/songbook/ui");
    l.take();
    l.send(Msg::Overview(overview()));
    l.click("shuffle-everything");
    let asked = l.take();
    assert!(
        matches!(&asked[..], [Request::Ids(q, Intent::Play { shuffle: true })] if *q == songbook_core::SongQuery::default())
    );
    l.send(Msg::Ids(
        Intent::Play { shuffle: true },
        SongIds {
            ids: vec![5, 6, 7],
            total: 3,
        },
    ));
    let c = l.last_command();
    assert_eq!(c.get("op").and_then(Value::as_str), Some("load"));
    assert_eq!(c.get("shuffle"), Some(&Value::Bool(true)));
    assert_eq!(
        c.get("queue").and_then(Value::as_array).map(Vec::len),
        Some(3)
    );
}

#[quench_test::test]
fn an_album_lists_its_songs_and_a_row_plays_the_album_from_there() {
    let l = live("/songbook/ui/album/7");
    let asked = l.take();
    assert!(
        asked
            .iter()
            .any(|r| matches!(r, Request::Songs(q, key) if q.album == 7 && key == "album:7"))
    );
    l.send(Msg::Songs("album:7".into(), songs()));
    assert_eq!(l.text_of("title"), "Ghetto Preacher");
    l.click_class("pl-song-main");
    let c = l
        .commands()
        .into_iter()
        .find(|c| c.get("op").and_then(Value::as_str) == Some("load"))
        .unwrap();
    assert_eq!(c.get("index").and_then(Value::as_i64), Some(0));
    assert_eq!(
        c.get("queue")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .filter_map(Value::as_i64)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
}

#[quench_test::test]
fn the_play_and_shuffle_buttons_of_a_page_play_all_of_it() {
    let l = live("/songbook/ui/artist/Nate%20Dogg");
    let asked = l.take();
    assert!(
        asked
            .iter()
            .any(|r| matches!(r, Request::Songs(q, _) if q.artist == "Nate Dogg")),
        "the name is decoded"
    );
    assert!(
        asked
            .iter()
            .any(|r| matches!(r, Request::Albums(q, _) if q.artist == "Nate Dogg"))
    );
    l.send(Msg::Songs("artist:Nate Dogg".into(), songs()));
    l.click("play-all");
    assert!(
        matches!(&l.take()[..], [Request::Ids(q, Intent::Play { shuffle: false })] if q.artist == "Nate Dogg")
    );
    l.click("shuffle-all");
    assert!(matches!(
        &l.take()[..],
        [Request::Ids(_, Intent::Play { shuffle: true })]
    ));
}

#[quench_test::test]
fn the_bar_shows_what_the_engine_plays_and_its_buttons_command_it() {
    let l = live("/songbook/ui");
    l.take();
    assert!(
        l.r.attr(l.find_id("bar"), "class")
            .unwrap()
            .contains("pl-bar-idle"),
        "nothing plays yet"
    );
    l.hear(&state(2, 1, true));
    // A song the page has not seen is asked for.
    let asked = l.take();
    assert!(
        asked
            .iter()
            .any(|r| matches!(r, Request::Songs(q, key) if q.ids == "2" && key == "meta"))
    );
    l.send(Msg::Songs(
        "meta".into(),
        SongList {
            total: 1,
            songs: vec![songs().songs[1].clone()],
        },
    ));
    assert_eq!(l.text_of("now-title"), "Hardest Man in Town");
    assert_eq!(l.text_of("now-artist"), "Nate Dogg, Snoop Dogg");
    assert_eq!(
        l.r.attr(l.find_id("play"), "data-state").as_deref(),
        Some("pause")
    );
    assert_eq!(l.text_of("pos"), "1:01");
    assert_eq!(l.text_of("dur"), "3:07");
    // The lock screen is told what plays.
    let meta = l
        .commands()
        .into_iter()
        .find(|c| c.get("op").and_then(Value::as_str) == Some("meta"))
        .unwrap();
    assert_eq!(
        meta.get("songs").and_then(Value::as_array).unwrap()[0]
            .get("title")
            .and_then(Value::as_str),
        Some("Hardest Man in Town")
    );

    l.click("play");
    assert_eq!(
        l.last_command().get("op").and_then(Value::as_str),
        Some("toggle")
    );
    l.click("next");
    assert_eq!(
        l.last_command().get("op").and_then(Value::as_str),
        Some("next")
    );
    l.click("prev");
    assert_eq!(
        l.last_command().get("op").and_then(Value::as_str),
        Some("prev")
    );
    l.click("shuffle");
    assert_eq!(l.last_command().get("on"), Some(&Value::Bool(true)));
    l.click("repeat");
    assert_eq!(
        l.last_command().get("mode").and_then(Value::as_str),
        Some("all")
    );
    // Commands go in order and every one has its own number.
    let numbers: Vec<_> = l
        .commands()
        .iter()
        .filter_map(|c| c.get("n").and_then(Value::as_i64))
        .collect();
    assert!(numbers.windows(2).all(|w| w[0] < w[1]), "{numbers:?}");

    l.r.fire(l.find_id("seek"), &Event::Change("120".into()));
    let seek = l.last_command();
    assert_eq!(
        (
            seek.get("op").and_then(Value::as_str),
            seek.get("to").and_then(Value::as_f64)
        ),
        (Some("seek"), Some(120.0))
    );
}

#[quench_test::test]
fn clicking_play_with_nothing_to_play_says_so() {
    let l = live("/songbook/ui");
    l.click("play");
    assert_eq!(l.text_of("toast"), "Pick something to play");
    tick(4000.0);
    assert!(!l.has("toast"), "the toast goes by itself");
}

#[quench_test::test]
fn searching_waits_for_the_typing_to_stop() {
    let l = live("/songbook/ui/search");
    l.take();
    l.type_into("search-input", "nob");
    l.type_into("search-input", "nobody");
    tick(100.0);
    assert!(l.take().is_empty(), "still typing");
    tick(200.0);
    assert_eq!(l.take(), vec![Request::Search("nobody".into())]);
    l.send(Msg::Found(
        "nobody".into(),
        Search {
            songs: vec![songs().songs[0].clone()],
            albums: vec![album()],
            artists: Vec::new(),
        },
    ));
    assert!(l.has("songs") && l.has("albums"));
    l.send(Msg::Found("zzz".into(), Search::default()));
    assert!(l.has("nothing"));
}

#[quench_test::test]
fn a_song_goes_into_a_playlist_that_exists_or_into_a_new_one() {
    let l = live("/songbook/ui/album/7");
    l.take();
    l.send(Msg::Playlists(vec![PlaylistInfo {
        id: 4,
        name: "Road trip".into(),
        songs: 12,
    }]));
    l.send(Msg::Songs("album:7".into(), songs()));
    // The row's ＋ button.
    let add =
        l.r.document_order()
            .into_iter()
            .find(|n| {
                l.r.attr(*n, "aria-label").as_deref() == Some("Add to a playlist")
                    && l.r.attr(*n, "id").as_deref() == Some("")
            })
            .unwrap();
    l.r.fire(add, &Event::Click);
    assert!(l.has("dialog-add"));
    l.click_class("pl-pick");
    assert_eq!(l.take(), vec![Request::AddToPlaylist(4, vec![1])]);
    assert!(!l.has("dialog-add"), "the box closes");

    // Into a new one: the playlist is made first, and the song goes in when it exists.
    let add =
        l.r.document_order()
            .into_iter()
            .find(|n| {
                l.r.attr(*n, "aria-label").as_deref() == Some("Add to a playlist")
                    && l.r.attr(*n, "id").as_deref() == Some("")
            })
            .unwrap();
    l.r.fire(add, &Event::Click);
    l.type_into("new-playlist-name", "Fresh");
    l.click("create-add");
    assert_eq!(l.take(), vec![Request::CreatePlaylist("Fresh".into())]);
    l.send(Msg::Playlist(PlaylistInfo {
        id: 9,
        name: "Fresh".into(),
        songs: 0,
    }));
    assert_eq!(l.take(), vec![Request::AddToPlaylist(9, vec![1])]);
    assert!(!l.has("dialog-add"));
}

#[quench_test::test]
fn a_playlist_page_lists_its_songs_and_takes_one_out() {
    let l = live("/songbook/ui/playlist/4");
    let asked = l.take();
    assert!(
        asked
            .iter()
            .any(|r| matches!(r, Request::Songs(q, key) if q.playlist == 4 && key == "playlist:4"))
    );
    l.send(Msg::Playlists(vec![PlaylistInfo {
        id: 4,
        name: "Road trip".into(),
        songs: 3,
    }]));
    l.send(Msg::Songs("playlist:4".into(), songs()));
    assert_eq!(l.text_of("title"), "Road trip");
    let out =
        l.r.document_order()
            .into_iter()
            .filter(|n| l.r.attr(*n, "aria-label").as_deref() == Some("Take out of the playlist"))
            .nth(1)
            .unwrap();
    l.r.fire(out, &Event::Click);
    assert_eq!(l.take(), vec![Request::SetPlaylist(4, vec![1, 3])]);
    // Deleting asks twice.
    l.click("delete-playlist");
    assert!(l.take().is_empty());
    l.click("delete-playlist");
    assert_eq!(l.take(), vec![Request::DeletePlaylist(4)]);
}

#[quench_test::test]
fn keeping_songs_on_the_device_is_a_command_and_the_engine_says_which_are_kept() {
    let l = live("/songbook/ui/album/7");
    l.take();
    l.send(Msg::Songs("album:7".into(), songs()));
    l.click("keep-all");
    let c = l.last_command();
    assert_eq!(c.get("op").and_then(Value::as_str), Some("keep"));
    assert_eq!(
        c.get("ids").and_then(Value::as_array).map(Vec::len),
        Some(3)
    );
    l.hear(&json!({ "ev": "cached", "ids": [3, 1] }));
    let marks: Vec<String> =
        l.r.document_order()
            .into_iter()
            .filter(|n| {
                l.r.attr(*n, "aria-label").as_deref() == Some("Keep on this device")
                    && l.r.attr(*n, "class").is_some_and(|c| c.contains("pl-icon"))
            })
            .map(|n| l.r.attr(n, "data-state").unwrap_or_default())
            .collect();
    assert_eq!(marks, ["kept", "keep", "kept"]);
}

#[quench_test::test]
fn the_saved_page_asks_for_the_songs_the_engine_keeps() {
    let l = live("/songbook/ui");
    l.take();
    l.hear(&json!({ "ev": "cached", "ids": [2, 1] }));
    l.hear(&json!({ "ev": "storage", "used": 5_242_880, "quota": 104_857_600 }));
    // Going there through the tab.
    let l2 = live("/songbook/ui/saved");
    l2.hear(&json!({ "ev": "cached", "ids": [2, 1] }));
    assert!(l2.has("page-saved"));
    l2.hear(&json!({ "ev": "storage", "used": 5_242_880, "quota": 104_857_600 }));
    assert!(
        l2.text_of("usage").contains("5.0 MB of 100.0 MB"),
        "{}",
        l2.text_of("usage")
    );
}

#[quench_test::test]
fn the_crossfade_is_set_in_the_settings_box() {
    let l = live("/songbook/ui");
    l.take();
    l.click("settings");
    assert!(l.has("dialog-settings"));
    assert_eq!(l.text_of("fade-label"), "Crossfade: off");
    l.r.fire(l.find_id("fade"), &Event::Change("6".into()));
    let c = l.last_command();
    assert_eq!(
        (
            c.get("op").and_then(Value::as_str),
            c.get("secs").and_then(Value::as_f64)
        ),
        (Some("fade"), Some(6.0))
    );
    l.hear(&json!({ "ev": "state", "playing": false, "id": 0, "index": -1, "len": 0, "pos": 0, "dur": 0, "shuffle": false, "repeat": "off", "fade": 6, "vol": 1 }));
    assert_eq!(l.text_of("fade-label"), "Crossfade: 6 s");
    l.click("close");
    assert!(!l.has("dialog-settings"));
}

#[quench_test::test]
fn the_songs_kept_after_playing_can_be_limited_where_the_engine_has_such_a_cache() {
    let l = live("/songbook/ui");
    l.take();
    l.click("settings");
    // The browser's engine says nothing of a cache: no setting.
    assert!(!l.has("cache"));
    l.hear(&json!({ "ev": "cache", "used": 3_145_728, "limit": 5_368_709_120_u64 }));
    assert_eq!(l.text_of("cache-label"), "Songs kept after playing: 5 GB");
    assert!(l.text_of("cache-used").contains("3.0 MB"));
    l.r.fire(l.find_id("cache"), &Event::Change("12".into()));
    let c = l.last_command();
    assert_eq!(
        (
            c.get("op").and_then(Value::as_str),
            c.get("gib").and_then(Value::as_i64)
        ),
        (Some("cache"), Some(12))
    );
}

#[quench_test::test]
fn the_queue_shows_what_comes_next_and_a_row_jumps_to_it() {
    let l = live("/songbook/ui");
    l.take();
    l.hear(&state(1, 0, true));
    l.hear(&json!({ "ev": "queue", "ids": [1, 2, 3], "index": 0, "len": 3 }));
    let asked = l.take();
    assert!(
        asked
            .iter()
            .any(|r| matches!(r, Request::Songs(q, key) if key == "meta" && q.ids.contains('2')))
    );
    l.send(Msg::Songs("meta".into(), songs()));
    l.click("queue");
    assert!(l.has("queue-list"));
    let jump =
        l.r.document_order()
            .into_iter()
            .filter(|n| l.r.attr(*n, "aria-label").as_deref() == Some("Play this"))
            .nth(2)
            .unwrap();
    l.r.fire(jump, &Event::Click);
    let c = l.last_command();
    assert_eq!(
        (
            c.get("op").and_then(Value::as_str),
            c.get("index").and_then(Value::as_i64)
        ),
        (Some("jump"), Some(2))
    );
}

#[quench_test::test]
fn a_phone_has_no_way_to_the_admin_pages_and_a_computer_has() {
    assert!(live_on("/songbook/ui", false).has("admin-link"));
    assert!(!live_on("/songbook/ui", true).has("admin-link"));
    assert!(live("/songbook/ui").has("logout"));
}

#[quench_test::test]
fn an_ended_session_and_an_unreachable_server_are_told() {
    let l = live("/songbook/ui");
    l.take();
    l.send(Msg::Failed("timeout".into()));
    assert!(l.text_of("banner").contains("cannot be reached"));
    l.send(Msg::SignedOut);
    assert!(l.text_of("banner").contains("Sign in again"));
    l.send(Msg::Rejected("A playlist needs a name.".into()));
    assert_eq!(l.text_of("toast"), "A playlist needs a name.");
}

#[quench_test::test]
fn an_unknown_address_is_not_found() {
    assert!(live("/songbook/ui/nowhere").has("not-found"));
}

/// An app that plays with the system's player: the engine's commands go to its function, one JSON text each, and its events
/// come back through `hear`; no hidden elements are drawn.
#[quench_test::test]
fn a_host_that_is_not_a_page_is_sent_the_commands_and_tells_the_events() {
    let fresh = Rc::new(quench_ui::platform::ManualPlatform::new());
    quench_ui::platform::install(fresh);
    let sent: Rc<RefCell<Vec<String>>> = Rc::default();
    let log = sent.clone();
    let Player {
        view,
        deliver,
        hear,
        ..
    } = player(
        Rc::new(|_| {}),
        Setup {
            base: "/songbook".into(),
            history: Rc::new(MemoryHistory::new("/songbook/ui")),
            login_url: String::new(),
            mobile: true,
            engine: Some(Rc::new(move |text| log.borrow_mut().push(text))),
            sign_out: None,
        },
    );
    let r = Rc::new(TestRenderer::new());
    let _mount = mount(&r, &r.root(), view);
    assert!(!r.html().contains("sb-engine"), "no hidden elements");
    hear(songbook_player::Event::parse(&json::to_string(&state(2, 1, true))).unwrap());
    deliver(Msg::Songs(
        "meta".into(),
        SongList {
            total: 1,
            songs: vec![songs().songs[1].clone()],
        },
    ));
    let play = r
        .document_order()
        .into_iter()
        .find(|n| r.attr(*n, "id").as_deref() == Some("play"))
        .expect("the play button");
    r.fire(play, &Event::Click);
    let last = sent.borrow().last().cloned().expect("a command was sent");
    assert!(last.contains("\"op\":\"toggle\""), "{last}");
}

#[quench_test::test]
fn on_a_phone_the_settings_are_a_tab_and_the_recent_shelf_holds_ten() {
    let l = live_on("/songbook/ui", true);
    l.take();
    l.send(Msg::Home(home()));
    // The tab bar has the settings, which open the box.
    assert!(l.has("tab-nav-settings"));
    l.click("tab-nav-settings");
    assert!(l.has("dialog-settings"));
    l.click("close");
    // Fifteen songs were played; ten are shown.
    let many: Vec<_> = (1..=15)
        .map(|i| song(i, &format!("Song {i}"), &["Ann"], "Album"))
        .collect();
    l.hear(&json!({ "ev": "recent", "ids": (1..=15).collect::<Vec<i64>>() }));
    l.send(Msg::Songs(
        "meta".into(),
        SongList {
            total: 15,
            songs: many,
        },
    ));
    let shelf = l.find_id("recent");
    assert_eq!(l.r.children(shelf).len(), 10);
}

quench_test::main!();
