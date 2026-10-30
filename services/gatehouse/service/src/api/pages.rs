//! What the browser pages draw from.
//!
//! The services dashboard, the account, the user administration, and what a link in an email stands for. Reads, plus the one write the client makes itself (enrolling an authenticator, whose secret must not
//! travel in a URL). Every other write is a form post to a `/ui/...` route that answers with a redirect.
//!
//! Behind the same session as the pages: the cookie or a bearer token. The administration checks the same catalog actions the
//! JSON user API does.

use crate::api::users::SubjectClaims;
use crate::catalog::PermissionCatalog;
use crate::email::{Mail, Sender};
use crate::email_change::Ticket;
use crate::notices;
use crate::notify::Preferences;
use crate::realm::{self, RealmError};
use crate::services::enabled_services;
use crate::tokens::{PURPOSE_EMAIL_CHANGE, VerificationTokens};
use crate::ui::common::SUPPORTED_LOCALES;
use crate::ui::locale::BrowserLocale;
use crate::ui::pages::confirm_email::ConfirmQuery;
use crate::ui::pages::unsubscribe::{TokenQuery, resolve};
use gatehouse_core::{
    Account, AdminUser, AdminUserRow, AdminUsers, CatalogService, ConfirmEmail, Dashboard, Grant,
    MfaEnrollRequest, MfaEnrollment, ServiceCard, Subscription, UnsubscribeInfo,
};
use quench_auth::domain::auth::{Actions, Role, User};
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_core::async_trait;
use quench_db::prelude::Db;
use quench_http::http::StatusCode;
use quench_http::prelude::{
    FromRequest, HttpError, Inject, Json, Path, Query, Request, Response, get, post,
};
use quench_serde::Serialize;
use std::sync::Arc;

/// Who is asking, as the pages themselves see it.
///
/// A verified session (cookie or bearer token), or - with `SERVICE_AUTH_ENABLED` off, the estate's dev switch - the same stand-in administrator the pages use (`admin`), so a local
/// run without sign-in shows the seeded account.
#[derive(Debug)]
pub struct PageClaims(pub Claims);

#[async_trait]
impl FromRequest for PageClaims {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let config = req
            .container()
            .get::<JwtConfig>()
            .map_err(|e| HttpError::status(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if !config.auth_enabled {
            return crate::ui::common::actor(req, &config)
                .await
                .map(Self)
                .ok_or_else(|| HttpError::status(StatusCode::UNAUTHORIZED, ""));
        }
        SubjectClaims::from_request(req)
            .await
            .map(|SubjectClaims(claims)| Self(claims))
    }
}

/// The claims, if they hold the catalog action `action`; else the `403` to send.
fn allowed(claims: Claims, action: &str) -> Result<Claims, Response> {
    if claims.can("gatehouse", action) {
        Ok(claims)
    } else {
        quench_log::warn!(
            "{} opened user administration without gatehouse:{action}",
            claims.sub
        );
        Err(json(
            StatusCode::FORBIDDEN,
            &quench_serde::json!({ "error": "ui_admin_forbidden" }),
        ))
    }
}

fn json<T: Serialize>(status: StatusCode, value: &T) -> Response {
    Response::json(status, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

fn ok<T: Serialize>(value: &T) -> Response {
    json(StatusCode::OK, value)
}

/// `{"error": "<translation key>"}` with the status that says why.
fn refusal(err: &RealmError) -> Response {
    json(
        err.status(),
        &quench_serde::json!({ "error": err.i18n_key() }),
    )
}

fn not_found() -> Response {
    json(
        StatusCode::NOT_FOUND,
        &quench_serde::json!({ "error": "ui_not_found" }),
    )
}

/// The cards, in the catalog's order, and whether the realm's own section is offered.
#[get("/api/v1/ui/dashboard")]
async fn dashboard(subject: PageClaims, Inject(catalog): Inject<PermissionCatalog>) -> Response {
    // Only the services this person may use: some grant on the service (a blanket one, or one scoped to a project...)
    // or a role that grants everything. A service they cannot read would only lead them to a refusal.
    let claims = &subject.0;
    let held = claims.permissions();
    let services = enabled_services(&catalog)
        .into_iter()
        .filter(|service| {
            service.public
                || claims.can(&service.name, "read")
                || held
                    .get(&service.name)
                    .is_some_and(|actions| !actions.is_empty())
        })
        .map(|service| ServiceCard {
            url: service.url,
            title_key: service.title_key,
            desc_key: service.desc_key,
            label: service.label,
            description: service.description,
            card_class: service.card_class,
        })
        .collect();
    ok(&Dashboard {
        admin: subject.0.has_role(Role::Admin.as_str()),
        services,
    })
}

/// The signed-in person's own profile, address, notification choices and MFA state.
#[get("/api/v1/ui/account")]
async fn account(subject: PageClaims, Inject(db): Inject<Db>) -> Response {
    let username = subject.0.sub;
    let user = match realm::get(&db, &username).await {
        Ok(user) => user,
        Err(err) => return refusal(&err),
    };

    // A preferences table that cannot be read must not lock the person out of
    // the rest of their account page: show the defaults and log why.
    let chosen = match Preferences::new(&db).effective(&username).await {
        Ok(chosen) => chosen,
        Err(err) => {
            quench_log::error!("could not read notification preferences for {username}: {err}");
            crate::ui::pages::account::default_subscriptions()
        }
    };
    let subscriptions = chosen
        .into_iter()
        .map(|(template, wanted)| Subscription {
            id: template.id.to_string(),
            service: template.service.to_string(),
            label_key: template.label_key(),
            wanted,
        })
        .collect();

    let avatar = user.avatar_url.as_deref().filter(|url| !url.is_empty());
    ok(&Account {
        has_notice_address: notices::notice_address(&user).is_some(),
        email_verified: user.email_verified_at.is_some(),
        username: user.username.clone(),
        display_name: user.display_name.clone(),
        // Never prefilled with an uploaded image's bytes.
        avatar_url: avatar
            .filter(|url| !url.starts_with("data:"))
            .map(str::to_string),
        has_avatar: avatar.is_some(),
        title: user.title.clone(),
        timezone: user.timezone.clone(),
        preferred_locale: user.preferred_locale.clone(),
        email: user.email.clone(),
        mfa_enabled: user.mfa_enabled,
        timezones: quench_tz::Zone::names().map(str::to_string).collect(),
        locales: SUPPORTED_LOCALES.iter().map(|l| (*l).to_string()).collect(),
        subscriptions,
    })
}

/// A fresh secret to enrol an authenticator with. Nothing is stored until the code is checked.
#[get("/api/v1/ui/mfa-enroll")]
async fn mfa_secret(subject: PageClaims) -> Response {
    match realm::begin_mfa_enrollment(&subject.0.sub) {
        Ok((secret, uri)) => {
            let qr = crate::mfa::provisioning_qr_data_uri(&uri).ok();
            ok(&MfaEnrollment { secret, uri, qr })
        }
        Err(err) => {
            quench_log::error!(
                "failed to begin MFA enrollment for {}: {err}",
                subject.0.sub
            );
            refusal(&RealmError::Internal)
        }
    }
}

/// Checks the first code and turns MFA on. A wrong code is a `400` the page can show next to the same secret.
#[post("/api/v1/ui/mfa-enroll")]
async fn mfa_enable(
    subject: PageClaims,
    Json(body): Json<MfaEnrollRequest>,
    Inject(db): Inject<Db>,
    Inject(mailer): Inject<Arc<dyn Sender>>,
    browser_locale: BrowserLocale,
) -> Response {
    let username = subject.0.sub;
    match realm::enable_mfa(&db, &username, &body.secret, &body.code).await {
        Ok(()) => {
            notices::notify_username(
                &**mailer,
                &db,
                &username,
                &Mail::MfaEnabled,
                browser_locale.0.as_deref(),
            )
            .await;
            ok(&quench_serde::json!({ "done": true }))
        }
        Err(RealmError::MfaCodeInvalid) => refusal(&RealmError::MfaCodeInvalid),
        Err(err) => {
            quench_log::error!("failed to enable MFA for {username}: {err:?}");
            refusal(&RealmError::Internal)
        }
    }
}

/// The most privileged role a user holds: what the list and the editor show.
fn primary_role(user: &User) -> Role {
    let roles = user.get_roles();
    for candidate in [Role::Admin, Role::Service] {
        if roles.contains(&candidate) {
            return candidate;
        }
    }
    Role::User
}

fn grants(permissions: &std::collections::BTreeMap<String, Actions>) -> Vec<Grant> {
    permissions
        .iter()
        .map(|(service, actions)| Grant {
            service: service.clone(),
            actions: actions.iter().cloned().collect(),
        })
        .collect()
}

fn format_timestamp(value: quench_core::time::Time) -> String {
    value.format("%Y-%m-%d %H:%M UTC")
}

/// The user list, and whether the person may create users and pick roles.
#[get("/api/v1/ui/admin/users")]
async fn admin_users(actor: PageClaims, Inject(db): Inject<Db>) -> Response {
    let actor = match allowed(actor.0, "read-users") {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let people = match realm::list(&db).await {
        Ok(people) => people,
        Err(err) => return refusal(&err),
    };
    let users = people
        .iter()
        .map(|user| AdminUserRow {
            username: user.username.clone(),
            roles: user.get_roles().iter().map(Role::to_string).collect(),
            wildcard: user.has_wildcard(),
            grants: grants(&user.get_permissions()),
        })
        .collect();
    ok(&AdminUsers {
        can_create: actor.can("gatehouse", "create-user"),
        admin: actor.has_role(Role::Admin.as_str()),
        actor: actor.sub,
        users,
    })
}

/// One user with the permission catalog, and which of its actions the person may take.
#[get("/api/v1/ui/admin/users/{username}")]
async fn admin_user(
    actor: PageClaims,
    Path(username): Path<String>,
    Inject(catalog): Inject<PermissionCatalog>,
    Inject(db): Inject<Db>,
) -> Response {
    let actor = match allowed(actor.0, "read-users") {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let user = match realm::get(&db, &username).await {
        Ok(user) => user,
        Err(err) => return refusal(&err),
    };
    let services = catalog
        .service_names()
        // A service anyone may use has nothing to grant.
        .filter(|name| !catalog.actions_for(name).is_empty())
        .map(|name| CatalogService {
            name: name.to_string(),
            label: catalog.label(name).to_string(),
            actions: catalog.actions_for(name).to_vec(),
        })
        .collect();
    ok(&AdminUser {
        username: user.username.clone(),
        role: primary_role(&user).as_str().to_string(),
        wildcard: user.has_wildcard(),
        held: grants(&user.get_permissions()),
        email: user.email.clone(),
        email_verified: user.email_verified_at.is_some(),
        disabled: user.is_disabled(),
        locked: user.is_locked(),
        mfa_enabled: user.mfa_enabled,
        created_at: format_timestamp(user.created_at),
        last_login_at: user.last_login_at.map(format_timestamp),
        is_self: user.username == actor.sub,
        viewer_admin: actor.has_role(Role::Admin.as_str()),
        can_edit: actor.can("gatehouse", "edit-user"),
        can_delete: actor.can("gatehouse", "delete-user"),
        can_manage_permissions: actor.can("gatehouse", "manage-permissions"),
        services,
        templates: catalog.template_names().map(str::to_string).collect(),
    })
}

/// The address a confirmation link would set. The link is the credential: no session.
#[get("/api/v1/ui/confirm-email")]
async fn confirm_email(
    Query(query): Query<ConfirmQuery>,
    Inject(tokens): Inject<VerificationTokens>,
) -> Response {
    let ticket = tokens
        .peek(PURPOSE_EMAIL_CHANGE, &query.token)
        .await
        .unwrap_or(None)
        .and_then(|raw| Ticket::decode(&raw));
    ticket.map_or_else(not_found, |ticket| {
        ok(&ConfirmEmail {
            new_email: ticket.new_email,
        })
    })
}

/// The kind of notification an unsubscribe link ends. The link is the credential: no session.
#[get("/api/v1/ui/unsubscribe")]
async fn unsubscribe(
    Query(query): Query<TokenQuery>,
    Inject(tokens): Inject<VerificationTokens>,
) -> Response {
    let token = query.token.unwrap_or_default();
    resolve(&tokens, &token)
        .await
        .map_or_else(not_found, |(_, template)| {
            ok(&UnsubscribeInfo {
                label_key: template.label_key(),
            })
        })
}

pub fn register_routes() {
    let _ = dashboard as fn(_, _) -> _;
    let _ = account as fn(_, _) -> _;
    let _ = mfa_secret as fn(_) -> _;
    let _ = mfa_enable as fn(_, _, _, _, _) -> _;
    let _ = admin_users as fn(_, _) -> _;
    let _ = admin_user as fn(_, _, _, _) -> _;
    let _ = confirm_email as fn(_, _) -> _;
    let _ = unsubscribe as fn(_, _) -> _;
}
