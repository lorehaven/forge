//! Realm user administration: plain POST-and-redirect forms, no JS required.
//! Mutations go through [`crate::realm`], shared with the JSON API.

use crate::catalog::PermissionCatalog;
use crate::realm::{self, RealmError, UserChanges};
use crate::ui::common::{UiPageKind, render_page, ui_path};
use async_trait::async_trait;
use http::StatusCode;
use quench_auth::domain::auth::{Actions, Permissions, Role, User};
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::domain::session::SessionDb;
use quench_auth::http::routers::ui::get_user_from_req;
use quench_db::prelude::Db;
use quench_http::prelude::{
    Form, FromRequest, HttpError, Inject, Path, Query, Request, Response, get, post,
};
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;
use serde::Deserialize;
use std::collections::HashMap;

/// `Element` has no conditional attribute setter for present/absent attrs.
trait AttrIf {
    fn attr_if(self, condition: bool, key: &str, value: &str) -> Self;
}

impl AttrIf for Element {
    fn attr_if(self, condition: bool, key: &str, value: &str) -> Self {
        if condition {
            self.attr(key, value)
        } else {
            self
        }
    }
}

/// Feedback carried across the redirect that follows every write.
#[derive(Deserialize, Default)]
pub struct Notice {
    /// A `RealmError` translation key, validated before it reaches the page.
    #[serde(default)]
    pub err: Option<String>,
    /// Set after a successful write.
    #[serde(default)]
    pub ok: Option<String>,
}

// --- Guard ---

/// One extractor per catalog action - a route without a gate won't compile.
macro_rules! admin_actor {
    ($name:ident, $action:literal) => {
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
                        tracing::warn!(
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
                    Self::NotPermitted => Err(forbidden_page()),
                }
            }
        }
    };
}

admin_actor!(ReadUsersActor, "read-users");
admin_actor!(CreateUserActor, "create-user");
admin_actor!(EditUserActor, "edit-user");
admin_actor!(DeleteUserActor, "delete-user");
admin_actor!(ManagePermissionsActor, "manage-permissions");

// --- Pages ---

#[get("/ui/admin/users")]
pub async fn users_page(
    actor: ReadUsersActor,
    Query(notice): Query<Notice>,
    Inject(db): Inject<Db>,
) -> Response {
    let actor = match actor.claims() {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    render_list(&db, &actor, &notice).await
}

#[get("/ui/admin/users/")]
pub async fn users_page_slash(
    actor: ReadUsersActor,
    Query(notice): Query<Notice>,
    Inject(db): Inject<Db>,
) -> Response {
    let actor = match actor.claims() {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    render_list(&db, &actor, &notice).await
}

#[get("/ui/admin/users/{username}")]
pub async fn edit_user(
    actor: ReadUsersActor,
    Path(username): Path<String>,
    Query(notice): Query<Notice>,
    Inject(catalog): Inject<PermissionCatalog>,
    Inject(db): Inject<Db>,
) -> Response {
    let actor = match actor.claims() {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    match realm::get(&db, &username).await {
        Ok(user) => render_edit(&catalog, &user, &actor, &notice),
        Err(_) => back_to_list(&RealmError::NotFound),
    }
}

// --- Forms ---

#[derive(Deserialize)]
pub struct CreateForm {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub role: Option<String>,
}

#[post("/ui/admin/users")]
pub async fn create_user(
    actor: CreateUserActor,
    Form(form): Form<CreateForm>,
    Inject(catalog): Inject<PermissionCatalog>,
    Inject(db): Inject<Db>,
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
    match realm::create(
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
    {
        // Straight to the editor - granting access is the obvious next step.
        Ok(user) => redirect(&format!(
            "/admin/users/{}?ok=created",
            urlencoding::encode(&user.username)
        )),
        Err(err) => back_to_list(&err),
    }
}

/// Read as a flat map: catalog actions are runtime data, so permission
/// fields are named `perm_<service>_<action>` and probed individually.
#[post("/ui/admin/users/{username}")]
pub async fn save_user(
    actor: EditUserActor,
    Path(username): Path<String>,
    Form(form): Form<HashMap<String, String>>,
    Inject(catalog): Inject<PermissionCatalog>,
    Inject(db): Inject<Db>,
    Inject(sessions): Inject<SessionDb>,
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
        Ok(_) => redirect(&format!(
            "/admin/users/{}?ok=saved",
            urlencoding::encode(&username)
        )),
        Err(err) => redirect(&format!(
            "/admin/users/{}?err={}",
            urlencoding::encode(&username),
            err.i18n_key()
        )),
    }
}

#[derive(Deserialize)]
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
            urlencoding::encode(&username)
        )),
        Err(err) => redirect(&format!(
            "/admin/users/{}?err={}",
            urlencoding::encode(&username),
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
            urlencoding::encode(&username),
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
            urlencoding::encode(&username)
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
            urlencoding::encode(&username)
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
            urlencoding::encode(&username)
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
) -> Response {
    if let Err(response) = actor.claims() {
        return response;
    }

    match realm::disable_mfa(&db, &username).await {
        Ok(()) => redirect(&format!(
            "/admin/users/{}?ok=saved",
            urlencoding::encode(&username)
        )),
        Err(err) => back_to_edit(&username, &err),
    }
}

/// Reads catalog-declared checkboxes, then folds in whatever `existing` held
/// with no checkbox (e.g. an API-only resource-scoped grant) so it survives.
pub fn permissions_from_form(
    catalog: &PermissionCatalog,
    form: &HashMap<String, String>,
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

// --- Rendering ---

pub async fn render_list(db: &Db, actor: &Claims, notice: &Notice) -> Response {
    let people = match realm::list(db).await {
        Ok(people) => people,
        Err(err) => return error_page(&err),
    };

    let mut rows = div().class("meta-list");
    if people.is_empty() {
        rows = rows.child(empty_state("ui_admin_no_users"));
    }
    for user in &people {
        rows = rows.child(user_row(user, &actor.sub));
    }

    let list_panel = div()
        .class("panel admin-panel")
        .child(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_admin_users_title"),
        )
        .child(rows);

    // Omitted, not disabled, when the action is out of reach entirely.
    let can_create = actor.can("gatehouse", "create-user");

    render_page(
        StatusCode::OK,
        content().class("admin-content").child(
            div()
                .class("admin-container")
                .child_opt(notice_banner(notice))
                .child(list_panel)
                .child_opt(can_create.then(|| create_panel(actor.has_role(Role::Admin.as_str())))),
        ),
        UiPageKind::Admin,
    )
}

pub fn user_row(user: &User, actor: &str) -> Element {
    let roles = user
        .get_roles()
        .iter()
        .map(Role::to_string)
        .collect::<Vec<_>>()
        .join(", ");

    // Spelled out, not an empty list - which would read as "can do nothing".
    let summary = if user.has_wildcard() {
        span()
            .class("admin-grant-all")
            .attr("data-i18n", "ui_admin_grants_all")
    } else {
        let grants = user.get_permissions();
        if grants.is_empty() {
            span()
                .class("admin-grant-none")
                .attr("data-i18n", "ui_admin_grants_none")
        } else {
            span().class("admin-grants").text(
                grants
                    .iter()
                    .map(|(service, actions)| {
                        format!(
                            "{service}: {}",
                            actions.iter().cloned().collect::<Vec<_>>().join("+")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        }
    };

    div()
        .class("admin-row")
        .child(
            div()
                .class("admin-row-main")
                .child(span().class("admin-username").text(&user.username))
                .child_opt(
                    (user.username == actor)
                        .then(|| span().class("admin-you").attr("data-i18n", "ui_admin_you")),
                )
                .child(span().class("admin-roles").text(&roles)),
        )
        .child(div().class("admin-row-grants").child(summary))
        .child(
            a().class("button admin-edit")
                .attr(
                    "href",
                    ui_path(&format!(
                        "/admin/users/{}",
                        urlencoding::encode(&user.username)
                    )),
                )
                .attr("data-i18n", "ui_admin_edit"),
        )
}

/// `show_role_select` is true only for a literal admin - anyone else's role
/// is forced server-side regardless, so the field's absence avoids a 403.
pub fn create_panel(show_role_select: bool) -> Element {
    let mut create_form = form()
        .attr("method", "post")
        .attr("action", ui_path("/admin/users"))
        .child(
            label()
                .attr("for", "new-username")
                .attr("data-i18n", "ui_admin_new_username"),
        )
        .child(
            input()
                .attr("type", "text")
                .attr("id", "new-username")
                .attr("name", "username")
                .attr("autocomplete", "off")
                .attr("required", "required"),
        )
        .child(
            label()
                .attr("for", "new-password")
                .attr("data-i18n", "ui_admin_new_password"),
        )
        .child(
            input()
                .attr("type", "password")
                .attr("id", "new-password")
                .attr("name", "password")
                .attr("autocomplete", "new-password")
                .attr("required", "required"),
        );

    if show_role_select {
        create_form = create_form
            .child(
                label()
                    .attr("for", "new-role")
                    .attr("data-i18n", "ui_admin_role"),
            )
            .child(role_select("new-role", &Role::User));
    }

    create_form = create_form
        .child(
            p().class("admin-hint")
                .attr("data-i18n", "ui_admin_new_hint"),
        )
        .child(
            button()
                .attr("type", "submit")
                .attr("data-i18n", "ui_admin_create"),
        );

    div()
        .class("panel admin-panel")
        .child(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_admin_create_title"),
        )
        .child(div().class("meta-list").child(create_form))
}

pub fn render_edit(
    catalog: &PermissionCatalog,
    user: &User,
    actor: &Claims,
    notice: &Notice,
) -> Response {
    let wildcard = user.has_wildcard();
    let held = user.get_permissions();
    let actor_is_admin = actor.has_role(Role::Admin.as_str());
    let can_edit = actor.can("gatehouse", "edit-user");
    let can_delete = actor.can("gatehouse", "delete-user");
    let can_manage_perms = actor.can("gatehouse", "manage-permissions");

    // Disabled, not omitted, for a wildcard target or a read-only viewer.
    let matrix_disabled = wildcard || !can_edit;
    let mut matrix = div().class("admin-matrix");
    for service in catalog.service_names() {
        matrix = matrix.child(permission_row(
            catalog,
            service,
            held.get(service),
            wildcard,
            matrix_disabled,
        ));
    }

    let role_row = if actor_is_admin {
        div()
            .child(
                label()
                    .attr("for", "role")
                    .attr("data-i18n", "ui_admin_role"),
            )
            .child(role_select("role", &primary_role(user)))
    } else {
        // Plain text, not a control: assigning admin/service isn't a catalog
        // action this actor holds, so a control here would only 403.
        div()
            .child(
                label()
                    .attr("for", "role")
                    .attr("data-i18n", "ui_admin_role"),
            )
            .child(span().attr("id", "role").text(primary_role(user).as_str()))
    };

    let details = if can_edit {
        let mut edit_form = form()
            .attr("method", "post")
            .attr(
                "action",
                ui_path(&format!(
                    "/admin/users/{}",
                    urlencoding::encode(&user.username)
                )),
            )
            .child(role_row)
            .child(
                div()
                    .class("admin-section-title")
                    .attr("data-i18n", "ui_admin_permissions"),
            )
            .child(matrix);

        if wildcard {
            // Shown, not hidden - explains why the matrix reads all-none.
            edit_form = edit_form.child(
                p().class("admin-hint")
                    .attr("data-i18n", "ui_admin_wildcard_note"),
            );
        }

        edit_form = edit_form
            .child(
                label()
                    .attr("for", "password")
                    .attr("data-i18n", "ui_admin_new_password_optional"),
            )
            .child(
                input()
                    .attr("type", "password")
                    .attr("id", "password")
                    .attr("name", "password")
                    .attr("autocomplete", "new-password"),
            )
            .child(
                button()
                    .attr("type", "submit")
                    .attr("data-i18n", "ui_admin_save"),
            );

        div()
            .class("panel admin-panel")
            .child(div().class("panel-title").text(&user.username))
            .child(div().class("meta-list").child(edit_form))
            .child_opt(
                can_manage_perms
                    .then(|| template_picker(catalog, user))
                    .flatten(),
            )
    } else {
        // Read-only viewer: same information, no `<form>` to submit it in.
        div()
            .class("panel admin-panel")
            .child(div().class("panel-title").text(&user.username))
            .child(
                div()
                    .class("meta-list")
                    .child(role_row)
                    .child(
                        div()
                            .class("admin-section-title")
                            .attr("data-i18n", "ui_admin_permissions"),
                    )
                    .child(matrix),
            )
    };

    let status = status_panel(user, can_edit, user.username != actor.sub);

    // No self-delete control: the realm would refuse it anyway.
    let danger = (can_delete && user.username != actor.sub).then(|| {
        div()
            .class("panel admin-panel admin-danger")
            .child(
                div()
                    .class("panel-title")
                    .attr("data-i18n", "ui_admin_delete_title"),
            )
            .child(
                div().class("meta-list").child(
                    form()
                        .attr("method", "post")
                        .attr(
                            "action",
                            ui_path(&format!(
                                "/admin/users/{}/delete",
                                urlencoding::encode(&user.username)
                            )),
                        )
                        .child(
                            p().class("admin-hint")
                                .attr("data-i18n", "ui_admin_delete_hint"),
                        )
                        .child(
                            button()
                                .attr("type", "submit")
                                .class("admin-delete")
                                .attr("data-i18n", "ui_admin_delete"),
                        ),
                ),
            )
    });

    render_page(
        StatusCode::OK,
        content().class("admin-content").child(
            div()
                .class("admin-container")
                .child_opt(notice_banner(notice))
                .child(
                    a().class("button admin-back")
                        .attr("href", ui_path("/admin/users"))
                        .attr("data-i18n", "ui_admin_back"),
                )
                .child(details)
                .child(status)
                .child_opt(danger),
        ),
        UiPageKind::Admin,
    )
}

/// Lifecycle/security state, plus the "an admin can undo this" actions - the
/// latter omitted, not disabled, for a viewer who can't edit.
pub fn status_panel(user: &User, can_edit: bool, allow_self_action: bool) -> Element {
    let mut rows = div()
        .class("meta-list")
        .child(status_row(
            "ui_admin_status_created",
            &format_timestamp(Some(user.created_at)),
        ))
        .child(match user.last_login_at {
            Some(ts) => status_row("ui_admin_status_last_login", &format_timestamp(Some(ts))),
            None => div()
                .class("admin-status-row")
                .child(
                    span()
                        .class("admin-hint")
                        .attr("data-i18n", "ui_admin_status_last_login"),
                )
                .child(span().attr("data-i18n", "ui_admin_status_never")),
        });

    if can_edit {
        rows = rows.child(status_action_row(
            "ui_admin_status_disabled",
            user.is_disabled(),
            "ui_admin_status_yes",
            "ui_admin_status_no",
            allow_self_action.then(|| {
                mfa_action_form(
                    &user.username,
                    if user.is_disabled() {
                        "enable"
                    } else {
                        "disable"
                    },
                    if user.is_disabled() {
                        "ui_admin_action_enable"
                    } else {
                        "ui_admin_action_disable"
                    },
                )
            }),
        ));

        rows = rows.child(status_action_row(
            "ui_admin_status_locked",
            user.is_locked(),
            "ui_admin_status_yes",
            "ui_admin_status_no",
            user.is_locked()
                .then(|| mfa_action_form(&user.username, "unlock", "ui_admin_action_unlock")),
        ));

        rows = rows.child(status_action_row(
            "ui_admin_status_mfa",
            user.mfa_enabled,
            "ui_admin_status_yes",
            "ui_admin_status_no",
            user.mfa_enabled.then(|| {
                mfa_action_form(&user.username, "mfa/disable", "ui_admin_action_mfa_disable")
            }),
        ));
    } else {
        rows = rows
            .child(status_row(
                "ui_admin_status_disabled",
                if user.is_disabled() {
                    "ui_admin_status_yes"
                } else {
                    "ui_admin_status_no"
                },
            ))
            .child(status_row(
                "ui_admin_status_locked",
                if user.is_locked() {
                    "ui_admin_status_yes"
                } else {
                    "ui_admin_status_no"
                },
            ))
            .child(status_row(
                "ui_admin_status_mfa",
                if user.mfa_enabled {
                    "ui_admin_status_yes"
                } else {
                    "ui_admin_status_no"
                },
            ));
    }

    div()
        .class("panel admin-panel")
        .child(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_admin_status_title"),
        )
        .child(rows)
}

fn status_row(label_key: &'static str, value: &str) -> Element {
    div()
        .class("admin-status-row")
        .child(span().class("admin-hint").attr("data-i18n", label_key))
        .child(span().text(value))
}

fn status_action_row(
    label_key: &'static str,
    active: bool,
    yes_key: &'static str,
    no_key: &'static str,
    action: Option<Element>,
) -> Element {
    div()
        .class("admin-status-row")
        .child(span().class("admin-hint").attr("data-i18n", label_key))
        .child(span().attr("data-i18n", if active { yes_key } else { no_key }))
        .child_opt(action)
}

fn mfa_action_form(username: &str, path_suffix: &str, button_key: &'static str) -> Element {
    form()
        .attr("method", "post")
        .attr(
            "action",
            ui_path(&format!(
                "/admin/users/{}/{path_suffix}",
                urlencoding::encode(username)
            )),
        )
        .child(
            button()
                .attr("type", "submit")
                .class("button")
                .attr("data-i18n", button_key),
        )
}

fn format_timestamp(value: Option<chrono::DateTime<chrono::Utc>>) -> String {
    match value {
        Some(ts) => ts.format("%Y-%m-%d %H:%M UTC").to_string(),
        None => String::new(),
    }
}

/// One checkbox per catalog action, flat (no inherent order between them).
fn permission_row(
    catalog: &PermissionCatalog,
    service: &str,
    held: Option<&Actions>,
    target_wildcard: bool,
    disabled: bool,
) -> Element {
    let mut checkboxes = div().class("admin-matrix-actions");
    for action in catalog.actions_for(service) {
        let field = format!("perm_{service}_{action}");
        // What the target holds, independent of whether this viewer may edit it.
        let checked = target_wildcard || held.is_some_and(|actions| actions.contains(action));

        let mut box_ = checkbox()
            .attr("id", &field)
            .attr("name", &field)
            .attr_if(checked, "checked", "checked");
        if disabled {
            box_ = box_.attr("disabled", "disabled");
        }

        checkboxes = checkboxes.child(
            div()
                .class("admin-matrix-action")
                .child(box_)
                .child(label().attr("for", &field).text(action)),
        );
    }

    div()
        .class("admin-matrix-row")
        .child(span().class("admin-service").text(catalog.label(service)))
        .child(checkboxes)
}

/// Assigns a named grant bundle in one click. Hidden with no templates, or
/// for a wildcard user who already reaches everything.
fn template_picker(catalog: &PermissionCatalog, user: &User) -> Option<Element> {
    if user.has_wildcard() {
        return None;
    }

    let mut names = catalog.template_names().peekable();
    names.peek()?;

    let mut select_control = select().attr("id", "template").attr("name", "template");
    for name in names {
        select_control = select_control.child(option().attr("value", name).text(name));
    }

    let picker_form = form()
        .attr("method", "post")
        .attr(
            "action",
            ui_path(&format!(
                "/admin/users/{}/template",
                urlencoding::encode(&user.username)
            )),
        )
        .child(
            label()
                .attr("for", "template")
                .attr("data-i18n", "ui_admin_template"),
        )
        .child(select_control)
        .child(
            p().class("admin-hint")
                .attr("data-i18n", "ui_admin_template_hint"),
        )
        .child(
            button()
                .attr("type", "submit")
                .attr("data-i18n", "ui_admin_apply_template"),
        );

    Some(
        div()
            .class("panel admin-panel")
            .child(
                div()
                    .class("panel-title")
                    .attr("data-i18n", "ui_admin_template_title"),
            )
            .child(div().class("meta-list").child(picker_form)),
    )
}

fn role_select(id: &str, selected: &Role) -> Element {
    let mut control = select().attr("id", id).attr("name", "role");
    for role in [Role::User, Role::Admin, Role::Service] {
        control = control.child(
            option()
                .attr("value", role.as_str())
                .attr("data-i18n", format!("ui_admin_role_{}", role.as_str()))
                .attr_if(&role == selected, "selected", "selected"),
        );
    }
    control
}

/// The role shown for a user holding more than one: most-privileged wins.
fn primary_role(user: &User) -> Role {
    let roles = user.get_roles();
    for candidate in [Role::Admin, Role::Service] {
        if roles.contains(&candidate) {
            return candidate;
        }
    }
    Role::User
}

/// Error key is validated against `RealmError`'s own set, not trusted
/// from the query string - a hand-crafted link can't inject arbitrary text.
pub fn notice_banner(notice: &Notice) -> Option<Element> {
    if let Some(key) = notice.err.as_deref().and_then(known_error_key) {
        return Some(p().class("admin-notice error").attr("data-i18n", key));
    }
    let key = match notice.ok.as_deref() {
        Some("created") => "ui_admin_ok_created",
        Some("saved") => "ui_admin_ok_saved",
        Some("deleted") => "ui_admin_ok_deleted",
        _ => return None,
    };
    Some(p().class("admin-notice ok").attr("data-i18n", key))
}

fn known_error_key(candidate: &str) -> Option<&'static str> {
    [
        RealmError::NotFound,
        RealmError::UsernameEmpty,
        RealmError::PasswordEmpty,
        RealmError::AlreadyExists,
        RealmError::UnknownGrants(Vec::new()),
        RealmError::LastAdmin,
        RealmError::SelfDemote,
        RealmError::SelfDelete,
        RealmError::SelfDisable,
        RealmError::UnknownTemplate,
        RealmError::RolesRequireAdmin,
        RealmError::Internal,
    ]
    .iter()
    .map(RealmError::i18n_key)
    .find(|known| *known == candidate)
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
        urlencoding::encode(username),
        err.i18n_key()
    ))
}

pub fn forbidden_page() -> Response {
    render_page(
        StatusCode::FORBIDDEN,
        content().class("admin-content").child(
            div().class("admin-container").child(
                div()
                    .class("panel admin-panel")
                    .child(
                        div()
                            .class("panel-title")
                            .attr("data-i18n", "ui_admin_forbidden_title"),
                    )
                    .child(
                        div().class("meta-list").child(
                            p().class("admin-hint")
                                .attr("data-i18n", "ui_admin_forbidden"),
                        ),
                    ),
            ),
        ),
        UiPageKind::Admin,
    )
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
        UiPageKind::Admin,
    )
}

pub fn register_routes() {
    let _ = users_page as fn(_, _, _) -> _;
    let _ = users_page_slash as fn(_, _, _) -> _;
    let _ = edit_user as fn(_, _, _, _, _) -> _;
    let _ = create_user as fn(_, _, _, _) -> _;
    let _ = save_user as fn(_, _, _, _, _, _) -> _;
    let _ = apply_template as fn(_, _, _, _, _, _) -> _;
    let _ = delete_user as fn(_, _, _, _) -> _;
    let _ = disable_user as fn(_, _, _) -> _;
    let _ = enable_user as fn(_, _, _) -> _;
    let _ = unlock_user as fn(_, _, _) -> _;
    let _ = disable_user_mfa as fn(_, _, _) -> _;
}
