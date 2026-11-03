//! The screens, as functions from the model to a view.

use std::fmt::Write as _;
use std::rc::Rc;

use quench_ui::prelude::*;
use quench_ui::router::{RouteContext, Router, link};
use songbook_core::{AlbumInfo, AlbumQuery, ArtistInfo, Mix, SongInfo, SongQuery};

use crate::engine::{Command, Repeat};
use crate::live::{Dialog, Env, Intent, Problem, Request};

fn aside() -> Element {
    element("aside")
}

fn small() -> Element {
    element("small")
}

// ── Small things ─────────────────────────────────────────────────────────────

/// `text` as one path segment.
pub(crate) fn enc(text: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// `3:07`.
#[must_use]
pub fn clock(seconds: f64) -> String {
    let total = if seconds.is_finite() && seconds > 0.0 {
        seconds as u64
    } else {
        0
    };
    format!("{}:{:02}", total / 60, total % 60)
}

/// `12.4 MB`.
fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_048_576.0)
}

/// A colour for a name, the same each time: the cover of something that has no picture.
/// The plain colour of a tint: where gradients are not drawn (the phone's native screens) this is what shows.
fn tint_solid(name: &str) -> String {
    let hash = name
        .bytes()
        .fold(7_u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
    format!("hsl({}, 48%, 28%)", hash % 360)
}

fn tint(name: &str) -> String {
    let hash = name
        .bytes()
        .fold(7_u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
    format!(
        "linear-gradient(135deg, hsl({} 45% 34%), hsl({} 50% 20%))",
        hash % 360,
        (hash / 7) % 360
    )
}

fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase()
}

fn cover(name: &str, big: bool) -> Element {
    cover_with(name, &initials(name), big)
}

/// A cover with a picture over the tile: when the service has no picture (it answers 404) the tile shows through.
fn cover_art(name: &str, big: bool, url: String) -> Element {
    cover(name, big).child(
        img()
            .attr("src", url)
            .attr("alt", "")
            .attr("loading", "lazy")
            .attr("decoding", "async")
            .class("pl-cover-img"),
    )
}

/// A cover tinted by `name` that says `label` (its initials unless something else says it better).
fn cover_with(name: &str, label: &str, big: bool) -> Element {
    div()
        .class(if big {
            "pl-cover pl-cover-big"
        } else {
            "pl-cover"
        })
        .css("background-color", {
            let name = name.to_string();
            move || tint_solid(&name)
        })
        .css("background-image", {
            let name = name.to_string();
            move || tint(&name)
        })
        .child(span().text(label.to_string()))
}

/// The cover of a mix: its tint, with an icon for what kind of list it is.
fn mix_cover(name: &str, icon: &str) -> Element {
    div()
        .class("pl-cover pl-cover-icon")
        .css("background-color", {
            let name = name.to_string();
            move || tint_solid(&name)
        })
        .css("background-image", {
            let name = name.to_string();
            move || tint(&name)
        })
        .child(ico(icon))
}

fn button_(label: &str) -> Element {
    button().attr("type", "button").attr("aria-label", label)
}

/// An icon, drawn by the stylesheet (`.pl-ico-{name}`), so that it is the same in every font.
fn ico(name: &str) -> Element {
    span()
        .class("pl-ico")
        .class(format!("pl-ico-{name}"))
        .attr("aria-hidden", "true")
}

/// A button with an icon and a word.
fn labelled(icon: &str, word: &str) -> View {
    fragment([
        ico(icon).into_view(),
        span().text(word.to_string()).into_view(),
    ])
}

fn glyph_button(label: &str, icon: &str, id: &str, on: impl Fn() + 'static) -> Element {
    button_(label)
        .attr("title", label)
        .attr("id", id)
        .class("pl-icon")
        .child(ico(icon))
        .on_click(on)
}

fn artists_line(song: &SongInfo) -> String {
    song.artists.join(", ")
}

// ── Frame ────────────────────────────────────────────────────────────────────

const NAV: [(&str, &str, &str, &str); 4] = [
    ("", "Home", "nav-home", "home"),
    ("/search", "Search", "nav-search", "search"),
    ("/playlists", "Playlists", "nav-playlists", "list"),
    ("/saved", "On this device", "nav-saved", "download"),
];

fn nav_items(env: &Env, router: &Router, short: bool) -> View {
    let (env, router) = (env.clone(), router.clone());
    View::dynamic(move || {
        let here = router.location().path;
        let mut nav = div().class("pl-nav");
        for (path, title, id, glyph) in NAV {
            let href = env.path(path);
            let current = here == href || (path.is_empty() && here == format!("{href}/"));
            let mut item = link(&href)
                .attr(
                    "id",
                    if short {
                        format!("tab-{id}")
                    } else {
                        id.into()
                    },
                )
                .class("pl-nav-item")
                .child(ico(glyph))
                .child(span().text(if short && path == "/saved" {
                    "Saved"
                } else {
                    title
                }));
            if current {
                item = item.attr("aria-current", "page");
            }
            nav = nav.child(item);
        }
        if short {
            // On a phone the settings are a tab too (a computer has them beside the queue).
            let env = env.clone();
            nav = nav.child(
                button_("Settings")
                    .attr("id", "tab-nav-settings")
                    .class("pl-nav-item")
                    .child(ico("settings"))
                    .child(span().text("Settings"))
                    .on_click(move || env.open(Dialog::Settings)),
            );
        }
        nav.into_view()
    })
}

/// The sidebar (a computer's): where to go, the playlists, and the way out.
pub(crate) fn side(env: &Env, router: &Router) -> View {
    let lists = {
        let env = env.clone();
        View::dynamic(move || {
            let mut list = div().class("pl-side-lists");
            for p in env.m.playlists.get() {
                list = list.child(
                    link(&env.path(&format!("/playlist/{}", p.id)))
                        .class("pl-side-list")
                        .text(p.name.clone()),
                );
            }
            list.into_view()
        })
    };
    let mut foot = div().class("pl-side-foot");
    if !env.mobile {
        // Only for someone who may use the admin pages (the service says so in the overview).
        let env = env.clone();
        foot = foot.child(View::dynamic(move || {
            if env.m.overview.get().is_some_and(|o| o.admin) {
                a().attr("href", format!("{}/ui/admin", env.base))
                    .attr("id", "admin-link")
                    .class("pl-pill")
                    .text("Admin")
                    .into_view()
            } else {
                View::Empty
            }
        }));
    }
    foot = foot.child(logout(env, "logout"));
    aside()
        .class("pl-side")
        .child(div().class("pl-brand").text("Songbook"))
        .child(nav_items(env, router, false))
        .child(div().class("pl-side-head").text("Your playlists"))
        .child(lists)
        .child(foot)
        .into_view()
}

/// The tabs along the bottom of a phone.
pub(crate) fn tabs(env: &Env, router: &Router) -> View {
    nav()
        .class("pl-tabs")
        .child(nav_items(env, router, true))
        .into_view()
}

/// Why the library is not on screen, when it is not.
pub(crate) fn banner(env: &Env) -> View {
    let env = env.clone();
    View::dynamic(move || match env.m.problem.get() {
        None => View::Empty,
        Some(Problem::SignedOut) => div()
            .attr("id", "banner")
            .class("pl-banner")
            .child(span().text("Your session ended. "))
            .child(
                a().attr("href", env.login_url.clone())
                    .text("Sign in again"),
            )
            .into_view(),
        Some(Problem::Failed(why)) => div()
            .attr("id", "banner")
            .class("pl-banner")
            .text(format!(
                "The server cannot be reached ({why}). The songs kept on this device still play."
            ))
            .into_view(),
    })
}

pub(crate) fn toast(env: &Env) -> View {
    let (when, text) = (env.m.toast.clone(), env.m.toast.clone());
    View::show(
        move || !when.get().is_empty(),
        move || {
            let text = text.clone();
            div()
                .attr("id", "toast")
                .attr("role", "status")
                .class("pl-toast")
                .text_fn(move || text.get())
                .into_view()
        },
    )
}

// ── The bar that plays ──────────────────────────────────────────────────────

fn controls(env: &Env, big: bool) -> View {
    let state = env.m.engine.clone();
    let mut row = div().class(if big {
        "pl-controls pl-controls-big"
    } else {
        "pl-controls"
    });
    row = row.child({
        let (env, s) = (env.clone(), state.clone());
        glyph_button("Shuffle", "shuffle", "shuffle", move || {
            env.engine(&Command::Shuffle(!s.get_untracked().shuffle));
        })
        .class_if("pl-on", {
            let s = state.clone();
            move || s.get().shuffle
        })
    });
    row = row.child({
        let env = env.clone();
        glyph_button("Previous", "prev", "prev", move || {
            env.engine(&Command::Prev);
        })
    });
    row = row.child({
        let (env, s) = (env.clone(), state.clone());
        let label = state.clone();
        button_("Play or pause")
            .attr("id", "play")
            .class("pl-play")
            .attr_fn("data-state", {
                let label = label.clone();
                move || if label.get().playing { "pause" } else { "play" }.to_string()
            })
            .child(ico("play").class_if("pl-ico-pause", move || label.get().playing))
            .on_click(move || {
                if s.get_untracked().id == 0 && s.get_untracked().len == 0 {
                    env.say("Pick something to play");
                } else {
                    env.engine(&Command::Toggle);
                }
            })
    });
    row = row.child({
        let env = env.clone();
        glyph_button("Next", "next", "next", move || env.engine(&Command::Next))
    });
    row = row.child({
        let (env, s) = (env.clone(), state.clone());
        let label = state.clone();
        button_("Repeat")
            .attr("id", "repeat")
            .class("pl-icon")
            .child(ico("repeat"))
            .child(span().class("pl-badge").text_fn({
                let label = label;
                move || {
                    if label.get().repeat == Repeat::One {
                        "1"
                    } else {
                        ""
                    }
                    .to_string()
                }
            }))
            .class_if("pl-on", {
                let s = state;
                move || s.get().repeat != Repeat::Off
            })
            .on_click(move || env.engine(&Command::Repeat(s.get_untracked().repeat.next())))
    });
    row.into_view()
}

fn progress(env: &Env) -> View {
    let s = env.m.engine.clone();
    let (pos, dur, max, value, label) = (s.clone(), s.clone(), s.clone(), s.clone(), s);
    let env = env.clone();
    div()
        .class("pl-progress")
        .child(
            span()
                .class("pl-time")
                .attr("id", "pos")
                .text_fn(move || clock(pos.get().pos)),
        )
        .child(
            input()
                .attr("type", "range")
                .attr("id", "seek")
                .attr("min", "0")
                .attr("step", "1")
                .attr("aria-label", "Position")
                .attr_fn("max", move || format!("{}", max.get().dur.max(1.0) as u64))
                .attr_fn("value", move || format!("{}", value.get().pos as u64))
                .on_change(move |v| {
                    if let Ok(to) = v.parse::<f64>() {
                        env.engine(&Command::Seek(to));
                    }
                }),
        )
        .child(
            span()
                .class("pl-time")
                .attr("id", "dur")
                .text_fn(move || clock(dur.get().dur)),
        )
        .attr_fn("data-pos", move || format!("{}", label.get().pos as u64))
        .into_view()
}

/// The cover of the song playing (its album's, or the single's, or its artist's picture), over a tile.
fn now_cover(env: &Env, big: bool) -> View {
    let env = env.clone();
    View::dynamic(move || {
        let id = env.m.engine.get().id;
        let Some(song) = env.song(id) else {
            return View::Empty;
        };
        // The bar's is small, the full screen's is the saved picture as it is.
        let url = env.art("song", &id.to_string(), if big { 0 } else { 160 });
        let cover = cover_art(&song.album, big, url);
        if big {
            cover.into_view()
        } else {
            cover.class("pl-bar-cover").into_view()
        }
    })
}

/// The song playing: its title and artists.
fn now_text(env: &Env) -> View {
    let env = env.clone();
    View::dynamic(move || {
        let s = env.m.engine.get();
        let song = env.song(s.id);
        let (title, who) = match (&song, s.id) {
            (Some(song), _) => (song.title.clone(), artists_line(song)),
            (None, 0) => (String::new(), String::new()),
            (None, _) => ("…".to_string(), String::new()),
        };
        div()
            .class("pl-now-text")
            .child(
                div()
                    .class("pl-now-title")
                    .attr("id", "now-title")
                    .text(title),
            )
            .child(
                div()
                    .class("pl-now-artist")
                    .attr("id", "now-artist")
                    .text(who),
            )
            .into_view()
    })
}

/// The bar along the bottom: the song, the controls and where it is in the song.
pub(crate) fn bar(env: &Env) -> View {
    let shown = env.m.engine.clone();
    let open = env.clone();
    let queue = env.clone();
    let settings = env.clone();
    footer()
        .attr("id", "bar")
        .class("pl-bar")
        .class_if("pl-bar-idle", move || {
            let s = shown.get();
            s.id == 0 && s.len == 0
        })
        .child(
            div()
                .class("pl-bar-song")
                .attr("id", "bar-song")
                .on_click(move || open.open(Dialog::Playing))
                .child(now_cover(env, false))
                .child(now_text(env)),
        )
        .child(
            div()
                .class("pl-bar-center")
                .child(controls(env, false))
                .child(progress(env)),
        )
        .child(
            div()
                .class("pl-bar-right")
                .child(glyph_button("Queue", "queue", "queue", move || {
                    queue.open(Dialog::Queue);
                }))
                .child(glyph_button(
                    "Settings",
                    "settings",
                    "settings",
                    move || {
                        settings.open(Dialog::Settings);
                    },
                )),
        )
        .into_view()
}

// ── Lists ────────────────────────────────────────────────────────────────────

/// Something to do for each song of a list.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Row {
    Plain,
    /// In playlist `id`: with a button that takes the song out.
    InPlaylist(i64),
}

fn song_row(env: &Env, list: &Rc<Vec<i64>>, index: usize, song: &SongInfo, kind: Row) -> View {
    let id = song.id;
    let playing = env.m.engine.clone();
    let kept = env.m.cached.clone();
    let mut row = li()
        .class("pl-song")
        .attr("data-id", id.to_string())
        .class_if("pl-current", move || playing.get().id == id)
        .child(
            button_("Play")
                .class("pl-song-main")
                .child(span().class("pl-song-n").text((index + 1).to_string()))
                .child(
                    div()
                        .class("pl-song-text")
                        .child(div().class("pl-song-title").text(song.title.clone()))
                        .child(div().class("pl-song-artists").text(artists_line(song))),
                )
                .child(
                    span()
                        .class("pl-song-dur")
                        .text(clock(f64::from(song.duration))),
                )
                .on_click({
                    let (env, list) = (env.clone(), Rc::clone(list));
                    move || env.play(list.to_vec(), i64::try_from(index).unwrap_or(0), false)
                }),
        );
    row = row.child({
        let env = env.clone();
        glyph_button("Play next", "queue-add", "", move || {
            env.engine(&Command::Enqueue {
                ids: vec![id],
                next: true,
            });
        })
    });
    row = row.child({
        let env = env.clone();
        glyph_button("Add to a playlist", "plus", "", move || {
            env.open(Dialog::AddTo(vec![id]));
        })
    });
    row = row.child({
        let (env, label) = (env.clone(), kept.clone());
        button_("Keep on this device")
            .class("pl-icon")
            .attr_fn("data-state", {
                let label = label.clone();
                move || {
                    if label.get().binary_search(&id).is_ok() {
                        "kept"
                    } else {
                        "keep"
                    }
                    .to_string()
                }
            })
            .child(ico("download").class_if("pl-ico-check", move || {
                label.get().binary_search(&id).is_ok()
            }))
            .on_click(move || {
                if kept.get_untracked().binary_search(&id).is_ok() {
                    env.engine(&Command::Drop(vec![id]));
                } else {
                    env.engine(&Command::Keep(vec![id]));
                    env.say("Keeping the song on this device");
                }
            })
    });
    if let Row::InPlaylist(playlist) = kind {
        let (env, list) = (env.clone(), Rc::clone(list));
        row = row.child(glyph_button(
            "Take out of the playlist",
            "close",
            "",
            move || {
                let mut rest = list.to_vec();
                if index < rest.len() {
                    rest.remove(index);
                }
                (env.ask)(Request::SetPlaylist(playlist, rest.clone()));
                env.m.songs.update(|(_, l)| {
                    l.songs.retain({
                        let mut seen = 0;
                        move |_| {
                            seen += 1;
                            seen - 1 != index
                        }
                    });
                    l.total = l.total.saturating_sub(1);
                });
            },
        ));
    }
    row.into_view()
}

/// The songs of the page `key`, one row each.
fn song_list(env: &Env, key: &str, kind: Row) -> View {
    let (env, key) = (env.clone(), key.to_string());
    View::dynamic(move || {
        let (shown, list) = env.m.songs.get();
        if shown != key {
            return p().class("pl-hint").text("Loading…").into_view();
        }
        if list.songs.is_empty() {
            return p()
                .class("pl-hint")
                .attr("id", "empty")
                .text("There are no songs here yet.")
                .into_view();
        }
        let ids = Rc::new(list.songs.iter().map(|s| s.id).collect::<Vec<_>>());
        let mut rows = ul().class("pl-songs").attr("id", "songs");
        for (i, song) in list.songs.iter().enumerate() {
            rows = rows.child(song_row(&env, &ids, i, song, kind));
        }
        let mut all = div().child(rows);
        if usize::try_from(list.total).unwrap_or(0) > list.songs.len() {
            all = all.child(p().class("pl-hint").text(format!(
                "Showing {} of {}.",
                list.songs.len(),
                list.total
            )));
        }
        all.into_view()
    })
}

/// The ids of the songs now listed for `key`.
fn listed(env: &Env, key: &str) -> Vec<i64> {
    let (shown, list) = env.m.songs.get_untracked();
    if shown == key {
        list.songs.iter().map(|s| s.id).collect()
    } else {
        Vec::new()
    }
}

/// The top of a page about one thing: its picture, name, and what to do with all its songs.
/// What a page of one thing is about: its kind, its name and the picture to show (an address under the service).
type About<'a> = (&'a str, &'a str, Option<String>);

fn hero(
    env: &Env,
    about: About<'_>,
    sub: String,
    query: SongQuery,
    key: &str,
    extra: Vec<View>,
) -> View {
    let (kind, title, art) = about;
    let (play, shuffle, add, keep) = (env.clone(), env.clone(), env.clone(), env.clone());
    let (q1, q2, key1, key2) = (query.clone(), query, key.to_string(), key.to_string());
    let mut actions = div()
        .class("pl-hero-actions")
        .child(
            button_("Play")
                .attr("id", "play-all")
                .class("pl-primary")
                .child(labelled("play", "Play"))
                .on_click(move || play.play_query(q1.clone(), false)),
        )
        .child(
            button_("Shuffle")
                .attr("id", "shuffle-all")
                .class("pl-secondary")
                .child(labelled("shuffle", "Shuffle"))
                .on_click(move || shuffle.play_query(q2.clone(), true)),
        )
        .child(
            button_("Add to a playlist")
                .attr("id", "add-all")
                .class("pl-secondary")
                .child(labelled("plus", "Playlist"))
                .on_click(move || add.open(Dialog::AddTo(listed(&add, &key1)))),
        )
        .child(
            button_("Keep on this device")
                .attr("id", "keep-all")
                .class("pl-secondary")
                .child(labelled("download", "Keep"))
                .on_click(move || {
                    let ids = listed(&keep, &key2);
                    if ids.is_empty() {
                        keep.say("Nothing to keep yet");
                    } else {
                        keep.say(format!("Keeping {} songs on this device", ids.len()));
                        keep.engine(&Command::Keep(ids));
                    }
                }),
        );
    for view in extra {
        actions = actions.child(view);
    }
    div()
        .class("pl-hero")
        .child(art.map_or_else(|| cover(title, true), |url| cover_art(title, true, url)))
        .child(
            div()
                .class("pl-hero-text")
                .child(small().class("pl-kind").text(kind.to_string()))
                .child(h1().attr("id", "title").text(title.to_string()))
                .child(p().class("pl-sub").text(sub))
                .child(actions),
        )
        .into_view()
}

// ── Cards ────────────────────────────────────────────────────────────────────

fn card(
    env: &Env,
    href: &str,
    title: &str,
    sub: String,
    id: String,
    play: Option<(SongQuery, Intent)>,
) -> View {
    card_with(env, href, title, sub, id, play, None)
}

/// A card with a picture (an address under the service) over its cover.
fn card_with(
    env: &Env,
    href: &str,
    title: &str,
    sub: String,
    id: String,
    play: Option<(SongQuery, Intent)>,
    art: Option<String>,
) -> View {
    let mut card = div().class("pl-card").attr("id", id).child(
        link(href)
            .class("pl-card-main")
            .child(art.map_or_else(|| cover(title, false), |url| cover_art(title, false, url)))
            .child(div().class("pl-card-title").text(title.to_string()))
            .child(div().class("pl-card-sub").text(sub)),
    );
    if let Some((query, intent)) = play {
        let env = env.clone();
        card = card.child(
            button_("Play")
                .class("pl-card-play")
                .child(ico("play"))
                .on_click(move || (env.ask)(Request::Ids(query.clone(), intent.clone()))),
        );
    }
    card.into_view()
}

fn album_card(env: &Env, a: &AlbumInfo) -> View {
    card_with(
        env,
        &env.path(&format!("/album/{}", a.id)),
        &a.title,
        format!("{} · {}", a.year.get(..4).unwrap_or(&a.year), a.artist),
        format!("album-{}", a.id),
        Some((
            SongQuery {
                album: a.id,
                ..SongQuery::default()
            },
            Intent::Play { shuffle: false },
        )),
        Some(env.art("album", &a.id.to_string(), 320)),
    )
}

fn artist_card(env: &Env, a: &ArtistInfo) -> View {
    card_with(
        env,
        &env.path(&format!("/artist/{}", enc(&a.name))),
        &a.name,
        format!("{} songs", a.songs),
        format!("artist-{}", enc(&a.name)),
        Some((
            SongQuery {
                artist: a.name.clone(),
                ..SongQuery::default()
            },
            Intent::Play { shuffle: false },
        )),
        Some(env.art("artist", &a.name, 320)),
    )
}

fn albums_grid(env: &Env, key: &str) -> View {
    let (env, key) = (env.clone(), key.to_string());
    View::dynamic(move || {
        let (shown, albums) = env.m.albums.get();
        if shown != key || albums.is_empty() {
            return View::Empty;
        }
        let mut grid = div().class("pl-grid").attr("id", "albums");
        for album in &albums {
            grid = grid.child(album_card(&env, album));
        }
        section().child(h2().text("Albums")).child(grid).into_view()
    })
}

// ── Pages ────────────────────────────────────────────────────────────────────

/// A titled shelf of the home page, with a link on its right if there is somewhere more to go.
fn shelf(title: &str, id: &str, more: Option<(&str, String)>, body: impl IntoView) -> View {
    let mut head = div()
        .class("pl-shelf-head")
        .child(h2().text(title.to_string()));
    if let Some((word, href)) = more {
        head = head.child(link(&href).class("pl-more").text(word.to_string()));
    }
    section()
        .attr("id", id.to_string())
        .child(head)
        .child(body)
        .into_view()
}

/// The songs as rows; a tap plays the list from there.
fn rows(env: &Env, songs: &[SongInfo], id: &str) -> View {
    let ids = Rc::new(songs.iter().map(|s| s.id).collect::<Vec<_>>());
    let mut list = ul().class("pl-songs").attr("id", id.to_string());
    for (i, song) in songs.iter().enumerate() {
        list = list.child(song_row(env, &ids, i, song, Row::Plain));
    }
    list.into_view()
}

/// A card for something made from the library (a mix): tinted by its name, plays it when tapped.
fn mix_card(env: &Env, mix: &Mix) -> View {
    let (play, query) = (env.clone(), mix.query());
    let intent = Intent::Play {
        shuffle: mix.shuffled(),
    };
    let (icon, sub) = if mix.kind == "mix" {
        ("shuffle", format!("Shuffled · {} songs", mix.songs))
    } else {
        ("star", format!("Top {}", mix.songs))
    };
    div()
        .class("pl-card")
        .child(
            button_(&format!("Play {}", mix.name))
                .attr("id", format!("mix-{}", enc(&mix.name)))
                .class("pl-card-main pl-card-button")
                .on_click(move || (play.ask)(Request::Ids(query.clone(), intent.clone())))
                .child(mix_cover(&mix.name, icon))
                .child(div().class("pl-card-title").text(mix.name.clone()))
                .child(div().class("pl-card-sub").text(sub)),
        )
        .child(span().class("pl-card-play").child(ico("play")))
        .into_view()
}

/// What was played last: a row of cards, a tap plays the list from there.
fn recent_shelf(env: &Env) -> View {
    let env = env.clone();
    View::dynamic(move || {
        let ids = env.m.recent.get();
        let songs: Vec<SongInfo> = ids.iter().filter_map(|id| env.song(*id)).take(10).collect();
        if songs.is_empty() {
            return View::Empty;
        }
        let queue = Rc::new(songs.iter().map(|s| s.id).collect::<Vec<_>>());
        let mut grid = div().class("pl-hrow").attr("id", "recent");
        for (i, song) in songs.iter().enumerate() {
            let art = env.art("song", &song.id.to_string(), 320);
            let (env, queue) = (env.clone(), Rc::clone(&queue));
            grid = grid.child(
                div().class("pl-card").child(
                    button_(&format!("Play {}", song.title))
                        .class("pl-card-main pl-card-button")
                        .on_click(move || {
                            env.play(queue.to_vec(), i64::try_from(i).unwrap_or(0), false);
                        })
                        .child(cover_art(&song.album, false, art))
                        .child(div().class("pl-card-title").text(song.title.clone()))
                        .child(div().class("pl-card-sub").text(artists_line(song))),
                ),
            );
        }
        shelf("Recently played", "shelf-recent", None, grid)
    })
}

pub(crate) fn home(env: &Env, _: &RouteContext) -> View {
    (env.ask)(Request::Home);
    let (everything, shuffled) = (env.clone(), env.clone());
    let picks = {
        let env = env.clone();
        View::dynamic(move || {
            let Some(home) = env.m.home.get() else {
                return p().class("pl-hint").text("Loading…").into_view();
            };
            if home.popular.is_empty() {
                return p()
                    .class("pl-hint")
                    .attr("id", "empty")
                    .text("The library has no songs with a file yet.")
                    .into_view();
            }
            let mut mixes = div().class("pl-hrow").attr("id", "mixes");
            for mix in &home.mixes {
                mixes = mixes.child(mix_card(&env, mix));
            }
            let mut artists = div().class("pl-hrow").attr("id", "artists");
            for a in &home.artists {
                artists = artists.child(artist_card(&env, a));
            }
            let mut albums = div().class("pl-hrow").attr("id", "albums");
            for a in &home.albums {
                albums = albums.child(album_card(&env, a));
            }
            let mut out = vec![shelf("Made for you", "shelf-mixes", None, mixes)];
            out.push(shelf(
                "Popular in your library",
                "shelf-popular",
                None,
                rows(&env, &home.popular, "popular"),
            ));
            out.push(shelf(
                "Artists to explore",
                "shelf-artists",
                Some(("All artists", env.path("/artists"))),
                artists,
            ));
            if !home.albums.is_empty() {
                out.push(shelf("New in your library", "shelf-albums", None, albums));
            }
            fragment(out)
        })
    };
    let categories = {
        let env = env.clone();
        View::dynamic(move || {
            let Some(o) = env.m.overview.get() else {
                return View::Empty;
            };
            if o.categories.is_empty() {
                return View::Empty;
            }
            let mut chips = div().class("pl-chips").attr("id", "categories");
            for c in &o.categories {
                chips = chips.child(
                    link(&env.path(&format!("/category/{}", enc(&c.name))))
                        .attr("id", format!("category-{}", enc(&c.name)))
                        .class("pl-chip")
                        .text(format!("{} · {}", c.name, c.songs)),
                );
            }
            shelf("Browse by category", "shelf-categories", None, chips)
        })
    };
    let lists = {
        let env = env.clone();
        View::dynamic(move || {
            let lists = env.m.playlists.get();
            if lists.is_empty() {
                return View::Empty;
            }
            let mut grid = div().class("pl-grid").attr("id", "playlist-cards");
            for p in lists {
                grid = grid.child(card(
                    &env,
                    &env.path(&format!("/playlist/{}", p.id)),
                    &p.name,
                    format!("{} songs", p.songs),
                    format!("playlist-{}", p.id),
                    Some((
                        SongQuery {
                            playlist: p.id,
                            ..SongQuery::default()
                        },
                        Intent::Play { shuffle: false },
                    )),
                ));
            }
            shelf(
                "Your playlists",
                "shelf-playlists",
                Some(("All", env.path("/playlists"))),
                grid,
            )
        })
    };
    div()
        .attr("id", "page-home")
        .class("pl-page")
        .child(h1().text("Home"))
        .child(
            div()
                .class("pl-hero-actions")
                .child(
                    button_("Shuffle everything")
                        .attr("id", "shuffle-everything")
                        .class("pl-primary")
                        .child(labelled("shuffle", "Shuffle everything"))
                        .on_click(move || shuffled.play_query(SongQuery::default(), true)),
                )
                .child(
                    button_("Play everything")
                        .attr("id", "play-everything")
                        .class("pl-secondary")
                        .child(labelled("play", "Play everything"))
                        .on_click(move || everything.play_query(SongQuery::default(), false)),
                ),
        )
        .child(recent_shelf(env))
        .child(picks)
        .child(lists)
        .child(categories)
        .into_view()
}

/// Every artist of the library, found by typing a part of the name.
pub(crate) fn artists(env: &Env, _: &RouteContext) -> View {
    let filter = create_signal(String::new());
    let list = {
        let (env, filter) = (env.clone(), filter.clone());
        View::dynamic(move || {
            let Some(o) = env.m.overview.get() else {
                return p().class("pl-hint").text("Loading…").into_view();
            };
            let words: Vec<String> = filter
                .get()
                .split_whitespace()
                .map(str::to_lowercase)
                .collect();
            let found: Vec<&ArtistInfo> = o
                .artists
                .iter()
                .filter(|a| {
                    let name = a.name.to_lowercase();
                    words.iter().all(|w| name.contains(w))
                })
                .collect();
            if found.is_empty() {
                return p()
                    .class("pl-hint")
                    .attr("id", "nothing")
                    .text("No artist like that.")
                    .into_view();
            }
            let mut grid = div().class("pl-grid").attr("id", "artists");
            for a in found.iter().take(300) {
                grid = grid.child(artist_card(&env, a));
            }
            let mut all = div().child(grid);
            if found.len() > 300 {
                all = all.child(p().class("pl-hint").text(format!(
                    "Showing 300 of {}: type to narrow them down.",
                    found.len()
                )));
            }
            all.into_view()
        })
    };
    div()
        .attr("id", "page-artists")
        .class("pl-page")
        .child(h1().text("Artists"))
        .child(
            input()
                .attr("id", "artist-filter")
                .attr("type", "search")
                .attr("placeholder", "Find an artist")
                .attr("autocomplete", "off")
                .class("pl-search")
                .on_input(move |v| filter.set(v.to_string())),
        )
        .child(list)
        .into_view()
}

pub(crate) fn search(env: &Env, _: &RouteContext) -> View {
    let ask = env.clone();
    let results = {
        let env = env.clone();
        View::dynamic(move || {
            let Some((text, found)) = env.m.found.get() else {
                return p()
                    .class("pl-hint")
                    .text("Search for a song, an album or an artist.")
                    .into_view();
            };
            if found.songs.is_empty() && found.albums.is_empty() && found.artists.is_empty() {
                return p()
                    .class("pl-hint")
                    .attr("id", "nothing")
                    .text(format!("Nothing found for “{text}”."))
                    .into_view();
            }
            let ids = Rc::new(found.songs.iter().map(|s| s.id).collect::<Vec<_>>());
            let mut songs = ul().class("pl-songs").attr("id", "songs");
            for (i, song) in found.songs.iter().enumerate() {
                songs = songs.child(song_row(&env, &ids, i, song, Row::Plain));
            }
            let mut out = div();
            if !found.songs.is_empty() {
                out = out.child(section().child(h2().text("Songs")).child(songs));
            }
            if !found.artists.is_empty() {
                let mut grid = div().class("pl-grid").attr("id", "artists");
                for a in &found.artists {
                    grid = grid.child(artist_card(&env, a));
                }
                out = out.child(section().child(h2().text("Artists")).child(grid));
            }
            if !found.albums.is_empty() {
                let mut grid = div().class("pl-grid").attr("id", "albums");
                for a in &found.albums {
                    grid = grid.child(album_card(&env, a));
                }
                out = out.child(section().child(h2().text("Albums")).child(grid));
            }
            out.into_view()
        })
    };
    div()
        .attr("id", "page-search")
        .class("pl-page")
        .child(h1().text("Search"))
        .child(
            input()
                .attr("id", "search-input")
                .attr("type", "search")
                .attr("placeholder", "What do you want to hear?")
                .attr("autocomplete", "off")
                .class("pl-search")
                .focus_if(|| true)
                .on_input(move |text| {
                    let generation = ask.m.search_gen.get().wrapping_add(1);
                    ask.m.search_gen.set(generation);
                    let (env, text) = (ask.clone(), text.trim().to_string());
                    quench_ui::platform::after(250, move || {
                        if env.m.search_gen.get() != generation {
                            return;
                        }
                        if text.is_empty() {
                            env.m.found.set(None);
                        } else {
                            (env.ask)(Request::Search(text));
                        }
                    });
                }),
        )
        .child(results)
        .into_view()
}

fn sub_songs(env: &Env, key: &str) -> String {
    let (shown, list) = env.m.songs.get();
    if shown == key {
        format!("{} songs", list.total)
    } else {
        String::new()
    }
}

pub(crate) fn artist(env: &Env, ctx: &RouteContext) -> View {
    let name = ctx.param("name").unwrap_or_default();
    let key = format!("artist:{name}");
    let query = SongQuery {
        artist: name.clone(),
        ..SongQuery::default()
    };
    (env.ask)(Request::Songs(
        SongQuery {
            limit: 500,
            ..query.clone()
        },
        key.clone(),
    ));
    (env.ask)(Request::Albums(
        AlbumQuery {
            artist: name.clone(),
            ..AlbumQuery::default()
        },
        key.clone(),
    ));
    let head = {
        let (env, name, key) = (env.clone(), name, key.clone());
        View::dynamic(move || {
            hero(
                &env,
                ("Artist", &name, Some(env.art("artist", &name, 320))),
                sub_songs(&env, &key),
                query.clone(),
                &key,
                Vec::new(),
            )
        })
    };
    div()
        .attr("id", "page-artist")
        .class("pl-page")
        .child(head)
        .child(albums_grid(env, &key))
        .child(
            section()
                .child(h2().text("Songs"))
                .child(song_list(env, &key, Row::Plain)),
        )
        .into_view()
}

pub(crate) fn album(env: &Env, ctx: &RouteContext) -> View {
    let id: i64 = ctx.param("id").and_then(|v| v.parse().ok()).unwrap_or(0);
    let key = format!("album:{id}");
    let query = SongQuery {
        album: id,
        ..SongQuery::default()
    };
    (env.ask)(Request::Songs(
        SongQuery {
            limit: 500,
            ..query.clone()
        },
        key.clone(),
    ));
    let head = {
        let (env, key) = (env.clone(), key.clone());
        View::dynamic(move || {
            let (shown, list) = env.m.songs.get();
            let (title, who) = list.songs.first().filter(|_| shown == key).map_or(
                (String::new(), String::new()),
                |s| {
                    (
                        s.album.clone(),
                        s.artists.first().cloned().unwrap_or_default(),
                    )
                },
            );
            let sub = if title.is_empty() {
                String::new()
            } else {
                format!("{who} · {} songs", list.total)
            };
            hero(
                &env,
                (
                    "Album",
                    &title,
                    Some(env.art("album", &id.to_string(), 320)),
                ),
                sub,
                query.clone(),
                &key,
                Vec::new(),
            )
        })
    };
    div()
        .attr("id", "page-album")
        .class("pl-page")
        .child(head)
        .child(song_list(env, &key, Row::Plain))
        .into_view()
}

pub(crate) fn category(env: &Env, ctx: &RouteContext) -> View {
    let name = ctx.param("name").unwrap_or_default();
    let key = format!("category:{name}");
    let query = SongQuery {
        category: name.clone(),
        ..SongQuery::default()
    };
    (env.ask)(Request::Songs(
        SongQuery {
            limit: 500,
            ..query.clone()
        },
        key.clone(),
    ));
    (env.ask)(Request::Albums(
        AlbumQuery {
            category: name.clone(),
            limit: 60,
            ..AlbumQuery::default()
        },
        key.clone(),
    ));
    let head = {
        let (env, name, key) = (env.clone(), name, key.clone());
        View::dynamic(move || {
            hero(
                &env,
                ("Category", &name, None),
                sub_songs(&env, &key),
                query.clone(),
                &key,
                Vec::new(),
            )
        })
    };
    div()
        .attr("id", "page-category")
        .class("pl-page")
        .child(head)
        .child(albums_grid(env, &key))
        .child(
            section()
                .child(h2().text("Songs"))
                .child(song_list(env, &key, Row::Plain)),
        )
        .into_view()
}

pub(crate) fn playlists(env: &Env, _: &RouteContext) -> View {
    (env.ask)(Request::Playlists);
    let name = create_signal(String::new());
    let make = {
        let (env, name) = (env.clone(), name.clone());
        move || {
            let text = name.get_untracked();
            if !text.trim().is_empty() {
                (env.ask)(Request::CreatePlaylist(text.trim().to_string()));
                name.set(String::new());
            }
        }
    };
    let list = {
        let env = env.clone();
        View::dynamic(move || {
            let lists = env.m.playlists.get();
            if lists.is_empty() {
                return p()
                    .class("pl-hint")
                    .attr("id", "empty")
                    .text("You have no playlists yet.")
                    .into_view();
            }
            let mut grid = div().class("pl-grid").attr("id", "playlist-cards");
            for p in lists {
                grid = grid.child(card(
                    &env,
                    &env.path(&format!("/playlist/{}", p.id)),
                    &p.name,
                    format!("{} songs", p.songs),
                    format!("playlist-{}", p.id),
                    Some((
                        SongQuery {
                            playlist: p.id,
                            ..SongQuery::default()
                        },
                        Intent::Play { shuffle: false },
                    )),
                ));
            }
            grid.into_view()
        })
    };
    div()
        .attr("id", "page-playlists")
        .class("pl-page")
        .child(h1().text("Playlists"))
        .child(
            form()
                .class("pl-form")
                .on_submit(make.clone())
                .child(
                    input()
                        .attr("id", "new-playlist")
                        .attr("type", "text")
                        .attr("placeholder", "New playlist")
                        .attr_fn("value", {
                            let n = name.clone();
                            move || n.get()
                        })
                        .on_input({
                            let n = name;
                            move |v| n.set(v.to_string())
                        }),
                )
                .child(
                    button_("Create")
                        .attr("id", "create-playlist")
                        .class("pl-primary")
                        .text("Create")
                        .on_click(make),
                ),
        )
        .child(list)
        .into_view()
}

pub(crate) fn playlist(env: &Env, ctx: &RouteContext) -> View {
    let id: i64 = ctx.param("id").and_then(|v| v.parse().ok()).unwrap_or(0);
    let key = format!("playlist:{id}");
    let query = SongQuery {
        playlist: id,
        ..SongQuery::default()
    };
    (env.ask)(Request::Songs(
        SongQuery {
            limit: 5000,
            ..query.clone()
        },
        key.clone(),
    ));
    let confirming = create_signal(false);
    let rename = create_signal(String::new());
    let head = {
        let (env, key, confirming, rename) = (env.clone(), key.clone(), confirming, rename);
        View::dynamic(move || {
            let info = env.m.playlists.get().into_iter().find(|p| p.id == id);
            let title = info.as_ref().map_or_else(String::new, |p| p.name.clone());
            let sub = sub_songs(&env, &key);
            let delete = {
                let (env, confirming) = (env.clone(), confirming.clone());
                button_("Delete")
                    .attr("id", "delete-playlist")
                    .class("pl-secondary")
                    .text_fn({
                        let c = confirming.clone();
                        move || {
                            if c.get() {
                                "Really delete?".to_string()
                            } else {
                                "Delete".to_string()
                            }
                        }
                    })
                    .on_click(move || {
                        if confirming.get_untracked() {
                            (env.ask)(Request::DeletePlaylist(id));
                            env.m.playlists.update(|l| l.retain(|p| p.id != id));
                            env.m.problem.set(None);
                            env.say("Playlist deleted");
                        } else {
                            confirming.set(true);
                        }
                    })
                    .into_view()
            };
            let renamer = {
                let (env, rename) = (env.clone(), rename.clone());
                form()
                    .class("pl-inline")
                    .on_submit({
                        let (env, rename) = (env.clone(), rename.clone());
                        move || {
                            let text = rename.get_untracked();
                            if !text.trim().is_empty() {
                                (env.ask)(Request::RenamePlaylist(id, text.trim().to_string()));
                                rename.set(String::new());
                            }
                        }
                    })
                    .child(
                        input()
                            .attr("id", "rename-playlist")
                            .attr("type", "text")
                            .attr("placeholder", "Rename")
                            .attr_fn("value", {
                                let r = rename.clone();
                                move || r.get()
                            })
                            .on_input({
                                let r = rename.clone();
                                move |v| r.set(v.to_string())
                            }),
                    )
                    .child(
                        button_("Rename")
                            .attr("id", "rename-save")
                            .class("pl-secondary")
                            .text("Rename")
                            .on_click({
                                let (env, rename) = (env, rename);
                                move || {
                                    let text = rename.get_untracked();
                                    if !text.trim().is_empty() {
                                        (env.ask)(Request::RenamePlaylist(
                                            id,
                                            text.trim().to_string(),
                                        ));
                                        rename.set(String::new());
                                    }
                                }
                            }),
                    )
                    .into_view()
            };
            hero(
                &env,
                ("Playlist", &title, None),
                sub,
                query.clone(),
                &key,
                vec![delete, renamer],
            )
        })
    };
    div()
        .attr("id", "page-playlist")
        .class("pl-page")
        .child(head)
        .child(song_list(env, &key, Row::InPlaylist(id)))
        .into_view()
}

pub(crate) fn saved(env: &Env, _: &RouteContext) -> View {
    let ids = env.m.cached.get_untracked();
    if ids.is_empty() {
        env.m
            .songs
            .set(("saved".into(), songbook_core::SongList::default()));
    } else {
        (env.ask)(Request::Songs(
            SongQuery {
                ids: ids
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
                limit: 2000,
                ..SongQuery::default()
            },
            "saved".into(),
        ));
    }
    let usage = {
        let env = env.clone();
        View::dynamic(move || {
            let (used, quota) = env.m.storage.get();
            let (done, total) = env.m.saving.get();
            let mut text = format!("{} songs on this device", env.m.cached.get().len());
            if quota > 0 {
                let _ = write!(
                    text,
                    " · the browser has used {} of {}",
                    megabytes(used),
                    megabytes(quota)
                );
            }
            if total > 0 && done < total {
                let _ = write!(text, " · keeping {done} of {total}…");
            }
            p().class("pl-sub")
                .attr("id", "usage")
                .text(text)
                .into_view()
        })
    };
    let clear = {
        let env = env.clone();
        button_("Remove all")
            .attr("id", "drop-all")
            .class("pl-secondary")
            .text("Remove all from this device")
            .on_click(move || {
                let all = env.m.cached.get_untracked();
                if !all.is_empty() {
                    env.engine(&Command::Drop(all));
                    env.m
                        .songs
                        .set(("saved".into(), songbook_core::SongList::default()));
                }
            })
    };
    div()
        .attr("id", "page-saved")
        .class("pl-page")
        .child(h1().text("On this device"))
        .child(
            p().class("pl-hint")
                .text("These songs play without the network and use no mobile data."),
        )
        .child(usage)
        .child(clear)
        .child(song_list(env, "saved", Row::Plain))
        .into_view()
}

pub(crate) fn not_found(_env: &Env) -> View {
    div()
        .attr("id", "not-found")
        .class("pl-page")
        .child(h1().text("Not found"))
        .child(
            p().class("pl-hint")
                .text("There is nothing at this address."),
        )
        .into_view()
}

// ── Boxes ────────────────────────────────────────────────────────────────────

fn sheet(env: &Env, id: &str, title: &str, body: View) -> View {
    let close = env.clone();
    div()
        .attr("id", id.to_string())
        .class("pl-sheet")
        .child(
            div()
                .class("pl-sheet-head")
                .child(h3().text(title.to_string()))
                .child(glyph_button("Close", "close", "close", move || {
                    close.close();
                })),
        )
        .child(div().class("pl-sheet-body").child(body))
        .into_view()
}

fn add_to(env: &Env, songs: Vec<i64>) -> View {
    let name = create_signal(String::new());
    let env = env.clone();
    let lists = {
        let (env, songs) = (env.clone(), songs.clone());
        View::dynamic(move || {
            let mut list = ul().class("pl-songs").attr("id", "choose-playlist");
            for p in env.m.playlists.get() {
                let (env, songs, name) = (env.clone(), songs.clone(), p.name.clone());
                list = list.child(
                    li().child(
                        button_("Add")
                            .class("pl-pick")
                            .text(format!("{} ({})", p.name, p.songs))
                            .on_click({
                                let id = p.id;
                                move || {
                                    (env.ask)(Request::AddToPlaylist(id, songs.clone()));
                                    env.say(format!("Added to {name}"));
                                    env.close();
                                }
                            }),
                    ),
                );
            }
            list.into_view()
        })
    };
    let make = {
        let (env, name, songs) = (env, name.clone(), songs);
        move || {
            let text = name.get_untracked();
            if !text.trim().is_empty() {
                env.m.pending_add.borrow_mut().clone_from(&songs);
                (env.ask)(Request::CreatePlaylist(text.trim().to_string()));
                name.set(String::new());
            }
        }
    };
    div()
        .child(
            form()
                .class("pl-form")
                .on_submit(make.clone())
                .child(
                    input()
                        .attr("id", "new-playlist-name")
                        .attr("type", "text")
                        .attr("placeholder", "New playlist")
                        .attr_fn("value", {
                            let n = name.clone();
                            move || n.get()
                        })
                        .on_input({
                            let n = name;
                            move |v| n.set(v.to_string())
                        }),
                )
                .child(
                    button_("Create and add")
                        .attr("id", "create-add")
                        .class("pl-primary")
                        .text("Create")
                        .on_click(make),
                ),
        )
        .child(lists)
        .into_view()
}

fn settings(env: &Env) -> View {
    let (fade, label, change) = (env.m.engine.clone(), env.m.engine.clone(), env.clone());
    let (volume, set_volume) = (env.m.engine.clone(), env.clone());
    let storage = env.m.storage.clone();
    let kept = env.m.cached.clone();
    let app = env.link.is_some();
    div()
        .child(
            div()
                .class("pl-setting")
                .child(
                    label_()
                        .attr("for", "fade")
                        .attr("id", "fade-label")
                        .text_fn(move || {
                            let secs = label.get().fade;
                            if secs < 0.5 {
                                "Crossfade: off".to_string()
                            } else {
                                format!("Crossfade: {} s", secs as u64)
                            }
                        }),
                )
                .child(
                    input()
                        .attr("id", "fade")
                        .attr("type", "range")
                        .attr("min", "0")
                        .attr("max", "12")
                        .attr("step", "1")
                        .attr_fn("value", move || format!("{}", fade.get().fade as u64))
                        .on_change(move |v| {
                            if let Ok(secs) = v.parse::<f64>() {
                                change.engine(&Command::Fade(secs));
                            }
                        }),
                )
                .child(
                    p().class("pl-hint").text(
                        "The next song starts while this one ends, and they overlap this long.",
                    ),
                ),
        )
        .child(
            div()
                .class("pl-setting")
                .child(label_().attr("for", "volume").text("Volume"))
                .child(
                    input()
                        .attr("id", "volume")
                        .attr("type", "range")
                        .attr("min", "0")
                        .attr("max", "100")
                        .attr("step", "1")
                        .attr_fn("value", move || {
                            format!("{}", (volume.get().volume * 100.0) as u64)
                        })
                        .on_change(move |v| {
                            if let Ok(percent) = v.parse::<f64>() {
                                set_volume.engine(&Command::Volume(percent / 100.0));
                            }
                        }),
                ),
        )
        .child(cache_setting(env))
        .child(logout(env, "logout-settings"))
        .child(p().class("pl-hint").attr("id", "device").text_fn(move || {
            let (used, quota) = storage.get();
            if app {
                format!("{} songs kept on this device.", kept.get().len())
            } else {
                format!(
                    "{} songs kept on this device. The browser has used {} of {}.",
                    kept.get().len(),
                    megabytes(used),
                    megabytes(quota)
                )
            }
        }))
        .into_view()
}

/// *Sign out*: a link to the page that does it, or (in an app, which has no such page) a button that does it.
fn logout(env: &Env, id: &str) -> Element {
    env.sign_out.as_ref().map_or_else(
        || {
            a().attr("href", format!("{}/ui/logout", env.base))
                .attr("id", id.to_string())
                .class("pl-pill")
                .text("Sign out")
        },
        |go| {
            let go = Rc::clone(go);
            button_("Sign out")
                .attr("id", id.to_string())
                .class("pl-pill")
                .text("Sign out")
                .on_click(move || go())
        },
    )
}

/// How much of what was played the app keeps for next time: only where the engine has such a cache (the app, not the browser).
fn cache_setting(env: &Env) -> View {
    let (shown, label, used, change) = (
        env.m.cache.clone(),
        env.m.cache.clone(),
        env.m.cache.clone(),
        env.clone(),
    );
    View::show(
        move || shown.get().1 > 0,
        move || {
            let (label, used, change) = (label.clone(), used.clone(), change.clone());
            div()
                .class("pl-setting")
                .child(
                    label_()
                        .attr("for", "cache")
                        .attr("id", "cache-label")
                        .text_fn({
                            let label = label.clone();
                            move || format!("Songs kept after playing: {} GB", label.get().1 >> 30)
                        }),
                )
                .child(
                    input()
                        .attr("id", "cache")
                        .attr("type", "range")
                        .attr("min", "1")
                        .attr("max", "50")
                        .attr("step", "1")
                        .attr_fn("value", move || format!("{}", label.get().1 >> 30))
                        .on_change(move |v| {
                            if let Ok(gib) = v.parse::<u32>() {
                                change.engine(&Command::CacheLimit(gib));
                            }
                        }),
                )
                .child(p().class("pl-hint").attr("id", "cache-used").text_fn(move || {
                    format!(
                        "A song you heard plays again without the network, until this is full and the oldest go. Used now: {}.",
                        megabytes(used.get().0)
                    )
                }))
                .into_view()
        },
    )
}

fn label_() -> Element {
    label()
}

fn queue_sheet(env: &Env) -> View {
    let env = env.clone();
    View::dynamic(move || {
        let q = env.m.queue.get();
        if q.ids.is_empty() {
            return p()
                .class("pl-hint")
                .attr("id", "queue-empty")
                .text("The queue is empty.")
                .into_view();
        }
        let mut list = ul().class("pl-songs").attr("id", "queue-list");
        for (offset, id) in q.ids.iter().enumerate() {
            let (title, who) = env.song(*id).map_or_else(
                || ("…".to_string(), String::new()),
                |s| (s.title.clone(), artists_line(&s)),
            );
            let at = q.index + i64::try_from(offset).unwrap_or(0);
            let mut row = li()
                .class("pl-song")
                .class_if("pl-current", move || offset == 0)
                .child(
                    button_("Play this")
                        .class("pl-song-main")
                        .child(span().class("pl-song-n").text(offset.to_string()))
                        .child(
                            div()
                                .class("pl-song-text")
                                .child(div().class("pl-song-title").text(title))
                                .child(div().class("pl-song-artists").text(who)),
                        )
                        .on_click({
                            let env = env.clone();
                            move || env.engine(&Command::Jump(at))
                        }),
                );
            if offset > 0 {
                let env = env.clone();
                row = row.child(glyph_button(
                    "Take out of the queue",
                    "close",
                    "",
                    move || env.engine(&Command::Remove(at)),
                ));
            }
            list = list.child(row);
        }
        list.into_view()
    })
}

fn playing_sheet(env: &Env) -> View {
    let (queue, settings_, keep) = (env.clone(), env.clone(), env.clone());
    let id = env.m.engine.clone();
    div()
        .class("pl-playing")
        .child(now_cover(env, true))
        .child(now_text(env))
        .child(progress(env))
        .child(controls(env, true))
        .child(
            div()
                .class("pl-hero-actions")
                .child(
                    button_("Queue")
                        .attr("id", "open-queue")
                        .class("pl-secondary")
                        .child(labelled("queue", "Queue"))
                        .on_click(move || queue.open(Dialog::Queue)),
                )
                .child(
                    button_("Settings")
                        .attr("id", "open-settings")
                        .class("pl-secondary")
                        .child(labelled("settings", "Settings"))
                        .on_click(move || settings_.open(Dialog::Settings)),
                )
                .child(
                    button_("Keep")
                        .attr("id", "keep-current")
                        .class("pl-secondary")
                        .child(labelled("download", "Keep"))
                        .on_click(move || {
                            let now = id.get_untracked().id;
                            if now != 0 {
                                keep.engine(&Command::Keep(vec![now]));
                                keep.say("Keeping the song on this device");
                            }
                        }),
                ),
        )
        .into_view()
}

/// The box that is open, if any.
pub(crate) fn dialogs(env: &Env) -> View {
    let env = env.clone();
    View::dynamic(move || match env.m.dialog.get() {
        None => View::Empty,
        Some(Dialog::AddTo(songs)) => {
            sheet(&env, "dialog-add", "Add to a playlist", add_to(&env, songs))
        }
        Some(Dialog::Settings) => sheet(&env, "dialog-settings", "Settings", settings(&env)),
        Some(Dialog::Queue) => sheet(&env, "dialog-queue", "Queue", queue_sheet(&env)),
        Some(Dialog::Playing) => sheet(&env, "dialog-playing", "Now playing", playing_sheet(&env)),
    })
}

#[cfg(test)]
mod tests {
    use super::{clock, enc};

    #[test]
    fn a_clock_reads_minutes_and_seconds() {
        assert_eq!(clock(187.4), "3:07");
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(f64::NAN), "0:00");
    }

    #[test]
    fn a_name_is_one_path_segment() {
        assert_eq!(enc("Nate Dogg"), "Nate%20Dogg");
        assert_eq!(enc("AC/DC"), "AC%2FDC");
        assert_eq!(enc("Żółć"), "%C5%BB%C3%B3%C5%82%C4%87");
    }
}
