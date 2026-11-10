//! One user's access: a row per service, and a popup per service with its checkboxes.
//!
//! The checkboxes live in the user's form, so one Save sends everything; a closed popup is only hidden (its boxes are
//! still part of the form), never removed. A service has the whole-service actions (`perm_<service>_<action>`) and, if the
//! catalog declares resource types, grants for single resources: one storage, one project, one target
//! (`perm_<service>_<type>:<id>:<action>`). The resources come from the services themselves; a grant already held for one
//! that is no longer listed is shown too, so saving never drops it unseen.

use gatehouse_core::{AdminUser, CatalogService, ResourceChoice};
use quench_ui::prelude::*;

use super::Env;

/// A scoped grant the user already holds, split into its parts.
struct Held<'a> {
    resource_type: &'a str,
    id: &'a str,
    action: &'a str,
}

/// `storage:media:read` as its type, id and action. The id is whatever lies between the type and the last colon, as the
/// services read it.
fn split<'a>(service: &CatalogService, grant: &'a str) -> Option<Held<'a>> {
    let (resource_type, rest) = grant.split_once(':')?;
    let (id, action) = rest.rsplit_once(':')?;
    service
        .resource_types
        .iter()
        .any(|known| known == resource_type)
        .then_some(Held {
            resource_type,
            id,
            action,
        })
}

fn held_actions<'a>(user: &'a AdminUser, service: &str) -> Vec<&'a str> {
    user.held
        .iter()
        .filter(|g| g.service == service)
        .flat_map(|g| g.actions.iter().map(String::as_str))
        .collect()
}

fn held_plain(user: &AdminUser, service: &str, action: &str) -> bool {
    user.wildcard || held_actions(user, service).contains(&action)
}

fn held_scoped(
    user: &AdminUser,
    service: &CatalogService,
    rtype: &str,
    id: &str,
    action: &str,
) -> bool {
    user.wildcard
        || held_actions(user, &service.name).iter().any(|grant| {
            split(service, grant)
                .is_some_and(|h| h.resource_type == rtype && h.id == id && h.action == action)
        })
}

/// A checkbox in the user's form; locked when the viewer may not change it or the user reaches everything anyway.
fn tick(name: String, label: &str, checked: bool, locked: bool) -> Element {
    let mut checkbox = input()
        .attr("type", "checkbox")
        .attr("id", name.clone())
        .attr("name", name.clone())
        .attr_bool("checked", move || checked);
    if locked {
        checkbox = checkbox.attr("disabled", "disabled");
    }
    div()
        .class("gh-check")
        .child(checkbox)
        .child(label_for(name, label))
}

fn label_for(name: String, text: &str) -> Element {
    label().attr("for", name).text(text.to_string())
}

/// `Storages`, `Projects`, `Targets`; the type's own name for one the translations do not know.
fn type_title(env: &Env, resource_type: &str) -> String {
    let key = format!("ui_admin_resource_{resource_type}");
    let text = env.t.t(&key);
    if text == key {
        resource_type.to_string()
    } else {
        text
    }
}

/// What the row says without opening the popup: the whole-service actions held, and how many single resources.
fn summary(env: &Env, user: &AdminUser, service: &CatalogService) -> String {
    if user.wildcard {
        return env.t.t("ui_admin_grants_all");
    }
    let plain: Vec<&str> = service
        .actions
        .iter()
        .map(String::as_str)
        .filter(|a| held_plain(user, &service.name, a))
        .collect();
    let scoped = held_actions(user, &service.name)
        .iter()
        .filter(|g| split(service, g).is_some())
        .count();
    let mut parts = Vec::new();
    if !plain.is_empty() {
        parts.push(plain.join(" + "));
    }
    if scoped > 0 {
        parts.push(format!("{scoped} {}", env.t.t("ui_admin_access_scoped")));
    }
    if parts.is_empty() {
        env.t.t("ui_admin_grants_none")
    } else {
        parts.join(" · ")
    }
}

/// The whole-service boxes.
fn whole(env: &Env, user: &AdminUser, service: &CatalogService, locked: bool) -> Element {
    let mut boxes = div().class("gh-matrix-actions");
    for action in &service.actions {
        boxes = boxes.child(tick(
            format!("perm_{}_{action}", service.name),
            action,
            held_plain(user, &service.name, action),
            locked,
        ));
    }
    div()
        .class("gh-access-block")
        .child(
            div()
                .class("gh-section-title")
                .text(env.t.t("ui_admin_access_whole")),
        )
        .child(
            p().class("gh-hint")
                .text(env.t.t("ui_admin_access_whole_hint")),
        )
        .child(boxes)
}

/// Every resource of a type that is listed, then the ones held that are not.
fn resources_of(
    user: &AdminUser,
    service: &CatalogService,
    resource_type: &str,
) -> (Vec<(ResourceChoice, bool)>, Option<String>) {
    let listed = user
        .resources
        .iter()
        .find(|kind| kind.service == service.name && kind.resource_type == resource_type);
    let mut rows: Vec<(ResourceChoice, bool)> = listed
        .map(|kind| kind.items.iter().cloned().map(|c| (c, true)).collect())
        .unwrap_or_default();
    for grant in held_actions(user, &service.name) {
        let Some(held) = split(service, grant) else {
            continue;
        };
        if held.resource_type == resource_type && !rows.iter().any(|(c, _)| c.id == held.id) {
            rows.push((
                ResourceChoice {
                    id: held.id.to_string(),
                    label: held.id.to_string(),
                },
                false,
            ));
        }
    }
    (rows, listed.and_then(|kind| kind.error.clone()))
}

/// One kind of resource: a row per resource with a box per action.
fn scoped(
    env: &Env,
    user: &AdminUser,
    service: &CatalogService,
    resource_type: &str,
    locked: bool,
) -> Element {
    let (rows, error) = resources_of(user, service, resource_type);
    let mut block = div()
        .class("gh-access-block")
        .attr("data-resource-type", resource_type.to_string())
        .child(
            div()
                .class("gh-section-title")
                .text(type_title(env, resource_type)),
        )
        .child(
            p().class("gh-hint")
                .text(env.t.t("ui_admin_access_scoped_hint")),
        );
    if let Some(why) = error {
        block = block.child(
            p().class("gh-hint gh-access-error")
                .text(format!("{} {why}", env.t.t("ui_admin_access_load_failed"))),
        );
    } else if rows.is_empty() {
        block = block.child(
            p().class("gh-hint")
                .text(env.t.t("ui_admin_access_none_listed")),
        );
    }
    let mut list = div().class("gh-resources");
    for (choice, listed) in rows {
        let mut boxes = div().class("gh-matrix-actions");
        for action in &service.actions {
            boxes = boxes.child(tick(
                format!(
                    "perm_{}_{resource_type}:{}:{action}",
                    service.name, choice.id
                ),
                action,
                held_scoped(user, service, resource_type, &choice.id, action),
                locked,
            ));
        }
        let mut name = div()
            .class("gh-resource-name")
            .child(span().text(choice.label.clone()));
        if !listed {
            name = name.child(
                span()
                    .class("gh-hint")
                    .text(env.t.t("ui_admin_access_not_listed")),
            );
        }
        list = list.child(
            div()
                .class("gh-resource-row")
                .attr("data-resource", choice.id.clone())
                .child(name)
                .child(boxes),
        );
    }
    block.child(list)
}

/// The popup of one service. Always in the page; `open` shows it.
fn popup(
    env: &Env,
    user: &AdminUser,
    service: &CatalogService,
    locked: bool,
    open: &Signal<bool>,
) -> Element {
    let mut body = div()
        .class("gh-modal-body")
        .child(whole(env, user, service, locked));
    for resource_type in &service.resource_types {
        body = body.child(scoped(env, user, service, resource_type, locked));
    }
    let (on_close, on_done) = (open.clone(), open.clone());
    let show = open.clone();
    let title_id = format!("access-title-{}", service.name);
    div()
        .class("gh-modal")
        .class_if("gh-modal-open", move || show.get())
        .attr("id", format!("access-{}", service.name))
        .attr("role", "dialog")
        .attr("aria-modal", "true")
        .attr("aria-labelledby", title_id.clone())
        .child(
            div()
                .class("gh-modal-card")
                .child(
                    div()
                        .class("gh-modal-head")
                        .child(
                            span()
                                .attr("id", title_id)
                                .class("gh-modal-title")
                                .text(service.label.clone()),
                        )
                        .child(
                            button()
                                .attr("type", "button")
                                .attr("id", format!("access-close-{}", service.name))
                                .attr("aria-label", env.t.t("ui_admin_access_close"))
                                .class("gh-btn gh-modal-close")
                                .text("×")
                                .on_click(move || on_close.set(false)),
                        ),
                )
                .child(body)
                .child(
                    div()
                        .class("gh-modal-foot")
                        .child(
                            p().class("gh-hint")
                                .text(env.t.t("ui_admin_access_save_hint")),
                        )
                        .child(
                            button()
                                .attr("type", "button")
                                .attr("id", format!("access-done-{}", service.name))
                                .class("gh-btn")
                                .text(env.t.t("ui_admin_access_done"))
                                .on_click(move || on_done.set(false)),
                        ),
                ),
        )
}

/// The Access section: a row per service with what it holds, and the button that opens its popup.
pub fn access(env: &Env, user: &AdminUser) -> Element {
    // Disabled, not omitted, for a wildcard target or a viewer who may not edit.
    let locked = user.wildcard || !user.can_edit;
    let mut rows = div().class("gh-access-list");
    for service in &user.services {
        let open = create_signal(false);
        let opener = open.clone();
        rows = rows.child(
            div()
                .class("gh-access-row")
                .attr("data-service", service.name.clone())
                .child(span().class("gh-service").text(service.label.clone()))
                .child(
                    span()
                        .class("gh-access-summary")
                        .text(summary(env, user, service)),
                )
                .child(
                    button()
                        .attr("type", "button")
                        .attr("id", format!("access-open-{}", service.name))
                        .class("gh-btn gh-access-edit")
                        .text(env.t.t(if locked {
                            "ui_admin_access_view"
                        } else {
                            "ui_admin_access_edit"
                        }))
                        .on_click(move || opener.set(true)),
                )
                .child(popup(env, user, service, locked, &open)),
        );
    }
    div()
        .attr("id", "access")
        .class("gh-access")
        .child(
            div()
                .class("gh-access-title")
                .text(env.t.t("ui_admin_permissions")),
        )
        // Tells the service the form lists every scoped grant, so an unticked one is removed.
        .child(
            input()
                .attr("type", "hidden")
                .attr("name", "scoped_grants")
                .attr("value", "1"),
        )
        .child(rows)
}
