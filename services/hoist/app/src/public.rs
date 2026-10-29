//! The public catalogue (served by hoist-service: F-Droid, IzzyOnDroid and Aptoide merged), as a tab of its own.
//!
//! The home page suggests the best rated apps that are not installed, a few for each of the biggest categories;
//! a category's name opens the whole category; the search field above looks through everything. An app has a page
//! with an Install button. What the services answer arrives as messages ([`PublicState::on_categories`],
//! [`PublicState::on_page`], [`PublicState::on_failed`]); what it asks goes out through `requests`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use hoist_core::{
    Category, PublicApp, PublicPage, PublicQuery, format_bytes, home_categories, suggestions,
};
use quench_ui::prelude::*;

use crate::live::Request;

/// How many categories the home page shows, and how many apps under each.
const HOME_CATEGORIES: usize = 5;
const HOME_APPS: usize = 4;
/// How many apps the server is asked for to fill a section (some are installed already and are skipped).
const HOME_FETCH: i64 = 16;
/// The page size of a category or a search.
const PAGE: i64 = 30;
/// The fewest ratings an app needs to count as "top rated".
const MIN_VOTES: i64 = 20;

/// Which page of the tab is open.
#[derive(Debug, Clone, PartialEq)]
pub enum PublicView {
    Home,
    Category(String),
    Search(String),
    App(Box<PublicApp>),
}

/// What the tab knows and shows.
#[derive(Clone)]
pub struct PublicState {
    pub view: Signal<PublicView>,
    categories: Signal<Vec<Category>>,
    sections: Signal<Vec<(String, Vec<PublicApp>)>>,
    list: Signal<Vec<PublicApp>>,
    more: Signal<bool>,
    loading: Signal<bool>,
    failure: Signal<String>,
    /// The page that was asked for last for the open category or search: an answer for any other is not for it.
    current: Rc<RefCell<PublicQuery>>,
    /// Where each page was opened from, so back goes there.
    trail: Rc<RefCell<Vec<PublicView>>>,
    started: Rc<Cell<bool>>,
    /// Home sections still waiting for their answer.
    waiting: Rc<Cell<usize>>,
    typed: Signal<String>,
    timer: Rc<Cell<Option<quench_ui::platform::Timer>>>,
    requests: Rc<dyn Fn(Request)>,
    install: Rc<dyn Fn(PublicApp)>,
    open: Rc<dyn Fn(String)>,
    /// Whether a package is on the device (read when the page is made, so it follows the device's list).
    installed: Rc<dyn Fn(&str) -> bool>,
}

impl PublicState {
    #[must_use]
    pub fn new(
        requests: Rc<dyn Fn(Request)>,
        install: Rc<dyn Fn(PublicApp)>,
        open: Rc<dyn Fn(String)>,
        installed: Rc<dyn Fn(&str) -> bool>,
    ) -> Self {
        Self {
            view: create_signal(PublicView::Home),
            categories: create_signal(Vec::new()),
            sections: create_signal(Vec::new()),
            list: create_signal(Vec::new()),
            more: create_signal(false),
            loading: create_signal(false),
            failure: create_signal(String::new()),
            current: Rc::default(),
            trail: Rc::default(),
            started: Rc::default(),
            waiting: Rc::default(),
            typed: create_signal(String::new()),
            timer: Rc::default(),
            requests,
            install,
            open,
            installed,
        }
    }

    /// The first look at the tab asks for the categories (once; [`reload`](Self::reload) asks again).
    pub fn start(&self) {
        if !self.started.replace(true) {
            self.reload();
        }
    }

    /// Asks for the categories again (and with them the home page).
    pub fn reload(&self) {
        self.failure.set(String::new());
        self.loading.set(true);
        (self.requests)(Request::PublicCategories);
    }

    /// The query that ranks a category: the best rated first, among apps enough people rated; `fallback` for a
    /// category where nobody did, which is ordered by downloads.
    fn ranked(category: &str, offset: i64, limit: i64, fallback: bool) -> PublicQuery {
        PublicQuery {
            query: None,
            category: Some(category.to_string()),
            sort: if fallback { "downloads" } else { "rating" }.into(),
            min_votes: (!fallback).then_some(MIN_VOTES),
            offset,
            limit,
        }
    }

    /// Opens a category's page.
    pub fn open_category(&self, name: &str) {
        self.go(PublicView::Category(name.to_string()));
        self.fetch(Self::ranked(name, 0, PAGE, false));
    }

    /// Opens the results of a search.
    pub fn open_search(&self, text: &str) {
        let text = text.trim().to_string();
        if text.is_empty() {
            self.view.set(PublicView::Home);
            self.trail.borrow_mut().clear();
            return;
        }
        // A search from the results of another replaces them; it does not pile up pages to go back through.
        if !matches!(self.view.get_untracked(), PublicView::Search(_)) {
            self.go(PublicView::Search(text.clone()));
        } else {
            self.view.set(PublicView::Search(text.clone()));
        }
        self.fetch(PublicQuery {
            query: Some(text),
            category: None,
            sort: "downloads".into(),
            min_votes: None,
            offset: 0,
            limit: PAGE,
        });
    }

    /// Opens an app's page.
    pub fn open_app(&self, app: PublicApp) {
        self.go(PublicView::App(Box::new(app)));
    }

    fn go(&self, to: PublicView) {
        let from = self.view.get_untracked();
        self.trail.borrow_mut().push(from);
        self.view.set(to);
    }

    /// One step back (to the page this one was opened from).
    pub fn back(&self) {
        let to = self.trail.borrow_mut().pop().unwrap_or(PublicView::Home);
        self.view.set(to);
    }

    fn fetch(&self, query: PublicQuery) {
        self.list.set(Vec::new());
        self.more.set(false);
        self.failure.set(String::new());
        self.loading.set(true);
        *self.current.borrow_mut() = query.clone();
        (self.requests)(Request::PublicPage(query));
    }

    /// The next page of the open category or search.
    pub fn load_more(&self) {
        let mut query = self.current.borrow().clone();
        query.offset += i64::try_from(self.list.get_untracked().len()).unwrap_or(0);
        self.loading.set(true);
        *self.current.borrow_mut() = query.clone();
        (self.requests)(Request::PublicPage(query));
    }

    /// The server's categories came: the biggest few become the home page's sections, each asked for.
    pub fn on_categories(&self, all: Vec<Category>) {
        let shown = home_categories(&all, HOME_CATEGORIES);
        self.categories.set(all);
        self.sections.set(Vec::new());
        self.waiting.set(shown.len());
        if shown.is_empty() {
            self.loading.set(false);
        }
        for category in shown {
            (self.requests)(Request::PublicPage(Self::ranked(
                &category.name,
                0,
                HOME_FETCH,
                false,
            )));
        }
    }

    /// A page came: for a section of the home page, or for the open category or search.
    pub fn on_page(&self, asked: &PublicQuery, page: PublicPage) {
        let is_current = *self.current.borrow() == *asked;
        let home_section = asked.limit == HOME_FETCH && asked.query.is_none();
        if home_section {
            let name = asked.category.clone().unwrap_or_default();
            // Nobody rated anything there: show what people download most.
            if page.items.is_empty() && asked.min_votes.is_some() {
                (self.requests)(Request::PublicPage(Self::ranked(
                    &name, 0, HOME_FETCH, true,
                )));
                return;
            }
            self.sections.update(|s| {
                if !page.items.is_empty() {
                    s.push((name, page.items));
                }
            });
            let left = self.waiting.get().saturating_sub(1);
            self.waiting.set(left);
            if left == 0 {
                self.loading.set(false);
                // Keep the biggest category first, whatever order the answers came in.
                let order: Vec<String> = self
                    .categories
                    .get_untracked()
                    .into_iter()
                    .map(|c| c.name)
                    .collect();
                self.sections.update(|s| {
                    s.sort_by_key(|(n, _)| order.iter().position(|o| o == n).unwrap_or(usize::MAX));
                });
            }
            return;
        }
        if !is_current {
            return;
        }
        // The category where nobody rated anything: ordered by downloads instead.
        if asked.offset == 0 && page.items.is_empty() && asked.min_votes.is_some() {
            let name = asked.category.clone().unwrap_or_default();
            self.fetch(Self::ranked(&name, 0, PAGE, true));
            return;
        }
        self.more.set(page.has_more());
        self.list.update(|l| l.extend(page.items));
        self.loading.set(false);
    }

    /// The public catalogue could not be read.
    pub fn on_failed(&self, why: &str) {
        self.failure.set(why.to_string());
        self.loading.set(false);
    }

    /// What the bar says: the tab's name, or the open category or app.
    #[must_use]
    pub fn title(&self) -> String {
        match self.view.get() {
            PublicView::Home | PublicView::Search(_) => "Public".to_string(),
            PublicView::Category(name) => name,
            PublicView::App(app) => app.title().to_string(),
        }
    }
}

/// `4 apps · ★ 4.5 (120) · 1.2 MiB`: what a row says under the name.
fn meta(app: &PublicApp) -> String {
    let mut parts = Vec::new();
    let rating = app.rating_text();
    if !rating.is_empty() {
        parts.push(rating);
    }
    if app.size_bytes > 0 {
        parts.push(format_bytes(app.size_bytes));
    }
    if !app.version_name.is_empty() {
        parts.push(format!("v{}", app.version_name));
    }
    parts.join(" \u{b7} ")
}

fn row(app: PublicApp, state: &PublicState) -> View {
    let (open_state, app_for_open, install, open) = (
        state.clone(),
        app.clone(),
        Rc::clone(&state.install),
        Rc::clone(&state.open),
    );
    let have = (state.installed)(&app.package_name);
    let (button_label, title) = (
        if have { "Open" } else { "Install" },
        app.title().to_string(),
    );
    let (app_for_action, package) = (app.clone(), app.package_name.clone());
    li().class("hoist-row")
        .child(
            button()
                .attr("type", "button")
                .class("hoist-row-main")
                .attr("aria-label", format!("Details {title}"))
                .child(crate::icons::apps("hoist-row-icon"))
                .child(
                    div()
                        .class("hoist-row-text")
                        .child(span().class("hoist-row-title").text(title.clone()))
                        .child(
                            span().class("hoist-row-package").text(
                                app.summary
                                    .clone()
                                    .filter(|s| !s.is_empty())
                                    .unwrap_or_else(|| app.package_name.clone()),
                            ),
                        )
                        .child(span().class("hoist-row-meta").text(meta(&app))),
                )
                .on_click(move || open_state.open_app(app_for_open.clone())),
        )
        .child(
            button()
                .attr("type", "button")
                .class("hoist-action")
                .attr("aria-label", format!("{button_label} {title}"))
                .text(button_label)
                .on_click(move || {
                    if have {
                        open(package.clone());
                    } else {
                        install(app_for_action.clone());
                    }
                }),
        )
        .into_view()
}

fn rows(apps: Vec<PublicApp>, state: &PublicState) -> View {
    let state = state.clone();
    ul().class("hoist-list")
        .children(apps.into_iter().map(|a| row(a, &state)))
        .into_view()
}

fn section(name: &str, apps: Vec<PublicApp>, state: &PublicState) -> View {
    let (go, label) = (state.clone(), name.to_string());
    let id = format!("see-all-{}", name.to_lowercase().replace(' ', "-"));
    div()
        .class("hoist-section")
        .child(
            div()
                .class("hoist-section-head")
                .child(h3().text(name.to_string()))
                .child(
                    button()
                        .attr("type", "button")
                        .attr("id", id)
                        .attr("aria-label", format!("See all {name}"))
                        .class("hoist-link")
                        .text("See all")
                        .on_click(move || go.open_category(&label)),
                ),
        )
        .child(rows(apps, state))
        .into_view()
}

/// The page of one app of the catalogue.
fn detail(app: &PublicApp, state: &PublicState) -> View {
    let fact = |name: &str, value: String| {
        div()
            .class("hoist-fact")
            .child(span().class("hoist-fact-name").text(name.to_string()))
            .child(span().class("hoist-fact-value").text(value))
    };
    let have = (state.installed)(&app.package_name);
    let (install, open, shown) = (
        Rc::clone(&state.install),
        Rc::clone(&state.open),
        app.clone(),
    );
    let source = match app.install_source.as_str() {
        "fdroid" => "F-Droid",
        "izzyondroid" => "IzzyOnDroid",
        "aptoide" => "Aptoide",
        other => other,
    };
    let mut page = div()
        .attr("id", "public-detail")
        .child(h2().text(app.title().to_string()))
        .child(p().class("hoist-package").text(app.package_name.clone()));
    if let Some(summary) = app.summary.as_deref().filter(|s| !s.is_empty()) {
        page = page.child(p().text(summary.to_string()));
    }
    page = page
        .child(fact(
            "Version",
            format!("{} ({})", app.version_name, app.version_code),
        ))
        .child(fact("Size", format_bytes(app.size_bytes)))
        .child(fact("From", source.to_string()));
    if let Some(category) = app.category.as_deref().filter(|c| !c.is_empty()) {
        page = page.child(fact("Category", category.to_string()));
    }
    if !app.rating_text().is_empty() {
        page = page.child(fact("Rating", app.rating_text()));
    }
    if let Some(n) = app.downloads.filter(|n| *n > 0) {
        page = page.child(fact("Downloads", n.to_string()));
    }
    page.child(
        button()
            .attr("type", "button")
            .attr("id", "public-action")
            .class("hoist-primary")
            .text(if have { "Open" } else { "Install" })
            .on_click(move || {
                if have {
                    open(shown.package_name.clone());
                } else {
                    install(shown.clone());
                }
            }),
    )
    .child(
        p().attr("id", "public-note").class("hoist-note").text(
            if app.is_verifiable() {
                "The download is checked against the catalogue's checksum before it is installed."
            } else {
                "The catalogue has no checksum for this download, so it cannot be checked here. The system still checks the app's signature when it installs."
            },
        ),
    )
    .child(
        app.description
            .as_deref()
            .filter(|d| !d.is_empty())
            .map(|d| p().class("hoist-description").text(d.to_string())),
    )
    .into_view()
}

/// The tab: a search field, and under it the home page, a category, the results of a search or an app.
#[must_use]
pub fn public_screen(state: &PublicState) -> View {
    state.start();
    let (typed, schedule) = (state.typed.clone(), state.clone());
    let body = state.clone();
    div()
        .attr("id", "public")
        .class("hoist-screen")
        // Always there (the field keeps what was typed and its focus while the results change); an app's page hides it.
        .child({
            let view = state.view.clone();
            div()
                .attr_fn("class", move || {
                    if matches!(view.get(), PublicView::App(_)) {
                        "hoist-searchbar hoist-hidden".to_string()
                    } else {
                        "hoist-searchbar".to_string()
                    }
                })
                .child(crate::icons::search("hoist-search-icon"))
                .child(
                    input()
                        .attr("type", "text")
                        .attr("id", "public-search")
                        .attr("placeholder", "Search all apps")
                        .attr("aria-label", "Search all apps")
                        .class("hoist-search")
                        .on_input(move |v| {
                            typed.set(v.to_string());
                            if let Some(t) = schedule.timer.take() {
                                t.cancel();
                            }
                            let (text, go) = (v.to_string(), schedule.clone());
                            schedule
                                .timer
                                .set(Some(quench_ui::platform::after(450, move || {
                                    let wanted = text.trim();
                                    if wanted.is_empty() {
                                        go.view.set(PublicView::Home);
                                        go.trail.borrow_mut().clear();
                                    } else if wanted.chars().count() >= 2 {
                                        go.open_search(wanted);
                                    }
                                })));
                        }),
                )
        })
        .child(
            div()
                .class("hoist-scroll")
                .child(View::dynamic(move || page(&body))),
        )
        .into_view()
}

/// What is under the search field.
fn page(state: &PublicState) -> View {
    let failure = state.failure.get();
    if !failure.is_empty() {
        let retry = state.clone();
        return div()
            .child(p().attr("role", "alert").class("hoist-note").text(failure))
            .child(
                button()
                    .attr("type", "button")
                    .attr("id", "public-retry")
                    .text("Try again")
                    .on_click(move || {
                        // The open page again, or the home page's categories.
                        match retry.view.get_untracked() {
                            PublicView::Category(name) => retry.open_category_again(&name),
                            PublicView::Search(text) => retry.open_search(&text),
                            _ => retry.reload(),
                        }
                    }),
            )
            .into_view();
    }
    match state.view.get() {
        PublicView::App(app) => detail(&app, state),
        PublicView::Home => {
            let sections = state.sections.get();
            let shown: Vec<View> = sections
                .into_iter()
                .filter_map(|(name, apps)| {
                    let left = suggestions(&apps, &|p| (state.installed)(p), HOME_APPS);
                    (!left.is_empty()).then(|| section(&name, left, state))
                })
                .collect();
            if shown.is_empty() {
                return empty(state, "Nothing to suggest yet");
            }
            div()
                .attr("id", "public-home")
                .child(h2().text("Top rated for you"))
                .children(shown)
                .into_view()
        }
        PublicView::Category(_) | PublicView::Search(_) => {
            let apps = state.list.get();
            if apps.is_empty() {
                return empty(state, "No apps found");
            }
            let more = state.more.get();
            let loading = state.loading.get();
            let next = state.clone();
            div()
                .attr("id", "public-list")
                .child(rows(apps, state))
                .child(more.then(|| {
                    button()
                        .attr("type", "button")
                        .attr("id", "public-more")
                        .class("hoist-primary")
                        .text(if loading { "Loading..." } else { "Show more" })
                        .on_click(move || next.load_more())
                }))
                .into_view()
        }
    }
}

/// What a page says while it waits, and when there is nothing to show.
fn empty(state: &PublicState, nothing: &str) -> View {
    p().attr("id", "public-empty")
        .class("hoist-note")
        .text(if state.loading.get() {
            "Loading..."
        } else {
            nothing
        })
        .into_view()
}

impl PublicState {
    fn open_category_again(&self, name: &str) {
        self.fetch(Self::ranked(name, 0, PAGE, false));
    }
}
