use actix_web::{HttpRequest, HttpResponse, Responder, get, http::header::ContentType, web};
use quench_auth::actix::routers::ui::get_user_from_req;
pub use quench_auth::prelude::Claims;
use quench_auth::prelude::JwtConfig;
pub use quench_starter::actix::routers::ui::{
    is_ui_authenticated, ui_asset_path, ui_login_redirect_for, ui_path,
};
use quench_web::prelude::*;
use serde::Deserialize;
use std::sync::LazyLock;

mod css;

const SUPPORTED_LOCALES: [&str; 5] = ["en-US", "pl-PL", "es-ES", "de-DE", "fr-FR"];

fn supported_locales() -> Vec<String> {
    SUPPORTED_LOCALES.iter().map(|s| s.to_string()).collect()
}

static UI_SHELL: LazyLock<AppShell> = LazyLock::new(|| {
    css::ensure_workbench_css();

    AppShellBuilder::new()
        .title("Workbench")
        .supported_locales(supported_locales())
        .default_theme(Theme::DefaultDark)
        .supported_themes(vec![Theme::DefaultDark])
        .header(ui_header())
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/workbench.css"),
        )])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
});

fn ui_header() -> Element {
    header()
        .child(
            div()
                .class("left-panel")
                .child(h2().attr("data-i18n", "header_label")),
        )
        .child(
            div()
                .class("right-panel")
                .child(locale_switch(Some(supported_locales()), None))
                .child(
                    a().attr("href", ui_path("/home"))
                        .class("button")
                        .attr("data-i18n", "ui_home_button"),
                )
                .child(
                    a().attr("href", ui_path("/logout"))
                        .class("button")
                        .attr("data-i18n", "ui_logout"),
                ),
        )
}

#[get("/assets/{path:.*}")]
pub async fn assets(path: web::Path<String>) -> impl Responder {
    quench_starter::actix::routers::ui::serve_assets(path, "dist/assets").await
}

pub fn render_page(mut builder: actix_web::HttpResponseBuilder, content: Element) -> HttpResponse {
    builder
        .content_type(ContentType::html())
        .body(UI_SHELL.page(div().class("page").child(content)))
}

pub fn ui_login_redirect() -> HttpResponse {
    quench_starter::actix::routers::ui::ui_login_redirect()
}

/// The signed-in identity behind a browser request - a bearer token if one is
/// already in the request's extensions (there never is, on the UI side, but
/// `get_user_from_req` checks both so API and UI code share one entry point),
/// otherwise the realm session cookie. `None` means "not signed in", not
/// "auth disabled" - `get_user_from_req` folds that into a synthetic
/// all-access `Claims` already.
pub async fn actor(request: &HttpRequest, config: &JwtConfig) -> Option<Claims> {
    get_user_from_req(request, config).await
}

/// What a redirect after a mutating form carries back to the page it returns
/// to - an error code or a success code, rendered as a plain (untranslated)
/// banner. These are operational messages about a form submission, not core
/// UI chrome, so unlike labels and buttons they are not run through i18n.
#[derive(Deserialize, Default)]
pub struct Notice {
    #[serde(default)]
    pub ok: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

pub fn notice_banner(notice: &Notice) -> Option<Element> {
    if let Some(code) = &notice.error {
        return Some(
            p().class("wb-notice wb-notice-error")
                .text(format!("error: {code}")),
        );
    }
    if let Some(code) = &notice.ok {
        return Some(p().class("wb-notice wb-notice-ok").text(code.clone()));
    }
    None
}

/// The assignee `<select>` plus its "assign to me" shortcut, shared by the
/// create-issue and issue-detail forms. "Unassigned" and "Me" come first -
/// before the (potentially long) alphabetical list of everyone else in the
/// realm - since those are the two choices actually made most often.
/// `current_user` is deliberately excluded from that alphabetical tail so it
/// never appears twice.
pub fn assignee_field(
    current_user: &str,
    users: &[crate::domain::realm_users::RealmUser],
    selected: Option<&str>,
) -> Element {
    let mut field = select().attr("id", "wb-assignee").attr("name", "assignee");

    field = field.child({
        let mut opt = option()
            .attr("value", "")
            .attr("data-i18n", "ui_field_unassigned");
        if selected.is_none_or(str::is_empty) {
            opt = opt.attr("selected", "true");
        }
        opt
    });

    field = field.child({
        let mut opt = option().attr("value", current_user).text("Me");
        if selected == Some(current_user) {
            opt = opt.attr("selected", "true");
        }
        opt
    });

    for user in users {
        if user.username == current_user {
            continue;
        }
        let mut opt = option()
            .attr("value", &user.username)
            .text(user.label().to_string());
        if selected == Some(user.username.as_str()) {
            opt = opt.attr("selected", "true");
        }
        field = field.child(opt);
    }

    div().class("wb-field-control").child(field).child(
        button()
            .attr("type", "button")
            .attr(
                "onclick",
                format!(
                    "document.getElementById('wb-assignee').value='{}'",
                    current_user.replace('\'', "\\'")
                ),
            )
            .class("wb-assign-me")
            .attr("data-i18n", "ui_assign_to_me"),
    )
}
