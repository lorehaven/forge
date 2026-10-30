//! Self-service "My Account": any signed-in user edits their own profile,
//! password, and MFA. Not `admin.rs`, which gates on catalog admin actions.

use crate::PublicBase;
use crate::avatar::{self, AvatarError};
use crate::catalog::PermissionCatalog;
use crate::email::{Mail, Recipient, Sender};
use crate::email_change::{self, Ticket};
use crate::notices;
use crate::notify::{Preferences, Template, catalog as notification_catalog};
use crate::ratelimit::{ClientIp, RateLimiter, policy};
use crate::realm::{self, RealmError, UserChanges};
use crate::tokens::VerificationTokens;
use crate::ui::common::{FormFields, SUPPORTED_LOCALES, ui_path};
use crate::ui::locale::BrowserLocale;
use quench_auth::domain::auth::Role;
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::domain::session::SessionDb;
use quench_auth::http::routers::ui::get_user_from_req;
use quench_core::async_trait;
use quench_db::prelude::Db;
use quench_http::http::StatusCode;
use quench_http::prelude::{
    Form, FromRequest, HttpError, Inject, Multipart, Request, Response, get, post,
};
use quench_serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

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
        get_user_from_req(req, &config).await.map_or_else(
            || Ok(Self::Redirect(super::auth::login_redirect())),
            |claims| Ok(Self::Claims(claims)),
        )
    }
}

/// The account page: the client draws it from `GET /api/v1/ui/account`; every write below answers with a redirect back
/// to `/ui/account?ok=...` or `?err=...`, which the page shows.
#[get("/ui/account")]
pub async fn account_page(actor: Actor) -> Response {
    match actor.or_redirect() {
        Ok(_) => super::client::signed_in(),
        Err(response) => response,
    }
}

/// The catalog with everyone's starting choices - for a person who has made none.
#[must_use]
pub fn default_subscriptions() -> Vec<(&'static Template, bool)> {
    notification_catalog::all()
        .iter()
        .map(|template| (template, template.default_on))
        .collect()
}

/// Which kinds of service notification this person wants. A checkbox per kind;
/// the account security mail (password, MFA, address) is not in this list
/// because it cannot be turned off.
#[post("/ui/account/notifications")]
pub async fn save_notifications(
    actor: Actor,
    Form(form): Form<FormFields>,
    Inject(db): Inject<Db>,
) -> Response {
    let actor = match actor.or_redirect() {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let preferences = Preferences::new(&db);
    for template in notification_catalog::all() {
        let wanted = form.contains_key(&format!("sub_{}", template.id));
        if let Err(err) = preferences.set(&actor.sub, template, wanted).await {
            quench_log::error!(
                "could not save the {} preference for {}: {err}",
                template.id,
                actor.sub
            );
            return redirect("/account?err=ui_admin_error_internal");
        }
    }
    redirect("/account?ok=notifications_saved")
}

/// Multipart, for the avatar file: text fields are gathered into the same flat
/// map `admin.rs::save_user` uses - a missing field means leave alone.
#[post("/ui/account")]
pub async fn save_account(
    actor: Actor,
    mut multipart: Multipart,
    Inject(catalog): Inject<PermissionCatalog>,
    Inject(db): Inject<Db>,
    Inject(sessions): Inject<SessionDb>,
) -> Response {
    let actor = match actor.or_redirect() {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    let mut form: HashMap<String, String> = HashMap::new();
    let mut avatar_file: Option<Vec<u8>> = None;
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(_) => return redirect("/account?err=ui_account_error_avatar_size"),
        };
        let name = field.name().unwrap_or_default().to_string();
        if name == "avatar_file" {
            match field.bytes().await {
                Ok(bytes) if !bytes.is_empty() => avatar_file = Some(bytes.to_vec()),
                Ok(_) => {}
                Err(_) => return redirect("/account?err=ui_account_error_avatar_size"),
            }
        } else if let Ok(value) = field.text().await {
            form.insert(name, value);
        }
    }

    let avatar_url = match resolve_avatar(&form, avatar_file.as_deref()) {
        Ok(avatar_url) => avatar_url,
        Err(key) => return redirect(&format!("/account?err={key}")),
    };
    let timezone = non_empty(&form, "timezone");
    let preferred_locale = non_empty(&form, "preferred_locale");
    let timezone_ok = timezone
        .as_deref()
        .is_none_or(|tz| quench_tz::Zone::named(tz).is_ok());
    let locale_ok = preferred_locale
        .as_deref()
        .is_none_or(|locale| SUPPORTED_LOCALES.contains(&locale));
    if !timezone_ok || !locale_ok {
        return redirect("/account?err=ui_account_error_invalid");
    }

    let changes = UserChanges {
        display_name: non_empty(&form, "display_name"),
        avatar_url,
        title: non_empty(&form, "title"),
        timezone,
        preferred_locale,
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

/// `Ok(None)` leaves the avatar alone; `Ok(Some(""))` clears it. An uploaded file
/// wins over a pasted URL, and "remove" wins over both.
fn resolve_avatar(
    form: &HashMap<String, String>,
    file: Option<&[u8]>,
) -> Result<Option<String>, &'static str> {
    if form.contains_key("avatar_remove") {
        return Ok(Some(String::new()));
    }
    if let Some(bytes) = file {
        return avatar::to_data_uri(bytes)
            .map(Some)
            .map_err(|err| match err {
                AvatarError::TooLarge => "ui_account_error_avatar_size",
                AvatarError::UnsupportedType => "ui_account_error_avatar_type",
            });
    }
    match non_empty(form, "avatar_url") {
        Some(url) if url.starts_with("https://") || url.starts_with("http://") => Ok(Some(url)),
        Some(_) => Err("ui_account_error_invalid"),
        None => Ok(None),
    }
}

/// The signed-in user's own picture, for the top bar of every service. 404 when
/// there is none, so the bar keeps its icon.
#[get("/ui/account/avatar")]
pub async fn account_avatar(actor: Actor, Inject(db): Inject<Db>) -> Response {
    let Ok(actor) = actor.or_redirect() else {
        return Response::not_found();
    };
    let Ok(user) = realm::get(&db, &actor.sub).await else {
        return Response::not_found();
    };
    let Some(url) = user.avatar_url.filter(|url| !url.is_empty()) else {
        return Response::not_found();
    };
    if url.starts_with("http://") || url.starts_with("https://") {
        return Response::new(StatusCode::FOUND).header("Location", url);
    }
    match avatar::from_data_uri(&url) {
        Some((mime, bytes)) => Response::from_bytes(StatusCode::OK, bytes.into())
            .header("Content-Type", mime)
            .header("Cache-Control", "private, no-cache")
            .header("X-Content-Type-Options", "nosniff"),
        None => Response::not_found(),
    }
}

fn non_empty(form: &HashMap<String, String>, key: &str) -> Option<String> {
    form.get(key)
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[post("/ui/account/password")]
pub async fn change_password(
    actor: Actor,
    Form(form): Form<FormFields>,
    Inject(catalog): Inject<PermissionCatalog>,
    Inject(db): Inject<Db>,
    Inject(sessions): Inject<SessionDb>,
    Inject(mailer): Inject<Arc<dyn Sender>>,
    browser_locale: BrowserLocale,
) -> Response {
    let actor = match actor.or_redirect() {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    let field = |key: &str| form.get(key).map(String::as_str).unwrap_or_default();
    if field("new_password") != field("confirm_password") {
        return redirect("/account?err=ui_account_error_password_mismatch");
    }

    match realm::change_password(
        &db,
        &catalog,
        &sessions,
        &actor.sub,
        field("current_password"),
        field("new_password"),
    )
    .await
    {
        Ok(user) => {
            notices::notify(
                &**mailer,
                &user,
                &Mail::PasswordChanged,
                browser_locale.0.as_deref(),
            )
            .await;
            redirect("/account?ok=password_changed")
        }
        Err(err) => redirect(&format!("/account?err={}", err.i18n_key())),
    }
}

#[derive(Debug, Deserialize)]
pub struct EmailChangeForm {
    pub email: String,
    pub email_current_password: String,
}

/// Step one of changing your address: prove it is you (current password), name
/// the new address, and get a link there. Nothing changes until the link is
/// followed - see `confirm_email`.
// One DI extractor per argument - the framework has no struct-of-extractors.
#[allow(clippy::too_many_arguments)]
#[post("/ui/account/email")]
pub async fn request_email_change(
    actor: Actor,
    Form(form): Form<EmailChangeForm>,
    Inject(db): Inject<Db>,
    Inject(tokens): Inject<VerificationTokens>,
    Inject(mailer): Inject<Arc<dyn Sender>>,
    Inject(base): Inject<PublicBase>,
    Inject(limiter): Inject<RateLimiter>,
    ip: ClientIp,
    browser_locale: BrowserLocale,
) -> Response {
    let actor = match actor.or_redirect() {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    let new_email = form.email.trim();
    if new_email.parse::<quench_mail::Address>().is_err() {
        return redirect(&format!(
            "/account?err={}",
            RealmError::EmailInvalid.i18n_key()
        ));
    }

    // Each request mails a stranger's address, so it is limited like the other
    // endpoints that do - before the password is even looked at.
    let verdict = limiter
        .check_all(&[
            ("email-change-user", &actor.sub, policy::EMAIL_CHANGE_USER),
            ("email-change-ip", &ip.0, policy::EMAIL_CHANGE_IP),
            (
                "email-change-address",
                new_email,
                policy::EMAIL_CHANGE_ADDRESS,
            ),
        ])
        .await;
    if !verdict.is_allowed() {
        return redirect("/account?err=ui_account_error_rate_limited");
    }

    let user = match realm::check_password(&db, &actor.sub, &form.email_current_password).await {
        Ok(user) => user,
        Err(err) => return redirect(&format!("/account?err={}", err.i18n_key())),
    };

    let already_theirs = user.email_verified_at.is_some()
        && user
            .email
            .as_deref()
            .is_some_and(|current| current.eq_ignore_ascii_case(new_email));
    if already_theirs {
        return redirect("/account?err=ui_account_error_email_same");
    }

    let ticket = Ticket {
        username: user.username.clone(),
        new_email: new_email.to_string(),
    };
    let link = match email_change::issue_link(&tokens, &base, &ticket).await {
        Ok(link) => link,
        Err(err) => {
            quench_log::error!(
                "could not issue an email-change token for {}: {err}",
                user.username
            );
            return redirect("/account?err=ui_account_error_email_not_sent");
        }
    };
    let recipient = Recipient {
        address: new_email,
        username: &user.username,
        locale: user
            .preferred_locale
            .as_deref()
            .or(browser_locale.0.as_deref()),
    };
    if let Err(err) = mailer
        .send(&recipient, &Mail::EmailChange { link: &link })
        .await
    {
        quench_log::error!(
            "failed to send the email-change confirmation for {}: {err}",
            user.username
        );
        return redirect("/account?err=ui_account_error_email_not_sent");
    }
    redirect("/account?ok=email_change_sent")
}

// --- MFA enrollment ---

/// The enrolment page: a fresh secret comes from `GET /api/v1/ui/mfa-enroll`, the code is checked by `POST` to the same.
#[get("/ui/account/mfa/enroll")]
pub async fn mfa_enroll_page(actor: Actor) -> Response {
    match actor.or_redirect() {
        Ok(_) => super::client::signed_in(),
        Err(response) => response,
    }
}

#[post("/ui/account/mfa/disable")]
pub async fn mfa_disable(
    actor: Actor,
    Inject(db): Inject<Db>,
    Inject(mailer): Inject<Arc<dyn Sender>>,
    browser_locale: BrowserLocale,
) -> Response {
    let actor = match actor.or_redirect() {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    // Only worth a notice if it was actually on.
    let was_on = realm::get(&db, &actor.sub)
        .await
        .is_ok_and(|user| user.mfa_enabled);
    match realm::disable_mfa(&db, &actor.sub).await {
        Ok(()) => {
            if was_on {
                notices::notify_username(
                    &**mailer,
                    &db,
                    &actor.sub,
                    &Mail::MfaDisabled,
                    browser_locale.0.as_deref(),
                )
                .await;
            }
            redirect("/account?ok=mfa_disabled")
        }
        Err(err) => redirect(&format!("/account?err={}", err.i18n_key())),
    }
}

fn redirect(path: &str) -> Response {
    Response::new(StatusCode::FOUND).header("Location", ui_path(path))
}

pub fn register_routes() {
    let _ = account_page as fn(_) -> _;
    let _ = save_account as fn(_, _, _, _, _) -> _;
    let _ = account_avatar as fn(_, _) -> _;
    let _ = change_password as fn(_, _, _, _, _, _, _) -> _;
    let _ = request_email_change as fn(_, _, _, _, _, _, _, _, _) -> _;
    let _ = save_notifications as fn(_, _, _) -> _;
    let _ = mfa_enroll_page as fn(_) -> _;
    let _ = mfa_disable as fn(_, _, _, _) -> _;
}

impl std::fmt::Debug for Actor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Actor { .. }")
    }
}
