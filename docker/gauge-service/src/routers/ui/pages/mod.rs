use crate::routers::ui::common::format;
use quench_web::prelude::*;

pub mod auth;
pub mod home;
pub mod host;

pub(super) fn register_routes() {
    auth::register_routes();
    home::register_routes();
    host::register_routes();
}

/// A label, a bar and its value. `None` shows an empty bar and a dash.
pub(crate) fn meter(label_key: &str, pct: Option<f64>) -> Element {
    let value = pct.unwrap_or(0.0).clamp(0.0, 100.0);
    div()
        .class("gg-meter")
        .child(span().attr("data-i18n", label_key.to_string()))
        .child(
            div()
                .class(format!("gg-bar {}", format::level(value)))
                .attr("role", "progressbar")
                .attr("aria-valuenow", format!("{value:.0}"))
                .child(span().attr("style", format!("width:{value:.1}%"))),
        )
        .child(element("b").text(pct.map_or_else(|| "—".to_string(), format::pct)))
}

/// Why a host is not answering, and how long ago it last did; `None` while it is up.
pub(crate) fn outage(host: &crate::domain::store::Host) -> Option<String> {
    let error = host.error.as_ref()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64());
    Some(host.last_ok.map_or_else(
        || error.clone(),
        |at| format!("{error} - last seen {}", format::ago(now - at)),
    ))
}

/// A coloured dot and a translated word.
pub(crate) fn state(up: bool) -> Element {
    let (class, key) = if up {
        ("gg-state gg-state-up", "ui_state_up")
    } else {
        ("gg-state gg-state-down", "ui_state_down")
    };
    span()
        .class(class)
        .child(span().class("gg-dot"))
        .child(span().attr("data-i18n", key))
}

/// Reloads the page while it is visible, so the numbers move without a script that re-renders them.
pub(crate) fn auto_refresh(seconds: u32) -> Element {
    script(format!(
        "setInterval(function(){{if(!document.hidden)location.reload();}},{});",
        seconds * 1000
    ))
}
