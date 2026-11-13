#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use songbook_core::{
    AlbumInfo, ArtistInfo, CategoryInfo, Home, Mix, Overview, PlaylistInfo, Resume, Search,
    SongInfo, SongQuery,
};
use songbook_native::api::{Api, Result};
use songbook_native::store::{Last, Store};
use songbook_native::tree::{Item, Tree};
use songbook_native::{Auth, Brain};

fn song(id: i64, title: &str, artist: &str) -> SongInfo {
    SongInfo {
        id,
        title: title.into(),
        artists: vec![artist.into()],
        album: "Album".into(),
        album_id: 7,
        year: "2001".into(),
        duration: 200,
    }
}

struct Fake {
    played: Mutex<Vec<i64>>,
    down: bool,
    resume: Mutex<Option<Resume>>,
    /// A library with many artists (they are listed by letter).
    many: bool,
}

impl Api for Fake {
    fn base(&self) -> String {
        "http://h/songbook/api/v1".into()
    }
    fn home(&self) -> Result<Home> {
        Ok(Home {
            popular: vec![song(1, "One", "Ann"), song(2, "Two", "Bob")],
            mixes: vec![
                Mix {
                    name: "Rap Mix".into(),
                    kind: "mix".into(),
                    category: "Rap".into(),
                    songs: 10,
                },
                Mix {
                    name: "Best of Rap".into(),
                    kind: "best".into(),
                    category: "Rap".into(),
                    songs: 5,
                },
            ],
            artists: vec![ArtistInfo {
                name: "Ann".into(),
                songs: 3,
                albums: 1,
            }],
            albums: vec![AlbumInfo {
                id: 7,
                title: "Best of Ann".into(),
                artist: "Ann".into(),
                year: "2001-05-01".into(),
                kind: "album".into(),
                songs: 2,
            }],
            ..Home::default()
        })
    }
    fn overview(&self) -> Result<Overview> {
        let crowd: Vec<ArtistInfo> = if self.many {
            (0..70)
                .map(|i| ArtistInfo {
                    name: format!("Band {i:02}"),
                    songs: 1,
                    albums: 1,
                })
                .collect()
        } else {
            Vec::new()
        };
        Ok(Overview {
            admin: false,
            artists: [
                crowd,
                vec![
                    ArtistInfo {
                        name: "Ann".into(),
                        songs: 3,
                        albums: 1,
                    },
                    ArtistInfo {
                        name: "Nate Dogg".into(),
                        songs: 4,
                        albums: 1,
                    },
                    ArtistInfo {
                        name: "Adam".into(),
                        songs: 1,
                        albums: 1,
                    },
                    ArtistInfo {
                        name: "50 Cent".into(),
                        songs: 1,
                        albums: 1,
                    },
                ],
            ]
            .concat(),
            categories: vec![CategoryInfo {
                name: "Rap & Soul".into(),
                songs: 9,
            }],
            ..Overview::default()
        })
    }
    fn albums(&self, artist: &str) -> Result<Vec<AlbumInfo>> {
        Ok(vec![AlbumInfo {
            id: 7,
            title: format!("Best of {artist}"),
            artist: artist.into(),
            year: "2001-05-01".into(),
            kind: "album".into(),
            songs: 2,
        }])
    }
    fn songs(&self, q: &SongQuery) -> Result<Vec<SongInfo>> {
        if q.album == 7 {
            return Ok(vec![
                song(10, "A", "Ann"),
                song(11, "B", "Ann"),
                song(12, "C", "Ann"),
            ]);
        }
        if !q.artist.is_empty() {
            return Ok(vec![song(20, "X", &q.artist)]);
        }
        if q.category == "Rap" {
            return Ok(vec![song(30, "R", "Bob")]);
        }
        Ok(vec![])
    }
    fn search(&self, text: &str) -> Result<Search> {
        Ok(Search {
            songs: vec![song(40, text, "Cy")],
            ..Search::default()
        })
    }
    fn recent(&self) -> Result<Vec<SongInfo>> {
        if self.down {
            return Err(songbook_native::Failure::Unreachable("down".into()));
        }
        Ok(vec![song(50, "Server", "Di")])
    }
    fn playlists(&self) -> Result<Vec<PlaylistInfo>> {
        Ok(vec![PlaylistInfo {
            id: 3,
            name: "Mine".into(),
            songs: 2,
        }])
    }
    fn resume(&self) -> Result<Option<Resume>> {
        Ok(self.resume.lock().unwrap().clone())
    }
    fn set_resume(&self, resume: &Resume) -> Result<Resume> {
        let kept = Resume {
            at: 1_800_000_000,
            ..resume.clone()
        };
        *self.resume.lock().unwrap() = Some(kept.clone());
        Ok(kept)
    }
    fn played(&self, id: i64) -> Result<()> {
        self.played.lock().unwrap().push(id);
        Ok(())
    }
}

fn dir(name: &str) -> Store {
    let d = std::env::temp_dir().join(format!("songbook-native-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    Store::new(d)
}

fn tree(name: &str, down: bool) -> (Tree, Store, Arc<Fake>) {
    let store = dir(name);
    let api = Arc::new(Fake {
        played: Mutex::new(vec![]),
        down,
        resume: Mutex::default(),
        many: name == "crowd",
    });
    (Tree::new(api.clone(), store.clone()), store, api)
}

#[quench_test::test]
fn the_root_lists_what_the_shell_does_not() {
    let (t, ..) = tree("root", false);
    let ids: Vec<String> = t
        .children("root")
        .unwrap()
        .into_iter()
        .map(|i| i.id)
        .collect();
    assert_eq!(ids, ["home", "artists", "playlists"], "the tabs of the car");
}

#[quench_test::test]
fn the_home_tab_has_the_web_players_sections_with_cards_for_artists_and_albums() {
    let (t, ..) = tree("home", false);
    let home = t.children("home").unwrap();
    let sections: Vec<&str> =
        home.iter()
            .map(|i| i.group.as_str())
            .fold(Vec::new(), |mut seen, g| {
                if seen.last() != Some(&g) {
                    seen.push(g);
                }
                seen
            });
    assert_eq!(
        sections,
        [
            "",
            "Made for you",
            "Popular in your library",
            "Artists to explore",
            "New in your library",
            "Browse by category"
        ]
    );
    let of = |group: &str| -> Vec<&Item> { home.iter().filter(|i| i.group == group).collect() };
    assert_eq!(of("Made for you")[0].id, "mix/0");
    assert!(
        of("Popular in your library")
            .iter()
            .all(|i| i.playable && !i.grid)
    );
    let artists = of("Artists to explore");
    assert_eq!(
        (artists[0].id.as_str(), artists[0].grid),
        ("artist/Ann", true)
    );
    let albums = of("New in your library");
    assert_eq!((albums[0].id.as_str(), albums[0].grid), ("album/7", true));
    assert_eq!(of("Browse by category")[0].id, "category/Rap%20%26%20Soul");
    assert_eq!(home[0].id, "recent", "and the way back to what was playing");
}

#[quench_test::test]
fn a_list_of_songs_opens_with_play_all_and_shuffle_that_play_the_whole_list() {
    let (t, ..) = tree("play", false);
    for place in [
        "album/7",
        "mix/0",
        "playlist/3",
        "category/Rap",
        "popular",
        "recent",
    ] {
        let kids = t.children(place).unwrap();
        assert_eq!(
            (kids[0].id.clone(), kids[1].id.clone()),
            (format!("play/{place}"), format!("shuffle/{place}")),
            "{place}"
        );
        assert!(kids[0].playable && !kids[0].browsable, "{place}");
    }
    let artist = t.children("artist/Nate%20Dogg").unwrap();
    assert_eq!(artist[0].id, "play/artist/Nate%20Dogg");
    assert_eq!(artist[1].id, "shuffle/artist/Nate%20Dogg");
    assert_eq!(artist[2].id, "artistsongs/Nate%20Dogg");
    assert_eq!((artist[3].grid, artist[3].group.as_str()), (true, "Albums"));

    let all = t.queue("play/album/7").unwrap().unwrap();
    assert_eq!((all.items.len(), all.start, all.shuffled), (3, 0, false));
    assert!(t.queue("shuffle/album/7").unwrap().unwrap().shuffled);
    assert!(
        t.queue("play/mix/0").unwrap().unwrap().shuffled,
        "a mix is shuffled either way"
    );
    assert!(!t.queue("play/mix/1").unwrap().unwrap().shuffled);
    assert!(t.queue("play/nowhere").unwrap().is_none());
    assert_eq!(t.item("play/album/7").unwrap().unwrap().title, "Play all");
}

#[quench_test::test]
fn many_artists_are_listed_by_letter_and_few_as_cards() {
    let (few, ..) = tree("few", false);
    let direct = few.children("artists").unwrap();
    assert_eq!(direct[0].id, "artist/Ann");
    assert!(direct.iter().all(|i| i.grid));
    let (many, ..) = tree("crowd", false);
    let letters: Vec<String> = many
        .children("artists")
        .unwrap()
        .into_iter()
        .map(|i| i.title)
        .collect();
    assert_eq!(letters, ["#", "A", "B", "N"]);
}

#[quench_test::test]
fn artists_are_listed_by_letter_and_then_by_name_with_albums() {
    let (t, ..) = tree("artists", false);
    let n = t.children("letter/N").unwrap();
    assert_eq!(
        (n[0].id.as_str(), n[0].subtitle.as_str()),
        ("artist/Nate%20Dogg", "4 songs")
    );
    let inside = t.children("artist/Nate%20Dogg").unwrap();
    assert_eq!(inside[2].id, "artistsongs/Nate%20Dogg");
    assert_eq!(inside[3].id, "album/7");
    assert_eq!(inside[3].subtitle, "2001 · Nate Dogg");
    let all = t.children("artistsongs/Nate%20Dogg").unwrap();
    assert_eq!(all[2].artist, "Nate Dogg", "after Play all and Shuffle");
    assert_eq!(all[2].id, "s:artist/Nate%20Dogg:20");
}

#[quench_test::test]
fn a_chosen_song_queues_its_place_and_starts_at_it() {
    let (t, ..) = tree("queue", false);
    let q = t.queue("s:album/7:11").unwrap().unwrap();
    assert_eq!((q.items.len(), q.start, q.shuffled), (3, 1, false));
    assert_eq!(q.items[1].uri, "http://h/songbook/api/v1/stream/11");
    assert_eq!(q.items[1].duration_ms, 200_000);
    assert!(t.queue("album/7").unwrap().is_none());
    assert!(t.queue("s:nowhere:1").unwrap().is_none());
}

#[quench_test::test]
fn a_mix_is_played_shuffled_and_a_best_of_is_not() {
    let (t, ..) = tree("mix", false);
    let mixes = t.children("mixes").unwrap();
    assert_eq!(mixes[0].subtitle, "Shuffled · 10 songs");
    assert_eq!(mixes[1].subtitle, "Top 5");
    assert!(t.queue("s:mix/0:30").unwrap().unwrap().shuffled);
    assert!(!t.queue("s:mix/1:30").unwrap().unwrap().shuffled);
}

#[quench_test::test]
fn search_and_item_and_categories() {
    let (t, ..) = tree("search", false);
    let found = t.search("nate dogg").unwrap();
    assert_eq!(found[0].id, "s:search/nate%20dogg:40");
    assert_eq!(
        t.item("s:search/nate%20dogg:40").unwrap().unwrap().title,
        "nate dogg"
    );
    assert!(t.item("nonsense").unwrap().is_none());
    assert_eq!(t.item("root").unwrap().unwrap().title, "Songbook");
    assert_eq!(
        t.children("categories").unwrap()[0].id,
        "category/Rap%20%26%20Soul"
    );
    assert_eq!(t.children("playlists").unwrap()[0].kind, "playlist");
}

#[quench_test::test]
fn recently_played_joins_the_servers_list_with_this_phones_and_survives_the_server_being_away() {
    let (t, store, api) = tree("recent", false);
    t.played(&song(60, "Local", "Ed"));
    assert_eq!(api.played.lock().unwrap().as_slice(), [60]);
    let ids: Vec<i64> = t
        .children("recent")
        .unwrap()
        .iter()
        .filter(|i| i.kind == "song")
        .map(|i| i.id.rsplit(':').next().unwrap().parse().unwrap())
        .collect();
    assert_eq!(ids, [50, 60]);
    let (away, ..) = {
        let api = Arc::new(Fake {
            played: Mutex::new(vec![]),
            down: true,
            resume: Mutex::default(),
            many: false,
        });
        (Tree::new(api, store), ())
    };
    let ids: Vec<String> = away
        .children("recent")
        .unwrap()
        .into_iter()
        .filter(|i| i.kind == "song")
        .map(|i| i.title)
        .collect();
    assert_eq!(ids, ["Local"]);
}

#[quench_test::test]
fn recently_played_shows_the_artists_and_albums_as_cards_above_the_songs() {
    let (t, _, _) = tree("recent-cards", false);
    t.played(&song(60, "Local", "Ed"));
    let items = t.children("recent").unwrap();
    let cards: Vec<(&str, &str, bool)> = items
        .iter()
        .filter(|i| i.kind == "artist" || i.kind == "album")
        .map(|i| (i.kind.as_str(), i.group.as_str(), i.grid))
        .collect();
    assert!(cards.contains(&("artist", "Artists", true)), "{cards:?}");
    assert!(cards.contains(&("album", "Albums", true)), "{cards:?}");
    assert!(items.iter().any(|i| i.id == "artist/Ed"), "artists open as folders");
    assert!(items.iter().any(|i| i.id == "album/7" && i.art.contains("/art/album/7")));
    let first_song = items.iter().position(|i| i.kind == "song").unwrap();
    assert!(items[..first_song].iter().any(|i| i.kind == "artist"), "cards come before the songs");
}

#[quench_test::test]
fn the_continue_row_is_the_song_that_was_playing() {
    let (t, store, _) = tree("last", false);
    assert_eq!(t.children("recent-root").unwrap()[0].title, "Server");
    store.set_last(&Last {
        id: "s:popular:2".into(),
        position_ms: 4000,
        song: song(2, "Two", "Bob"),
    });
    assert_eq!(t.children("recent-root").unwrap()[0].title, "Two");
    assert_eq!(t.children("suggested-root").unwrap().len(), 2);
}

#[quench_test::test]
fn the_phone_remembers_forty_songs_newest_first_each_once() {
    let store = dir("forty");
    for i in 0..45 {
        store.add_recent(&song(i, "S", "A"), 40);
    }
    store.add_recent(&song(44, "S", "A"), 40);
    let r = store.recent();
    assert_eq!((r.len(), r[0].id, r[1].id), (40, 44, 43));
}

// ── Auth against a stand-in Gatehouse + Songbook ────────────────────────────

#[derive(Default)]
struct World {
    logins: u32,
    refreshes: u32,
    /// Tokens the library accepts.
    valid_access: String,
    refresh_refused: bool,
    library_hits: Vec<String>,
}

fn serve(world: Arc<Mutex<World>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let w = Arc::clone(&world);
            std::thread::spawn(move || answer(stream, &w));
        }
    });
    base
}

fn answer(mut stream: TcpStream, world: &Mutex<World>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let target = line.split_whitespace().nth(1).unwrap().to_string();
    let (mut length, mut auth) = (0usize, String::new());
    loop {
        let mut h = String::new();
        reader.read_line(&mut h).unwrap();
        if h.trim().is_empty() {
            break;
        }
        let (k, v) = h.split_once(':').unwrap();
        match k.to_ascii_lowercase().as_str() {
            "content-length" => length = v.trim().parse().unwrap(),
            "authorization" => auth = v.trim().to_string(),
            _ => {}
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    let mut w = world.lock().unwrap();
    let (status, text) = if target.ends_with("/auth/login") {
        w.logins += 1;
        w.valid_access = "access-login".into();
        (200, r#"{"access_token":"access-login","refresh_token":"refresh-1","token_type":"Bearer","expires_in":900}"#.to_string())
    } else if target.ends_with("/auth/refresh") {
        w.refreshes += 1;
        if w.refresh_refused {
            (401, String::new())
        } else {
            w.valid_access = format!("access-r{}", w.refreshes);
            (
                200,
                format!(
                    r#"{{"access_token":"access-r{0}","refresh_token":"refresh-r{0}","token_type":"Bearer","expires_in":900}}"#,
                    w.refreshes
                ),
            )
        }
    } else {
        w.library_hits.push(auth.clone());
        if auth == format!("Bearer {}", w.valid_access) || (w.valid_access == "open") {
            (
                200,
                r#"{"songs":[],"total":0,"artists":[],"categories":[],"albums":0}"#.to_string(),
            )
        } else {
            (401, String::new())
        }
    };
    drop(w);
    let _ = write!(
        stream,
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    );
}

#[quench_test::test]
fn signing_in_keeps_the_session_and_a_new_run_resumes_it_without_the_password() {
    let world = Arc::new(Mutex::new(World::default()));
    let base = serve(Arc::clone(&world));
    let store = dir("login");
    let auth = Auth::new(store.clone());
    assert!(!auth.signed_in());
    auth.login(&base, "ann", "secret").unwrap();
    assert!(auth.signed_in());
    assert_eq!(auth.get("/library/overview").map(|_| ()), Ok(()));
    // A new run of the program.
    let again = Auth::new(store);
    assert!(again.signed_in());
    assert_eq!(again.settings().user, "ann");
    assert_eq!(world.lock().unwrap().logins, 1);
}

#[quench_test::test]
fn a_refused_access_token_is_renewed_once_and_the_new_refresh_token_is_kept() {
    let world = Arc::new(Mutex::new(World::default()));
    let base = serve(Arc::clone(&world));
    let store = dir("renew");
    let auth = Auth::new(store.clone());
    auth.login(&base, "ann", "secret").unwrap();
    world.lock().unwrap().valid_access = "something-else".into(); // the one held is now refused
    // The renewal makes `access-r1` valid, and the repeated request carries it.
    world.lock().unwrap().refresh_refused = false;
    let result = auth.get("/library/overview");
    assert_eq!(world.lock().unwrap().refreshes, 1, "{result:?}");
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(store.token().as_deref(), Some("refresh-r1"));
}

#[quench_test::test]
fn a_refusal_of_the_refresh_token_signs_out_but_an_absent_server_does_not() {
    let world = Arc::new(Mutex::new(World::default()));
    let base = serve(Arc::clone(&world));
    let store = dir("refused");
    let auth = Auth::new(store.clone());
    auth.login(&base, "ann", "secret").unwrap();
    world.lock().unwrap().valid_access = "other".into();
    world.lock().unwrap().refresh_refused = true;
    assert_eq!(
        auth.get("/library/overview"),
        Err(songbook_native::Failure::SignedOut)
    );
    assert!(!auth.signed_in());
    assert!(store.token().is_none());

    // A server that is not there: signed in stays signed in.
    let store = dir("away");
    store.set_token("refresh-x");
    store.set_settings(&songbook_native::store::Settings {
        server: "http://127.0.0.1:1".into(),
        user: "ann".into(),
        open: false,
    });
    let auth = Auth::new(store);
    assert!(matches!(
        auth.get("/library/overview"),
        Err(songbook_native::Failure::Unreachable(_))
    ));
    assert!(auth.signed_in());
}

#[quench_test::test]
fn a_wrong_password_says_so_and_a_blank_user_asks_for_an_open_server() {
    let world = Arc::new(Mutex::new(World::default()));
    let base = serve(Arc::clone(&world));
    let auth = Auth::new(dir("open"));
    world.lock().unwrap().valid_access = "open".into();
    auth.login(&base, "", "").unwrap();
    assert!(auth.signed_in());
    assert_eq!(auth.token(), Ok(None));
    assert!(auth.get("/library/overview").is_ok());
    world.lock().unwrap().valid_access = "closed".into();
    assert!(matches!(
        Auth::new(dir("closed")).login(&base, "", ""),
        Err(songbook_native::Failure::Refused(_))
    ));
}

#[quench_test::test]
fn a_sign_in_or_sign_out_in_one_program_is_the_others_too() {
    let world = Arc::new(Mutex::new(World::default()));
    let base = serve(Arc::clone(&world));
    let store = dir("two-programs");
    let (screens, service) = (Auth::new(store.clone()), Auth::new(store));
    assert!(!service.signed_in());
    screens.login(&base, "ann", "secret").unwrap();
    assert!(service.signed_in(), "the service sees the sign-in");
    assert_eq!(service.settings().user, "ann");
    assert!(service.get("/library/overview").is_ok(), "and can use it");
    screens.sign_out();
    assert!(!service.signed_in(), "and the sign-out");
    assert_eq!(service.token(), Err(songbook_native::Failure::SignedOut));
}

// ── The conversation ─────────────────────────────────────────────────────────

#[quench_test::test]
fn the_shell_asks_in_lines_and_is_answered_in_lines() {
    let store = dir("proto");
    let api = Arc::new(Fake {
        played: Mutex::new(vec![]),
        down: false,
        resume: Mutex::default(),
        many: false,
    });
    let brain = Brain::with(store.clone(), Arc::new(Auth::new(store)), api);
    // Not signed in: the library is refused, with the code the shell acts on.
    let a = brain.handle(r#"{"id":1,"op":"children","parent":"root"}"#);
    assert!(a.contains(r#""id":1"#) && a.contains("signed_out"), "{a}");
    let s = brain.handle(r#"{"id":2,"op":"status"}"#);
    assert!(s.contains(r#""signed_in":false"#), "{s}");
    assert!(brain.handle("nonsense").contains("bad_request"));
    assert!(
        brain
            .handle(r#"{"id":3,"op":"fly"}"#)
            .contains("unknown op")
    );
    let l = brain.handle(r#"{"id":4,"op":"last_load"}"#);
    assert!(l.contains(r#""last":null"#), "{l}");
}

#[quench_test::test]
fn where_the_person_stopped_goes_to_the_service_and_comes_back() {
    let store = dir("resume");
    // An open session (a development server): signed in with no password.
    store.set_settings(&songbook_native::store::Settings {
        server: "http://127.0.0.1:1".into(),
        user: String::new(),
        open: true,
    });
    let api = Arc::new(Fake {
        played: Mutex::new(vec![]),
        down: false,
        resume: Mutex::default(),
        many: false,
    });
    let brain = Brain::with(store.clone(), Arc::new(Auth::new(store)), api);
    let nothing = brain.handle(r#"{"id":1,"op":"resume_get"}"#);
    assert!(nothing.contains(r#""resume":null"#), "{nothing}");
    let put = brain.handle(
        r#"{"id":2,"op":"resume_put","song_id":7,"position_ms":61000,"queue":[5,6,7,8],"index":2,"device":"phone"}"#,
    );
    assert!(put.contains(r#""at":1800000000"#), "{put}");
    let got = brain.handle(r#"{"id":3,"op":"resume_get"}"#);
    for part in [
        r#""song_id":7"#,
        r#""position_ms":61000"#,
        r#""queue":[5,6,7,8]"#,
        r#""index":2"#,
        r#""device":"phone""#,
    ] {
        assert!(got.contains(part), "{part} in {got}");
    }
}

quench_test::main!();
