//! The top bar pieces every forge service repeats: a home icon on the left and
//! a user menu (profile, log out) on the right. Each service still assembles
//! its own `header()`, so titles and extra controls stay its own business.

use quench_starter::http::routers::ui::{ui_asset_path, ui_path};
use quench_web::prelude::*;

pub const SUPPORTED_LOCALES: [&str; 5] = ["en-US", "pl-PL", "es-ES", "de-DE", "fr-FR"];

pub fn supported_locales() -> Vec<String> {
    SUPPORTED_LOCALES.iter().map(|s| s.to_string()).collect()
}

/// Renders `rules` into `dist/assets/css/<file_name>`, where the shell's stylesheet link points.
pub fn write_css(file_name: &str, rules: &[CssRule]) {
    let css = rules
        .iter()
        .map(CssRule::render)
        .collect::<Vec<_>>()
        .join("\n");

    let _ = std::fs::create_dir_all("dist/assets/css");
    let _ = std::fs::write(format!("dist/assets/css/{file_name}"), css);
}

/// Set when someone picks a language in the switch. quench's own script writes
/// `qlocale` on first load (with the default), so that cookie alone can't say
/// whether anyone chose anything; this marker can.
pub const LOCALE_CHOSEN_COOKIE: &str = "qlocale_chosen";

// quench dispatches `localeChanged` only from the switch's `updateLocale`.
const MARK_LOCALE_CHOSEN: &str = "window.addEventListener('localeChanged',function(){document.cookie='qlocale_chosen=1; max-age=31536000; path=/; SameSite=Lax';});";

// quench's session watcher renews an expired session with a POST to /refresh. With
// several tabs open they all notice at about the same time, and a refresh token
// is single-use, so all but the first or second lose and land on the login page.
// Tabs on one origin take turns (Web Locks); whoever waited re-checks first,
// because the tab ahead of it has usually renewed the shared cookies already.
// Wraps quench's global `tryRefresh` (and reads its global `SESSION_STATUS_URL`);
// does nothing where either is missing.
const SERIALIZE_REFRESH: &str = "(function(){if(!navigator.locks||typeof tryRefresh!=='function'||typeof SESSION_STATUS_URL==='undefined')return;const original=tryRefresh;tryRefresh=function(){return navigator.locks.request('forge-session-refresh',async function(){try{const r=await fetch(SESSION_STATUS_URL,{credentials:'same-origin',headers:{'Accept':'application/json'},cache:'no-store'});if(r.ok){const s=await r.json();if(s&&s.authenticated===true)return true;}}catch(e){}return original();});};})();";

/// The shell every service builds: dark theme, no side nav, the service's own stylesheet.
/// `None` leaves the top panel out (login pages carry their own bar).
pub fn app_shell(title: &str, css_file: &str, header: Option<Element>) -> AppShell {
    let builder = AppShellBuilder::new()
        .title(title)
        .supported_locales(supported_locales())
        .default_theme(Theme::DefaultDark)
        .supported_themes(vec![Theme::DefaultDark])
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path(&format!("/css/{css_file}")),
        )])
        .scripts(vec![
            Script::inline(MARK_LOCALE_CHOSEN),
            Script::inline(SERIALIZE_REFRESH),
        ])
        .with_nav(false)
        .resources_prefix(ui_path(""));

    match header {
        Some(header) => builder.header(header),
        None => builder.with_header(false),
    }
    .build()
}

/// Which controls the bar carries. Left: `leading`, home icon, title. Right: locale switch, user menu.
#[derive(Default)]
pub struct TopBar {
    pub leading: Option<Element>,
    pub show_home: bool,
    pub show_locale_switch: bool,
    /// Profile link for the user menu; `None` shows no menu at all.
    pub user_menu_profile: Option<String>,
}

impl TopBar {
    pub fn build(self, title: Element) -> Element {
        header()
            .child(
                div()
                    .class("left-panel")
                    .child_opt(self.leading)
                    .child_opt(self.show_home.then(|| home_button(&ui_path("/home"))))
                    .child(title),
            )
            .child(
                div()
                    .class("right-panel")
                    .child_opt(
                        self.show_locale_switch
                            .then(|| locale_switch(Some(supported_locales()), None)),
                    )
                    .child_opt(
                        self.user_menu_profile
                            .map(|profile| user_menu(&profile, &ui_path("/logout"))),
                    ),
            )
    }
}

/// Where "edit profile" leads from a service other than gatehouse: gatehouse owns
/// the account page, so the link is absolute, built from `GATEHOUSE_URL`.
pub fn gatehouse_profile_url() -> String {
    let base = envmnt::get_or("GATEHOUSE_URL", "");
    format!("{}/ui/account", base.trim_end_matches('/'))
}

/// Icon-only link; the translated label is kept for screen readers.
pub fn home_button(href: &str) -> Element {
    a().attr("href", href)
        .class("topbar-home")
        .child(i().class("fa-solid").class("fa-house"))
        .child(span().class("sr-only").attr("data-i18n", "ui_home_button"))
}

/// `<details>` gives the pop-out for free; the script only closes it on an
/// outside click or Escape.
///
/// The trigger is the user icon in a circle. The user's picture is served at
/// `<profile_href>/avatar` (gatehouse's account page); it is loaded hidden and
/// swaps in for the icon only once it has actually loaded, so a user with no
/// picture (a 404) keeps the icon. The header is static across users, which is
/// why this is decided in the browser.
pub fn user_menu(profile_href: &str, logout_href: &str) -> Element {
    let avatar = element("img")
        .class("topbar-avatar")
        .attr(
            "src",
            format!("{}/avatar", profile_href.trim_end_matches('/')),
        )
        .attr("alt", "")
        .attr("onload", "this.parentNode.classList.add('has-avatar')");

    let trigger = element("summary")
        .class("topbar-user-trigger")
        .child(i().class("fa-solid").class("fa-user"))
        .child(avatar)
        .child(span().class("sr-only").attr("data-i18n", "ui_user_menu"));

    let list = div()
        .class("topbar-user-list")
        .child(menu_item(profile_href, "fa-user-pen", "ui_profile_edit"))
        .child(menu_item(logout_href, "fa-right-from-bracket", "ui_logout"));

    element("details")
        .class("topbar-user")
        .child(trigger)
        .child(list)
        .child(script(CLOSE_ON_OUTSIDE.to_string()).raw().defer())
}

fn menu_item(href: &str, icon: &str, key: &str) -> Element {
    a().attr("href", href)
        .class("topbar-user-item")
        .child(i().class("fa-solid").class(icon))
        .child(span().attr("data-i18n", key))
}

const CLOSE_ON_OUTSIDE: &str = "(function(){\
const close=function(except){document.querySelectorAll('details.topbar-user[open]').forEach(function(d){if(d!==except)d.removeAttribute('open');});};\
document.addEventListener('click',function(e){close(e.target.closest('details.topbar-user'));});\
document.addEventListener('keydown',function(e){if(e.key==='Escape')close(null);});\
})();";

pub fn css_rules() -> Vec<CssRule> {
    let icon_button = |selector: &str| {
        CssRule::new(selector)
            .property("display", "inline-flex")
            .property("align-items", "center")
            .property("justify-content", "center")
            .property("width", "2.4rem")
            .property("height", "2.4rem")
            .property("border-radius", "var(--q-shell-panel-radius)")
            .property("color", "var(--q-shell-text)")
            .property("cursor", "pointer")
            .property("list-style", "none")
            .property("font-size", "1.3rem")
            .property("text-decoration", "none")
            .property("transition", "color 0.3s ease, background-color 0.3s ease")
            .child(CssRule::new("&:hover").property("background-color", "var(--bs-gray-700)"))
            .child(CssRule::new("&::-webkit-details-marker").property("display", "none"))
    };

    vec![
        CssRule::new(".sr-only")
            .property("position", "absolute")
            .property("width", "1px")
            .property("height", "1px")
            .property("overflow", "hidden")
            .property("clip", "rect(0 0 0 0)")
            .property("white-space", "nowrap"),
        icon_button("a.topbar-home"),
        icon_button("summary.topbar-user-trigger")
            .property("border-radius", "50%")
            .property("border", "0.1rem solid var(--q-shell-text)")
            .property("overflow", "hidden")
            .property("box-sizing", "border-box")
            .child(
                CssRule::new("img.topbar-avatar")
                    .property("display", "none")
                    .property("width", "100%")
                    .property("height", "100%")
                    .property("object-fit", "cover")
                    .property("border-radius", "50%"),
            )
            .child(CssRule::new("&.has-avatar img.topbar-avatar").property("display", "block"))
            .child(CssRule::new("&.has-avatar i").property("display", "none")),
        CssRule::new(".topbar-user")
            .property("position", "relative")
            .child(
                CssRule::new(".topbar-user-list")
                    .property("position", "absolute")
                    .property("right", "0")
                    .property("top", "calc(100% + 0.4rem)")
                    .property("z-index", "20")
                    .property("min-width", "12rem")
                    .property("padding", "0.3rem")
                    .property("display", "flex")
                    .property("flex-direction", "column")
                    .property("background-color", "var(--q-shell-panel-bg-strong)")
                    .property("border", "var(--q-shell-panel-border)")
                    .property("border-radius", "var(--q-shell-panel-radius)")
                    .property("box-shadow", "var(--q-shell-panel-shadow)"),
            ),
        CssRule::new("a.topbar-user-item")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "0.6rem")
            .property("padding", "0.5rem 0.75rem")
            .property("border-radius", "var(--q-shell-panel-radius)")
            .property("color", "var(--q-shell-text)")
            .property("text-decoration", "none")
            .child(CssRule::new("&:hover").property("background-color", "var(--bs-gray-700)"))
            .child(
                CssRule::new("i")
                    .property("width", "1.2rem")
                    .property("text-align", "center"),
            ),
    ]
}
