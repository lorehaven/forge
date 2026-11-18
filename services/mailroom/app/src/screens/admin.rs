//! The administrator's panel: the numbers, the limits, everybody's mailboxes (never their mail), what the services delivered, and
//! the record of what was done. The service refuses all of it to anybody else; the page does not offer it either.

use mailroom_core::admin::{Settings, SettingsInput};
use quench_ui::form::Field;
use quench_ui::prelude::*;
use quench_ui::router::link;
use std::rc::Rc;

use super::{Env, form_error, labelled, text_box, waiting};
use crate::live::{Request, State};

/// Rows per page of the lists.
pub const PAGE: u32 = 50;

pub(crate) const SECTIONS: [(&str, &str); 5] = [
    ("overview", "mr_admin_overview"),
    ("settings", "mr_admin_settings"),
    ("mailboxes", "mr_admin_mailboxes"),
    ("deliveries", "mr_admin_deliveries"),
    ("audit", "mr_admin_audit"),
];

/// Bytes as megabytes with one decimal (`0.0` for nothing).
#[must_use]
pub fn megabytes(bytes: u64) -> String {
    let tenths = bytes.saturating_mul(10) / (1024 * 1024);
    format!("{}.{} MB", tenths / 10, tenths % 10)
}

fn tabs(env: &Env, current: &str) -> Element {
    div()
        .class("mr-tabs")
        .children(SECTIONS.iter().map(|(section, key)| {
            let href = if *section == "overview" {
                env.path("/admin")
            } else {
                env.path(&format!("/admin/{section}"))
            };
            let mut tab = link(&href)
                .class("mr-tab")
                .attr("data-section", *section)
                .text(env.t.t(key));
            if *section == current {
                tab = tab.attr("aria-current", "page").class("mr-current");
            }
            tab.into_view()
        }))
}

fn stat(env: &Env, key: &str, value: String, tone: &str) -> Element {
    div()
        .class(format!("mr-card mr-stat mr-{tone}"))
        .attr("data-stat", key.to_string())
        .child(div().class("mr-stat-value").text(value))
        .child(div().class("mr-muted").text(env.t.t(key)))
}

fn overview(env: &Env, state: &State) -> View {
    (env.ask)(Request::Stats);
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || {
        let Some(s) = state.stats.get() else {
            return waiting(&env);
        };
        div()
            .class("mr-stats")
            .child(stat(
                &env,
                "mr_admin_stat_mailboxes",
                s.mailboxes.to_string(),
                "plain",
            ))
            .child(stat(
                &env,
                "mr_admin_stat_messages",
                s.messages.to_string(),
                "plain",
            ))
            .child(stat(
                &env,
                "mr_admin_stat_storage",
                megabytes(s.bytes),
                "plain",
            ))
            .child(stat(
                &env,
                "mr_admin_stat_delivered",
                s.delivered_24h.to_string(),
                "green",
            ))
            .child(stat(
                &env,
                "mr_admin_stat_failed",
                s.failed_24h.to_string(),
                if s.failed_24h > 0 { "red" } else { "plain" },
            ))
            .child(stat(
                &env,
                "mr_admin_stat_budget",
                s.budget_left.to_string(),
                "plain",
            ))
            .into_view()
    })
}

fn settings_form(env: &Env, current: &Settings) -> View {
    let daily = Field::new("daily-budget", &current.daily_budget.to_string());
    let per_source = Field::new("hourly-source", &current.hourly_per_source.to_string());
    let per_person = Field::new("hourly-person", &current.hourly_per_person.to_string());
    let quota = Field::new("quota-mb", &current.quota_mb.to_string());
    let error: Signal<Option<&'static str>> = create_signal(None);

    let save = {
        let (env, error) = (env.clone(), error.clone());
        let (daily, per_source, per_person, quota) = (
            daily.clone(),
            per_source.clone(),
            per_person.clone(),
            quota.clone(),
        );
        move || {
            let number = |f: &Field| f.value_untracked().trim().parse::<u32>().ok();
            let (Some(daily), Some(per_source), Some(per_person), Some(quota)) = (
                number(&daily),
                number(&per_source),
                number(&per_person),
                number(&quota),
            ) else {
                error.set(Some("the limits must be whole numbers"));
                return;
            };
            error.set(None);
            (env.ask)(Request::SaveSettings(SettingsInput {
                daily_budget: Some(daily),
                hourly_per_source: Some(per_source),
                hourly_per_person: Some(per_person),
                quota_mb: Some(quota),
            }));
        }
    };
    form()
        .attr("id", "settings-form")
        .class("mr-card mr-form")
        .on_submit(save)
        .child(p().attr("id", "domain").class("mr-muted").text(format!(
            "{} @{}",
            env.t.t("mr_admin_domain"),
            current.domain
        )))
        .child(h3().text(env.t.t("mr_admin_limits")))
        .child(
            div()
                .class("mr-row")
                .child(labelled(
                    "daily-budget",
                    &env.t.t("mr_admin_daily_budget"),
                    text_box(&daily),
                ))
                .child(labelled(
                    "hourly-source",
                    &env.t.t("mr_admin_hourly"),
                    text_box(&per_source),
                ))
                .child(labelled(
                    "hourly-person",
                    &env.t.t("mr_admin_hourly_person"),
                    text_box(&per_person),
                ))
                .child(labelled(
                    "quota-mb",
                    &env.t.t("mr_admin_quota"),
                    text_box(&quota),
                )),
        )
        .child(form_error(&error))
        .child(
            div().class("mr-actions").child(
                button()
                    .attr("type", "submit")
                    .attr("id", "save-settings")
                    .class("mr-btn mr-primary")
                    .text(env.t.t("mr_admin_save")),
            ),
        )
        .into_view()
}

fn settings(env: &Env, state: &State) -> View {
    (env.ask)(Request::Settings);
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || {
        state
            .settings
            .get()
            .map_or_else(|| waiting(&env), |s| settings_form(&env, &s))
    })
}

fn pager(
    env: &Env,
    offset: u32,
    shown: usize,
    total: u32,
    go: impl Fn(u32) + Clone + 'static,
) -> Element {
    let (older, newer) = (offset + PAGE < total, offset > 0);
    let (back, forward) = (go.clone(), go);
    div()
        .class("mr-pager")
        .child(span().attr("id", "range").class("mr-muted").text(format!(
            "{}–{} / {total}",
            if shown == 0 { 0 } else { offset + 1 },
            offset as usize + shown
        )))
        .child(
            button()
                .attr("type", "button")
                .attr("id", "newer")
                .class("mr-btn")
                .attr_bool("disabled", move || !newer)
                .on_click(move || back(offset.saturating_sub(PAGE)))
                .text(env.t.t("mr_list_newer")),
        )
        .child(
            button()
                .attr("type", "button")
                .attr("id", "older")
                .class("mr-btn")
                .attr_bool("disabled", move || !older)
                .on_click(move || forward(offset + PAGE))
                .text(env.t.t("mr_list_older")),
        )
}

fn head(env: &Env, keys: &[&str]) -> Element {
    element("thead").child(element("tr").children(keys.iter().map(|k| {
        element("th")
            .text(if k.is_empty() {
                String::new()
            } else {
                env.t.t(k)
            })
            .into_view()
    })))
}

fn mailboxes(env: &Env, state: &State) -> View {
    // Where the list is, for the pager: a page does not know its own offset.
    let at = create_signal(0u32);
    (env.ask)(Request::Mailboxes { offset: 0 });
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || {
        let Some(page) = state.mailboxes.get() else {
            return waiting(&env);
        };
        let rows = page.items.iter().map(|m| {
            let (ask, owner, disabled) = (Rc::clone(&env.ask), m.owner.clone(), m.disabled);
            let toggle = {
                let (ask, owner) = (Rc::clone(&ask), owner.clone());
                button()
                    .attr("type", "button")
                    .class("mr-btn mr-small")
                    .attr("data-action", if disabled { "enable" } else { "disable" })
                    .on_click(move || {
                        ask(Request::SetDisabled {
                            owner: owner.clone(),
                            disabled: !disabled,
                        })
                    })
                    .text(env.t.t(if disabled {
                        "mr_admin_enable"
                    } else {
                        "mr_admin_disable"
                    }))
            };
            let remove = {
                let (ask, owner) = (Rc::clone(&ask), owner);
                let sure = create_signal(false);
                let (shown, ok, plain) = (
                    sure.clone(),
                    env.t.t("mr_admin_delete_confirm"),
                    env.t.t("mr_admin_delete"),
                );
                button()
                    .attr("type", "button")
                    .class("mr-btn mr-small mr-danger")
                    .attr("data-action", "delete")
                    .on_click(move || {
                        if sure.get_untracked() {
                            ask(Request::RemoveMailbox(owner.clone()));
                        } else {
                            sure.set(true);
                        }
                    })
                    .text_fn(move || {
                        if shown.get() {
                            ok.clone()
                        } else {
                            plain.clone()
                        }
                    })
            };
            element("tr")
                .attr("data-mailbox", m.owner.clone())
                .child(element("td").text(m.owner.clone()))
                .child(element("td").text(m.address.clone()))
                .child(element("td").text(m.messages.to_string()))
                .child(element("td").text(megabytes(m.bytes)))
                .child(
                    element("td").text(
                        m.last_seen
                            .map_or_else(|| env.t.t("mr_admin_never"), |seen| env.stamp(seen)),
                    ),
                )
                .child(element("td").text(if disabled {
                    env.t.t("mr_admin_off")
                } else {
                    String::new()
                }))
                .child(element("td").child(toggle).child(remove))
                .into_view()
        });
        div()
            .class("mr-card")
            .child(
                p().class("mr-muted")
                    .text(env.t.t("mr_admin_mailboxes_hint")),
            )
            .child(
                element("table")
                    .attr("id", "admin-mailboxes")
                    .class("mr-table")
                    .child(head(
                        &env,
                        &[
                            "mr_admin_owner",
                            "mr_admin_address",
                            "mr_admin_messages",
                            "mr_admin_size",
                            "mr_admin_last_seen",
                            "mr_admin_status",
                            "",
                        ],
                    ))
                    .child(element("tbody").children(rows)),
            )
            .child(pager(&env, at.get(), page.items.len(), page.total, {
                let (ask, at) = (Rc::clone(&env.ask), at.clone());
                move |offset| {
                    at.set(offset);
                    ask(Request::Mailboxes { offset })
                }
            }))
            .into_view()
    })
}

fn deliveries(env: &Env, state: &State) -> View {
    let at = create_signal(0u32);
    (env.ask)(Request::Deliveries { offset: 0 });
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || {
        let Some(page) = state.deliveries.get() else {
            return waiting(&env);
        };
        let rows = page.items.iter().map(|d| {
            element("tr")
                .attr("data-delivery", d.id.clone())
                .child(element("td").text(env.stamp(d.created)))
                .child(element("td").text(if d.kind.is_empty() {
                    d.source.clone()
                } else {
                    format!("{} ({})", d.source, d.kind)
                }))
                .child(element("td").text(d.recipients.join(", ")))
                .child(element("td").text(d.subject.clone()))
                .child(element("td").text(d.status.clone()))
                .child(element("td").text(d.delivered.to_string()))
                .child(element("td").text(d.error.clone().unwrap_or_default()))
                .into_view()
        });
        div()
            .class("mr-card")
            .child(
                p().class("mr-muted")
                    .text(env.t.t("mr_admin_deliveries_hint")),
            )
            .child(
                element("table")
                    .attr("id", "admin-deliveries")
                    .class("mr-table")
                    .child(head(
                        &env,
                        &[
                            "mr_admin_when",
                            "mr_admin_source",
                            "mr_admin_recipients",
                            "mr_admin_subject",
                            "mr_admin_status",
                            "mr_admin_delivered",
                            "mr_admin_error",
                        ],
                    ))
                    .child(element("tbody").children(rows)),
            )
            .child(pager(&env, at.get(), page.items.len(), page.total, {
                let (ask, at) = (Rc::clone(&env.ask), at.clone());
                move |offset| {
                    at.set(offset);
                    ask(Request::Deliveries { offset })
                }
            }))
            .into_view()
    })
}

fn audit(env: &Env, state: &State) -> View {
    let at = create_signal(0u32);
    (env.ask)(Request::Audit { offset: 0 });
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || {
        let Some(page) = state.audit.get() else {
            return waiting(&env);
        };
        let rows = page.items.iter().map(|e| {
            element("tr")
                .child(element("td").text(env.stamp(e.at)))
                .child(element("td").text(e.actor.clone()))
                .child(element("td").text(e.action.clone()))
                .child(element("td").text(e.target.clone()))
                .child(element("td").text(e.detail.clone()))
                .into_view()
        });
        div()
            .class("mr-card")
            .child(
                element("table")
                    .attr("id", "admin-audit")
                    .class("mr-table")
                    .child(head(
                        &env,
                        &[
                            "mr_admin_when",
                            "mr_admin_actor",
                            "mr_admin_action",
                            "mr_admin_target",
                            "mr_admin_detail",
                        ],
                    ))
                    .child(element("tbody").children(rows)),
            )
            .child(pager(&env, at.get(), page.items.len(), page.total, {
                let (ask, at) = (Rc::clone(&env.ask), at.clone());
                move |offset| {
                    at.set(offset);
                    ask(Request::Audit { offset })
                }
            }))
            .into_view()
    })
}

/// The panel, one section of it. A person who is not an administrator is told so (the service would refuse anyway).
#[must_use]
pub(crate) fn page(env: &Env, state: &State, section: &str) -> View {
    let (env, state, section) = (env.clone(), state.clone(), section.to_string());
    View::dynamic(move || -> View {
        let Some(me) = state.me.get() else {
            return waiting(&env);
        };
        if !me.admin {
            return div()
                .class("mr-page")
                .child(
                    p().attr("id", "not-allowed")
                        .class("fui-empty")
                        .text(env.t.t("mr_admin_not_allowed")),
                )
                .into_view();
        }
        let body = match section.as_str() {
            "settings" => settings(&env, &state),
            "mailboxes" => mailboxes(&env, &state),
            "deliveries" => deliveries(&env, &state),
            "audit" => audit(&env, &state),
            _ => overview(&env, &state),
        };
        div()
            .class("mr-page")
            .child(
                h1().attr("id", "admin-title")
                    .text(env.t.t("mr_admin_title")),
            )
            .child(tabs(&env, &section))
            .child(body)
            .into_view()
    })
}
