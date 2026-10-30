//! Realm user administration: the forms post here and get a redirect back to the page (which the client draws).
//! Mutations go through [`crate::realm`], shared with the JSON API.

use crate::PublicBase;
use crate::catalog::PermissionCatalog;
use crate::email::{Mail, Sender};
use crate::invites;
use crate::notices;
use crate::realm::{self, RealmError, UserChanges};
use crate::tokens::VerificationTokens;
use crate::ui::common::{ActorOrRedirect, FormFields, ui_path};
use quench_auth::domain::auth::{Actions, Permissions, Role, User};
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::domain::session::SessionDb;
use quench_auth::http::routers::ui::get_user_from_req;
use quench_core::async_trait;
use quench_db::prelude::Db;
use quench_http::http::StatusCode;
use quench_http::prelude::{
    Form, FromRequest, HttpError, Inject, Path, Request, Response, get, post,
};
use quench_serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

// --- Guard ---

/// One extractor per catalog action - a route without a gate won't compile.
macro_rules! admin_actor {
    ($name:ident, $action:literal) => {
        #[derive(Debug)]
        pub enum $name {
            Yes(Claims),
            NotSignedIn,
            NotPermitted,
        }

        #[async_trait]
        impl FromRequest for $name {
            async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
                let Ok(config) = req.container().get::<JwtConfig>() else {
                    return Ok(Self::NotSignedIn);
                };
                match get_user_from_req(req, &config).await {
                    None => Ok(Self::NotSignedIn),
                    Some(claims) if claims.can("gatehouse", $action) => Ok(Self::Yes(claims)),
                    Some(claims) => {
                        quench_log::warn!(
                            "{} opened an admin page without gatehouse:{}",
                            claims.sub,
                            $action
                        );
                        Ok(Self::NotPermitted)
                    }
                }
            }
        }

        impl $name {
            fn claims(self) -> Result<Claims, Response> {
                match self {
                    Self::Yes(claims) => Ok(claims),
                    Self::NotSignedIn => Err(super::auth::login_redirect()),
                    Self::NotPermitted => Err(Response::new(StatusCode::FORBIDDEN)),
                }
            }
        }
    };
}

admin_actor!(CreateUserActor, "create-user");
admin_actor!(EditUserActor, "edit-user");
admin_actor!(DeleteUserActor, "delete-user");
admin_actor!(ManagePermissionsActor, "manage-permissions");

// --- Pages ---

/// The user list and the editor.
///
/// The client draws them from `GET /api/v1/ui/admin/users[/{username}]` (which checks the
/// catalog action); every write below answers with a redirect back to the page with `?ok=...` or `?err=...`.
#[get("/ui/admin/users")]
pub async fn users_page(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/admin/users/")]
pub async fn users_page_slash(who: ActorOrRedirect) -> Response {
    page(who)
}

#[get("/ui/admin/users/{username}")]
pub async fn edit_user(who: ActorOrRedirect, Path(_username): Path<String>) -> Response {
    page(who)
}

fn page(who: ActorOrRedirect) -> Response {
    match who.or_redirect() {
        Ok(_) => super::client::signed_in(),
        Err(response) => response,
    }
}

// --- Forms ---

#[derive(Debug, Deserialize)]
pub struct CreateForm {
    pub username: String,
    /// Optional when an email address is given: that account is invited instead.
    #[serde(default)]
    pub password: String,
    /// With an address, the user is invited to choose their own password.
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
}

#[post("/ui/admin/users")]
pub async fn create_user(
    actor: CreateUserActor,
    Form(form): Form<CreateForm>,
    Inject(catalog): Inject<PermissionCatalog>,
    Inject(db): Inject<Db>,
    Inject(tokens): Inject<VerificationTokens>,
    Inject(mailer): Inject<Arc<dyn Sender>>,
    Inject(base): Inject<PublicBase>,
) -> Response {
    let actor = match actor.claims() {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let actor_is_admin = actor.has_role(Role::Admin.as_str());
    // Non-admin submissions are forced to `User` here too - defense in depth,
    // `realm::create` is the real boundary.
    let roles = if actor_is_admin {
        vec![parse_role(form.role.as_deref())]
    } else {
        vec![Role::User]
    };

    // No permissions on create - a new user starts with none, granted on edit.
    // An address means an invitation: the person picks their own password and
    // the administrator never handles it (any password typed here is ignored).
    let email = form
        .email
        .as_deref()
        .map(str::trim)
        .filter(|email| !email.is_empty());
    let created = match email {
        Some(email) => {
            realm::create_invited(
                &db,
                &catalog,
                actor_is_admin,
                &form.username,
                roles,
                Permissions::new(),
                email,
            )
            .await
        }
        None => {
            realm::create(
                &db,
                &catalog,
                actor_is_admin,
                &form.username,
                &form.password,
                roles,
                Permissions::new(),
                None,
            )
            .await
        }
    };

    match created {
        // Straight to the editor - granting access is the obvious next step.
        Ok(user) => {
            let outcome = if email.is_some() {
                invite_outcome(&tokens, &**mailer, &base, &user).await
            } else {
                "created"
            };
            redirect(&format!(
                "/admin/users/{}?ok={outcome}",
                quench_serde::urlencoded::percent_encode(&user.username)
            ))
        }
        Err(err) => back_to_list(&err),
    }
}

/// Sends the invitation and names what to tell the administrator.
async fn invite_outcome(
    tokens: &VerificationTokens,
    mailer: &dyn Sender,
    base: &PublicBase,
    user: &User,
) -> &'static str {
    match invites::send_invite(tokens, mailer, base, user, None).await {
        Ok(()) => "invited",
        Err(err) => {
            quench_log::error!("could not send the invitation to {}: {err}", user.username);
            "invite_failed"
        }
    }
}

/// Another invitation for an account whose address is still unconfirmed - the
/// first was lost, expired, or never arrived.
#[post("/ui/admin/users/{username}/invite")]
pub async fn resend_invite(
    actor: EditUserActor,
    Path(username): Path<String>,
    Inject(db): Inject<Db>,
    Inject(tokens): Inject<VerificationTokens>,
    Inject(mailer): Inject<Arc<dyn Sender>>,
    Inject(base): Inject<PublicBase>,
) -> Response {
    if let Err(response) = actor.claims() {
        return response;
    }
    let user = match realm::get(&db, &username).await {
        Ok(user) => user,
        Err(err) => return back_to_list(&err),
    };
    if user.email.as_deref().is_none_or(|e| e.trim().is_empty()) {
        return back_to_edit(&username, &RealmError::InviteNeedsEmail);
    }
    if user.email_verified_at.is_some() {
        return back_to_edit(&username, &RealmError::AlreadyConfirmed);
    }
    let outcome = invite_outcome(&tokens, &**mailer, &base, &user).await;
    redirect(&format!(
        "/admin/users/{}?ok={outcome}",
        quench_serde::urlencoded::percent_encode(&username)
    ))
}

/// Read as a flat map: catalog actions are runtime data, so permission
/// fields are named `perm_<service>_<action>` and probed individually.
#[post("/ui/admin/users/{username}")]
pub async fn save_user(
    actor: EditUserActor,
    Path(username): Path<String>,
    Form(form): Form<FormFields>,
    Inject(catalog): Inject<PermissionCatalog>,
    Inject(db): Inject<Db>,
    Inject(sessions): Inject<SessionDb>,
    Inject(mailer): Inject<Arc<dyn Sender>>,
) -> Response {
    let actor = match actor.claims() {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let actor_is_admin = actor.has_role(Role::Admin.as_str());

    // Read before writing so a resource-scoped grant (no checkbox for it) survives.
    let existing_permissions = match realm::get(&db, &username).await {
        Ok(user) => user.get_permissions(),
        Err(err) => return back_to_list(&err),
    };

    let changes = UserChanges {
        // Empty box means "leave it alone", not "set an empty password".
        password: form
            .get("password")
            .map(String::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        // Absent role field means "leave alone", not "reset to user" - avoids
        // silently demoting an admin via their own password-only edit.
        roles: form
            .get("role")
            .map(String::as_str)
            .filter(|_| actor_is_admin)
            .map(|value| vec![parse_role(Some(value))]),
        permissions: Some(permissions_from_form(
            &catalog,
            &form,
            &existing_permissions,
        )),
        ..UserChanges::default()
    };

    let password_changed = changes.password.is_some();
    match realm::update(
        &db,
        &catalog,
        &sessions,
        &actor.sub,
        actor_is_admin,
        &username,
        changes,
    )
    .await
    {
        Ok(user) => {
            if password_changed {
                notices::notify(&**mailer, &user, &Mail::PasswordChanged, None).await;
            }
            redirect(&format!(
                "/admin/users/{}?ok=saved",
                quench_serde::urlencoded::percent_encode(&username)
            ))
        }
        Err(err) => redirect(&format!(
            "/admin/users/{}?err={}",
            quench_serde::urlencoded::percent_encode(&username),
            err.i18n_key()
        )),
    }
}

#[derive(Debug, Deserialize)]
pub struct ApplyTemplateForm {
    pub template: String,
}

#[post("/ui/admin/users/{username}/template")]
pub async fn apply_template(
    actor: ManagePermissionsActor,
    Path(username): Path<String>,
    Form(form): Form<ApplyTemplateForm>,
    Inject(catalog): Inject<PermissionCatalog>,
    Inject(db): Inject<Db>,
    Inject(sessions): Inject<SessionDb>,
) -> Response {
    let actor = match actor.claims() {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    match realm::apply_template(
        &db,
        &catalog,
        &sessions,
        &actor.sub,
        &username,
        &form.template,
    )
    .await
    {
        Ok(_) => redirect(&format!(
            "/admin/users/{}?ok=saved",
            quench_serde::urlencoded::percent_encode(&username)
        )),
        Err(err) => redirect(&format!(
            "/admin/users/{}?err={}",
            quench_serde::urlencoded::percent_encode(&username),
            err.i18n_key()
        )),
    }
}

#[post("/ui/admin/users/{username}/delete")]
pub async fn delete_user(
    actor: DeleteUserActor,
    Path(username): Path<String>,
    Inject(db): Inject<Db>,
    Inject(sessions): Inject<SessionDb>,
) -> Response {
    let actor = match actor.claims() {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    match realm::delete(&db, &sessions, &actor.sub, &username).await {
        Ok(()) => redirect("/admin/users?ok=deleted"),
        Err(err) => redirect(&format!(
            "/admin/users/{}?err={}",
            quench_serde::urlencoded::percent_encode(&username),
            err.i18n_key()
        )),
    }
}

/// Support recovery: disable/enable, unlock, force-disable MFA - all gated
/// on `edit-user`, the same "change this account" capability as save.
#[post("/ui/admin/users/{username}/disable")]
pub async fn disable_user(
    actor: EditUserActor,
    Path(username): Path<String>,
    Inject(db): Inject<Db>,
) -> Response {
    let actor = match actor.claims() {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    if username == actor.sub {
        return back_to_edit(&username, &RealmError::SelfDisable);
    }

    match realm::set_disabled(&db, &username, true).await {
        Ok(_) => redirect(&format!(
            "/admin/users/{}?ok=saved",
            quench_serde::urlencoded::percent_encode(&username)
        )),
        Err(err) => back_to_edit(&username, &err),
    }
}

#[post("/ui/admin/users/{username}/enable")]
pub async fn enable_user(
    actor: EditUserActor,
    Path(username): Path<String>,
    Inject(db): Inject<Db>,
) -> Response {
    if let Err(response) = actor.claims() {
        return response;
    }

    match realm::set_disabled(&db, &username, false).await {
        Ok(_) => redirect(&format!(
            "/admin/users/{}?ok=saved",
            quench_serde::urlencoded::percent_encode(&username)
        )),
        Err(err) => back_to_edit(&username, &err),
    }
}

#[post("/ui/admin/users/{username}/unlock")]
pub async fn unlock_user(
    actor: EditUserActor,
    Path(username): Path<String>,
    Inject(db): Inject<Db>,
) -> Response {
    if let Err(response) = actor.claims() {
        return response;
    }

    match realm::unlock(&db, &username).await {
        Ok(_) => redirect(&format!(
            "/admin/users/{}?ok=saved",
            quench_serde::urlencoded::percent_encode(&username)
        )),
        Err(err) => back_to_edit(&username, &err),
    }
}

/// Lost-authenticator recovery: admin never sees the secret, just turns MFA off.
#[post("/ui/admin/users/{username}/mfa/disable")]
pub async fn disable_user_mfa(
    actor: EditUserActor,
    Path(username): Path<String>,
    Inject(db): Inject<Db>,
    Inject(mailer): Inject<Arc<dyn Sender>>,
) -> Response {
    if let Err(response) = actor.claims() {
        return response;
    }

    // Only worth a notice if it was actually on.
    let was_on = realm::get(&db, &username)
        .await
        .is_ok_and(|user| user.mfa_enabled);
    match realm::disable_mfa(&db, &username).await {
        Ok(()) => {
            if was_on {
                notices::notify_username(&**mailer, &db, &username, &Mail::MfaDisabled, None).await;
            }
            redirect(&format!(
                "/admin/users/{}?ok=saved",
                quench_serde::urlencoded::percent_encode(&username)
            ))
        }
        Err(err) => back_to_edit(&username, &err),
    }
}

/// Reads catalog-declared checkboxes, then folds in whatever `existing` held
/// with no checkbox (e.g. an API-only resource-scoped grant) so it survives.
pub fn permissions_from_form<S: std::hash::BuildHasher>(
    catalog: &PermissionCatalog,
    form: &HashMap<String, String, S>,
    existing: &Permissions,
) -> Permissions {
    let mut result: Permissions = catalog
        .service_names()
        .filter_map(|service| {
            let actions: Actions = catalog
                .actions_for(service)
                .iter()
                .filter(|action| form.contains_key(&format!("perm_{service}_{action}")))
                .cloned()
                .collect();
            (!actions.is_empty()).then(|| (service.to_string(), actions))
        })
        .collect();

    for (service, actions) in existing {
        let plain: std::collections::HashSet<&str> = catalog
            .actions_for(service)
            .iter()
            .map(String::as_str)
            .collect();
        for action in actions {
            if !plain.contains(action.as_str()) {
                result
                    .entry(service.clone())
                    .or_default()
                    .insert(action.clone());
            }
        }
    }

    result
}

/// Missing/unrecognised role is a plain user, never admin.
pub fn parse_role(value: Option<&str>) -> Role {
    value.and_then(Role::parse).unwrap_or(Role::User)
}

fn redirect(path: &str) -> Response {
    Response::new(StatusCode::FOUND).header("Location", ui_path(path))
}

fn back_to_list(err: &RealmError) -> Response {
    redirect(&format!("/admin/users?err={}", err.i18n_key()))
}

fn back_to_edit(username: &str, err: &RealmError) -> Response {
    redirect(&format!(
        "/admin/users/{}?err={}",
        quench_serde::urlencoded::percent_encode(username),
        err.i18n_key()
    ))
}

pub fn register_routes() {
    let _ = users_page as fn(_) -> _;
    let _ = users_page_slash as fn(_) -> _;
    let _ = edit_user as fn(_, _) -> _;
    let _ = create_user as fn(_, _, _, _, _, _, _) -> _;
    let _ = resend_invite as fn(_, _, _, _, _, _) -> _;
    let _ = save_user as fn(_, _, _, _, _, _, _) -> _;
    let _ = apply_template as fn(_, _, _, _, _, _) -> _;
    let _ = delete_user as fn(_, _, _, _) -> _;
    let _ = disable_user as fn(_, _, _) -> _;
    let _ = enable_user as fn(_, _, _) -> _;
    let _ = unlock_user as fn(_, _, _) -> _;
    let _ = disable_user_mfa as fn(_, _, _, _) -> _;
}
