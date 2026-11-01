//! The screens with no host: mounted into a test tree, driven by events and the clock, read back as text.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::cell::RefCell;
use std::rc::Rc;

use quench_ui::Event;
use quench_ui::prelude::*;
use quench_ui::router::{History, MemoryHistory};
use quench_ui::test_renderer::NodeId;
use songbook_app::{Msg, Request, Setup, Songbook, songbook};
use songbook_core::{Album, ArtistRow, Browse, Filter, Job, NewArtist, State, Suggestion, Track};

type Asked = Rc<RefCell<Vec<Request>>>;

thread_local! {
    static CLOCK: RefCell<Option<Rc<quench_ui::platform::ManualPlatform>>> = const { RefCell::new(None) };
}

/// Moves this test's clock on. Every test gets a clock of its own: the platform is per thread and the runner reuses threads.
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
    history: Rc<MemoryHistory>,
    _mount: quench_ui::Mount<TestRenderer>,
}

impl Live {
    fn find(&self, tag: &str, attr: (&str, &str)) -> NodeId {
        self.r
            .document_order()
            .into_iter()
            .find(|n| {
                self.r.tag(*n).as_deref() == Some(tag)
                    && self.r.attr(*n, attr.0).as_deref() == Some(attr.1)
            })
            .unwrap_or_else(|| panic!("no <{tag} {}={}>: {}", attr.0, attr.1, self.r.html()))
    }

    /// Opens every category of the side panel (they start folded).
    fn expand(&self) {
        let groups: Vec<NodeId> = self
            .r
            .document_order()
            .into_iter()
            .filter(|n| {
                self.r
                    .attr(*n, "class")
                    .is_some_and(|c| c.contains("sb-group-btn"))
                    && self.r.attr(*n, "aria-expanded").as_deref() == Some("false")
            })
            .collect();
        for group in groups {
            self.r.fire(group, &Event::Click);
        }
    }

    fn has(&self, id: &str) -> bool {
        self.r
            .document_order()
            .into_iter()
            .any(|n| self.r.attr(n, "id").as_deref() == Some(id))
    }

    fn click(&self, tag: &str, attr: (&str, &str)) {
        self.r.fire(self.find(tag, attr), &Event::Click);
    }

    fn text_of(&self, id: &str) -> String {
        let node = self
            .r
            .document_order()
            .into_iter()
            .find(|n| self.r.attr(*n, "id").as_deref() == Some(id))
            .unwrap_or_else(|| panic!("no #{id}: {}", self.r.html()));
        self.r.text_of(node)
    }

    fn type_into(&self, id: &str, text: &str) {
        self.r
            .fire(self.find("input", ("id", id)), &Event::Input(text.into()));
    }

    fn send(&self, msg: Msg) {
        (self.deliver)(msg);
    }

    fn take(&self) -> Vec<Request> {
        std::mem::take(&mut *self.asked.borrow_mut())
    }
}

fn live(path: &str) -> Live {
    let fresh = Rc::new(quench_ui::platform::ManualPlatform::new());
    quench_ui::platform::install(fresh.clone());
    CLOCK.with(|c| *c.borrow_mut() = Some(fresh));
    let asked: Asked = Rc::default();
    let log = asked.clone();
    let history = Rc::new(MemoryHistory::new(path));
    let Songbook { view, deliver } = songbook(
        Rc::new(move |q| log.borrow_mut().push(q)),
        Setup {
            base: "/songbook".into(),
            language: "en-US".into(),
            login_url: "/songbook/ui/login".into(),
            history: history.clone(),
            busy_ms: 1_500,
            idle_ms: 5_000,
        },
    );
    let r = Rc::new(TestRenderer::new());
    let mount = mount(&r, &r.root(), view);
    Live {
        r,
        asked,
        deliver,
        history,
        _mount: mount,
    }
}

fn artist(name: &str, category: &str, tracks: u32, synced: bool) -> ArtistRow {
    ArtistRow {
        name: name.into(),
        pinned: false,
        category: category.into(),
        deezer_name: if synced { name.into() } else { String::new() },
        deezer_id: if synced { "359".into() } else { String::new() },
        fans: "100".into(),
        tracks,
        synced,
    }
}

fn state() -> State {
    State {
        artists: vec![
            artist("Nate Dogg", "American Rap", 106, true),
            artist("Kartky", "Polish Rap", 12, true),
            artist("Fresh", "Uncategorized", 0, false),
        ],
        categories: vec!["American Rap".into(), "Polish Rap".into()],
        uncategorized: "Uncategorized".into(),
        genres: vec!["Rap/Hip Hop".into(), "R&B".into()],
        suggestions: vec![
            Suggestion {
                artist: "Warren G".into(),
                deezer_id: "712".into(),
                fans: "1".into(),
                related_to_count: 3,
                related_to: "Nate Dogg; Snoop Dogg; Nas; Eminem".into(),
                scope: "American Rap".into(),
            },
            Suggestion {
                artist: "Daz".into(),
                deezer_id: "9".into(),
                fans: "1".into(),
                related_to_count: 1,
                related_to: "Nate Dogg".into(),
                scope: "American Rap".into(),
            },
        ],
    }
}

fn track(id: i64, title: &str, url: &str, official: bool, manual: bool) -> Track {
    Track {
        id,
        title: title.into(),
        artists: "Nate Dogg, Snoop Dogg".into(),
        url: url.into(),
        official,
        manual,
    }
}

fn browse() -> Browse {
    Browse {
        total: 3,
        linked: 2,
        shown: 3,
        albums: vec![Album {
            artist: "Nate Dogg".into(),
            album: "Ghetto Preacher".into(),
            year: "1999".into(),
            id: 6_047_731,
            kind: "album".into(),
            genre: "Rap/Hip Hop; R&B".into(),
            tracks: vec![
                track(
                    1,
                    "Nobody Does It Better",
                    "https://youtu.be/a",
                    true,
                    false,
                ),
                track(2, "Hardest Man in Town", "https://youtu.be/b", false, true),
                track(3, "Puppy Love", "", false, false),
            ],
        }],
    }
}

fn started(path: &str) -> Live {
    let l = live(path);
    l.send(Msg::State(state()));
    l.send(Msg::Browse(browse()));
    l.take();
    l
}

#[quench_test::test]
fn the_app_asks_for_the_state_the_job_and_the_albums_on_arrival() {
    let l = live("/songbook/ui/library");
    let asked = l.take();
    assert!(asked.contains(&Request::State), "{asked:?}");
    assert!(asked.contains(&Request::Job { from: 0 }), "{asked:?}");
    assert!(
        asked.contains(&Request::Browse(Filter::default())),
        "{asked:?}"
    );
    assert!(l.has("waiting"), "{}", l.r.html());
}

#[quench_test::test]
fn the_side_panel_groups_the_followed_artists_by_category() {
    let l = started("/songbook/ui/library");
    l.expand();
    let text = l.r.text();
    assert!(text.contains("Followed artists (3)"), "{text}");
    assert!(
        text.contains("American Rap (1)")
            && text.contains("Polish Rap (1)")
            && text.contains("Uncategorized (1)"),
        "{text}"
    );
    assert!(
        text.contains("106 songs") && text.contains("12 songs"),
        "{text}"
    );
    assert!(text.contains("Not synced yet"), "{text}");
    assert_eq!(
        l.r.attr(l.find("div", ("data-artist", "Nate Dogg")), "class")
            .as_deref()
            .map(str::trim),
        Some("sb-artist")
    );
    // The page links are there, and the library is current.
    assert!(
        l.r.attr(l.find("a", ("id", "nav-library")), "aria-current")
            .is_some(),
        "{}",
        l.r.html()
    );
    assert_eq!(
        l.r.attr(l.find("a", ("id", "nav-pipeline")), "href")
            .as_deref(),
        Some("/songbook/ui/pipeline")
    );
}

#[quench_test::test]
fn the_categories_start_folded_and_fold_again() {
    let l = started("/songbook/ui/library");
    let listed = |l: &Live| {
        l.r.document_order()
            .into_iter()
            .any(|n| l.r.attr(n, "data-artist").as_deref() == Some("Nate Dogg"))
    };
    assert!(!listed(&l), "{}", l.r.html());
    l.expand();
    assert!(listed(&l));
    l.click("button", ("data-fold", "American Rap"));
    assert!(!listed(&l));
    l.click("button", ("data-fold", "American Rap"));
    assert!(listed(&l));
}

#[quench_test::test]
fn the_library_opens_with_the_statistics_of_the_whole_collection() {
    let l = started("/songbook/ui/library");
    assert!(!l.has("stats"), "nothing to show before the numbers come");
    l.send(Msg::Stats(songbook_core::Stats {
        artists: 61,
        credited: 2122,
        albums: 679,
        songs: 8395,
        downloaded: 8141,
        missing: 254,
        missing_linked: 4,
    }));
    assert_eq!(l.text_of("stat-artists"), "Artists612122 credited in all");
    assert_eq!(l.text_of("stat-albums"), "Albums679");
    assert_eq!(l.text_of("stat-songs"), "Songs8395");
    assert_eq!(
        l.text_of("stat-downloaded"),
        "Downloaded814197% of the songs"
    );
    assert_eq!(
        l.text_of("stat-missing"),
        "Missing2544 with a link · 250 without"
    );
}

#[quench_test::test]
fn the_statistics_are_asked_for_when_the_page_opens_and_after_a_change() {
    let l = live("/songbook/ui/library");
    assert!(l.take().contains(&Request::Stats));
    l.send(Msg::CategorySet);
    assert!(l.take().contains(&Request::Stats));
}

#[quench_test::test]
fn the_library_shows_albums_with_their_songs_and_links() {
    let l = started("/songbook/ui/library");
    assert_eq!(
        l.text_of("summary"),
        "All artists · 3 songs · 2 with a YouTube link"
    );
    let text = l.r.text();
    assert!(
        text.contains("Ghetto Preacher")
            && text.contains("1999")
            && text.contains("Rap/Hip Hop")
            && text.contains("R&B"),
        "{text}"
    );
    assert!(
        text.contains("Nobody Does It Better") && text.contains("Puppy Love"),
        "{text}"
    );
    assert!(
        text.contains("manual") && text.contains("no link"),
        "{text}"
    );
    // One play link for each linked song; the official one is ticked.
    assert_eq!(
        l.r.attr(l.find("a", ("href", "https://youtu.be/a")), "title")
            .as_deref(),
        Some("Official / artist channel")
    );
    assert_eq!(
        l.r.attr(l.find("a", ("href", "https://youtu.be/b")), "title")
            .as_deref(),
        Some("Unofficial upload")
    );
    assert_eq!(
        l.r.attr(l.find("a", ("href", "https://youtu.be/b")), "target")
            .as_deref(),
        Some("_blank")
    );
    assert_eq!(
        l.r.document_order()
            .into_iter()
            .filter(|n| l.r.attr(*n, "class").as_deref() == Some("sb-ok"))
            .count(),
        1
    );
}

#[quench_test::test]
fn the_summary_says_when_only_the_first_are_shown_and_when_nothing_matches() {
    let l = started("/songbook/ui/library");
    let mut b = browse();
    b.total = 900;
    b.shown = 300;
    l.send(Msg::Browse(b));
    assert!(
        l.text_of("summary").ends_with(" · showing first 300"),
        "{}",
        l.text_of("summary")
    );
    l.send(Msg::Browse(Browse {
        total: 0,
        linked: 0,
        shown: 0,
        albums: vec![],
    }));
    assert!(l.has("nothing"), "{}", l.r.html());
}

#[quench_test::test]
fn the_filters_ask_for_the_albums_again() {
    let l = started("/songbook/ui/library");
    l.r.fire(
        l.find("button", ("data-chip", "links-missing")),
        &Event::Click,
    );
    assert_eq!(
        l.take(),
        vec![Request::Browse(Filter {
            links: "missing".into(),
            ..Filter::default()
        })]
    );
    assert_eq!(
        l.r.attr(
            l.find("button", ("data-chip", "links-missing")),
            "aria-pressed"
        )
        .as_deref(),
        Some("true")
    );

    l.r.fire(l.find("button", ("data-chip", "genre")), &Event::Click);
    let asked = l.take();
    assert_eq!(
        asked,
        vec![Request::Browse(Filter {
            links: "missing".into(),
            genre: "Rap/Hip Hop".into(),
            ..Filter::default()
        })]
    );

    l.click("button", ("data-chip", "category-all"));
    l.click("button", ("data-chip", "links-all"));
    let asked = l.take();
    assert_eq!(
        asked.last(),
        Some(&Request::Browse(Filter {
            genre: "Rap/Hip Hop".into(),
            ..Filter::default()
        }))
    );
}

#[quench_test::test]
fn choosing_an_artist_filters_by_it_and_the_summary_names_it() {
    let l = started("/songbook/ui/library");
    l.expand();
    let main = l.r.children(l.find("div", ("data-artist", "Kartky")))[0];
    l.r.fire(main, &Event::Click);
    assert_eq!(
        l.take(),
        vec![Request::Browse(Filter {
            artist: "Kartky".into(),
            ..Filter::default()
        })]
    );
    assert!(
        l.text_of("summary").starts_with("Kartky · "),
        "{}",
        l.text_of("summary")
    );
    l.click("button", ("id", "all-artists"));
    assert_eq!(l.take(), vec![Request::Browse(Filter::default())]);
}

#[quench_test::test]
fn searching_waits_for_a_pause_in_the_typing() {
    let l = started("/songbook/ui/library");
    l.type_into("q", "pup");
    l.type_into("q", "puppy");
    assert!(l.take().is_empty());
    tick(250.0);
    assert_eq!(
        l.take(),
        vec![Request::Browse(Filter {
            q: "puppy".into(),
            ..Filter::default()
        })]
    );
}

#[quench_test::test]
fn a_link_is_edited_in_a_dialog() {
    let l = started("/songbook/ui/library");
    assert!(!l.has("dialog-link"));
    l.r.fire(
        l.find("button", ("aria-label", "Add link for Puppy Love")),
        &Event::Click,
    );
    assert!(l.has("dialog-link"));
    assert!(l.r.attr(l.find("a", ("id", "link-search")), "href").unwrap().starts_with("https://www.youtube.com/results?search_query=Nate%20Dogg%2C%20Snoop%20Dogg%20Puppy%20Love"));
    l.type_into("link-url", "  https://youtu.be/new  ");
    l.r.fire(l.find("form", ("class", "sb-dialog-form")), &Event::Submit);
    assert_eq!(
        l.take(),
        vec![Request::SaveLink {
            track_id: 3,
            url: "https://youtu.be/new".into()
        }]
    );
    l.send(Msg::LinkSaved);
    assert!(!l.has("dialog-link"));
    assert!(l.text_of("snack").contains("Link saved"));
    let asked = l.take();
    assert!(
        asked.contains(&Request::State) && asked.contains(&Request::Browse(Filter::default())),
        "{asked:?}"
    );
    tick(4_000.0);
    assert_eq!(l.text_of("snack"), "");
}

#[quench_test::test]
fn a_link_is_removed_from_the_dialog() {
    let l = started("/songbook/ui/library");
    l.r.fire(
        l.find(
            "button",
            ("aria-label", "Edit link for Nobody Does It Better"),
        ),
        &Event::Click,
    );
    assert_eq!(
        l.r.attr(l.find("input", ("id", "link-url")), "value")
            .as_deref(),
        Some("https://youtu.be/a")
    );
    l.click("button", ("id", "link-remove"));
    assert_eq!(
        l.take(),
        vec![Request::SaveLink {
            track_id: 1,
            url: String::new()
        }]
    );
    l.send(Msg::LinkSaved);
    assert!(l.text_of("snack").contains("Link removed"));
}

#[quench_test::test]
fn the_dialog_closes_on_cancel() {
    let l = started("/songbook/ui/library");
    l.click("button", ("id", "add-artist"));
    assert!(l.has("dialog-add"));
    l.click("button", ("id", "dialog-cancel"));
    assert!(!l.has("dialog-add"));
}

#[quench_test::test]
fn an_album_is_blacklisted_after_asking() {
    let l = started("/songbook/ui/library");
    l.r.fire(
        l.find(
            "button",
            ("class", "fui-btn fui-btn-tertiary fui-btn-sm sb-blacklist"),
        ),
        &Event::Click,
    );
    assert!(
        l.r.text()
            .contains("“Ghetto Preacher” by Nate Dogg will be removed"),
        "{}",
        l.r.text()
    );
    l.r.fire(l.find("form", ("class", "sb-dialog-form")), &Event::Submit);
    assert_eq!(
        l.take(),
        vec![Request::Blacklist {
            artist: "Nate Dogg".into(),
            album_id: 6_047_731
        }]
    );
    l.send(Msg::Blacklisted(11));
    assert!(!l.has("dialog-blacklist"));
    assert!(l.text_of("snack").contains("Blacklisted Ghetto Preacher"));
}

#[quench_test::test]
fn an_artist_is_followed_from_the_dialog() {
    let l = started("/songbook/ui/library");
    l.click("button", ("id", "add-artist"));
    l.type_into("artist-name", " Gibbs ");
    l.type_into("artist-id", "4242");
    l.r.fire(
        l.find("select", ("id", "artist-category")),
        &Event::Input("Polish Rap".into()),
    );
    l.r.fire(l.find("form", ("class", "sb-dialog-form")), &Event::Submit);
    assert_eq!(
        l.take(),
        vec![Request::AddArtists(vec![NewArtist {
            name: "Gibbs".into(),
            id: "4242".into(),
            category: "Polish Rap".into()
        }])]
    );
    l.send(Msg::ArtistsAdded(vec!["Gibbs".into()]));
    assert!(!l.has("dialog-add"));
    assert!(
        l.text_of("snack")
            .contains("Added Gibbs. Run Sync to fetch it.")
    );
    assert_eq!(l.take(), vec![Request::State]);
}

#[quench_test::test]
fn following_nobody_does_nothing_and_an_old_name_says_so() {
    let l = started("/songbook/ui/library");
    l.click("button", ("id", "add-artist"));
    l.r.fire(l.find("form", ("class", "sb-dialog-form")), &Event::Submit);
    assert!(l.take().is_empty());
    l.type_into("artist-name", "Nas");
    l.r.fire(l.find("form", ("class", "sb-dialog-form")), &Event::Submit);
    l.take();
    l.send(Msg::ArtistsAdded(vec![]));
    assert!(l.text_of("snack").contains("Nas is already in the list"));
}

#[quench_test::test]
fn an_artist_is_removed_after_asking() {
    let l = started("/songbook/ui/library");
    l.expand();
    l.click("button", ("id", "all-artists"));
    l.r.fire(
        l.find("button", ("aria-label", "Remove Kartky")),
        &Event::Click,
    );
    assert!(l.r.text().contains("Kartky will stop being followed"));
    l.r.fire(l.find("form", ("class", "sb-dialog-form")), &Event::Submit);
    assert_eq!(
        l.take().last(),
        Some(&Request::RemoveArtist("Kartky".into()))
    );
    l.send(Msg::ArtistRemoved);
    assert!(l.text_of("snack").contains("Removed Kartky"));
    let asked = l.take();
    assert!(asked.contains(&Request::State), "{asked:?}");
}

#[quench_test::test]
fn the_category_of_an_artist_changes_in_a_dialog() {
    let l = started("/songbook/ui/library");
    l.expand();
    l.r.fire(
        l.find("button", ("aria-label", "Change category of Kartky")),
        &Event::Click,
    );
    assert_eq!(
        l.r.attr(l.find("select", ("id", "category-pick")), "name")
            .as_deref(),
        Some("category-pick")
    );
    l.r.fire(
        l.find("select", ("id", "category-pick")),
        &Event::Input("American Rap".into()),
    );
    l.r.fire(l.find("form", ("class", "sb-dialog-form")), &Event::Submit);
    assert_eq!(
        l.take(),
        vec![Request::SetCategory {
            name: "Kartky".into(),
            category: "American Rap".into()
        }]
    );
    l.send(Msg::CategorySet);
    assert!(!l.has("dialog-category"));
    assert!(l.text_of("snack").contains("Category saved"));
}

#[quench_test::test]
fn the_artists_of_a_category_are_listed_alphabetically() {
    let l = started("/songbook/ui/library");
    let mut s = state();
    s.artists = vec![
        artist("zeta", "Polish Rap", 1, true),
        artist("Beta", "Polish Rap", 1, true),
        artist("alpha", "Polish Rap", 1, true),
        artist("Gamma", "Polish Rap", 1, true),
    ];
    l.send(Msg::State(s));
    l.expand();
    let order: Vec<String> =
        l.r.document_order()
            .into_iter()
            .filter_map(|n| l.r.attr(n, "data-artist"))
            .collect();
    assert_eq!(order, ["alpha", "Beta", "Gamma", "zeta"]);
}

#[quench_test::test]
fn a_category_is_renamed_in_a_dialog_and_the_filter_follows() {
    let l = started("/songbook/ui/library");
    l.click("button", ("data-rename", "Polish Rap"));
    l.type_into("category-rename", "Rap PL");
    l.send(Msg::Browse(songbook_core::Browse {
        total: 0,
        linked: 0,
        shown: 0,
        albums: Vec::new(),
    }));
    l.take();
    l.r.fire(l.find("form", ("class", "sb-dialog-form")), &Event::Submit);
    assert_eq!(
        l.take(),
        vec![Request::RenameCategory {
            name: "Polish Rap".into(),
            new_name: "Rap PL".into()
        }]
    );
    l.send(Msg::CategoryRenamed);
    assert!(!l.has("dialog-rename-category"));
    assert!(l.text_of("snack").contains("Rap PL"), "{}", l.r.text());
    // The state is read again; Uncategorized has no rename button.
    assert!(l.take().contains(&Request::State));
    assert!(
        !l.r.document_order()
            .into_iter()
            .any(|n| l.r.attr(n, "data-rename").as_deref() == Some("Uncategorized"))
    );
}

#[quench_test::test]
fn a_new_category_is_created_first_and_then_used() {
    let l = started("/songbook/ui/library");
    l.expand();
    l.r.fire(
        l.find("button", ("aria-label", "Change category of Kartky")),
        &Event::Click,
    );
    l.type_into("category-new", "Latino");
    l.r.fire(l.find("form", ("class", "sb-dialog-form")), &Event::Submit);
    assert_eq!(l.take(), vec![Request::AddCategory("Latino".into())]);
    l.send(Msg::CategoryAdded(vec!["Latino".into()]));
    assert_eq!(
        l.take(),
        vec![Request::SetCategory {
            name: "Kartky".into(),
            category: "Latino".into()
        }]
    );
}

#[quench_test::test]
fn the_service_refusing_is_said_and_keeps_the_dialog() {
    let l = started("/songbook/ui/library");
    l.r.fire(
        l.find("button", ("aria-label", "Add link for Puppy Love")),
        &Event::Click,
    );
    l.send(Msg::Rejected(
        "URL must start with http:// or https://".into(),
    ));
    assert!(l.has("dialog-link"));
    assert!(l.text_of("snack").contains("URL must start"));
}

#[quench_test::test]
fn the_pipeline_starts_jobs_with_the_options_set_here() {
    let l = started(
        "/songbook/pipeline"
            .replace("/songbook/pipeline", "/songbook/ui/pipeline")
            .as_str(),
    );
    assert!(l.has("page-pipeline"));
    assert!(
        l.text_of("scope-note")
            .starts_with("Steps 2 and 3 run for all artists"),
        "{}",
        l.text_of("scope-note")
    );
    l.click("button", ("id", "job-sync"));
    l.click("button", ("id", "singles"));
    l.click("button", ("id", "job-sync"));
    l.click("button", ("id", "retry"));
    l.type_into("limit-links", "50");
    l.click("button", ("id", "job-youtube"));
    l.type_into("limit-download", "0");
    l.click("button", ("id", "job-download"));
    l.click("button", ("id", "job-convert"));
    l.click("button", ("id", "job-tags"));
    let jobs: Vec<_> = l
        .take()
        .into_iter()
        .filter_map(|r| match r {
            Request::StartJob(j) => Some(j),
            _ => None,
        })
        .collect();
    assert_eq!(jobs.len(), 6);
    assert_eq!((jobs[0].name.as_str(), jobs[0].singles), ("sync", false));
    assert_eq!((jobs[1].name.as_str(), jobs[1].singles), ("sync", true));
    assert_eq!(
        (jobs[2].name.as_str(), jobs[2].limit, jobs[2].retry_missing),
        ("youtube", Some(50), true)
    );
    assert_eq!((jobs[3].name.as_str(), jobs[3].limit), ("download", None));
    assert_eq!(jobs[4].name, "convert");
    assert_eq!(jobs[5].name, "tags");
}

#[quench_test::test]
fn the_selected_artist_is_the_scope_of_the_pipeline() {
    let l = started("/songbook/ui/pipeline");
    l.expand();
    l.r.fire(
        l.r.children(l.find("div", ("data-artist", "Kartky")))[0],
        &Event::Click,
    );
    assert!(
        l.text_of("scope-note")
            .starts_with("Steps 2 and 3 run for Kartky only"),
        "{}",
        l.text_of("scope-note")
    );
    l.click("button", ("id", "job-youtube"));
    let Some(Request::StartJob(j)) = l
        .take()
        .into_iter()
        .find(|r| matches!(r, Request::StartJob(_)))
    else {
        panic!("no job")
    };
    assert_eq!(j.artist, "Kartky");
}

#[quench_test::test]
fn running_everything_starts_the_pipeline_with_both_limits_and_retrying_the_misses() {
    let l = started("/songbook/ui/pipeline");
    l.expand();
    l.click("div", ("data-artist", "Kartky")); // not a button: nothing happens
    l.take();
    l.click("button", ("id", "singles"));
    l.type_into("limit-links", "40");
    l.type_into("limit-download", "25");
    l.click("button", ("id", "job-pipeline"));
    let Some(Request::StartJob(j)) = l
        .take()
        .into_iter()
        .find(|r| matches!(r, Request::StartJob(_)))
    else {
        panic!("no job")
    };
    assert_eq!(j.name, "pipeline");
    assert!(
        j.singles && j.retry_missing,
        "retry_missing is not up to the toggle"
    );
    assert_eq!((j.limit, j.limit_download), (Some(40), Some(25)));
    // off while a job runs, like the other buttons
    l.send(Msg::Job(Job {
        name: Some("pipeline".into()),
        running: true,
        error: None,
        log: String::new(),
        offset: 0,
    }));
    assert!(
        l.r.attr(l.find("button", ("id", "job-pipeline")), "disabled")
            .is_some()
    );
}

#[quench_test::test]
fn the_buttons_are_off_while_a_job_runs_and_a_second_start_is_said() {
    let l = started("/songbook/ui/pipeline");
    assert!(
        l.r.attr(l.find("button", ("id", "job-sync")), "disabled")
            .is_none()
    );
    l.send(Msg::Job(Job {
        name: Some("sync".into()),
        running: true,
        error: None,
        log: "fetching\n".into(),
        offset: 9,
    }));
    assert!(
        l.r.attr(l.find("button", ("id", "job-sync")), "disabled")
            .is_some()
    );
    assert!(l.text_of("job-pill").contains("Running sync"));
    l.send(Msg::Started(false));
    assert!(l.text_of("snack").contains("A job is already running"));
}

#[quench_test::test]
fn the_job_is_polled_from_where_the_log_ends_and_faster_while_it_runs() {
    let l = live("/songbook/ui/activity");
    assert!(l.take().contains(&Request::Job { from: 0 }));
    tick(5_100.0);
    assert_eq!(l.take(), vec![Request::Job { from: 0 }]);
    l.send(Msg::Job(Job {
        name: Some("sync".into()),
        running: true,
        error: None,
        log: "abc".into(),
        offset: 3,
    }));
    tick(5_100.0);
    assert_eq!(l.take(), vec![Request::Job { from: 3 }]);
    // Running now: the next ask comes after 1.5 s, not 5.
    tick(3_100.0);
    let asked = l.take();
    assert!(
        asked.len() >= 2 && asked.iter().all(|r| *r == Request::Job { from: 3 }),
        "{asked:?}"
    );
}

#[quench_test::test]
fn the_log_is_one_element_that_keeps_its_place_while_it_grows() {
    let l = started("/songbook/ui/activity");
    let job = |log: &str, offset: u64| {
        Msg::Job(Job {
            name: Some("sync".into()),
            running: true,
            error: None,
            log: log.into(),
            offset,
        })
    };
    l.send(job("one\n", 4));
    let before = l.find("pre", ("id", "log"));
    l.send(job("two\n", 8));
    l.send(job("", 8));
    l.send(job("three\n", 14));
    // A new element for every poll would start at the top each time: it has to stay the same one, and follow its end.
    assert_eq!(l.find("pre", ("id", "log")), before);
    assert_eq!(l.text_of("log"), "one\ntwo\nthree\n");
    assert!(l.r.attr(before, "data-follow").is_some());
}

#[quench_test::test]
fn the_activity_screen_gathers_the_log_and_starts_over_for_a_new_job() {
    let l = started("/songbook/ui/activity");
    assert_eq!(l.text_of("status"), "Idle");
    assert_eq!(l.text_of("log"), "No job has run yet.");
    l.send(Msg::Job(Job {
        name: Some("sync".into()),
        running: true,
        error: None,
        log: "one\n".into(),
        offset: 4,
    }));
    l.send(Msg::Job(Job {
        name: Some("sync".into()),
        running: true,
        error: None,
        log: "two\n".into(),
        offset: 8,
    }));
    assert_eq!(l.text_of("status"), "Running sync…");
    assert_eq!(l.text_of("log"), "one\ntwo\n");
    l.send(Msg::Job(Job {
        name: Some("youtube".into()),
        running: true,
        error: None,
        log: "fresh\n".into(),
        offset: 6,
    }));
    assert_eq!(l.text_of("log"), "fresh\n");
}

#[quench_test::test]
fn a_finished_job_refreshes_the_library_and_says_so() {
    let l = started("/songbook/ui/activity");
    l.send(Msg::Job(Job {
        name: Some("download".into()),
        running: true,
        error: None,
        log: String::new(),
        offset: 0,
    }));
    l.take();
    l.send(Msg::Job(Job {
        name: Some("download".into()),
        running: false,
        error: None,
        log: "done\n".into(),
        offset: 5,
    }));
    assert_eq!(l.text_of("status"), "Finished download");
    assert!(l.text_of("snack").contains("Finished download"));
    let asked = l.take();
    assert!(
        asked.contains(&Request::State) && asked.contains(&Request::Browse(Filter::default())),
        "{asked:?}"
    );

    l.send(Msg::Job(Job {
        name: Some("convert".into()),
        running: true,
        error: None,
        log: String::new(),
        offset: 0,
    }));
    l.send(Msg::Job(Job {
        name: Some("convert".into()),
        running: false,
        error: Some("ffmpeg missing".into()),
        log: "x".into(),
        offset: 1,
    }));
    assert_eq!(l.text_of("status"), "Error: ffmpeg missing");
    assert!(
        l.text_of("log").ends_with("ERROR: ffmpeg missing"),
        "{}",
        l.text_of("log")
    );
    assert!(
        l.text_of("snack")
            .contains("convert failed: ffmpeg missing")
    );
    assert!(l.text_of("job-pill").contains("convert failed"));
}

#[quench_test::test]
fn starting_a_job_clears_the_log_and_asks_at_once() {
    let l = started("/songbook/ui/activity");
    l.send(Msg::Job(Job {
        name: Some("sync".into()),
        running: false,
        error: None,
        log: "old\n".into(),
        offset: 4,
    }));
    l.take();
    l.send(Msg::Started(true));
    assert_eq!(l.text_of("log"), "");
    assert_eq!(l.take(), vec![Request::Job { from: 0 }]);
}

#[quench_test::test]
fn suggestions_are_ticked_and_followed_with_a_sync() {
    let l = started("/songbook/ui/discover");
    let text = l.r.text();
    assert!(
        text.contains("Warren G") && text.contains("3× via Nate Dogg, Snoop Dogg, Nas"),
        "{text}"
    );
    assert!(text.contains("Current list: American Rap."), "{text}");
    assert!(!l.has("add-suggested"));
    l.r.fire(
        l.find("button", ("data-suggestion", "Warren G")),
        &Event::Click,
    );
    l.r.fire(l.find("button", ("data-suggestion", "Daz")), &Event::Click);
    assert!(l.text_of("add-suggested").contains("Add 2 artists & sync"));
    l.r.fire(l.find("button", ("data-suggestion", "Daz")), &Event::Click);
    assert!(l.text_of("add-suggested").contains("Add 1 artist & sync"));
    l.click("button", ("id", "add-suggested"));
    assert_eq!(
        l.take(),
        vec![Request::AddArtists(vec![NewArtist {
            name: "Warren G".into(),
            id: "712".into(),
            category: String::new()
        }])]
    );
    l.send(Msg::ArtistsAdded(vec!["Warren G".into()]));
    assert!(!l.has("add-suggested"));
    let asked = l.take();
    assert!(asked.contains(&Request::State), "{asked:?}");
    assert!(
        asked
            .iter()
            .any(|r| matches!(r, Request::StartJob(j) if j.name == "sync")),
        "{asked:?}"
    );
}

#[quench_test::test]
fn similar_artists_are_found_for_a_scope_and_a_top() {
    let l = started("/songbook/ui/discover");
    l.r.fire(
        l.find("select", ("id", "scope")),
        &Event::Input("Polish Rap".into()),
    );
    l.type_into("top", "25");
    l.click("button", ("id", "job-similar"));
    let Some(Request::StartJob(j)) = l
        .take()
        .into_iter()
        .find(|r| matches!(r, Request::StartJob(_)))
    else {
        panic!("no job")
    };
    assert_eq!(
        (j.name.as_str(), j.category.as_str(), j.top),
        ("similar", "Polish Rap", Some(25))
    );
}

#[quench_test::test]
fn no_suggestions_says_so() {
    let l = live("/songbook/ui/discover");
    l.send(Msg::State(State {
        suggestions: vec![],
        ..state()
    }));
    assert!(l.has("no-suggestions"));
}

#[quench_test::test]
fn the_home_screen_is_the_library_and_an_unknown_one_is_said() {
    assert!(live("/songbook/ui/home").has("page-library"));
    assert!(live("/songbook/ui").has("page-library"));
    assert!(live("/songbook/ui/nowhere").has("not-found"));
}

#[quench_test::test]
fn the_navigation_moves_between_screens_without_a_page_load() {
    let l = started("/songbook/ui/library");
    l.click("a", ("id", "nav-discover"));
    assert!(l.has("page-discover"));
    assert!(!l.has("page-library"));
    assert_eq!(l.history.location().path, "/songbook/ui/discover");
    l.click("a", ("id", "nav-library"));
    assert!(l.has("page-library"));
}

#[quench_test::test]
fn problems_show_as_a_banner_and_go_with_the_next_answer() {
    let l = started("/songbook/ui/library");
    l.send(Msg::Failed("cannot reach the service: offline".into()));
    assert!(l.text_of("problem").contains("offline"));
    l.send(Msg::Browse(browse()));
    assert!(!l.has("problem"));
    l.send(Msg::SignedOut);
    assert_eq!(
        l.r.attr(l.find("a", ("id", "sign-in")), "href").as_deref(),
        Some("/songbook/ui/login")
    );
}

#[quench_test::test]
fn picking_a_language_tells_the_host_and_translates_the_screens() {
    let l = started("/songbook/ui/library");
    l.expand();
    l.r.fire(
        l.find("select", ("id", "language")),
        &Event::Input("pl-PL".into()),
    );
    assert!(l.take().contains(&Request::Language("pl-PL".into())));
    let text = l.r.text();
    assert!(
        text.contains("Biblioteka") && text.contains("Obserwowani artyści"),
        "{text}"
    );
    assert!(text.contains("106 utworów"), "{text}");
}

quench_test::main!();
