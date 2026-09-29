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
use crate::ui::common::{SUPPORTED_LOCALES, UiPageKind, render_page, ui_path};
use crate::ui::locale::BrowserLocale;
use async_trait::async_trait;
use http::StatusCode;
use quench_auth::domain::auth::{Role, User};
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::domain::session::SessionDb;
use quench_auth::http::routers::ui::get_user_from_req;
use quench_db::prelude::Db;
use quench_http::prelude::{
    Form, FromRequest, HttpError, Inject, Multipart, Query, Request, Response, get, post,
};
use quench_web::prelude::*;
use serde::Deserialize;
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

    // A preferences table that cannot be read must not lock the person out of
    // the rest of their account page: show the defaults and log why.
    let subscriptions = match Preferences::new(&db).effective(&actor.sub).await {
        Ok(subscriptions) => subscriptions,
        Err(err) => {
            tracing::error!(
                "could not read notification preferences for {}: {err}",
                actor.sub
            );
            default_subscriptions()
        }
    };
    render_account_page_with(&user, &notice, &subscriptions)
}

/// The catalog with everyone's starting choices - for a person who has made none.
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
    Form(form): Form<HashMap<String, String>>,
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
            tracing::error!(
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
        .is_none_or(|tz| tz.parse::<chrono_tz::Tz>().is_ok());
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
    Form(form): Form<HashMap<String, String>>,
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

#[derive(Deserialize)]
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
            tracing::error!(
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
        tracing::error!(
            "failed to send the email-change confirmation for {}: {err}",
            user.username
        );
        return redirect("/account?err=ui_account_error_email_not_sent");
    }
    redirect("/account?ok=email_change_sent")
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
    Inject(mailer): Inject<Arc<dyn Sender>>,
    browser_locale: BrowserLocale,
) -> Response {
    let actor = match actor.or_redirect() {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    match realm::enable_mfa(&db, &actor.sub, &form.secret, &form.code).await {
        Ok(()) => {
            notices::notify_username(
                &**mailer,
                &db,
                &actor.sub,
                &Mail::MfaEnabled,
                browser_locale.0.as_deref(),
            )
            .await;
            redirect("/account?ok=mfa_enabled")
        }
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
        .map(|user| user.mfa_enabled)
        .unwrap_or(false);
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

// --- Rendering ---

pub fn render_account_page(user: &User, notice: &Notice) -> Response {
    render_account_page_with(user, notice, &default_subscriptions())
}

pub fn render_account_page_with(
    user: &User,
    notice: &Notice,
    subscriptions: &[(&'static Template, bool)],
) -> Response {
    let profile_form = form()
        .attr("method", "post")
        .attr("enctype", "multipart/form-data")
        .attr("action", ui_path("/account"))
        .child(labeled_text(
            "display_name",
            "ui_account_display_name",
            user.display_name.as_deref(),
        ))
        .child(avatar_rows(user.avatar_url.as_deref()))
        .child(labeled_text(
            "title",
            "ui_account_title_field",
            user.title.as_deref(),
        ))
        .child(labeled_select(
            "timezone",
            "ui_account_timezone",
            chrono_tz::TZ_VARIANTS.iter().map(|tz| tz.name()),
            user.timezone.as_deref(),
        ))
        .child(labeled_select(
            "preferred_locale",
            "ui_account_preferred_locale",
            SUPPORTED_LOCALES.iter().copied(),
            user.preferred_locale.as_deref(),
        ))
        .child(
            button()
                .attr("type", "submit")
                .attr("data-i18n", "ui_account_save"),
        );

    let profile_panel = div()
        .class("panel admin-panel")
        .child(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_account_profile_title"),
        )
        .child(div().class("meta-list").child(profile_form));

    let email_panel = email_panel(user);
    let notifications_panel = notifications_panel(user, subscriptions);

    let password_panel = div()
        .class("panel admin-panel")
        .child(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_account_password_title"),
        )
        .child(
            div().class("meta-list").child(
                form()
                    .attr("method", "post")
                    .attr("action", ui_path("/account/password"))
                    .child(password_row(
                        "current_password",
                        "ui_account_current_password",
                        "current-password",
                    ))
                    .child(password_row(
                        "new_password",
                        "ui_account_new_password",
                        "new-password",
                    ))
                    .child(password_row(
                        "confirm_password",
                        "ui_account_confirm_password",
                        "new-password",
                    ))
                    .child(
                        button()
                            .attr("type", "submit")
                            .attr("data-i18n", "ui_account_password_change"),
                    ),
            ),
        );

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
                .child_opt(notice_banner(notice))
                .child(profile_panel)
                .child(email_panel)
                .child(notifications_panel)
                .child(password_panel)
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
        .child_opt(qr_image(uri))
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
        .child(form_row(
            "code",
            "ui_account_mfa_code",
            element("input")
                .attr("type", "text")
                .attr("id", "code")
                .attr("name", "code")
                .attr("inputmode", "numeric")
                .attr("autocomplete", "one-time-code")
                .attr("autofocus", "autofocus")
                .attr("required", "required"),
        ))
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

/// Scannable copy of the provisioning URI; the text below it stays as the fallback.
fn qr_image(uri: &str) -> Option<Element> {
    let src = crate::mfa::provisioning_qr_data_uri(uri).ok()?;
    Some(
        div().class("mfa-qr").child(
            element("img")
                .attr("src", &src)
                .attr("alt", "QR code")
                .attr("width", "200")
                .attr("height", "200"),
        ),
    )
}

fn labeled_text(name: &str, key: &'static str, value: Option<&str>) -> Element {
    form_row(
        name,
        key,
        input()
            .attr("type", "text")
            .attr("id", name)
            .attr("name", name)
            .attr("value", value.unwrap_or_default()),
    )
}

/// A checkbox per kind of service notification, grouped by service.
fn notifications_panel(user: &User, subscriptions: &[(&'static Template, bool)]) -> Element {
    let mut form_el = form()
        .attr("method", "post")
        .attr("action", ui_path("/account/notifications"));

    let mut last_service = "";
    for (template, wanted) in subscriptions {
        if template.service != last_service {
            last_service = template.service;
            form_el = form_el.child(div().class("admin-section-title").attr(
                "data-i18n",
                format!("ui_notification_service_{}", template.service),
            ));
        }
        let field = format!("sub_{}", template.id);
        let mut checkbox = checkbox().attr("id", &field).attr("name", &field);
        if *wanted {
            checkbox = checkbox.attr("checked", "checked");
        }
        form_el = form_el.child(
            div().class("admin-matrix-action").child(checkbox).child(
                label()
                    .attr("for", &field)
                    .attr("data-i18n", template.label_key()),
            ),
        );
    }
    form_el = form_el
        .child(
            p().class("admin-hint")
                .attr("data-i18n", "ui_account_notifications_hint"),
        )
        .child(
            button()
                .attr("type", "submit")
                .attr("data-i18n", "ui_account_notifications_save"),
        );

    let mut body = div().class("meta-list");
    if crate::notices::notice_address(user).is_none() {
        body = body.child(
            p().class("admin-hint")
                .attr("data-i18n", "ui_account_notifications_no_address"),
        );
    }
    div()
        .class("panel admin-panel")
        .child(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_account_notifications_title"),
        )
        .child(body.child(form_el))
}

/// The address on file (and whether it is confirmed), and the form to change it.
fn email_panel(user: &User) -> Element {
    let current = match user
        .email
        .as_deref()
        .map(str::trim)
        .filter(|address| !address.is_empty())
    {
        Some(address) => {
            let status = if user.email_verified_at.is_some() {
                "ui_account_email_confirmed"
            } else {
                "ui_account_email_unconfirmed"
            };
            div()
                .class("admin-status-row")
                .child(span().attr("id", "current-email").text(address))
                .child(span().class("admin-hint").attr("data-i18n", status))
        }
        None => div().child(
            p().class("admin-hint")
                .attr("data-i18n", "ui_account_email_none"),
        ),
    };

    div()
        .class("panel admin-panel")
        .child(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_account_email_title"),
        )
        .child(
            div().class("meta-list").child(current).child(
                form()
                    .attr("method", "post")
                    .attr("action", ui_path("/account/email"))
                    .child(form_row(
                        "email",
                        "ui_account_email_new",
                        input()
                            .attr("type", "email")
                            .attr("id", "email")
                            .attr("name", "email")
                            .attr("autocomplete", "email")
                            .attr("required", "required"),
                    ))
                    .child(password_row(
                        "email_current_password",
                        "ui_account_current_password",
                        "current-password",
                    ))
                    .child(
                        p().class("admin-hint")
                            .attr("data-i18n", "ui_account_email_hint"),
                    )
                    .child(
                        button()
                            .attr("type", "submit")
                            .attr("data-i18n", "ui_account_email_submit"),
                    ),
            ),
        )
}

fn password_row(name: &str, key: &'static str, autocomplete: &str) -> Element {
    form_row(
        name,
        key,
        input()
            .attr("type", "password")
            .attr("id", name)
            .attr("name", name)
            .attr("autocomplete", autocomplete)
            .attr("required", "required"),
    )
}

/// A choice from a fixed list, blank meaning "not set". A stored value that is no
/// longer in the list stays selectable, so opening the page never silently drops it.
fn labeled_select<'a>(
    name: &str,
    key: &'static str,
    choices: impl Iterator<Item = &'a str>,
    current: Option<&str>,
) -> Element {
    let current = current.filter(|value| !value.is_empty());
    let mut choices: Vec<&str> = choices.collect();
    if let Some(value) = current
        && !choices.contains(&value)
    {
        choices.insert(0, value);
    }

    let mut control = select()
        .attr("id", name)
        .attr("name", name)
        .child(option().attr("value", "").text("-"));
    for choice in choices {
        let mut item = option().attr("value", choice).text(choice);
        if current == Some(choice) {
            item = item.attr("selected", "selected");
        }
        control = control.child(item);
    }
    form_row(name, key, control)
}

/// Current picture, a URL field (never prefilled with an uploaded image's bytes),
/// the file picker, and a way to clear it.
fn avatar_rows(current: Option<&str>) -> Element {
    let current = current.filter(|value| !value.is_empty());
    let pasted_url = current.filter(|value| !value.starts_with("data:"));

    div()
        .child_opt(current.map(|_| {
            div().class("form-row").child(
                element("img")
                    .class("account-avatar")
                    .attr("src", ui_path("/account/avatar"))
                    .attr("alt", "")
                    .attr("width", "64")
                    .attr("height", "64"),
            )
        }))
        .child(labeled_text(
            "avatar_url",
            "ui_account_avatar_url",
            pasted_url,
        ))
        .child(form_row(
            "avatar_file",
            "ui_account_avatar_file",
            file_picker("avatar_file", "ui_account_avatar_choose"),
        ))
        .child(
            p().class("admin-hint")
                .attr("data-i18n", "ui_account_avatar_hint"),
        )
        .child_opt(current.map(|_| {
            form_row(
                "avatar_remove",
                "ui_account_avatar_remove",
                element("input")
                    .attr("type", "checkbox")
                    .attr("id", "avatar_remove")
                    .attr("name", "avatar_remove")
                    .attr("value", "1"),
            )
        }))
}

/// The native file input is visually hidden: a button (its label) on the left and a
/// read-only box with the chosen file's name on the right stand in for it.
fn file_picker(name: &str, button_key: &'static str) -> Element {
    div()
        .class("file-picker")
        .child(
            element("input")
                .class("file-picker-native")
                .attr("type", "file")
                .attr("id", name)
                .attr("name", name)
                .attr("accept", "image/png,image/jpeg,image/gif,image/webp")
                .attr(
                    "onchange",
                    "this.parentNode.querySelector('.file-picker-name').value=this.files.length?this.files[0].name:''",
                ),
        )
        .child(
            label()
                .class("file-picker-button")
                .attr("for", name)
                .child(i().class("fa-solid").class("fa-folder-open"))
                .child(span().attr("data-i18n", button_key)),
        )
        .child(
            element("input")
                .class("file-picker-name")
                .attr("type", "text")
                .attr("readonly", "readonly")
                .attr("tabindex", "-1"),
        )
}

/// Label left, control right - one shape for every field on the page.
fn form_row(name: &str, key: &'static str, control: Element) -> Element {
    div()
        .class("form-row")
        .child(label().attr("for", name).attr("data-i18n", key))
        .child(control)
}

pub fn notice_banner(notice: &Notice) -> Option<Element> {
    if let Some(key) = notice.err.as_deref().and_then(known_error_key) {
        return Some(p().class("admin-notice error").attr("data-i18n", key));
    }
    let key = match notice.ok.as_deref() {
        Some("saved") => "ui_account_ok_saved",
        Some("password_changed") => "ui_account_ok_password_changed",
        Some("mfa_enabled") => "ui_account_ok_mfa_enabled",
        Some("mfa_disabled") => "ui_account_ok_mfa_disabled",
        Some("email_change_sent") => "ui_account_ok_email_change_sent",
        Some("notifications_saved") => "ui_account_ok_notifications_saved",
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
        RealmError::CurrentPasswordInvalid,
        RealmError::EmailInvalid,
        RealmError::Internal,
    ]
    .iter()
    .map(RealmError::i18n_key)
    .chain([
        "ui_account_error_invalid",
        "ui_account_error_avatar_type",
        "ui_account_error_avatar_size",
        "ui_account_error_password_mismatch",
        "ui_account_error_email_same",
        "ui_account_error_email_not_sent",
        "ui_account_error_rate_limited",
    ])
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
    let _ = account_avatar as fn(_, _) -> _;
    let _ = change_password as fn(_, _, _, _, _, _, _) -> _;
    let _ = request_email_change as fn(_, _, _, _, _, _, _, _, _) -> _;
    let _ = save_notifications as fn(_, _, _) -> _;
    let _ = mfa_enroll_page as fn(_) -> _;
    let _ = mfa_enroll_submit as fn(_, _, _, _, _) -> _;
    let _ = mfa_disable as fn(_, _, _, _) -> _;
}
