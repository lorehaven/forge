#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use songbook_core::{
    AlbumInfo, ArtistInfo, CategoryInfo, Home, Mix, Overview, PlaylistInfo, Search, SongInfo,
    SongQuery,
};
use songbook_native::api::{Api, Result};
use songbook_native::store::{Last, Store};
use songbook_native::tree::Tree;
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
            ..Home::default()
        })
    }
    fn overview(&self) -> Result<Overview> {
        Ok(Overview {
            artists: vec![
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
    assert_eq!(
        ids,
        [
            "recent",
            "mixes",
            "popular",
            "artists",
            "categories",
            "playlists"
        ]
    );
}

#[quench_test::test]
fn artists_are_listed_by_letter_and_then_by_name_with_albums() {
    let (t, ..) = tree("artists", false);
    let letters: Vec<String> = t
        .children("artists")
        .unwrap()
        .into_iter()
        .map(|i| i.title)
        .collect();
    assert_eq!(letters, ["#", "A", "N"]);
    let n = t.children("letter/N").unwrap();
    assert_eq!(
        (n[0].id.as_str(), n[0].subtitle.as_str()),
        ("artist/Nate%20Dogg", "4 songs")
    );
    let inside = t.children("artist/Nate%20Dogg").unwrap();
    assert_eq!(inside[0].id, "artistsongs/Nate%20Dogg");
    assert_eq!(inside[1].id, "album/7");
    assert_eq!(inside[1].subtitle, "2001 · Nate Dogg");
    let all = t.children("artistsongs/Nate%20Dogg").unwrap();
    assert_eq!(all[0].artist, "Nate Dogg");
    assert_eq!(all[0].id, "s:artist/Nate%20Dogg:20");
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
        .map(|i| i.id.rsplit(':').next().unwrap().parse().unwrap())
        .collect();
    assert_eq!(ids, [50, 60]);
    let (away, ..) = {
        let api = Arc::new(Fake {
            played: Mutex::new(vec![]),
            down: true,
        });
        (Tree::new(api, store), ())
    };
    let ids: Vec<String> = away
        .children("recent")
        .unwrap()
        .into_iter()
        .map(|i| i.title)
        .collect();
    assert_eq!(ids, ["Local"]);
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

// ── The conversation ─────────────────────────────────────────────────────────

#[quench_test::test]
fn the_shell_asks_in_lines_and_is_answered_in_lines() {
    let store = dir("proto");
    let api = Arc::new(Fake {
        played: Mutex::new(vec![]),
        down: false,
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

quench_test::main!();
