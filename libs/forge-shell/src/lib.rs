//! The frame forge's browser clients share: the top bar and the style and script that go with it.
//!
//! The top bar has what the old server-rendered pages had: the services dashboard and the home button, the brand, a
//! language picker with flags, the theme menu (light or dark, a palette) and the account menu (edit profile, sign
//! out). Links to gatehouse are drawn with a path and filled in by [`SCRIPT`] from the address the page was told.

use std::rc::Rc;

use quench_ui::i18n::I18n;
use quench_ui::prelude::*;
use quench_ui::router::link;

/// The design tokens, the base rules and the components of forge's pages, then the top bar's own.
pub const STYLE: &str = concat!(
    include_str!("../assets/tokens.css"),
    "\n",
    include_str!("../assets/base.css"),
    "\n",
    include_str!("../assets/components.css"),
    "\n",
    include_str!("../assets/shell.css"),
);

/// The script every client page loads: theme, palette and language cookies, gatehouse links, dropdowns.
pub const SCRIPT: &str = include_str!("../assets/shell.js");

/// The palettes a person can pick, in menu order.
pub const PALETTES: [&str; 6] = ["green", "teal", "blue", "purple", "rose", "orange"];

/// What differs from one service to the next.
#[derive(Debug, Clone, Copy)]
pub struct Brand {
    /// Shown next to the mark.
    pub name: &'static str,
    /// The mark's icon: one of the `fui-ico-*` names (`eye`, `list-check`...).
    pub icon: &'static str,
}

/// An icon: a mask painted with the text colour.
#[must_use]
pub fn icon(name: &str) -> Element {
    i().class("fui-ico").class(format!("fui-ico-{name}"))
}

/// `pl-PL` as `🇵🇱`: the region's two letters as regional indicators. Nothing for a locale with no region.
#[must_use]
pub fn flag(locale: &str) -> String {
    let region = locale.split(['-', '_']).nth(1).unwrap_or_default();
    if region.len() != 2 || !region.chars().all(|c| c.is_ascii_alphabetic()) {
        return String::new();
    }
    region
        .to_ascii_uppercase()
        .chars()
        .filter_map(|c| char::from_u32(0x1F1E6 + (c as u32 - 'A' as u32)))
        .collect()
}

fn theme_menu(t: &I18n) -> Element {
    let mode = |value: &'static str, glyph: &str, key: &str| {
        button()
            .attr("type", "button")
            .class("fui-theme-mode")
            .attr("data-mode", value)
            .attr("data-fui-mode", value)
            .child(icon(glyph))
            .child(span().text(t.t(key)))
    };
    let mut swatches = div().class("fui-theme-palettes");
    for name in PALETTES {
        let label = t.t(&format!("ui_palette_{name}"));
        swatches = swatches.child(
            button()
                .attr("type", "button")
                .class("fui-swatch")
                .attr("data-palette", name)
                .attr("data-fui-palette", name)
                .attr("title", label.clone())
                .child(span().class("sr-only").text(label)),
        );
    }
    element("details")
        .class("fui-dropdown")
        .child(
            element("summary")
                .class("fui-icon-btn")
                .attr("title", t.t("ui_theme_toggle"))
                .child(icon("palette"))
                .child(span().class("sr-only").text(t.t("ui_theme_toggle"))),
        )
        .child(
            div()
                .class("fui-dropdown-menu fui-theme-menu")
                .child(
                    div()
                        .child(div().class("fui-theme-label").text(t.t("ui_theme_mode")))
                        .child(
                            div()
                                .class("fui-theme-modes")
                                .child(mode("light", "sun", "ui_theme_light"))
                                .child(mode("dark", "moon", "ui_theme_dark")),
                        ),
                )
                .child(
                    div()
                        .child(div().class("fui-theme-label").text(t.t("ui_theme_palette")))
                        .child(swatches),
                ),
        )
}

fn user_menu(t: &I18n, bar: &Bar) -> Element {
    let logout = bar.logout.as_str();
    let avatar = img()
        .attr("alt", "")
        .attr("data-gh-src", "/ui/account/avatar");
    element("details")
        .class("fui-dropdown")
        .child(
            element("summary")
                .class("fui-avatar")
                .attr("title", t.t("ui_user_menu"))
                .child(icon("user"))
                .child(avatar)
                .child(span().class("sr-only").text(t.t("ui_user_menu"))),
        )
        .child(
            div()
                .class("fui-dropdown-menu")
                .child(
                    a().attr("data-gh-href", "/ui/account")
                        .attr("href", "/gatehouse/ui/account")
                        .class("fui-dropdown-item")
                        .child(icon("user"))
                        .child(span().text(t.t("ui_profile_edit"))),
                )
                .child(match &bar.on_logout {
                    Some(out) => {
                        let out = Rc::clone(out);
                        button()
                            .attr("type", "button")
                            .attr("id", "sign-out")
                            .class("fui-dropdown-item")
                            .on_click(move || out())
                            .child(icon("logout"))
                            .child(span().text(t.t("ui_logout")))
                    }
                    None => a()
                        .attr("href", logout.to_string())
                        .class("fui-dropdown-item")
                        .child(icon("logout"))
                        .child(span().text(t.t("ui_logout"))),
                }),
        )
}

/// Where the bar's links go and what it offers.
#[derive(Clone)]
pub struct Bar {
    pub brand: Brand,
    /// The service's icon as the page serves it (`{base}/ui/assets/favicon.png`); the brand's glyph stands in without.
    pub image: Option<String>,
    /// The service's home screen.
    pub home: String,
    /// Whether `home` is a route of the client (a link inside the page) or a page of its own.
    pub spa: bool,
    /// Whether the account menu (edit profile, sign out) is there: a client that signs in by itself has none.
    pub account: bool,
    /// Whether the brand is in the top bar on wide screens too: with no side panel the bar is where it lives.
    pub brand_in_bar: bool,
    /// Where signing out starts (a full page load).
    pub logout: String,
    /// When the client signs out itself (it keeps its own session), what the menu's sign-out does instead of the link.
    pub on_logout: Option<Rc<dyn Fn()>>,
    /// Whether the services-dashboard button is there. Gatehouse is the dashboard, so its bar has none.
    pub dashboard: bool,
    /// The locales to pick from, the first is the fallback.
    pub languages: &'static [&'static str],
    /// Called with the locale the person picked.
    pub on_language: Rc<dyn Fn(&str)>,
}

fn mark(bar: &Bar) -> Element {
    bar.image.as_ref().map_or_else(
        || span().class("fui-brand-mark").child(icon(bar.brand.icon)),
        |src| {
            span()
                .class("fui-brand-mark fui-brand-image")
                .child(img().attr("src", src.clone()).attr("alt", ""))
        },
    )
}

/// The brand alone, as the head of a side panel the client draws itself (a client with a navigation of its own).
#[must_use]
pub fn brand_head(bar: &Bar) -> Element {
    div().class("fui-sidebar-head").child(
        div()
            .class("fui-brand")
            .child(mark(bar))
            .child(span().class("fui-brand-name").text(bar.brand.name)),
    )
}

/// The side panel: the service's icon and name, then whatever navigation the client puts under it.
#[must_use]
pub fn sidebar(bar: &Bar, nav: Vec<View>) -> Element {
    element("aside")
        .class("fui-sidebar")
        .child(
            div().class("fui-sidebar-head").child(
                div()
                    .class("fui-brand")
                    .child(mark(bar))
                    .child(span().class("fui-brand-name").text(bar.brand.name)),
            ),
        )
        .child(
            element("nav")
                .class("fui-nav")
                .attr("aria-label", "Main")
                .children(nav),
        )
}

/// A link to a screen of the client, or to a page, as the bar is told.
fn anchor(bar: &Bar, href: &str) -> Element {
    if bar.spa {
        link(href)
    } else {
        a().attr("href", href.to_string())
    }
}

/// The top bar, in the language `t` speaks now. Build it where the language is read, so it follows a change.
#[must_use]
pub fn topbar(bar: &Bar, t: &Rc<I18n>) -> Element {
    let pick = {
        let (t, on) = (Rc::clone(t), Rc::clone(&bar.on_language));
        select()
            .attr("id", "language")
            .attr("aria-label", t.t("locale_label"))
            .on_input(move |code| {
                t.set_language(code);
                on(code);
            })
    };
    let options = bar.languages.iter().map(|code| {
        let t = Rc::clone(t);
        option()
            .attr("value", *code)
            .attr_bool("selected", move || t.language() == *code)
            .text(format!("{} {code}", flag(code)).trim().to_string())
            .into_view()
    });
    header()
        .class("fui-topbar")
        .child(
            div()
                .class("fui-topbar-nav")
                .child(bar.dashboard.then(|| {
                    a().attr("data-gh-href", "/ui/home")
                        .attr("href", "/gatehouse/ui/home")
                        .attr("id", "dashboard")
                        .class("fui-icon-btn")
                        .attr("title", t.t("ui_dashboard_button"))
                        .child(icon("grid"))
                        .child(span().class("sr-only").text(t.t("ui_dashboard_button")))
                }))
                .child(
                    anchor(bar, &bar.home)
                        .attr("id", "home")
                        .class("fui-icon-btn")
                        .attr("title", t.t("ui_home_button"))
                        .child(icon("house"))
                        .child(span().class("sr-only").text(t.t("ui_home_button"))),
                ),
        )
        .child(
            anchor(bar, &bar.home)
                .attr("id", "title")
                .class(if bar.brand_in_bar {
                    "fui-brand"
                } else {
                    "fui-brand fui-hide-desktop"
                })
                .child(mark(bar))
                .child(span().class("fui-brand-name").text(bar.brand.name)),
        )
        .child(div().class("fui-topbar-spacer"))
        .child(
            div()
                .class("fui-topbar-actions")
                .child(
                    (!bar.languages.is_empty())
                        .then(|| div().class("locale-switch").child(pick.children(options))),
                )
                .child(theme_menu(t))
                .child(bar.account.then(|| user_menu(t, bar))),
        )
}
