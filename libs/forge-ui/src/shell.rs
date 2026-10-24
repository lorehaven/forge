//! The page frame every service shares.
//!
//! Desktop: a fixed sidebar (brand, navigation) beside a scrolling column with a slim top bar
//! (language, theme, account). Below 1024px the sidebar becomes a drawer that the top bar's menu
//! button opens; below 640px paddings tighten and tables/modals adapt (see `layout.css`).
//! Pages that run without a session (gatehouse's login and friends) use [`UiShell::auth`].

use crate::components::{icon, icon_button, icon_link, t};
use crate::scripts::{MARK_LOCALE_CHOSEN, SERIALIZE_REFRESH, SHELL_JS, THEME_SYNC};
use quench_starter::http::routers::ui::{ui_asset_path, ui_path};
use quench_web::prelude::*;

pub const SUPPORTED_LOCALES: [&str; 5] = ["en-US", "pl-PL", "es-ES", "de-DE", "fr-FR"];

pub fn supported_locales() -> Vec<String> {
    SUPPORTED_LOCALES.iter().map(|s| s.to_string()).collect()
}

const FAVICON: &[u8] = include_bytes!("../assets/favicon.png");

const INTER_FONT: &str =
    "https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap";

#[derive(Clone, Debug)]
pub struct NavItem {
    /// Font Awesome solid icon name, without the `fa-` prefix.
    pub icon: String,
    pub label_key: String,
    /// Absolute path (use `ui_path`). The item whose href is the longest prefix of the URL is highlighted.
    pub href: String,
}

impl NavItem {
    pub fn new(icon: &str, label_key: &str, href: impl Into<String>) -> Self {
        Self {
            icon: icon.to_string(),
            label_key: label_key.to_string(),
            href: href.into(),
        }
    }
}

/// A link nested under a [`NavItem`], known only at request time (e.g. one per project).
#[derive(Clone, Debug)]
pub struct NavChild {
    /// Literal text, already localized.
    pub label: String,
    pub href: String,
    /// The page being shown belongs to this entry.
    pub active: bool,
}

impl NavChild {
    pub fn new(label: impl Into<String>, href: impl Into<String>, active: bool) -> Self {
        Self {
            label: label.into(),
            href: href.into(),
            active,
        }
    }
}

/// Per-request additions to the sidebar; the shell itself is built once.
#[derive(Clone, Debug, Default)]
pub struct Sidebar {
    /// Sub-lists keyed by the `href` of the [`NavItem`] they hang under.
    pub children: Vec<(String, Vec<NavChild>)>,
    /// Replaces the configured navigation entirely (a service whose sidebar is its own list).
    pub nav: Option<Element>,
}

impl Sidebar {
    pub fn under(mut self, parent_href: impl Into<String>, children: Vec<NavChild>) -> Self {
        self.children.push((parent_href.into(), children));
        self
    }

    pub fn replacing_nav(mut self, nav: Element) -> Self {
        self.nav = Some(nav);
        self
    }
}

#[derive(Clone, Debug, Default)]
pub struct NavSection {
    pub heading_key: Option<String>,
    pub items: Vec<NavItem>,
}

impl NavSection {
    pub fn new(items: Vec<NavItem>) -> Self {
        Self {
            heading_key: None,
            items,
        }
    }

    pub fn titled(heading_key: &str, items: Vec<NavItem>) -> Self {
        Self {
            heading_key: Some(heading_key.to_string()),
            items,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ShellConfig {
    /// Browser tab title.
    pub title: String,
    /// Stylesheet the service writes with [`crate::write_css`].
    pub css_file: String,
    /// Shown next to the brand mark and in the mobile top bar.
    pub brand_name: String,
    /// Font Awesome icon inside the brand mark.
    pub brand_icon: String,
    pub nav: Vec<NavSection>,
    /// Where "edit profile" leads; `None` leaves the account menu out.
    pub profile_href: Option<String>,
    /// The services dashboard (gatehouse). `None` leaves its icon out - gatehouse itself has no use for it.
    pub dashboard_href: Option<String>,
}

impl ShellConfig {
    pub fn new(brand_name: &str, css_file: &str, brand_icon: &str) -> Self {
        Self {
            title: brand_name.to_string(),
            css_file: css_file.to_string(),
            brand_name: brand_name.to_string(),
            brand_icon: brand_icon.to_string(),
            nav: Vec::new(),
            profile_href: None,
            dashboard_href: None,
        }
    }

    /// Adds the services-dashboard icon to the top bar, first in line.
    pub fn dashboard(mut self, href: impl Into<String>) -> Self {
        self.dashboard_href = Some(href.into());
        self
    }

    pub fn nav(mut self, sections: Vec<NavSection>) -> Self {
        self.nav = sections;
        self
    }

    pub fn profile(mut self, href: impl Into<String>) -> Self {
        self.profile_href = Some(href.into());
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub enum PageWidth {
    Normal,
    Wide,
    Fill,
}

#[derive(Clone, Debug)]
enum Frame {
    Sidebar,
    Auth,
}

/// Built once per service (typically in a `LazyLock`).
#[derive(Clone, Debug)]
pub struct UiShell {
    app: AppShell,
    cfg: ShellConfig,
    frame: Frame,
}

impl UiShell {
    /// The signed-in frame: sidebar + top bar + scrolling content.
    pub fn new(cfg: ShellConfig) -> Self {
        Self::build(cfg, Frame::Sidebar)
    }

    /// The signed-out frame: centred card under a slim bar with language and theme.
    pub fn auth(cfg: ShellConfig) -> Self {
        Self::build(cfg, Frame::Auth)
    }

    fn build(cfg: ShellConfig, frame: Frame) -> Self {
        let app = AppShellBuilder::new()
            .title(&cfg.title)
            .supported_locales(supported_locales())
            .default_theme(Theme::DefaultDark)
            .supported_themes(vec![Theme::DefaultDark, Theme::DefaultLight])
            .links(vec![
                Link::new("preconnect", "https://fonts.gstatic.com").attr("crossorigin", ""),
                Link::new("stylesheet", INTER_FONT),
                Link::new(
                    "stylesheet",
                    &ui_asset_path(&format!("/css/{}", cfg.css_file)),
                ),
            ])
            .scripts(vec![
                Script::inline(THEME_SYNC),
                Script::inline(MARK_LOCALE_CHOSEN),
                Script::inline(SERIALIZE_REFRESH),
                Script::inline(SHELL_JS),
            ])
            .with_nav(false)
            .with_header(false)
            .footer(div().attr("hidden", "true"))
            .resources_prefix(ui_path(""))
            .build();
        // quench-web writes its own favicon while building the shell; replace it with forge's.
        let _ = std::fs::write("dist/assets/favicon.png", FAVICON);
        Self { app, cfg, frame }
    }

    /// Full HTML document with `content` inside the page column.
    pub fn page(&self, content: Element) -> String {
        self.page_with(content, PageWidth::Normal)
    }

    /// Like [`page`](Self::page) but without the max-width cap - for tables, trees and boards.
    pub fn page_wide(&self, content: Element) -> String {
        self.page_with(content, PageWidth::Wide)
    }

    /// The content fills the whole area below the top bar, edge to edge, and scrolls its own panes -
    /// for chat-style screens.
    pub fn page_fill(&self, content: Element) -> String {
        self.page_with(content, PageWidth::Fill)
    }

    /// Like the other `page*` methods, with `sidebar` applied to this request's sidebar.
    pub fn page_in(&self, content: Element, width: PageWidth, sidebar: &Sidebar) -> String {
        self.page_with_sidebar(content, width, sidebar)
    }

    fn page_with(&self, content: Element, width: PageWidth) -> String {
        self.page_with_sidebar(content, width, &Sidebar::default())
    }

    fn page_with_sidebar(&self, content: Element, width: PageWidth, sidebar: &Sidebar) -> String {
        let framed = match self.frame {
            Frame::Sidebar => self.sidebar_frame(content, width, sidebar),
            Frame::Auth => self.auth_frame(content),
        };
        self.app.page(framed)
    }

    fn sidebar_frame(&self, content: Element, width: PageWidth, sidebar: &Sidebar) -> Element {
        let page = div().class("fui-page").class(match width {
            PageWidth::Normal => "",
            PageWidth::Wide => "fui-page-wide",
            PageWidth::Fill => "fui-page-fill",
        });
        div()
            .class("fui-app")
            .child(self.sidebar(sidebar))
            .child(div().class("fui-scrim").on_click("fuiCloseDrawer()"))
            .child(
                div().class("fui-main").child(self.topbar()).child(
                    element("main")
                        .class("fui-content")
                        .attr("id", "fui-content")
                        .child(page.child(content)),
                ),
            )
    }

    fn auth_frame(&self, content: Element) -> Element {
        div()
            .class("fui-auth")
            .child(
                div().class("fui-auth-bar").child(self.brand(None)).child(
                    div()
                        .class("fui-topbar-actions")
                        .child(locale_switch(Some(supported_locales()), None))
                        .child(theme_menu()),
                ),
            )
            .child(div().class("fui-auth-body").child(content))
    }

    fn brand(&self, href: Option<String>) -> Element {
        a().attr("href", href.unwrap_or_else(|| ui_path("/home")))
            .class("fui-brand")
            .child(
                span()
                    .class("fui-brand-mark")
                    .child(icon(&self.cfg.brand_icon)),
            )
            .child(span().class("fui-brand-name").text(&self.cfg.brand_name))
    }

    fn sidebar(&self, extra: &Sidebar) -> Element {
        let nav = extra.nav.clone().unwrap_or_else(|| self.nav(extra));

        element("aside")
            .class("fui-sidebar")
            .attr("id", "fui-sidebar")
            .child(
                div()
                    .class("fui-sidebar-head")
                    .child(self.brand(None))
                    .child(
                        icon_button("xmark", "ui_menu_close")
                            .class("fui-sidebar-close")
                            .on_click("fuiCloseDrawer()"),
                    ),
            )
            .child(nav)
    }

    fn nav(&self, extra: &Sidebar) -> Element {
        let mut nav = element("nav").class("fui-nav").attr("aria-label", "Main");
        for section in &self.cfg.nav {
            let mut group = div().class("fui-nav-section");
            if let Some(key) = &section.heading_key {
                group = group.child(div().class("fui-nav-heading").attr("data-i18n", key));
            }
            for item in &section.items {
                group = group.child(
                    a().attr("href", &item.href)
                        .class("fui-nav-item")
                        .child(icon(&item.icon))
                        .child(t(&item.label_key)),
                );
                let children = extra
                    .children
                    .iter()
                    .filter(|(parent, _)| *parent == item.href)
                    .flat_map(|(_, children)| children);
                let mut sub = div().class("fui-nav-sub");
                let mut any = false;
                for child in children {
                    any = true;
                    let mut link = a()
                        .attr("href", &child.href)
                        .class("fui-nav-subitem")
                        .text(&child.label);
                    if child.active {
                        link = link.attr("aria-current", "page");
                    }
                    sub = sub.child(link);
                }
                if any {
                    group = group.child(sub);
                }
            }
            nav = nav.child(group);
        }
        nav
    }

    fn topbar(&self) -> Element {
        header()
            .class("fui-topbar")
            .child(
                icon_button("bars", "ui_menu_open")
                    .class("fui-menu-button")
                    .attr("aria-controls", "fui-sidebar")
                    .on_click("fuiOpenDrawer()"),
            )
            .child(
                div()
                    .class("fui-topbar-nav")
                    .child_opt(
                        self.cfg.dashboard_href.as_deref().map(|href| {
                            icon_link("table-cells-large", href, "ui_dashboard_button")
                        }),
                    )
                    .child(icon_link("house", &ui_path("/home"), "ui_home_button")),
            )
            .child(
                div()
                    .class("fui-topbar-title fui-hide-desktop")
                    .text(&self.cfg.brand_name),
            )
            .child(div().class("fui-topbar-spacer"))
            .child(
                div()
                    .class("fui-topbar-actions")
                    .child(locale_switch(Some(supported_locales()), None))
                    .child(theme_menu())
                    .child_opt(
                        self.cfg
                            .profile_href
                            .as_deref()
                            .map(|profile| user_menu(profile, &ui_path("/logout"))),
                    ),
            )
    }
}

/// Palettes a person can pick, in menu order. Matches `PALETTES` in the theme script and `tokens.css`.
pub const PALETTES: [&str; 6] = ["green", "teal", "blue", "purple", "rose", "orange"];

/// Theme menu: light/dark and a colour palette, each remembered in a cookie. The active mode and
/// palette are highlighted by CSS from `html[data-fui-theme]` / `html[data-fui-palette]`.
pub fn theme_menu() -> Element {
    let trigger = element("summary")
        .class("fui-icon-btn")
        .attr("data-i18n-title", "ui_theme_toggle")
        .child(icon("palette"))
        .child(t("ui_theme_toggle").class("sr-only"));

    let mode = |value: &str, icon_name: &str, key: &str| {
        button()
            .attr("type", "button")
            .class("fui-theme-mode")
            .attr("data-mode", value)
            .on_click(format!("fuiSetTheme('{value}')"))
            .child(icon(icon_name))
            .child(t(key))
    };

    let mut palettes = div().class("fui-theme-palettes");
    for name in PALETTES {
        palettes = palettes.child(
            button()
                .attr("type", "button")
                .class("fui-swatch")
                .attr("data-palette", name)
                .attr("data-i18n-title", format!("ui_palette_{name}"))
                .on_click(format!("fuiSetPalette('{name}')"))
                .child(t(&format!("ui_palette_{name}")).class("sr-only")),
        );
    }

    element("details")
        .class("fui-dropdown")
        .child(trigger)
        .child(
            div()
                .class("fui-dropdown-menu fui-theme-menu")
                .child(
                    div()
                        .child(
                            div()
                                .class("fui-theme-label")
                                .attr("data-i18n", "ui_theme_mode"),
                        )
                        .child(
                            div()
                                .class("fui-theme-modes")
                                .child(mode("light", "sun", "ui_theme_light"))
                                .child(mode("dark", "moon", "ui_theme_dark")),
                        ),
                )
                .child(
                    div()
                        .child(
                            div()
                                .class("fui-theme-label")
                                .attr("data-i18n", "ui_theme_palette"),
                        )
                        .child(palettes),
                ),
        )
}

/// Account dropdown: avatar (or user icon) opening edit-profile and log-out.
///
/// The user's picture is served at `<profile_href>/avatar` (gatehouse's account page); it is loaded
/// hidden and swaps in for the icon only once it has actually loaded, so a user with no picture
/// (a 404) keeps the icon. The frame is static across users, which is why this is decided in the browser.
pub fn user_menu(profile_href: &str, logout_href: &str) -> Element {
    let avatar = element("img")
        .attr(
            "src",
            format!("{}/avatar", profile_href.trim_end_matches('/')),
        )
        .attr("alt", "")
        .attr("onload", "this.parentNode.classList.add('has-avatar')");

    let trigger = element("summary")
        .attr("class", "fui-avatar")
        .attr("data-i18n-title", "ui_user_menu")
        .child(icon("user"))
        .child(avatar)
        .child(t("ui_user_menu").class("sr-only"));

    element("details")
        .class("fui-dropdown")
        .child(trigger)
        .child(
            div()
                .class("fui-dropdown-menu")
                .child(menu_item(profile_href, "user-pen", "ui_profile_edit"))
                .child(menu_item(logout_href, "right-from-bracket", "ui_logout")),
        )
}

pub fn menu_item(href: &str, icon_name: &str, key: &str) -> Element {
    a().attr("href", href)
        .class("fui-dropdown-item")
        .child(icon(icon_name))
        .child(t(key))
}

/// Where "edit profile" leads from a service other than gatehouse: gatehouse owns
/// the account page, so the link is absolute, built from `GATEHOUSE_URL`.
pub fn gatehouse_profile_url() -> String {
    let base = quench_core::env::get_or("GATEHOUSE_URL", "");
    format!("{}/ui/account", base.trim_end_matches('/'))
}

/// The services dashboard: gatehouse's home page, absolute, built from `GATEHOUSE_URL`.
pub fn gatehouse_dashboard_url() -> String {
    let base = quench_core::env::get_or("GATEHOUSE_URL", "");
    format!("{}/ui/home", base.trim_end_matches('/'))
}
