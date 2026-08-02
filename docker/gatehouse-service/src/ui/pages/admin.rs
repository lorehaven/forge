//! The realm's user administration pages.
//!
//! Plain forms with a POST and a redirect, like the login page next door, rather
//! than the htmx pages switchboard uses: gatehouse has no other interactive
//! surface, and a form that works without JavaScript is one less thing between an
//! administrator and getting back into the estate.
//!
//! Every mutation goes through [`crate::realm`], which the JSON API also calls -
//! so "the realm must keep an admin" is enforced once, not once per surface.

use crate::catalog::PermissionCatalog;
use crate::realm::{self, RealmError, UserChanges};
use crate::ui::common::{UiPageKind, render_page, ui_path};
use actix_web::{HttpRequest, HttpResponse, Responder, get, post, web};
use quench_auth::actix::routers::ui::get_user_from_req;
use quench_auth::prelude::{Actions, Claims, JwtConfig, Permissions, Role, SessionDb, User};
use quench_db::prelude::Db;
use quench_web::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

/// `Element` has no conditional attribute setter, and `selected`/`disabled` are
/// exactly the attributes that want one - they are either present or absent, with
/// no falsy value.
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

// ---------------------------------------------------------------------------
// Guard
// ---------------------------------------------------------------------------

/// Who the caller is, once established they hold the `gatehouse` catalog
/// action a page or form needs.
///
/// Three outcomes rather than two: no session goes to the login page, a session
/// lacking the action gets a 403 page. Bouncing a signed-in, insufficiently
/// privileged user to the login form would look like their session had expired.
///
/// `Claims::can` treats a wildcard role (`admin`/`service`) as satisfying any
/// action, `gatehouse`'s included - so a real admin keeps reaching every page
/// below with no separate check, which is exactly the "emergency" fallback
/// these per-action gates are meant to make otherwise unnecessary.
enum Access {
    Yes(Claims),
    NotSignedIn,
    NotPermitted,
}

async fn access_for(req: &HttpRequest, config: &JwtConfig, action: &str) -> Access {
    match get_user_from_req(req, config).await {
        None => Access::NotSignedIn,
        Some(claims) if claims.can("gatehouse", action) => Access::Yes(claims),
        Some(claims) => {
            tracing::warn!(
                "{} opened an admin page without gatehouse:{action}",
                claims.sub
            );
            Access::NotPermitted
        }
    }
}

/// Every page and every form starts here, naming the one action it needs.
/// `Ok` carries the actor's full claims - realm rules need the username, and
/// rendering needs to know what else this actor may do, to avoid offering a
/// control that would only 403 if used.
macro_rules! actor {
    ($req:expr, $config:expr, $action:expr) => {
        match access_for(&$req, &$config, $action).await {
            Access::Yes(claims) => claims,
            Access::NotSignedIn => return super::auth::login_redirect(),
            Access::NotPermitted => return forbidden_page(),
        }
    };
}

// ---------------------------------------------------------------------------
// Pages
// ---------------------------------------------------------------------------

#[get("/admin/users")]
pub(super) async fn users_page(
    req: HttpRequest,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    notice: web::Query<Notice>,
) -> impl Responder {
    let actor = actor!(req, config, "read-users");
    render_list(&db, &actor, &notice).await
}

#[get("/admin/users/")]
pub(super) async fn users_page_slash(
    req: HttpRequest,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    notice: web::Query<Notice>,
) -> impl Responder {
    let actor = actor!(req, config, "read-users");
    render_list(&db, &actor, &notice).await
}

#[get("/admin/users/{username}")]
pub(super) async fn edit_user(
    req: HttpRequest,
    path: web::Path<String>,
    config: web::Data<JwtConfig>,
    catalog: web::Data<PermissionCatalog>,
    db: web::Data<Db>,
    notice: web::Query<Notice>,
) -> impl Responder {
    let actor = actor!(req, config, "read-users");
    let username = path.into_inner();

    match realm::get(&db, &username).await {
        Ok(user) => render_edit(&catalog, &user, &actor, &notice),
        Err(_) => back_to_list(&RealmError::NotFound),
    }
}

// ---------------------------------------------------------------------------
// Forms
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateForm {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub role: Option<String>,
}

#[post("/admin/users")]
pub(super) async fn create_user(
    req: HttpRequest,
    form: web::Form<CreateForm>,
    config: web::Data<JwtConfig>,
    catalog: web::Data<PermissionCatalog>,
    db: web::Data<Db>,
) -> impl Responder {
    let actor = actor!(req, config, "create-user");
    let form = form.into_inner();
    let actor_is_admin = actor.has_role(Role::Admin.as_str());
    // Only an admin's create form carries a role field at all (see
    // `create_panel`); anyone else's submission is forced to `User` regardless
    // of what a tampered form might claim - `realm::create` enforces the same
    // rule, this is defense in depth, not the boundary itself.
    let roles = if actor_is_admin {
        vec![parse_role(form.role.as_deref())]
    } else {
        vec![Role::User]
    };

    // Deliberately no permissions here: a new user starts with no access, and
    // the edit page is where it is granted. Two steps, but the first one cannot
    // accidentally hand out the estate.
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
        // Straight to the editor, since a user with no grants cannot do anything
        // yet and granting is the obvious next thing.
        Ok(user) => redirect(&format!(
            "/admin/users/{}?ok=created",
            urlencoding::encode(&user.username)
        )),
        Err(err) => back_to_list(&err),
    }
}

/// The edit form, read as a flat map.
///
/// A declared struct cannot express one field per service-and-action when the
/// catalog defines them at runtime, so the permission controls are named
/// `perm_<service>_<action>` and probed out of the map one at a time.
#[post("/admin/users/{username}")]
pub(super) async fn save_user(
    req: HttpRequest,
    path: web::Path<String>,
    form: web::Form<HashMap<String, String>>,
    config: web::Data<JwtConfig>,
    catalog: web::Data<PermissionCatalog>,
    db: web::Data<Db>,
    sessions: web::Data<Arc<SessionDb>>,
) -> impl Responder {
    let actor = actor!(req, config, "edit-user");
    let username = path.into_inner();
    let form = form.into_inner();
    let actor_is_admin = actor.has_role(Role::Admin.as_str());

    // Read before writing so a resource-scoped grant set through the API -
    // this form has no field for one - survives a save made through the plain
    // checkbox matrix instead of being silently dropped.
    let existing_permissions = match realm::get(&db, &username).await {
        Ok(user) => user.get_permissions(),
        Err(err) => return back_to_list(&err),
    };

    let changes = UserChanges {
        // An empty password box means "leave it alone", not "set an empty
        // password" - the realm would reject the latter anyway.
        password: form
            .get("password")
            .map(String::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        // Only an admin's form carries a role field at all (see `render_edit`);
        // its absence means "leave roles alone", not "reset to user" - editing
        // an admin's password must not silently demote them.
        roles: form
            .get("role")
            .map(String::as_str)
            .filter(|_| actor_is_admin)
            .map(|value| vec![parse_role(Some(value))]),
        permissions: Some(permissions_from_form(&catalog, &form, &existing_permissions)),
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

#[post("/admin/users/{username}/template")]
pub(super) async fn apply_template(
    req: HttpRequest,
    path: web::Path<String>,
    form: web::Form<ApplyTemplateForm>,
    config: web::Data<JwtConfig>,
    catalog: web::Data<PermissionCatalog>,
    db: web::Data<Db>,
    sessions: web::Data<Arc<SessionDb>>,
) -> impl Responder {
    let actor = actor!(req, config, "manage-permissions");
    let username = path.into_inner();

    match realm::apply_template(
        &db,
        &catalog,
        &sessions,
        &actor.sub,
        &username,
        &form.into_inner().template,
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

#[post("/admin/users/{username}/delete")]
pub(super) async fn delete_user(
    req: HttpRequest,
    path: web::Path<String>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    sessions: web::Data<Arc<SessionDb>>,
) -> impl Responder {
    let actor = actor!(req, config, "delete-user");
    let username = path.into_inner();

    match realm::delete(&db, &sessions, &actor.sub, &username).await {
        Ok(()) => redirect("/admin/users?ok=deleted"),
        Err(err) => redirect(&format!(
            "/admin/users/{}?err={}",
            urlencoding::encode(&username),
            err.i18n_key()
        )),
    }
}

/// Reads one `perm_<service>_<action>` checkbox per action the catalog
/// declares, for every service it declares - then folds in whatever `existing`
/// held that this form never had a box for.
///
/// Driven by the catalog rather than by what the form happens to contain, so a
/// field named after a service or action this deployment does not know about
/// is ignored here rather than rejected later - the same posture
/// `permissions_from_form` has always taken, just with one more level of
/// nesting now that a service can declare more than two grantable actions.
///
/// The fold-in matters because the checkbox matrix only ever renders and reads
/// back plain, enumerated actions (`permission_row`, `catalog.actions_for`) - a
/// resource-scoped grant like `conveyor:project:<id>:write`, only settable
/// through the API today, has no checkbox here at all. Without this, saving
/// any other change on this page - a password, a role - would rebuild the
/// permissions map from checkboxes alone and silently erase it.
fn permissions_from_form(
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

/// A missing or unrecognised role is an ordinary user. Never an admin: a mangled
/// form should not be able to grant the realm away.
fn parse_role(value: Option<&str>) -> Role {
    value.and_then(Role::parse).unwrap_or(Role::User)
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

async fn render_list(db: &web::Data<Db>, actor: &Claims, notice: &Notice) -> HttpResponse {
    let people = match realm::list(db).await {
        Ok(people) => people,
        Err(err) => return error_page(&err),
    };

    let mut rows = div().class("meta-list");
    if people.is_empty() {
        rows = rows.child(div().class("empty").attr("data-i18n", "ui_admin_no_users"));
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

    // Omitted, not disabled, for an actor who cannot create a user - there is
    // nothing honest to disable a form control into when the whole action is
    // out of reach.
    let can_create = actor.can("gatehouse", "create-user");

    render_page(
        HttpResponse::Ok(),
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

fn user_row(user: &User, actor: &str) -> Element {
    let roles = user
        .get_roles()
        .iter()
        .map(Role::to_string)
        .collect::<Vec<_>>()
        .join(", ");

    // The wildcard is spelled out rather than shown as an empty grant list, which
    // would read as "this admin can do nothing".
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

/// `show_role_select` is only ever true for a literal admin - anyone else's
/// created user is forced to `User` server-side regardless (see
/// `create_user`), so offering the control at all would just be an invitation
/// to a 403 the field's absence avoids entirely.
fn create_panel(show_role_select: bool) -> Element {
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

fn render_edit(
    catalog: &PermissionCatalog,
    user: &User,
    actor: &Claims,
    notice: &Notice,
) -> HttpResponse {
    let wildcard = user.has_wildcard();
    let held = user.get_permissions();
    let actor_is_admin = actor.has_role(Role::Admin.as_str());
    let can_edit = actor.can("gatehouse", "edit-user");
    let can_delete = actor.can("gatehouse", "delete-user");
    let can_manage_perms = actor.can("gatehouse", "manage-permissions");

    // Disabled, not omitted, for a wildcard target - the note below explains
    // why. Also disabled for a viewer who can see this page (`read-users`) but
    // cannot change it (`edit-user`) - see the `can_edit` branch: that case
    // renders these same disabled boxes outside any `<form>`, so there is
    // nothing for a viewer's browser to submit at all.
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
        // Not editable and not submitted (see `save_user`): assigning `admin`
        // or `service` stays behind the literal role, not a catalog action, so
        // this reads as plain text instead of a control that would only 403.
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
            // Shown, not hidden: an admin whose matrix rendered as all-none
            // would look like a bug, and the note is what explains the
            // disabled controls.
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
        // A viewer who can see this account (`read-users`) but not change it
        // (`edit-user`): the same information, no `<form>` around any of it -
        // there is nothing here for their browser to submit.
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

    // No delete control for yourself: the realm would refuse it, and offering a
    // button that always fails is worse than not offering one. Also none
    // without `delete-user`, for the same reason.
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
        HttpResponse::Ok(),
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
                .child_opt(danger),
        ),
        UiPageKind::Admin,
    )
}

/// One service, one checkbox per action the catalog declares for it.
///
/// A flat set of independent boxes rather than a level select: the catalog
/// can declare any number of actions per service (switchboard's `launch`,
/// `stop`, `delete-model`), and there is no ordering between them to make a
/// single-select control honest. Action names are the operator's own
/// vocabulary from `permissions.toml`, not application copy, so they render as
/// plain text rather than through `data-i18n` - the same treatment the
/// service name next to them already gets.
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
        // What the target actually holds - independent of whether *this*
        // viewer may change it. Conflating the two would show a read-only
        // viewer every box checked, not what the user in front of them holds.
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

/// A one-click way to assign a named bundle of grants instead of checking each
/// box by hand. Hidden entirely when the catalog defines no templates, and for
/// a wildcard user - a role that already reaches everything has no use for a
/// bundle that reaches less.
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

/// The role the select shows for a user holding more than one.
///
/// The data model allows a set; this control does not, because a wildcard makes
/// any additional role irrelevant. Showing the most privileged one keeps the form
/// honest about what the user can currently do.
fn primary_role(user: &User) -> Role {
    let roles = user.get_roles();
    for candidate in [Role::Admin, Role::Service] {
        if roles.contains(&candidate) {
            return candidate;
        }
    }
    Role::User
}

/// Success or failure from the write that redirected here.
///
/// The error key is checked against the ones `RealmError` can produce rather than
/// being trusted from the query string, so a hand-crafted link cannot put
/// arbitrary text on the page.
fn notice_banner(notice: &Notice) -> Option<Element> {
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
        RealmError::UnknownTemplate,
        RealmError::RolesRequireAdmin,
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

fn back_to_list(err: &RealmError) -> HttpResponse {
    redirect(&format!("/admin/users?err={}", err.i18n_key()))
}

fn forbidden_page() -> HttpResponse {
    render_page(
        HttpResponse::Forbidden(),
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

fn error_page(err: &RealmError) -> HttpResponse {
    render_page(
        HttpResponse::build(err.status()),
        content().class("admin-content").child(
            div().class("admin-container").child(
                p().class("admin-notice error")
                    .attr("data-i18n", err.i18n_key()),
            ),
        ),
        UiPageKind::Admin,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> PermissionCatalog {
        let dir = std::env::temp_dir().join(format!("admin-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("permissions.toml");
        std::fs::write(
            &path,
            r#"
            [services.conveyor]
            actions = ["read", "write"]
            resource_types = ["project"]
            "#,
        )
        .unwrap();
        let result = PermissionCatalog::load_from(&path.to_string_lossy()).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    #[test]
    fn a_resource_scoped_grant_survives_a_plain_checkbox_save() {
        let catalog = catalog();
        let mut existing = Permissions::new();
        existing.insert(
            "conveyor".to_string(),
            ["project:abc-123:write".to_string()].into_iter().collect(),
        );

        // The form checks conveyor's plain "read" box and leaves "write"
        // unchecked - as if an admin were narrowing the blanket grant, with
        // no idea the resource-scoped one even exists.
        let mut form = HashMap::new();
        form.insert("perm_conveyor_read".to_string(), "on".to_string());

        let result = permissions_from_form(&catalog, &form, &existing);
        let conveyor = result.get("conveyor").expect("conveyor grants survive");

        assert!(conveyor.contains("read"), "the checked box is honoured");
        assert!(!conveyor.contains("write"), "the unchecked plain box is dropped");
        assert!(
            conveyor.contains("project:abc-123:write"),
            "the resource-scoped grant this form has no box for is preserved"
        );
    }

    #[test]
    fn a_plain_grant_can_still_be_revoked() {
        let catalog = catalog();
        let mut existing = Permissions::new();
        existing.insert("conveyor".to_string(), ["read".to_string()].into_iter().collect());

        // Nothing checked at all - unchecking every box should still clear a
        // plain grant, not treat it as "unknown, so preserve it".
        let form = HashMap::new();

        let result = permissions_from_form(&catalog, &form, &existing);
        assert!(
            result.get("conveyor").is_none_or(|actions| !actions.contains("read")),
            "an unchecked plain action is actually revoked"
        );
    }
}
