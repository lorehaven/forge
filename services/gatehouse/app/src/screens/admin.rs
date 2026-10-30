//! The realm's user administration: the list with the form to add a user, and one user's page (role, grants by service, the
//! named bundles, the account's state and what an administrator can do about it). The forms are posted by the browser to
//! `/ui/admin/users...`, which redirects back here with `?ok=` or `?err=`; the service checks the catalog action each time.

use gatehouse_core::{AdminUser, AdminUserRow, AdminUsers, Grant, notice, segment};
use quench_ui::prelude::*;
use quench_ui::router::{RouteContext, link};

use super::{Env, hint, input_named, panel, post_form, query, submit, waiting};
use crate::live::{Problem, Request, State};

const ROLES: [&str; 3] = ["user", "admin", "service"];

/// What a refused read shows in place of the page.
fn refused(env: &Env, problem: &Option<Problem>) -> Option<View> {
    match problem {
        Some(Problem::Forbidden) => Some(
            div()
                .class("admin-container")
                .child(panel(
                    env,
                    "ui_admin_forbidden_title",
                    p().attr("id", "forbidden")
                        .class("gh-hint")
                        .text(env.t.t("ui_admin_forbidden")),
                ))
                .into_view(),
        ),
        Some(Problem::NotFound) => Some(super::not_found(env)),
        _ => None,
    }
}

pub(crate) fn admin_users(env: &Env, state: &State, ctx: &RouteContext) -> View {
    let flash = notice::admin(&query(ctx));
    state.admin_users.set(None);
    (env.ask)(Request::AdminUsers);
    let (env, users, problem) = (
        env.clone(),
        state.admin_users.clone(),
        state.problem.clone(),
    );
    View::dynamic(move || {
        if let Some(view) = refused(&env, &problem.get()) {
            return view;
        }
        users.get().map_or_else(
            || waiting(&env),
            |users| list(&env, &users, flash).into_view(),
        )
    })
}

fn list(env: &Env, users: &AdminUsers, flash: Option<gatehouse_core::Notice>) -> Element {
    let mut rows = div().class("gh-rows").attr("id", "users");
    if users.users.is_empty() {
        rows = rows.child(
            p().attr("id", "no-users")
                .class("fui-empty")
                .text(env.t.t("ui_admin_no_users")),
        );
    }
    for user in &users.users {
        rows = rows.child(user_row(env, user, &users.actor));
    }
    div()
        .class("admin-container")
        .child(super::notice(env, flash))
        .child(panel(env, "ui_admin_users_title", rows))
        // Omitted, not disabled, when the action is out of reach entirely.
        .child(users.can_create.then(|| create_panel(env, users.admin)))
}

fn grants_summary(env: &Env, user: &AdminUserRow) -> Element {
    // Spelled out, not an empty list - which would read as "can do nothing".
    if user.wildcard {
        span()
            .class("gh-grant-all")
            .text(env.t.t("ui_admin_grants_all"))
    } else if user.grants.is_empty() {
        span()
            .class("gh-grant-none")
            .text(env.t.t("ui_admin_grants_none"))
    } else {
        span().class("gh-grants").text(
            user.grants
                .iter()
                .map(|g| format!("{}: {}", g.service, g.actions.join("+")))
                .collect::<Vec<_>>()
                .join(", "),
        )
    }
}

fn user_row(env: &Env, user: &AdminUserRow, actor: &str) -> Element {
    div()
        .class("gh-user-row")
        .attr("data-user", user.username.clone())
        .child(
            div()
                .class("gh-user-main")
                .child(span().class("gh-username").text(user.username.clone()))
                .child(
                    (user.username == actor)
                        .then(|| span().class("gh-you").text(env.t.t("ui_admin_you"))),
                )
                .child(span().class("gh-roles").text(user.roles.join(", "))),
        )
        .child(
            div()
                .class("gh-user-grants")
                .child(grants_summary(env, user)),
        )
        .child(
            link(&env.path(&format!("/admin/users/{}", segment(&user.username))))
                .class("gh-btn gh-edit")
                .text(env.t.t("ui_admin_edit")),
        )
}

fn role_select(env: &Env, id: &str, selected: &str) -> Element {
    let mut control = select()
        .class("gh-input")
        .attr("id", id.to_string())
        .attr("name", "role");
    for role in ROLES {
        let chosen = role == selected;
        control = control.child(
            option()
                .attr("value", role)
                .attr_bool("selected", move || chosen)
                .text(env.t.t(&format!("ui_admin_role_{role}"))),
        );
    }
    control
}

/// `show_role_select` is true only for a literal administrator: anyone else's role is forced by the service anyway.
fn create_panel(env: &Env, show_role_select: bool) -> Element {
    let mut form = post_form(env.path("/admin/users"))
        .attr("id", "create-form")
        .child(input_named(
            env,
            "new-username",
            "username",
            "ui_admin_new_username",
            "text",
            "off",
            true,
        ))
        .child(input_named(
            env,
            "new-password",
            "password",
            "ui_admin_new_password",
            "password",
            "new-password",
            false,
        ))
        .child(input_named(
            env,
            "new-email",
            "email",
            "ui_admin_new_email",
            "email",
            "off",
            false,
        ))
        .child(hint(env, "ui_admin_new_email_hint"));
    if show_role_select {
        form = form.child(
            div()
                .class("gh-field")
                .child(
                    label()
                        .attr("for", "new-role")
                        .text(env.t.t("ui_admin_role")),
                )
                .child(role_select(env, "new-role", "user")),
        );
    }
    form = form
        .child(hint(env, "ui_admin_new_hint"))
        .child(submit(env, "ui_admin_create"));
    panel(env, "ui_admin_create_title", form)
}

pub(crate) fn admin_user(env: &Env, state: &State, ctx: &RouteContext) -> View {
    let flash = notice::admin(&query(ctx));
    let username = ctx.param("username").unwrap_or_default();
    state.admin_user.set(None);
    (env.ask)(Request::AdminUser(username));
    let (env, user, problem) = (env.clone(), state.admin_user.clone(), state.problem.clone());
    View::dynamic(move || {
        if let Some(view) = refused(&env, &problem.get()) {
            return view;
        }
        user.get().map_or_else(
            || waiting(&env),
            |user| editor(&env, &user, flash).into_view(),
        )
    })
}

fn editor(env: &Env, user: &AdminUser, flash: Option<gatehouse_core::Notice>) -> Element {
    let danger = (user.can_delete && !user.is_self).then(|| delete_panel(env, user));
    div()
        .class("admin-container")
        .child(super::notice(env, flash))
        .child(
            link(&env.path("/admin/users"))
                .attr("id", "back")
                .class("gh-btn gh-back")
                .text(env.t.t("ui_admin_back")),
        )
        .child(details_panel(env, user))
        .child(
            user.can_manage_permissions
                .then(|| template_panel(env, user))
                .flatten(),
        )
        .child(status_panel(env, user))
        .child(danger)
}

fn held(user: &AdminUser, service: &str, action: &str) -> bool {
    user.wildcard
        || user
            .held
            .iter()
            .any(|g: &Grant| g.service == service && g.actions.iter().any(|a| a == action))
}

fn role_row(env: &Env, user: &AdminUser) -> Element {
    let control = if user.viewer_admin {
        role_select(env, "role", &user.role)
    } else {
        // Plain text, not a control: assigning admin/service isn't an action this viewer holds, so a control would only 403.
        span().attr("id", "role").text(user.role.clone())
    };
    div()
        .class("gh-field")
        .child(label().attr("for", "role").text(env.t.t("ui_admin_role")))
        .child(control)
}

fn matrix(user: &AdminUser) -> Element {
    // Disabled, not omitted, for a wildcard target or a read-only viewer.
    let disabled = user.wildcard || !user.can_edit;
    let mut matrix = div().class("gh-matrix");
    for service in &user.services {
        let mut actions = div().class("gh-matrix-actions");
        for action in &service.actions {
            let field = format!("perm_{}_{action}", service.name);
            let checked = held(user, &service.name, action);
            let mut checkbox = input()
                .attr("type", "checkbox")
                .attr("id", field.clone())
                .attr("name", field.clone())
                .attr_bool("checked", move || checked);
            if disabled {
                checkbox = checkbox.attr("disabled", "disabled");
            }
            actions = actions.child(
                div()
                    .class("gh-check")
                    .child(checkbox)
                    .child(label().attr("for", field).text(action.clone())),
            );
        }
        matrix = matrix.child(
            div()
                .class("gh-matrix-row")
                .child(span().class("gh-service").text(service.label.clone()))
                .child(actions),
        );
    }
    matrix
}

/// The user's panel: an edit form, or the same information with no form for a read-only viewer.
fn details_panel(env: &Env, user: &AdminUser) -> Element {
    let body = div()
        .child(role_row(env, user))
        .child(
            div()
                .class("gh-section-title")
                .text(env.t.t("ui_admin_permissions")),
        )
        .child(matrix(user));
    let body = if user.can_edit {
        let mut form = post_form(env.path(&format!("/admin/users/{}", segment(&user.username))))
            .attr("id", "edit-form")
            .child(body);
        if user.wildcard {
            // Shown, not hidden - explains why the matrix reads all-none.
            form = form.child(hint(env, "ui_admin_wildcard_note"));
        }
        div().child(
            form.child(
                div()
                    .class("gh-field")
                    .child(
                        label()
                            .attr("for", "password")
                            .text(env.t.t("ui_admin_new_password_optional")),
                    )
                    .child(
                        input()
                            .class("gh-input")
                            .attr("type", "password")
                            .attr("id", "password")
                            .attr("name", "password")
                            .attr("autocomplete", "new-password"),
                    ),
            )
            .child(submit(env, "ui_admin_save")),
        )
    } else {
        body
    };
    div()
        .class("gh-panel")
        .child(
            div()
                .class("gh-panel-title")
                .attr("id", "user-name")
                .text(user.username.clone()),
        )
        .child(div().class("gh-meta").child(body))
}

/// Assigns a named grant bundle in one click. Hidden with no bundles, or for a wildcard user who already reaches everything.
fn template_panel(env: &Env, user: &AdminUser) -> Option<Element> {
    if user.wildcard || user.templates.is_empty() {
        return None;
    }
    let mut pick = select()
        .class("gh-input")
        .attr("id", "template")
        .attr("name", "template");
    for name in &user.templates {
        pick = pick.child(option().attr("value", name.clone()).text(name.clone()));
    }
    let form = post_form(env.path(&format!(
        "/admin/users/{}/template",
        segment(&user.username)
    )))
    .attr("id", "template-form")
    .child(
        div()
            .class("gh-field")
            .child(
                label()
                    .attr("for", "template")
                    .text(env.t.t("ui_admin_template")),
            )
            .child(pick),
    )
    .child(hint(env, "ui_admin_template_hint"))
    .child(submit(env, "ui_admin_apply_template"));
    Some(panel(env, "ui_admin_template_title", form))
}

fn yes_no(env: &Env, on: bool) -> Element {
    span().text(env.t.t(if on {
        "ui_admin_status_yes"
    } else {
        "ui_admin_status_no"
    }))
}

fn status_row(env: &Env, label_key: &str, value: Element, action: Option<Element>) -> Element {
    div()
        .class("gh-status-row")
        .child(span().class("gh-hint").text(env.t.t(label_key)))
        .child(value)
        .child(action)
}

/// A one-button form that posts to `/ui/admin/users/{name}/{suffix}`.
fn action_form(env: &Env, user: &AdminUser, suffix: &str, key: &str) -> Element {
    post_form(env.path(&format!(
        "/admin/users/{}/{suffix}",
        segment(&user.username)
    )))
    .class("gh-inline")
    .attr("data-action", suffix.to_string())
    .child(
        button()
            .attr("type", "submit")
            .class("gh-btn")
            .text(env.t.t(key)),
    )
}

/// Lifecycle and security state, plus the "an administrator can undo this" actions - those omitted, not disabled, for a
/// viewer who cannot edit.
fn status_panel(env: &Env, user: &AdminUser) -> Element {
    let can = user.can_edit;
    let mut rows = div()
        .child(status_row(
            env,
            "ui_admin_status_created",
            span().text(user.created_at.clone()),
            None,
        ))
        .child(user.last_login_at.as_ref().map_or_else(
            || {
                status_row(
                    env,
                    "ui_admin_status_last_login",
                    span().text(env.t.t("ui_admin_status_never")),
                    None,
                )
            },
            |at| {
                status_row(
                    env,
                    "ui_admin_status_last_login",
                    span().text(at.clone()),
                    None,
                )
            },
        ));
    rows = rows
        .child(status_row(
            env,
            "ui_admin_status_disabled",
            yes_no(env, user.disabled),
            (can && !user.is_self).then(|| {
                if user.disabled {
                    action_form(env, user, "enable", "ui_admin_action_enable")
                } else {
                    action_form(env, user, "disable", "ui_admin_action_disable")
                }
            }),
        ))
        .child(status_row(
            env,
            "ui_admin_status_locked",
            yes_no(env, user.locked),
            (can && user.locked)
                .then(|| action_form(env, user, "unlock", "ui_admin_action_unlock")),
        ))
        .child(status_row(
            env,
            "ui_admin_status_mfa",
            yes_no(env, user.mfa_enabled),
            (can && user.mfa_enabled)
                .then(|| action_form(env, user, "mfa/disable", "ui_admin_action_mfa_disable")),
        ));
    // Only where there is an address to confirm; an unconfirmed one can be sent another invitation.
    if user.email.is_some() {
        rows = rows.child(status_row(
            env,
            "ui_admin_status_email_confirmed",
            yes_no(env, user.email_verified),
            (can && !user.email_verified)
                .then(|| action_form(env, user, "invite", "ui_admin_action_resend_invite")),
        ));
    }
    panel(env, "ui_admin_status_title", rows)
}

/// No self-delete control is shown: the realm would refuse it anyway.
fn delete_panel(env: &Env, user: &AdminUser) -> Element {
    let form = post_form(env.path(&format!("/admin/users/{}/delete", segment(&user.username))))
        .attr("id", "delete-form")
        .child(hint(env, "ui_admin_delete_hint"))
        .child(
            button()
                .attr("type", "submit")
                .class("gh-btn gh-danger")
                .text(env.t.t("ui_admin_delete")),
        );
    panel(env, "ui_admin_delete_title", form).class("gh-danger-panel")
}
