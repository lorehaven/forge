//! Self-service "My Account" - the one page a signed-in user (any role) can
//! reach to edit their own profile, change their password, and turn MFA on
//! or off. `admin.rs` is deliberately not reused for this even though the
//! two share `crate::realm`: that page is gated on the `gatehouse` catalog's
//! admin actions, and every signed-in user - not just those with
//! `edit-user` - needs to be able to manage their own account.

use crate::catalog::PermissionCatalog;
use crate::realm::{self, RealmError, UserChanges};
use crate::ui::common::{UiPageKind, render_page, ui_path};
use actix_web::{HttpRequest, HttpResponse, Responder, get, post, web};
use quench_auth::actix::routers::ui::get_user_from_req;
use quench_auth::prelude::{Claims, JwtConfig, Role, SessionDb, User};
use quench_db::prelude::Db;
use quench_web::prelude::*;
use serde::Deserialize;
use std::sync::Arc;

async fn actor_or_redirect(req: &HttpRequest, config: &JwtConfig) -> Result<Claims, HttpResponse> {
    get_user_from_req(req, config)
        .await
        .ok_or_else(super::auth::login_redirect)
}

/// Feedback carried across the redirect that follows every write here.
#[derive(Deserialize, Default)]
pub struct Notice {
    #[serde(default)]
    pub err: Option<String>,
    #[serde(default)]
    pub ok: Option<String>,
}

#[get("/account")]
pub(super) async fn account_page(
    req: HttpRequest,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    notice: web::Query<Notice>,
) -> impl Responder {
    let actor = match actor_or_redirect(&req, &config).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    let user = match realm::get(&db, &actor.sub).await {
        Ok(user) => user,
        Err(err) => return error_page(&err),
    };

    render_account_page(&user, &notice)
}

/// The profile-and-password form, read as a flat map for the same reason
/// `admin.rs::save_user` does: not every field is always present, and a
/// missing one means "leave alone", not "clear".
#[post("/account")]
pub(super) async fn save_account(
    req: HttpRequest,
    form: web::Form<std::collections::HashMap<String, String>>,
    config: web::Data<JwtConfig>,
    catalog: web::Data<PermissionCatalog>,
    db: web::Data<Db>,
    sessions: web::Data<Arc<SessionDb>>,
) -> impl Responder {
    let actor = match actor_or_redirect(&req, &config).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let form = form.into_inner();

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

fn non_empty(form: &std::collections::HashMap<String, String>, key: &str) -> Option<String> {
    form.get(key)
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

// ---------------------------------------------------------------------------
// MFA enrollment
// ---------------------------------------------------------------------------

#[get("/account/mfa/enroll")]
pub(super) async fn mfa_enroll_page(
    req: HttpRequest,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    let actor = match actor_or_redirect(&req, &config).await {
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

#[post("/account/mfa/enroll")]
pub(super) async fn mfa_enroll_submit(
    req: HttpRequest,
    form: web::Form<MfaEnrollForm>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    let actor = match actor_or_redirect(&req, &config).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    match realm::enable_mfa(&db, &actor.sub, &form.secret, &form.code).await {
        Ok(()) => redirect("/account?ok=mfa_enabled"),
        Err(RealmError::MfaCodeInvalid) => {
            // Re-rendered directly, not redirected: the not-yet-persisted
            // secret only ever travels in a POST body, never a URL, so a
            // failed attempt has to show the same secret again rather than
            // bounce through a GET that would have to carry it in the query
            // string instead.
            let uri = crate::mfa::provisioning_uri(&form.secret, &actor.sub).unwrap_or_default();
            render_mfa_enroll_page(&form.secret, &uri, true)
        }
        Err(err) => {
            tracing::error!("failed to enable MFA for {}: {err:?}", actor.sub);
            redirect("/account?err=ui_admin_error_internal")
        }
    }
}

#[post("/account/mfa/disable")]
pub(super) async fn mfa_disable(
    req: HttpRequest,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    let actor = match actor_or_redirect(&req, &config).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    match realm::disable_mfa(&db, &actor.sub).await {
        Ok(()) => redirect("/account?ok=mfa_disabled"),
        Err(err) => redirect(&format!("/account?err={}", err.i18n_key())),
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn render_account_page(user: &User, notice: &Notice) -> HttpResponse {
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
        HttpResponse::Ok(),
        content().class("admin-content").child(
            div()
                .class("admin-container")
                .child(profile_panel)
                .child(mfa_panel),
        ),
        UiPageKind::Account,
    )
}

fn render_mfa_enroll_page(secret: &str, uri: &str, error: bool) -> HttpResponse {
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
        HttpResponse::Ok(),
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

fn notice_banner(notice: &Notice) -> Option<Element> {
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

/// Same reasoning as `admin.rs`'s own allowlist: only a `RealmError::i18n_key`
/// that could actually reach this page is trusted onto it, so a hand-crafted
/// `?err=` cannot put arbitrary text on the page.
fn known_error_key(candidate: &str) -> Option<&'static str> {
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

fn redirect(path: &str) -> HttpResponse {
    HttpResponse::Found()
        .append_header(("Location", ui_path(path)))
        .finish()
}

fn error_page(err: &RealmError) -> HttpResponse {
    render_page(
        HttpResponse::build(err.status()),
        content().class("admin-content").child(
            div().class("admin-container").child(
                p().class("admin-notice error")
                    .attr("data-i18n", err.i18n_key()),
            ),
        ),
        UiPageKind::Account,
    )
}
