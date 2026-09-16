//! Self-service "My Account": any signed-in user edits their own profile,
//! password, and MFA. Not `admin.rs`, which gates on catalog admin actions.

use crate::catalog::PermissionCatalog;
use crate::realm::{self, RealmError, UserChanges};
use crate::ui::common::{UiPageKind, render_page, ui_path};
use async_trait::async_trait;
use http::StatusCode;
use quench_auth::domain::auth::{Role, User};
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::domain::session::SessionDb;
use quench_auth::http::routers::ui::get_user_from_req;
use quench_db::prelude::Db;
use quench_http::prelude::{
    Form, FromRequest, HttpError, Inject, Query, Request, Response, get, post,
};
use quench_web::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;

/// Claims, or the login redirect - not `HttpError`, which can't carry a redirect.
pub enum Actor {
    Claims(Claims),
    Redirect(Response),
}

impl Actor {
    fn or_redirect(self) -> Result<Claims, Response> {
        match self {
            Self::Claims(claims) => Ok(claims),
            Self::Redirect(resp) => Err(resp),
        }
    }
}

#[async_trait]
impl FromRequest for Actor {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let Ok(config) = req.container().get::<JwtConfig>() else {
            return Ok(Self::Redirect(super::auth::login_redirect()));
        };
        match get_user_from_req(req, &config).await {
            Some(claims) => Ok(Self::Claims(claims)),
            None => Ok(Self::Redirect(super::auth::login_redirect())),
        }
    }
}

/// Feedback carried across the redirect that follows every write here.
#[derive(Deserialize, Default)]
pub struct Notice {
    #[serde(default)]
    pub err: Option<String>,
    #[serde(default)]
    pub ok: Option<String>,
}

#[get("/ui/account")]
pub async fn account_page(
    actor: Actor,
    Query(notice): Query<Notice>,
    Inject(db): Inject<Db>,
) -> Response {
    let actor = match actor.or_redirect() {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    let user = match realm::get(&db, &actor.sub).await {
        Ok(user) => user,
        Err(err) => return error_page(&err),
    };

    render_account_page(&user, &notice)
}

/// Flat map like `admin.rs::save_user` - a missing field means leave alone.
#[post("/ui/account")]
pub async fn save_account(
    actor: Actor,
    Form(form): Form<HashMap<String, String>>,
    Inject(catalog): Inject<PermissionCatalog>,
    Inject(db): Inject<Db>,
    Inject(sessions): Inject<SessionDb>,
) -> Response {
    let actor = match actor.or_redirect() {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    let changes = UserChanges {
        password: form
            .get("password")
            .map(String::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        display_name: non_empty(&form, "display_name"),
        avatar_url: non_empty(&form, "avatar_url"),
        title: non_empty(&form, "title"),
        timezone: non_empty(&form, "timezone"),
        preferred_locale: non_empty(&form, "preferred_locale"),
        ..UserChanges::default()
    };

    let actor_is_admin = actor.has_role(Role::Admin.as_str());
    match realm::update(
        &db,
        &catalog,
        &sessions,
        &actor.sub,
        actor_is_admin,
        &actor.sub,
        changes,
    )
    .await
    {
        Ok(_) => redirect("/account?ok=saved"),
        Err(err) => redirect(&format!("/account?err={}", err.i18n_key())),
    }
}

fn non_empty(form: &HashMap<String, String>, key: &str) -> Option<String> {
    form.get(key)
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

// --- MFA enrollment ---

#[get("/ui/account/mfa/enroll")]
pub async fn mfa_enroll_page(actor: Actor) -> Response {
    let actor = match actor.or_redirect() {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    match realm::begin_mfa_enrollment(&actor.sub) {
        Ok((secret, uri)) => render_mfa_enroll_page(&secret, &uri, false),
        Err(err) => {
            tracing::error!("failed to begin MFA enrollment for {}: {err}", actor.sub);
            redirect("/account?err=ui_admin_error_internal")
        }
    }
}

#[derive(Deserialize)]
pub struct MfaEnrollForm {
    pub secret: String,
    pub code: String,
}

#[post("/ui/account/mfa/enroll")]
pub async fn mfa_enroll_submit(
    actor: Actor,
    Form(form): Form<MfaEnrollForm>,
    Inject(db): Inject<Db>,
) -> Response {
    let actor = match actor.or_redirect() {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    match realm::enable_mfa(&db, &actor.sub, &form.secret, &form.code).await {
        Ok(()) => redirect("/account?ok=mfa_enabled"),
        Err(RealmError::MfaCodeInvalid) => {
            // Re-rendered directly, not redirected - the secret never travels in a URL.
            let uri = crate::mfa::provisioning_uri(&form.secret, &actor.sub).unwrap_or_default();
            render_mfa_enroll_page(&form.secret, &uri, true)
        }
        Err(err) => {
            tracing::error!("failed to enable MFA for {}: {err:?}", actor.sub);
            redirect("/account?err=ui_admin_error_internal")
        }
    }
}

#[post("/ui/account/mfa/disable")]
pub async fn mfa_disable(actor: Actor, Inject(db): Inject<Db>) -> Response {
    let actor = match actor.or_redirect() {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    match realm::disable_mfa(&db, &actor.sub).await {
        Ok(()) => redirect("/account?ok=mfa_disabled"),
        Err(err) => redirect(&format!("/account?err={}", err.i18n_key())),
    }
}

// --- Rendering ---

pub fn render_account_page(user: &User, notice: &Notice) -> Response {
    let mut profile_form = form()
        .attr("method", "post")
        .attr("action", ui_path("/account"))
        .child(labeled_text(
            "display_name",
            "ui_account_display_name",
            user.display_name.as_deref(),
        ))
        .child(labeled_text(
            "avatar_url",
            "ui_account_avatar_url",
            user.avatar_url.as_deref(),
        ))
        .child(labeled_text(
            "title",
            "ui_account_title_field",
            user.title.as_deref(),
        ))
        .child(labeled_text(
            "timezone",
            "ui_account_timezone",
            user.timezone.as_deref(),
        ))
        .child(labeled_text(
            "preferred_locale",
            "ui_account_preferred_locale",
            user.preferred_locale.as_deref(),
        ))
        .child(
            label()
                .attr("for", "password")
                .attr("data-i18n", "ui_account_new_password"),
        )
        .child(
            input()
                .attr("type", "password")
                .attr("id", "password")
                .attr("name", "password")
                .attr("autocomplete", "new-password"),
        )
        .child(
            p().class("admin-hint")
                .attr("data-i18n", "ui_account_password_hint"),
        )
        .child(
            button()
                .attr("type", "submit")
                .attr("data-i18n", "ui_account_save"),
        );

    if let Some(banner) = notice_banner(notice) {
        profile_form = profile_form.child(banner);
    }

    let profile_panel = div()
        .class("panel admin-panel")
        .child(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_account_profile_title"),
        )
        .child(div().class("meta-list").child(profile_form));

    let mfa_panel = if user.mfa_enabled {
        div()
            .class("panel admin-panel")
            .child(
                div()
                    .class("panel-title")
                    .attr("data-i18n", "ui_account_mfa_title"),
            )
            .child(
                div()
                    .class("meta-list")
                    .child(
                        p().class("admin-hint")
                            .attr("data-i18n", "ui_account_mfa_enabled"),
                    )
                    .child(
                        form()
                            .attr("method", "post")
                            .attr("action", ui_path("/account/mfa/disable"))
                            .child(
                                button()
                                    .attr("type", "submit")
                                    .attr("data-i18n", "ui_account_mfa_disable"),
                            ),
                    ),
            )
    } else {
        div()
            .class("panel admin-panel")
            .child(
                div()
                    .class("panel-title")
                    .attr("data-i18n", "ui_account_mfa_title"),
            )
            .child(
                div()
                    .class("meta-list")
                    .child(
                        p().class("admin-hint")
                            .attr("data-i18n", "ui_account_mfa_disabled"),
                    )
                    .child(
                        a().class("button")
                            .attr("href", ui_path("/account/mfa/enroll"))
                            .attr("data-i18n", "ui_account_mfa_enable"),
                    ),
            )
    };

    render_page(
        StatusCode::OK,
        content().class("admin-content").child(
            div()
                .class("admin-container")
                .child(profile_panel)
                .child(mfa_panel),
        ),
        UiPageKind::Account,
    )
}

pub fn render_mfa_enroll_page(secret: &str, uri: &str, error: bool) -> Response {
    let mut enroll_form = form()
        .attr("method", "post")
        .attr("action", ui_path("/account/mfa/enroll"))
        .child(
            element("input")
                .attr("type", "hidden")
                .attr("name", "secret")
                .attr("value", secret),
        )
        .child(
            p().class("admin-hint")
                .attr("data-i18n", "ui_account_mfa_enroll_hint"),
        )
        .child(p().class("admin-mono").text(uri))
        .child(
            label()
                .attr("for", "mfa-secret")
                .attr("data-i18n", "ui_account_mfa_secret"),
        )
        .child(
            span()
                .attr("id", "mfa-secret")
                .class("admin-mono")
                .text(secret),
        )
        .child(
            label()
                .attr("for", "code")
                .attr("data-i18n", "ui_account_mfa_code"),
        )
        .child(
            element("input")
                .attr("type", "text")
                .attr("id", "code")
                .attr("name", "code")
                .attr("inputmode", "numeric")
                .attr("autocomplete", "one-time-code")
                .attr("autofocus", "autofocus")
                .attr("required", "required"),
        )
        .child(
            button()
                .attr("type", "submit")
                .attr("data-i18n", "ui_account_mfa_verify"),
        );

    if error {
        enroll_form = enroll_form.child(
            p().class("admin-notice error")
                .attr("data-i18n", "ui_admin_error_mfa_code_invalid"),
        );
    }

    render_page(
        StatusCode::OK,
        content().class("admin-content").child(
            div().class("admin-container").child(
                div()
                    .class("panel admin-panel")
                    .child(
                        div()
                            .class("panel-title")
                            .attr("data-i18n", "ui_account_mfa_enroll_title"),
                    )
                    .child(div().class("meta-list").child(enroll_form)),
            ),
        ),
        UiPageKind::Account,
    )
}

fn labeled_text(name: &str, key: &'static str, value: Option<&str>) -> Element {
    div()
        .child(label().attr("for", name).attr("data-i18n", key))
        .child(
            input()
                .attr("type", "text")
                .attr("id", name)
                .attr("name", name)
                .attr("value", value.unwrap_or_default()),
        )
}

pub fn notice_banner(notice: &Notice) -> Option<Element> {
    if let Some(key) = notice.err.as_deref().and_then(known_error_key) {
        return Some(p().class("admin-notice error").attr("data-i18n", key));
    }
    let key = match notice.ok.as_deref() {
        Some("saved") => "ui_account_ok_saved",
        Some("mfa_enabled") => "ui_account_ok_mfa_enabled",
        Some("mfa_disabled") => "ui_account_ok_mfa_disabled",
        _ => return None,
    };
    Some(p().class("admin-notice ok").attr("data-i18n", key))
}

/// Same allowlist reasoning as `admin.rs` - a hand-crafted `?err=` can't inject text.
pub fn known_error_key(candidate: &str) -> Option<&'static str> {
    [
        RealmError::PasswordEmpty,
        RealmError::NotFound,
        RealmError::MfaCodeInvalid,
        RealmError::Internal,
    ]
    .iter()
    .map(RealmError::i18n_key)
    .find(|known| *known == candidate)
}

fn redirect(path: &str) -> Response {
    Response::new(StatusCode::FOUND).header("Location", ui_path(path))
}

pub fn error_page(err: &RealmError) -> Response {
    render_page(
        err.status(),
        content().class("admin-content").child(
            div().class("admin-container").child(
                p().class("admin-notice error")
                    .attr("data-i18n", err.i18n_key()),
            ),
        ),
        UiPageKind::Account,
    )
}

pub fn register_routes() {
    let _ = account_page as fn(_, _, _) -> _;
    let _ = save_account as fn(_, _, _, _, _) -> _;
    let _ = mfa_enroll_page as fn(_) -> _;
    let _ = mfa_enroll_submit as fn(_, _, _) -> _;
    let _ = mfa_disable as fn(_, _) -> _;
}
