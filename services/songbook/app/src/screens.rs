//! The screens, as functions from the model to a view.

use std::cell::Cell;
use std::rc::Rc;

use quench_ui::form::Field;
use quench_ui::prelude::*;
use quench_ui::router::{Router, link};
use songbook_core::{Album, ArtistRow, Filter, JobStart, NewArtist, Schedule, Track};

use crate::live::{Dialog, Env, Pending, Problem, Request};

/// The screens in the side panel: path, title key, element id, icon.
pub(crate) const PAGES: [(&str, &str, &str, &str); 5] = [
    ("/library", "ui_nav_library", "nav-library", "database"),
    ("/pipeline", "ui_nav_pipeline", "nav-pipeline", "layers"),
    ("/discover", "ui_nav_discover", "nav-discover", "users"),
    ("/schedule", "ui_nav_schedule", "nav-schedule", "clock"),
    ("/activity", "ui_nav_activity", "nav-activity", "activity"),
];

fn article() -> Element {
    element("article")
}

fn small() -> Element {
    element("small")
}

fn ol() -> Element {
    element("ol")
}

fn h4() -> Element {
    element("h4")
}

// ── Frame ────────────────────────────────────────────────────────────────────

/// The four screens, under the brand.
pub(crate) fn pages_nav(env: &Env, router: &Router) -> View {
    let (env, router) = (env.clone(), router.clone());
    View::dynamic(move || {
        let here = router.location().path;
        let mut nav = div();
        for (path, key, id, icon) in PAGES {
            let href = env.path(path);
            let home = path == "/library"
                && (here == env.path("") || here == env.path("/") || here == env.path("/home"));
            let mut item = link(&href)
                .attr("id", id)
                .class("fui-nav-item")
                .child(forge_shell::icon(icon))
                .child(span().text(env.t.t(key)));
            if home {
                item = item.attr("aria-current", "page");
            }
            nav = nav.child(item);
        }
        nav.into_view()
    })
}

/// The followed artists, by category, and the way to follow another.
pub(crate) fn artists_nav(env: &Env) -> View {
    let env = env.clone();
    View::dynamic(move || {
        let t = &env.t;
        let selected = env.m.filter.get().artist;
        let state = env.m.state.get();
        let mut nav = div().class("sb-artists");
        nav = nav.child(div().class("sb-group").text(state.as_ref().map_or_else(
            || t.t("ui_followed"),
            |s| format!("{} ({})", t.t("ui_followed"), s.artists.len()),
        )));
        nav = nav.child(
            button()
                .attr("type", "button")
                .attr("id", "all-artists")
                .class(if selected.is_empty() {
                    "sb-artist-main sb-all sb-sel"
                } else {
                    "sb-artist-main sb-all"
                })
                .on_click({
                    let env = env.clone();
                    move || pick_artist(&env, "")
                })
                .child(span().class("sb-avatar").text("∗"))
                .child(
                    span()
                        .class("sb-txt")
                        .child(strong().text(t.t("ui_all_artists"))),
                ),
        );
        if let Some(state) = &state {
            let groups = state
                .categories
                .iter()
                .chain(std::iter::once(&state.uncategorized));
            for category in groups {
                let mut members: Vec<&ArtistRow> = state
                    .artists
                    .iter()
                    .filter(|a| &a.category == category)
                    .collect();
                if members.is_empty() {
                    continue;
                }
                // Alphabetical, whatever order they were followed in.
                members.sort_by_cached_key(|a| (a.name.to_lowercase(), a.name.clone()));
                let expanded = env.m.open.get().contains(category);
                let head = category_head(
                    &env,
                    category,
                    members.len(),
                    expanded,
                    category != &state.uncategorized,
                );
                nav = nav.child(head);
                if expanded {
                    for artist in members {
                        nav = nav.child(artist_row(&env, artist, artist.name == selected));
                    }
                }
            }
        }
        nav.child(
            button()
                .attr("type", "button")
                .attr("id", "add-artist")
                .class("fui-btn fui-btn-secondary sb-add")
                .on_click({
                    let m = env.m.clone();
                    move || m.dialog.set(Some(Dialog::Add))
                })
                .child(forge_shell::icon("plus"))
                .child(span().text(t.t("ui_add_artist"))),
        )
        .into_view()
    })
}

/// The header of a category in the side panel: it folds and unfolds the list, and (for a real category) renames it.
fn category_head(
    env: &Env,
    category: &str,
    count: usize,
    expanded: bool,
    renamable: bool,
) -> Element {
    let t = &env.t;
    let fold = button()
        .attr("type", "button")
        .attr("aria-expanded", if expanded { "true" } else { "false" })
        .attr("data-fold", category.to_string())
        .class("sb-group sb-group-btn")
        .on_click({
            let (m, category) = (env.m.clone(), category.to_string());
            move || {
                m.open.update(|open| {
                    if let Some(at) = open.iter().position(|c| c == &category) {
                        open.remove(at);
                    } else {
                        open.push(category.clone());
                    }
                });
            }
        })
        .child(
            span()
                .class("sb-chevron")
                .text(if expanded { "▾" } else { "▸" }),
        )
        .child(span().text(format!("{category} ({count})")));
    let mut head = div().class("sb-group-row").child(fold);
    if renamable {
        head = head.child(
            button()
                .attr("type", "button")
                .class("sb-icon-btn sb-rename-btn")
                .attr("data-rename", category.to_string())
                .attr(
                    "aria-label",
                    t.tr(
                        "ui_catrename_aria",
                        &[("name", category.to_string().into())],
                    ),
                )
                .attr("title", t.t("ui_catrename_title"))
                .on_click({
                    let (m, category) = (env.m.clone(), category.to_string());
                    move || m.dialog.set(Some(Dialog::RenameCategory(category.clone())))
                })
                .child(span().text("✎")),
        );
    }
    head
}

fn pick_artist(env: &Env, name: &str) {
    env.m.filter.update(|f| f.artist = name.to_string());
    // The artist's category is open: it is where the selection is seen. It folds like any other afterwards.
    let category = env.m.state.get_untracked().and_then(|s| {
        s.artists
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.category.clone())
    });
    if let Some(category) = category {
        env.m.open.update(|open| {
            if !open.contains(&category) {
                open.push(category);
            }
        });
    }
    env.load();
}

fn artist_row(env: &Env, artist: &ArtistRow, selected: bool) -> Element {
    let t = &env.t;
    let title = if artist.deezer_name.is_empty() {
        t.t("ui_not_synced")
    } else {
        t.tr(
            "ui_artist_tooltip",
            &[
                ("name", artist.deezer_name.clone().into()),
                ("id", artist.deezer_id.clone().into()),
                ("fans", artist.fans.clone().into()),
            ],
        )
    };
    let sub = if artist.synced {
        t.tr("ui_tracks_n", &[("n", artist.tracks.into())])
    } else {
        t.t("ui_not_synced")
    };
    let initial = artist
        .name
        .chars()
        .next()
        .map_or_else(|| "?".to_string(), |c| c.to_uppercase().collect());
    let name = artist.name.clone();
    let mut row = div()
        .class("sb-artist")
        .attr("data-artist", artist.name.clone());
    if selected {
        row = row.class("sb-sel");
    }
    if artist.synced && artist.tracks == 0 {
        row = row.class("sb-warn");
    }
    row.child(
        button()
            .attr("type", "button")
            .class("sb-artist-main")
            .attr("title", title)
            .on_click({
                let (env, name) = (env.clone(), name.clone());
                move || pick_artist(&env, &name)
            })
            .child(span().class("sb-avatar").text(initial))
            .child(
                span()
                    .class("sb-txt")
                    .child(strong().text(artist.name.clone()))
                    .child(small().text(sub)),
            ),
    )
    .child(
        button()
            .attr("type", "button")
            .class("sb-icon-btn sb-category-btn")
            .attr(
                "aria-label",
                t.tr(
                    "ui_change_category_aria",
                    &[("name", artist.name.clone().into())],
                ),
            )
            .attr(
                "title",
                t.tr(
                    "ui_category_tooltip",
                    &[("category", artist.category.clone().into())],
                ),
            )
            .on_click({
                let (m, artist) = (env.m.clone(), artist.clone());
                move || m.dialog.set(Some(Dialog::Category(artist.clone())))
            })
            .child(forge_shell::icon("folder")),
    )
    .child(
        button()
            .attr("type", "button")
            .class("sb-icon-btn sb-remove-btn")
            .attr(
                "aria-label",
                t.tr(
                    "ui_remove_artist_aria",
                    &[("name", artist.name.clone().into())],
                ),
            )
            .on_click({
                let m = env.m.clone();
                move || m.dialog.set(Some(Dialog::Remove(name.clone())))
            })
            .text("✕"),
    )
}

/// The row above every screen: the search and the pill that shows a running job.
pub(crate) fn toolbar(env: &Env) -> View {
    let (e, generation) = (env.clone(), Rc::new(Cell::new(0_u32)));
    let search = {
        let t = Rc::clone(&env.t);
        View::dynamic(move || {
            let (e, generation) = (e.clone(), Rc::clone(&generation));
            label()
                .class("sb-search")
                .child(span().class("sr-only").text(t.t("ui_search_label")))
                .child(
                    input()
                        .attr("id", "q")
                        .attr("type", "search")
                        .attr("autocomplete", "off")
                        .attr("placeholder", t.t("ui_search_placeholder"))
                        .attr("aria-label", t.t("ui_search_label"))
                        .on_input(move |text| {
                            e.m.filter.update(|f| f.q = text.to_string());
                            // Asks once the typing pauses.
                            let n = generation.get().wrapping_add(1);
                            generation.set(n);
                            let (e, generation) = (e.clone(), Rc::clone(&generation));
                            let _ = quench_ui::platform::after(200, move || {
                                if generation.get() == n {
                                    e.load();
                                }
                            });
                        }),
                )
                .into_view()
        })
    };
    let pill = {
        let env = env.clone();
        View::dynamic(move || {
            let job = env.m.job.get();
            let name = job.name.clone().unwrap_or_default();
            if job.running {
                pill_link(
                    &env,
                    env.t.tr("ui_job_running", &[("name", name.into())]),
                    false,
                )
            } else if job.error.is_some() {
                pill_link(
                    &env,
                    env.t.tr("ui_job_failed", &[("name", name.into())]),
                    true,
                )
            } else {
                View::Empty
            }
        })
    };
    let progress = {
        let m = env.m.clone();
        View::show(
            move || m.job.get().running,
            || div().class("sb-progress").child(span()).into_view(),
        )
    };
    div()
        .class("sb-toolbar")
        .child(search)
        .child(pill)
        .child(progress)
        .into_view()
}

fn pill_link(env: &Env, text: String, error: bool) -> View {
    link(&env.path("/activity"))
        .attr("id", "job-pill")
        .class(if error {
            "sb-pill sb-pill-error"
        } else {
            "sb-pill"
        })
        .text(text)
        .into_view()
}

/// What went wrong with the service, over the screen.
pub(crate) fn banner(env: &Env, login_url: &str) -> View {
    let (env, login) = (env.clone(), login_url.to_string());
    View::dynamic(move || match env.m.problem.get() {
        Some(Problem::Failed(why)) => p()
            .attr("id", "problem")
            .class("fui-alert fui-alert-error")
            .text(why)
            .into_view(),
        Some(Problem::SignedOut) => p()
            .attr("id", "problem")
            .class("fui-alert fui-alert-error")
            .child(span().text(format!("{} ", env.t.t("ui_session_ended"))))
            .child(
                a().attr("id", "sign-in")
                    .attr("href", login.clone())
                    .text(env.t.t("ui_sign_in")),
            )
            .into_view(),
        None => View::Empty,
    })
}

/// The message that shows for a few seconds.
pub(crate) fn snack(env: &Env) -> View {
    let m = env.m.clone();
    div()
        .attr("id", "snack")
        .attr("role", "status")
        .class_if("sb-show", {
            let m = m.clone();
            move || m.snack.get().is_some()
        })
        .child(View::text_fn(move || m.snack.get().unwrap_or_default()))
        .into_view()
}

pub(crate) fn not_found(env: &Env) -> View {
    div()
        .class("sb-page")
        .child(link(&env.path("/library")).text(env.t.t("ui_nav_library")))
        .child(
            p().attr("id", "not-found")
                .class("fui-empty")
                .text(env.t.t("ui_nothing")),
        )
        .into_view()
}

fn head(env: &Env, title: &str, intro: &str) -> Element {
    div()
        .class("sb-head")
        .child(h2().class("sb-title").text(env.t.t(title)))
        .child(p().class("sb-intro").text(env.t.t(intro)))
}

fn chip(text: String, on: bool, id: &str, click: impl Fn() + 'static) -> Element {
    button()
        .attr("type", "button")
        .attr("data-chip", id)
        .attr("aria-pressed", if on { "true" } else { "false" })
        .class(if on { "sb-chip sb-on" } else { "sb-chip" })
        .on_click(click)
        .text(text)
}

fn waiting(env: &Env) -> View {
    p().attr("id", "waiting")
        .class("fui-empty")
        .text(env.t.t("ui_waiting"))
        .into_view()
}

// ── Library ──────────────────────────────────────────────────────────────────

const LINK_FILTERS: [(&str, &str); 4] = [
    ("", "ui_links_all"),
    ("missing", "ui_links_missing"),
    ("has", "ui_links_has"),
    ("unofficial", "ui_links_unofficial"),
];

const SORTS: [(&str, &str); 4] = [
    ("", "ui_sort_library"),
    ("top", "ui_sort_top"),
    ("artists", "ui_sort_artists"),
    ("suggested", "ui_sort_suggested"),
];

pub(crate) fn library(env: &Env) -> View {
    let (chips, summary, result) = (filters(env), summary(env), albums(env));
    div()
        .attr("id", "page-library")
        .class("sb-page")
        .child(View::dynamic({
            let env = env.clone();
            move || head(&env, "ui_library_title", "ui_library_intro").into_view()
        }))
        .child(stats_strip(env))
        .child(chips)
        .child(summary)
        .child(result)
        .into_view()
}

/// The numbers on top of the library: how big it is, and how much of it is on disk.
fn stats_strip(env: &Env) -> View {
    let env = env.clone();
    View::dynamic(move || {
        let t = &env.t;
        let Some(s) = env.m.stats.get() else {
            return View::Empty;
        };
        let card = |id: &str, label: String, value: u32, sub: Option<String>, class: &str| {
            let mut c = div()
                .attr("id", format!("stat-{id}"))
                .class(format!("sb-stat {class}"))
                .child(span().class("sb-stat-label").text(label))
                .child(strong().class("sb-stat-value").text(value.to_string()));
            if let Some(sub) = sub {
                c = c.child(small().class("sb-stat-sub").text(sub));
            }
            c
        };
        // Rounded, but never 100% while something is missing.
        let percent = if s.songs == 0 {
            0
        } else {
            let p = (u64::from(s.downloaded) * 100 + u64::from(s.songs) / 2) / u64::from(s.songs);
            let p = u32::try_from(p).unwrap_or(100);
            if s.missing > 0 { p.min(99) } else { p }
        };
        div()
            .attr("id", "stats")
            .class("sb-stats")
            .child(card(
                "artists",
                t.t("ui_stat_artists"),
                s.artists,
                Some(t.tr("ui_stat_artists_sub", &[("n", s.credited.into())])),
                "",
            ))
            .child(card("albums", t.t("ui_stat_albums"), s.albums, None, ""))
            .child(card("songs", t.t("ui_stat_songs"), s.songs, None, ""))
            .child(card(
                "downloaded",
                t.t("ui_stat_downloaded"),
                s.downloaded,
                Some(t.tr("ui_stat_downloaded_sub", &[("pct", percent.into())])),
                "sb-stat-ok",
            ))
            .child(card(
                "missing",
                t.t("ui_stat_missing"),
                s.missing,
                Some(t.tr(
                    "ui_stat_missing_sub",
                    &[
                        ("linked", s.missing_linked.into()),
                        ("unlinked", (s.missing - s.missing_linked).into()),
                    ],
                )),
                if s.missing == 0 { "" } else { "sb-stat-warn" },
            ))
            .into_view()
    })
}

/// The category, genre and link chips.
fn filters(env: &Env) -> View {
    let env = env.clone();
    View::dynamic(move || {
        let t = &env.t;
        let filter = env.m.filter.get();
        let state = env.m.state.get();
        let set = |change: fn(&mut Filter, &str), value: String| {
            let env = env.clone();
            move || {
                env.m.filter.update(|f| change(f, &value));
                env.load();
            }
        };
        let mut categories = div()
            .class("sb-chips")
            .attr("id", "filter-categories")
            .attr("role", "group");
        categories = categories.child(chip(
            t.t("ui_all_categories"),
            filter.category.is_empty(),
            "category-all",
            set(|f, v| f.category = v.to_string(), String::new()),
        ));
        let mut genres = div()
            .class("sb-chips")
            .attr("id", "filter-genres")
            .attr("role", "group");
        genres = genres.child(chip(
            t.t("ui_all_genres"),
            filter.genre.is_empty(),
            "genre-all",
            set(|f, v| f.genre = v.to_string(), String::new()),
        ));
        if let Some(state) = &state {
            for c in state
                .categories
                .iter()
                .chain(std::iter::once(&state.uncategorized))
            {
                categories = categories.child(chip(
                    c.clone(),
                    &filter.category == c,
                    "category",
                    set(|f, v| f.category = v.to_string(), c.clone()),
                ));
            }
            for g in &state.genres {
                genres = genres.child(chip(
                    g.clone(),
                    &filter.genre == g,
                    "genre",
                    set(|f, v| f.genre = v.to_string(), g.clone()),
                ));
            }
        }
        let mut links = div()
            .class("sb-chips")
            .attr("id", "filter-links")
            .attr("role", "group");
        for (value, key) in LINK_FILTERS {
            links = links.child(chip(
                t.t(key),
                filter.links == value,
                &format!("links-{}", if value.is_empty() { "all" } else { value }),
                set(|f, v| f.links = v.to_string(), value.to_string()),
            ));
        }
        let mut sorts = div()
            .class("sb-chips")
            .attr("id", "filter-sort")
            .attr("role", "group");
        for (value, key) in SORTS {
            sorts = sorts.child(chip(
                t.t(key),
                filter.sort == value,
                &format!("sort-{}", if value.is_empty() { "library" } else { value }),
                set(|f, v| f.sort = v.to_string(), value.to_string()),
            ));
        }
        div()
            .child(sorts)
            .child(categories)
            .child(genres)
            .child(links)
            .into_view()
    })
}

fn summary(env: &Env) -> View {
    let env = env.clone();
    View::dynamic(move || {
        let t = &env.t;
        let Some(browse) = env.m.browse.get() else {
            return h3()
                .attr("id", "summary")
                .text(t.t("ui_nav_library"))
                .into_view();
        };
        let artist = env.m.filter.get().artist;
        let who = if artist.is_empty() {
            t.t("ui_all_artists")
        } else {
            artist
        };
        let mut text = t.tr(
            "ui_summary",
            &[
                ("who", who.into()),
                ("total", browse.total.into()),
                ("linked", browse.linked.into()),
            ],
        );
        if browse.total > browse.shown {
            text.push_str(" · ");
            text.push_str(&t.tr("ui_summary_shown", &[("shown", browse.shown.into())]));
        }
        h3().attr("id", "summary").text(text).into_view()
    })
}

fn albums(env: &Env) -> View {
    let env = env.clone();
    View::dynamic(move || {
        let Some(browse) = env.m.browse.get() else {
            return waiting(&env);
        };
        if browse.albums.is_empty() {
            return div()
                .attr("id", "nothing")
                .class("fui-empty")
                .text(env.t.t("ui_nothing"))
                .into_view();
        }
        div()
            .attr("id", "albums")
            .children(browse.albums.iter().map(|a| album(&env, a).into_view()))
            .into_view()
    })
}

fn album(env: &Env, a: &Album) -> Element {
    let t = &env.t;
    let tags = [a.year.clone(), a.kind.clone()]
        .into_iter()
        .chain(a.genre.split("; ").map(str::to_string))
        .filter(|x| !x.is_empty());
    // A ranked list (top, suggested) is one card without an album: no title, no blacklist, artists on each track. By artist,
    // a card is titled with the artist's name and also has no blacklist (it is not an album: id 0).
    let ranked = a.id == 0;
    let with_artists = ranked && a.album.is_empty();
    article()
        .class("sb-card")
        .attr("data-album", a.id.to_string())
        .child((!ranked).then(|| {
            button()
                .attr("type", "button")
                .class("fui-btn fui-btn-tertiary fui-btn-sm sb-blacklist")
                .attr("title", t.t("ui_blacklist_title"))
                .on_click({
                    let (m, a) = (env.m.clone(), a.clone());
                    move || m.dialog.set(Some(Dialog::Blacklist(a.clone())))
                })
                .text(format!("⊘ {}", t.t("ui_blacklist")))
        }))
        .child((!a.album.is_empty()).then(|| h4().class("sb-album").text(a.album.clone())))
        .child((!ranked).then(|| div().class("sb-sub").text(a.artist.clone())))
        .child(
            div()
                .class("sb-chips")
                .children(tags.map(|x| span().class("sb-chip sb-static").text(x).into_view())),
        )
        .child(
            ol().class("sb-tracks").children(
                a.tracks
                    .iter()
                    .enumerate()
                    .map(|(i, tr)| track(env, i + 1, tr, with_artists).into_view()),
            ),
        )
}

fn track(env: &Env, number: usize, tr: &Track, with_artists: bool) -> Element {
    let t = &env.t;
    let has = !tr.url.is_empty();
    let shown = if with_artists {
        format!("{} – {}", tr.artists, tr.title)
    } else {
        tr.title.clone()
    };
    li().attr("data-song", tr.id.to_string())
        .child(span().class("sb-no").text(number.to_string()))
        .child(
            span()
                .class("sb-t")
                .attr("title", shown.clone())
                .text(shown),
        )
        .child(tr.score.map(|n| {
            span()
                .class("sb-score")
                .attr("title", "Deezer rank + YouTube views")
                .text(n.to_string())
        }))
        .child(tr.manual.then(|| {
            span()
                .class("sb-manual")
                .attr("title", t.t("ui_manual_title"))
                .text(t.t("ui_manual"))
        }))
        .child((has && tr.official).then(|| {
            span()
                .class("sb-ok")
                .attr("title", t.t("ui_official_title"))
                .text("✓")
        }))
        .child(if has {
            a().attr("href", tr.url.clone())
                .attr("target", "_blank")
                .attr("rel", "noopener")
                .attr(
                    "aria-label",
                    t.tr(
                        "ui_open_youtube_aria",
                        &[("title", tr.title.clone().into())],
                    ),
                )
                .attr(
                    "title",
                    t.t(if tr.official {
                        "ui_official_title"
                    } else {
                        "ui_unofficial_title"
                    }),
                )
                .class("sb-icon-btn sb-play")
                .text("▶")
        } else {
            span().class("sb-nolink").text(t.t("ui_no_link"))
        })
        .child(
            button()
                .attr("type", "button")
                .class("sb-icon-btn sb-edit")
                .attr(
                    "aria-label",
                    t.tr(
                        if has {
                            "ui_edit_link_aria"
                        } else {
                            "ui_add_link_aria"
                        },
                        &[("title", tr.title.clone().into())],
                    ),
                )
                .attr(
                    "title",
                    t.t(if has { "ui_edit_link" } else { "ui_add_link" }),
                )
                .on_click({
                    let (m, tr) = (env.m.clone(), tr.clone());
                    move || m.dialog.set(Some(Dialog::Link(tr.clone())))
                })
                .text("✎"),
        )
}

// ── Pipeline ─────────────────────────────────────────────────────────────────

fn limit(field: &Field) -> Option<u32> {
    field
        .value_untracked()
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|n| *n > 0)
}

/// Starts a job with the options the screens hold.
fn start(env: &Env, name: &str) {
    let m = &env.m;
    // The whole pipeline takes both limits: `limit` is the one of find links, `limit_download` the one of download.
    let everything = name == "pipeline";
    // "purge-preview" is the purge job that only lists.
    let (name, dry_run) = if name == "purge-preview" {
        ("purge", true)
    } else {
        (name, false)
    };
    (env.ask)(Request::StartJob(JobStart {
        name: name.to_string(),
        dry_run,
        singles: m.singles.get_untracked(),
        artist: m.filter.get_untracked().artist,
        limit: if name == "youtube" || everything {
            limit(&m.limit_links)
        } else if name == "download" {
            limit(&m.limit_download)
        } else {
            None
        },
        limit_download: if everything {
            limit(&m.limit_download)
        } else {
            None
        },
        top: limit(&m.top),
        per_artist: None,
        category: m.scope.value_untracked(),
        // The pipeline always looks again at songs that had no match.
        retry_missing: m.retry.get_untracked() || everything,
    }));
}

/// A button that starts a job, off while one runs.
fn job_button(
    env: &Env,
    name: &'static str,
    label: &str,
    class: &str,
    title: Option<String>,
) -> Element {
    let (go, m) = (env.clone(), env.m.clone());
    let b = button()
        .attr("type", "button")
        .attr("id", format!("job-{name}"))
        .class(format!("fui-btn {class}"))
        .attr_bool("disabled", move || m.job.get().running)
        .on_click(move || start(&go, name))
        .text(label.to_string());
    match title {
        Some(title) => b.attr("title", title),
        None => b,
    }
}

fn toggle(label: String, title: String, id: &'static str, flag: &Signal<bool>) -> Element {
    let (on, flip) = (flag.clone(), flag.clone());
    button()
        .attr("type", "button")
        .attr("id", id)
        .attr("title", title)
        .class("sb-chip")
        .class_if("sb-on", move || on.get())
        .attr_fn("aria-pressed", {
            let flag = flag.clone();
            move || flag.get().to_string()
        })
        .on_click(move || flip.update(|v| *v = !*v))
        .text(label)
}

fn number(env: &Env, field: &Field, label: &str) -> Element {
    label_for(
        field.name(),
        label,
        field
            .input()
            .attr("type", "number")
            .attr("min", "0")
            .attr("placeholder", env.t.t("ui_max_all"))
            .class("fui-input fui-input-sm"),
    )
    .class("sb-opt")
}

fn label_for(id: &str, text: &str, control: Element) -> Element {
    label()
        .attr("for", id.to_string())
        .child(span().text(text.to_string()))
        .child(control)
}

#[allow(clippy::too_many_lines)]
pub(crate) fn pipeline(env: &Env) -> View {
    let env = env.clone();
    View::dynamic(move || {
        let t = &env.t;
        let m = env.m.clone();
        let step = |n: u32, title: &str, text: &str| {
            vec![
                span()
                    .class("fui-badge fui-badge-brand")
                    .text(t.tr("ui_step", &[("n", n.into())]))
                    .into_view(),
                h3().text(t.t(title)).into_view(),
                p().text(t.t(text)).into_view(),
            ]
        };
        let toolbar = || div().class("sb-actions");
        div()
            .attr("id", "page-pipeline")
            .class("sb-page")
            .child(head(&env, "ui_pipeline_title", "ui_pipeline_intro"))
            .child(View::dynamic({
                let (m, t) = (m.clone(), Rc::clone(&env.t));
                move || {
                    let artist = m.filter.get().artist;
                    let text = if artist.is_empty() {
                        t.t("ui_scope_all")
                    } else {
                        t.tr("ui_scope_one", &[("name", artist.into())])
                    };
                    div()
                        .attr("id", "scope-note")
                        .class("fui-alert fui-alert-info")
                        .text(text)
                        .into_view()
                }
            }))
            .child(
                article()
                    .attr("id", "run-everything")
                    .class("sb-card sb-step")
                    .child(h3().text(t.t("ui_run_all_title")))
                    .child(p().text(t.t("ui_run_all_text")))
                    .child(toolbar().child(job_button(
                        &env,
                        "pipeline",
                        &t.t("ui_run_all"),
                        "fui-btn-primary",
                        Some(t.t("ui_run_all_hint")),
                    ))),
            )
            .child(
                div()
                    .class("sb-steps")
                    .child(
                        article()
                            .class("sb-card sb-step")
                            .children(step(1, "ui_step_library", "ui_step_library_text"))
                            .child(
                                toolbar()
                                    .child(job_button(
                                        &env,
                                        "sync",
                                        &t.t("ui_sync"),
                                        "fui-btn-primary",
                                        None,
                                    ))
                                    .child(toggle(
                                        t.t("ui_singles"),
                                        t.t("ui_singles_title"),
                                        "singles",
                                        &m.singles,
                                    )),
                            )
                            .child(toolbar().child(job_button(
                                &env,
                                "tags",
                                &t.t("ui_rebuild_tags"),
                                "fui-btn-tertiary",
                                Some(t.t("ui_rebuild_tags_title")),
                            ))),
                    )
                    .child(
                        article()
                            .class("sb-card sb-step")
                            .children(step(2, "ui_step_links", "ui_step_links_text"))
                            .child(
                                toolbar()
                                    .child(job_button(
                                        &env,
                                        "youtube",
                                        &t.t("ui_find_links"),
                                        "fui-btn-secondary",
                                        None,
                                    ))
                                    .child(toggle(
                                        t.t("ui_retry_missing"),
                                        t.t("ui_retry_missing_title"),
                                        "retry",
                                        &m.retry,
                                    )),
                            )
                            .child(toolbar().child(number(
                                &env,
                                &m.limit_links,
                                &t.t("ui_max_tracks"),
                            ))),
                    )
                    .child(
                        article()
                            .class("sb-card sb-step")
                            .children(step(3, "ui_step_download", "ui_step_download_text"))
                            .child(
                                toolbar()
                                    .child(job_button(
                                        &env,
                                        "download",
                                        &t.t("ui_download"),
                                        "fui-btn-secondary",
                                        None,
                                    ))
                                    .child(job_button(
                                        &env,
                                        "convert",
                                        &t.t("ui_convert"),
                                        "fui-btn-secondary",
                                        None,
                                    )),
                            )
                            .child(toolbar().child(number(
                                &env,
                                &m.limit_download,
                                &t.t("ui_max_tracks"),
                            ))),
                    ),
            )
            .child(
                article()
                    .attr("id", "purge-blacklisted")
                    .class("sb-card sb-step")
                    .child(h3().text(t.t("ui_purge_title")))
                    .child(p().text(t.t("ui_purge_text")))
                    .child(
                        toolbar()
                            .child(job_button(
                                &env,
                                "purge-preview",
                                &t.t("ui_purge_preview"),
                                "fui-btn-secondary",
                                Some(t.t("ui_purge_preview_title")),
                            ))
                            .child(job_button(
                                &env,
                                "purge",
                                &t.t("ui_purge_delete"),
                                "fui-btn-danger",
                                Some(t.t("ui_purge_delete_title")),
                            )),
                    ),
            )
            .into_view()
    })
}

// ── Discover ─────────────────────────────────────────────────────────────────

/// A `<select>` bound to `field`; `options` are `(value, text)`.
fn choice(field: &Field, options: Vec<(String, String)>) -> Element {
    let pick = field.clone();
    let rows = options.into_iter().map(|(value, text)| {
        let (field, v) = (field.clone(), value.clone());
        option()
            .attr("value", value)
            .attr_bool("selected", move || field.value() == v)
            .text(text)
            .into_view()
    });
    select()
        .class("fui-select fui-select-sm")
        .attr("id", field.name().to_string())
        .attr("name", field.name().to_string())
        .on_input(move |v| pick.set(v))
        .children(rows)
}

pub(crate) fn discover(env: &Env) -> View {
    let env = env.clone();
    View::dynamic(move || {
        let t = &env.t;
        let m = env.m.clone();
        let scope = {
            let (env, m) = (env.clone(), m.clone());
            View::dynamic(move || {
                let mut options = vec![(String::new(), env.t.t("ui_all_categories"))];
                if let Some(s) = m.state.get() {
                    options.extend(
                        s.categories
                            .iter()
                            .chain(std::iter::once(&s.uncategorized))
                            .map(|c| (c.clone(), c.clone())),
                    );
                }
                label_for(
                    m.scope.name(),
                    &env.t.t("ui_scope"),
                    choice(&m.scope, options),
                )
                .class("sb-opt")
                .into_view()
            })
        };
        let note = {
            let (env, m) = (env.clone(), m.clone());
            View::dynamic(move || {
                let scope = m
                    .state
                    .get()
                    .and_then(|s| s.suggestions.first().map(|x| x.scope.clone()))
                    .unwrap_or_default();
                if scope.is_empty() {
                    View::Empty
                } else {
                    span()
                        .attr("id", "suggestion-scope")
                        .text(format!(
                            " {}",
                            env.t.tr("ui_current_list", &[("scope", scope.into())])
                        ))
                        .into_view()
                }
            })
        };
        div()
            .attr("id", "page-discover")
            .class("sb-page")
            .child(
                div()
                    .class("sb-head")
                    .child(h2().class("sb-title").text(t.t("ui_discover_title")))
                    .child(
                        p().class("sb-intro")
                            .text(t.t("ui_discover_intro"))
                            .child(note),
                    ),
            )
            .child(
                article().class("sb-card").child(
                    div()
                        .class("sb-actions")
                        .child(scope)
                        .child(number_plain(&m.top, &t.t("ui_top")))
                        .child(job_button(
                            &env,
                            "similar",
                            &t.t("ui_find_similar"),
                            "fui-btn-primary",
                            None,
                        )),
                ),
            )
            .child(suggestions(&env))
            .child(fab(&env))
            .into_view()
    })
}

fn number_plain(field: &Field, text: &str) -> Element {
    label_for(
        field.name(),
        text,
        field
            .input()
            .attr("type", "number")
            .attr("min", "1")
            .class("fui-input fui-input-sm"),
    )
    .class("sb-opt")
}

fn suggestions(env: &Env) -> View {
    let env = env.clone();
    View::dynamic(move || {
        let t = &env.t;
        let list = env.m.state.get().map(|s| s.suggestions).unwrap_or_default();
        if list.is_empty() {
            return div()
                .attr("id", "no-suggestions")
                .class("fui-empty")
                .text(t.t("ui_no_suggestions"))
                .into_view();
        }
        let picked = env.m.picked.get();
        let cells = list.iter().map(|s| {
            let on = picked.contains(&s.artist);
            let via: Vec<&str> = s.related_to.split("; ").take(3).collect();
            let name = s.artist.clone();
            let m = env.m.clone();
            button()
                .attr("type", "button")
                .attr("data-suggestion", s.artist.clone())
                .attr("aria-pressed", if on { "true" } else { "false" })
                .class(if on { "sb-sug sb-on" } else { "sb-sug" })
                .on_click(move || {
                    m.picked.update(|p| {
                        if let Some(at) = p.iter().position(|x| x == &name) {
                            p.remove(at);
                        } else {
                            p.push(name.clone());
                        }
                    });
                })
                .child(span().class("sb-check").text(if on { "☑" } else { "☐" }))
                .child(
                    span()
                        .child(strong().text(s.artist.clone()))
                        .child(small().text(t.tr(
                            "ui_via",
                            &[
                                ("n", s.related_to_count.into()),
                                ("names", via.join(", ").into()),
                            ],
                        )))
                        .child((!s.best.is_empty()).then(|| {
                            let songs: Vec<String> = s
                                .best
                                .iter()
                                .map(|b| format!("{} ({})", b.title, b.score))
                                .collect();
                            small()
                                .class("sb-best")
                                .text(t.tr("ui_best", &[("songs", songs.join(", ").into())]))
                        })),
                )
                .into_view()
        });
        div()
            .attr("id", "suggestions")
            .class("sb-grid")
            .children(cells)
            .into_view()
    })
}

/// The button that follows the ticked suggestions and syncs.
fn fab(env: &Env) -> View {
    let env = env.clone();
    View::dynamic(move || {
        let n = env.m.picked.get().len();
        if n == 0 {
            return View::Empty;
        }
        let go = env.clone();
        button()
            .attr("type", "button")
            .attr("id", "add-suggested")
            .class("fui-btn fui-btn-primary sb-fab")
            .on_click(move || {
                let picked = go.m.picked.get_untracked();
                let items: Vec<NewArtist> =
                    go.m.state
                        .get_untracked()
                        .map(|s| s.suggestions)
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|s| picked.contains(&s.artist))
                        .map(|s| NewArtist {
                            name: s.artist,
                            id: s.deezer_id,
                            category: String::new(),
                        })
                        .collect();
                *go.m.pending.borrow_mut() = Pending::AddSuggested;
                (go.ask)(Request::AddArtists(items));
            })
            .text(env.t.tr("ui_fab", &[("n", n.into())]))
            .into_view()
    })
}

// ── Activity ─────────────────────────────────────────────────────────────────

/// The nightly run: when, which stages, and how the last one went.
#[allow(clippy::too_many_lines)]
pub(crate) fn schedule(env: &Env) -> View {
    let env = env.clone();
    View::dynamic(move || {
        let t = &env.t;
        let m = env.m.clone();
        let status = {
            let (m, t) = (m.clone(), Rc::clone(&env.t));
            View::dynamic(move || {
                let Some(s) = m.schedule.get() else {
                    return p()
                        .attr("id", "sched-waiting")
                        .class("fui-empty")
                        .text(t.t("ui_waiting"))
                        .into_view();
                };
                let zone = if s.zone_in_use.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", s.zone_in_use)
                };
                let next = if s.next_run.is_empty() {
                    t.t("ui_sched_off")
                } else {
                    t.tr(
                        "ui_sched_next",
                        &[("when", format!("{}{zone}", s.next_run).into())],
                    )
                };
                let last = if s.last_run.is_empty() {
                    t.t("ui_sched_never")
                } else {
                    t.tr(
                        "ui_sched_last",
                        &[
                            ("when", s.last_run.clone().into()),
                            ("result", s.last_result.into()),
                        ],
                    )
                };
                div()
                    .attr("id", "sched-status")
                    .class("fui-alert fui-alert-info")
                    .child(div().attr("id", "sched-next").text(next))
                    .child(div().attr("id", "sched-last").text(last))
                    .into_view()
            })
        };
        let stages = div()
            .class("sb-actions")
            .attr("id", "sched-stages")
            .child(toggle(
                t.t("ui_sched_sync"),
                t.t("ui_sched_sync_title"),
                "sched-sync",
                &m.sched_sync,
            ))
            .child(toggle(
                t.t("ui_sched_links"),
                t.t("ui_sched_links_title"),
                "sched-links",
                &m.sched_links,
            ))
            .child(toggle(
                t.t("ui_sched_download"),
                t.t("ui_sched_download_title"),
                "sched-download",
                &m.sched_download,
            ))
            .child(toggle(
                t.t("ui_sched_convert"),
                t.t("ui_sched_convert_title"),
                "sched-convert",
                &m.sched_convert,
            ))
            .child(toggle(
                t.t("ui_sched_suggestions"),
                t.t("ui_sched_suggestions_title"),
                "sched-suggestions",
                &m.sched_suggestions,
            ));
        let options = div()
            .class("sb-actions")
            .child(toggle(
                t.t("ui_sched_fresh"),
                t.t("ui_sched_fresh_title"),
                "sched-fresh",
                &m.sched_fresh,
            ))
            .child(toggle(
                t.t("ui_singles"),
                t.t("ui_singles_title"),
                "sched-singles",
                &m.sched_singles,
            ));
        let when = div()
            .class("sb-actions")
            .child(toggle(
                t.t("ui_sched_enabled"),
                t.t("ui_sched_enabled_title"),
                "sched-enabled",
                &m.sched_enabled,
            ))
            .child(
                label_for(
                    m.sched_time.name(),
                    &t.t("ui_sched_time"),
                    m.sched_time
                        .input()
                        .attr("type", "time")
                        .class("fui-input fui-input-sm"),
                )
                .class("sb-opt"),
            )
            .child(
                label_for(
                    m.sched_zone.name(),
                    &t.t("ui_sched_zone"),
                    m.sched_zone
                        .input()
                        .attr("placeholder", "Europe/Warsaw")
                        .attr("title", t.t("ui_sched_zone_title"))
                        .class("fui-input fui-input-sm"),
                )
                .class("sb-opt"),
            );
        let save = {
            let env = env.clone();
            button()
                .attr("type", "button")
                .attr("id", "sched-save")
                .class("fui-btn fui-btn-primary")
                .on_click(move || {
                    let m = &env.m;
                    let kept = m.schedule.get_untracked().unwrap_or_default();
                    (env.ask)(Request::SaveSchedule(Schedule {
                        enabled: m.sched_enabled.get_untracked(),
                        time: m.sched_time.value_untracked(),
                        zone: m.sched_zone.value_untracked(),
                        sync: m.sched_sync.get_untracked(),
                        links: m.sched_links.get_untracked(),
                        download: m.sched_download.get_untracked(),
                        convert: m.sched_convert.get_untracked(),
                        suggestions: m.sched_suggestions.get_untracked(),
                        fresh_ratings: m.sched_fresh.get_untracked(),
                        singles: m.sched_singles.get_untracked(),
                        ..kept
                    }));
                })
                .text(t.t("ui_sched_save"))
        };
        div()
            .attr("id", "page-schedule")
            .class("sb-page")
            .child(head(&env, "ui_sched_title", "ui_sched_intro"))
            .child(status)
            .child(
                article()
                    .class("sb-card sb-step")
                    .child(h3().text(t.t("ui_sched_when")))
                    .child(when)
                    .child(h3().text(t.t("ui_sched_stages")))
                    .child(stages)
                    .child(h3().text(t.t("ui_sched_options")))
                    .child(options)
                    .child(div().class("sb-actions").child(save).child(job_button(
                        &env,
                        "scheduled",
                        &t.t("ui_sched_run_now"),
                        "fui-btn-secondary",
                        Some(t.t("ui_sched_run_now_title")),
                    ))),
            )
            .into_view()
    })
}

pub(crate) fn activity(env: &Env) -> View {
    let env = env.clone();
    View::dynamic(move || {
        let status = {
            let env = env.clone();
            View::dynamic(move || {
                let job = env.m.job.get();
                let name = job.name.clone().unwrap_or_default();
                let text = if job.running {
                    env.t.tr("ui_status_running", &[("name", name.into())])
                } else if let Some(why) = job.error {
                    env.t.tr("ui_status_error", &[("why", why.into())])
                } else if job.name.is_some() {
                    env.t.tr("ui_status_finished", &[("name", name.into())])
                } else {
                    env.t.t("ui_idle")
                };
                h3().attr("id", "status").text(text).into_view()
            })
        };
        // One element that lives as long as the page, with the text changing inside it: a new element every poll would
        // start at the top each time. `data-follow` keeps it at its end while the job writes, until the reader scrolls up.
        let log = {
            let env = env.clone();
            pre()
                .attr("id", "log")
                .attr("data-follow", "")
                .class("sb-log")
                .text_fn(move || {
                    let log = env.m.log.get();
                    let job = env.m.job.get();
                    let mut text = if log.is_empty() && job.name.is_none() {
                        env.t.t("ui_no_job")
                    } else {
                        log
                    };
                    if let Some(why) = job.error {
                        text.push_str("\nERROR: ");
                        text.push_str(&why);
                    }
                    text
                })
        };
        div()
            .attr("id", "page-activity")
            .class("sb-page")
            .child(head(&env, "ui_activity_title", "ui_activity_intro"))
            .child(article().class("sb-card").child(status).child(log))
            .into_view()
    })
}

// ── Dialogs ──────────────────────────────────────────────────────────────────

pub(crate) fn dialogs(env: &Env) -> View {
    let env = env.clone();
    View::dynamic(move || match env.m.dialog.get() {
        None => View::Empty,
        Some(Dialog::Link(track)) => link_dialog(&env, &track),
        Some(Dialog::Blacklist(album)) => blacklist_dialog(&env, &album),
        Some(Dialog::Remove(name)) => remove_dialog(&env, &name),
        Some(Dialog::Category(artist)) => category_dialog(&env, &artist),
        Some(Dialog::RenameCategory(name)) => rename_category_dialog(&env, &name),
        Some(Dialog::Add) => add_dialog(&env),
    })
}

/// A dialog over the page: `body` and `actions` go in a form, so Enter sends it.
fn modal(
    env: &Env,
    id: &str,
    title: &str,
    body: Vec<Element>,
    actions: Vec<Element>,
    submit: impl Fn() + 'static,
) -> View {
    let close = {
        let m = env.m.clone();
        move || m.dialog.set(None)
    };
    let escape = close.clone();
    div()
        .attr("id", id.to_string())
        .class("fui-modal fui-open")
        .child(div().class("fui-modal-scrim").on_click(close))
        .child(
            div()
                .class("fui-modal-panel")
                .attr("role", "dialog")
                .attr("aria-modal", "true")
                .attr("aria-label", title.to_string())
                .trap_focus()
                .on_keydown(move |k| {
                    if k.key == "Escape" {
                        escape();
                    }
                })
                .child(
                    div()
                        .class("fui-modal-header")
                        .child(h3().text(title.to_string())),
                )
                .child(
                    form()
                        .class("sb-dialog-form")
                        .on_submit(submit)
                        .child(
                            div()
                                .class("fui-modal-body")
                                .children(body.into_iter().map(Element::into_view)),
                        )
                        .child(
                            div()
                                .class("fui-modal-footer")
                                .children(actions.into_iter().map(Element::into_view)),
                        ),
                ),
        )
        .into_view()
}

fn cancel_button(env: &Env) -> Element {
    let m = env.m.clone();
    button()
        .attr("type", "button")
        .attr("id", "dialog-cancel")
        .class("fui-btn fui-btn-secondary")
        .on_click(move || m.dialog.set(None))
        .text(env.t.t("ui_cancel"))
}

fn submit_button(text: String, danger: bool) -> Element {
    button()
        .attr("type", "submit")
        .attr("id", "dialog-ok")
        .class(if danger {
            "fui-btn fui-btn-danger"
        } else {
            "fui-btn fui-btn-primary"
        })
        .text(text)
}

fn field_row(id: &str, text: &str, hint: Option<String>, control: Element) -> Element {
    div()
        .class("fui-field")
        .child(
            label()
                .attr("for", id.to_string())
                .class("fui-label")
                .text(text.to_string()),
        )
        .child(control)
        .child(hint.map(|h| small().class("fui-hint").text(h)))
}

fn link_dialog(env: &Env, track: &Track) -> View {
    let t = &env.t;
    let url = Field::new("link-url", &track.url);
    let save = {
        let (env, url, id) = (env.clone(), url.clone(), track.id);
        move |clear: bool| {
            let text = if clear {
                String::new()
            } else {
                url.value_untracked().trim().to_string()
            };
            *env.m.pending.borrow_mut() = Pending::Link {
                cleared: text.is_empty(),
            };
            (env.ask)(Request::SaveLink {
                track_id: id,
                url: text,
            });
        }
    };
    let query = format!("{} {}", track.artists, track.title);
    let search = format!(
        "https://www.youtube.com/results?search_query={}",
        songbook_core::api::encode(&query)
    );
    let remove = {
        let save = save.clone();
        button()
            .attr("type", "button")
            .attr("id", "link-remove")
            .class("fui-btn fui-btn-tertiary")
            .on_click(move || save(true))
            .text(t.t("ui_link_remove"))
    };
    modal(
        env,
        "dialog-link",
        &t.t("ui_link_title"),
        vec![
            p().class("sb-sub")
                .text(format!("{} — {}", track.artists, track.title)),
            field_row(
                "link-url",
                &t.t("ui_link_url"),
                Some(t.t("ui_link_hint")),
                url.input()
                    .attr("type", "url")
                    .attr("autocomplete", "off")
                    .class("fui-input")
                    .focus_on_mount(),
            ),
            a().attr("id", "link-search")
                .attr("href", search)
                .attr("target", "_blank")
                .attr("rel", "noopener")
                .class("fui-btn fui-btn-link")
                .text(t.t("ui_link_search")),
        ],
        vec![
            remove,
            cancel_button(env),
            submit_button(t.t("ui_save"), false),
        ],
        move || save(false),
    )
}

fn blacklist_dialog(env: &Env, album: &Album) -> View {
    let t = &env.t;
    let go = {
        let (env, album) = (env.clone(), album.clone());
        move || {
            *env.m.pending.borrow_mut() = Pending::Blacklist {
                album: album.album.clone(),
            };
            (env.ask)(Request::Blacklist {
                artist: album.artist.clone(),
                album_id: album.id,
            });
        }
    };
    modal(
        env,
        "dialog-blacklist",
        &t.t("ui_bl_title"),
        vec![p().text(t.tr(
            "ui_bl_text",
            &[
                ("album", album.album.clone().into()),
                ("artist", album.artist.clone().into()),
            ],
        ))],
        vec![cancel_button(env), submit_button(t.t("ui_blacklist"), true)],
        go,
    )
}

fn remove_dialog(env: &Env, name: &str) -> View {
    let t = &env.t;
    let go = {
        let (env, name) = (env.clone(), name.to_string());
        move || {
            *env.m.pending.borrow_mut() = Pending::Remove { name: name.clone() };
            (env.ask)(Request::RemoveArtist(name.clone()));
        }
    };
    modal(
        env,
        "dialog-remove",
        &t.t("ui_rm_title"),
        vec![p().text(t.tr("ui_rm_text", &[("name", name.to_string().into())]))],
        vec![cancel_button(env), submit_button(t.t("ui_remove"), true)],
        go,
    )
}

/// `(value, text)` for every category an artist can be put in; the empty value is none.
fn category_options(env: &Env) -> Vec<(String, String)> {
    let mut options = Vec::new();
    if let Some(s) = env.m.state.get_untracked() {
        options.push((String::new(), s.uncategorized.clone()));
        options.extend(s.categories.iter().map(|c| (c.clone(), c.clone())));
    }
    options
}

fn category_dialog(env: &Env, artist: &ArtistRow) -> View {
    let t = &env.t;
    let uncategorized = env
        .m
        .state
        .get_untracked()
        .map(|s| s.uncategorized)
        .unwrap_or_default();
    let current = if artist.category == uncategorized {
        String::new()
    } else {
        artist.category.clone()
    };
    let pick = Field::new("category-pick", &current);
    let fresh = Field::new("category-new", "");
    let genres = Field::new("artist-genres", &artist.genres);
    let go = {
        let (env, pick, fresh, genres, name, had) = (
            env.clone(),
            pick.clone(),
            fresh.clone(),
            genres.clone(),
            artist.name.clone(),
            artist.genres.clone(),
        );
        move || {
            let wanted = genres.value_untracked().trim().to_string();
            if wanted != had {
                (env.ask)(Request::SetGenres {
                    name: name.clone(),
                    genres: wanted,
                });
            }
            let new = fresh.value_untracked().trim().to_string();
            if new.is_empty() {
                (env.ask)(Request::SetCategory {
                    name: name.clone(),
                    category: pick.value_untracked(),
                });
            } else {
                *env.m.pending.borrow_mut() = Pending::NewCategoryThen {
                    name: name.clone(),
                    category: new.clone(),
                };
                (env.ask)(Request::AddCategory(new));
            }
        }
    };
    modal(
        env,
        "dialog-category",
        &t.t("ui_cat_title"),
        vec![
            p().class("sb-sub").text(artist.name.clone()),
            field_row(
                "category-pick",
                &t.t("ui_cat_label"),
                None,
                choice(&pick, category_options(env)).class("fui-select"),
            ),
            field_row(
                "category-new",
                &t.t("ui_cat_new"),
                Some(t.t("ui_cat_new_hint")),
                fresh.input().attr("autocomplete", "off").class("fui-input"),
            ),
            field_row(
                "artist-genres",
                &t.t("ui_genres_label"),
                Some(t.t("ui_genres_hint")),
                genres
                    .input()
                    .attr("autocomplete", "off")
                    .attr("placeholder", "Rap/Hip Hop")
                    .class("fui-input"),
            ),
        ],
        vec![cancel_button(env), submit_button(t.t("ui_save"), false)],
        go,
    )
}

fn rename_category_dialog(env: &Env, name: &str) -> View {
    let t = &env.t;
    let field = Field::new("category-rename", name);
    let go = {
        let (env, field, name) = (env.clone(), field.clone(), name.to_string());
        move || {
            let new_name = field.value_untracked().trim().to_string();
            if new_name.is_empty() || new_name == name {
                env.m.dialog.set(None);
                return;
            }
            *env.m.pending.borrow_mut() = Pending::RenameCategory {
                from: name.clone(),
                to: new_name.clone(),
            };
            (env.ask)(Request::RenameCategory {
                name: name.clone(),
                new_name,
            });
        }
    };
    modal(
        env,
        "dialog-rename-category",
        &t.t("ui_catrename_title"),
        vec![field_row(
            "category-rename",
            &t.t("ui_catrename_label"),
            Some(t.t("ui_catrename_hint")),
            field.input().attr("autocomplete", "off").class("fui-input"),
        )],
        vec![cancel_button(env), submit_button(t.t("ui_save"), false)],
        go,
    )
}

fn add_dialog(env: &Env) -> View {
    let t = &env.t;
    let name = Field::new("artist-name", "");
    let id = Field::new("artist-id", "");
    let category = Field::new("artist-category", "");
    let go = {
        let (env, name, id, category) = (env.clone(), name.clone(), id.clone(), category.clone());
        move || {
            let typed = name.value_untracked().trim().to_string();
            if typed.is_empty() {
                return;
            }
            *env.m.pending.borrow_mut() = Pending::AddOne {
                name: typed.clone(),
            };
            (env.ask)(Request::AddArtists(vec![NewArtist {
                name: typed,
                id: id.value_untracked().trim().to_string(),
                category: category.value_untracked(),
            }]));
        }
    };
    modal(
        env,
        "dialog-add",
        &t.t("ui_add_title"),
        vec![
            field_row(
                "artist-name",
                &t.t("ui_add_name"),
                None,
                name.input()
                    .attr("required", "required")
                    .attr("autocomplete", "off")
                    .class("fui-input")
                    .focus_on_mount(),
            ),
            field_row(
                "artist-id",
                &t.t("ui_add_id"),
                Some(t.t("ui_add_id_hint")),
                id.input()
                    .attr("inputmode", "numeric")
                    .attr("autocomplete", "off")
                    .class("fui-input"),
            ),
            field_row(
                "artist-category",
                &t.t("ui_add_category"),
                Some(t.t("ui_add_category_hint")),
                choice(&category, category_options(env)).class("fui-select"),
            ),
        ],
        vec![cancel_button(env), submit_button(t.t("ui_add"), false)],
        go,
    )
}
