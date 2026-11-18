//! The administrator's panel: the numbers, the sending server and its limits, everybody's mailboxes (never their mail), the
//! queue, and the record of what was done. The service refuses all of it to anybody else; the page does not offer it either.

use mailroom_core::admin::{Identity, Settings, SettingsInput, TransportInput};
use mailroom_core::draft::TransportDraft;
use quench_ui::form::Field;
use quench_ui::prelude::*;
use quench_ui::router::link;
use std::rc::Rc;

use super::{Env, area, choice, form_error, labelled, secret_box, text_box, waiting};
use crate::live::{Request, State};

/// Rows per page of the lists.
pub const PAGE: u32 = 50;

const SECTIONS: [(&str, &str); 5] = [
    ("overview", "mr_admin_overview"),
    ("settings", "mr_admin_settings"),
    ("mailboxes", "mr_admin_mailboxes"),
    ("queue", "mr_admin_queue"),
    ("audit", "mr_admin_audit"),
];

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

fn stat(env: &Env, key: &str, value: u32, tone: &str) -> Element {
    div()
        .class(format!("mr-card mr-stat mr-{tone}"))
        .attr("data-stat", key.to_string())
        .child(div().class("mr-stat-value").text(value.to_string()))
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
            .child(stat(&env, "mr_admin_stat_accounts", s.accounts, "plain"))
            .child(stat(
                &env,
                "mr_admin_stat_failing",
                s.accounts_failing,
                if s.accounts_failing > 0 {
                    "red"
                } else {
                    "plain"
                },
            ))
            .child(stat(&env, "mr_admin_stat_messages", s.messages, "plain"))
            .child(stat(&env, "mr_admin_stat_queued", s.queued, "plain"))
            .child(stat(&env, "mr_admin_stat_sent", s.sent_24h, "green"))
            .child(stat(
                &env,
                "mr_admin_stat_failed",
                s.failed_24h,
                if s.failed_24h > 0 { "red" } else { "plain" },
            ))
            .child(stat(&env, "mr_admin_stat_budget", s.budget_left, "plain"))
            .into_view()
    })
}

/// `Name <address>` per line, the default marked with a leading `*`.
#[must_use]
pub fn identities_text(list: &[Identity]) -> String {
    list.iter()
        .map(|i| {
            let star = if i.default { "* " } else { "" };
            if i.name.is_empty() {
                format!("{star}{}", i.address)
            } else {
                format!("{star}{} <{}>", i.name, i.address)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The reverse of [`identities_text`].
#[must_use]
pub fn parse_identities(text: &str) -> Vec<Identity> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|line| {
            let (default, line) = line
                .strip_prefix('*')
                .map_or((false, line), |rest| (true, rest.trim()));
            match line.rsplit_once('<') {
                Some((name, rest)) => Identity {
                    address: rest.trim_end_matches('>').trim().to_string(),
                    name: name.trim().trim_matches('"').to_string(),
                    default,
                },
                None => Identity {
                    address: line.to_string(),
                    name: String::new(),
                    default,
                },
            }
        })
        .collect()
}

fn numbers(text: &str) -> Vec<u16> {
    text.split([',', ' ', '\n'])
        .filter_map(|p| p.trim().parse().ok())
        .collect()
}

fn lines(text: &str) -> Vec<String> {
    text.split([',', '\n'])
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

fn settings_form(env: &Env, state: &State, current: &Settings) -> View {
    let t = current.transport.clone();
    let host = Field::new("transport-host", t.as_ref().map_or("", |t| t.host.as_str()));
    let port = Field::new(
        "transport-port",
        &t.as_ref()
            .map_or_else(|| "587".to_string(), |t| t.port.to_string()),
    );
    let security = Field::new(
        "transport-security",
        t.as_ref().map_or("starttls", |t| t.security.as_str()),
    );
    let user = Field::new(
        "transport-username",
        t.as_ref().map_or("", |t| t.username.as_str()),
    );
    let password = Field::new("transport-password", "");
    let certificate = Field::new("transport-certificate", "");
    let identities = Field::new("identities", &identities_text(&current.identities));
    let hosts = Field::new("private-hosts", &current.private_hosts.join("\n"));
    let ports = Field::new(
        "extra-ports",
        &current
            .extra_ports
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(", "),
    );
    let daily = Field::new("daily-budget", &current.daily_budget.to_string());
    let hourly = Field::new("hourly-limit", &current.hourly_per_source.to_string());
    let max_accounts = Field::new("max-accounts", &current.max_accounts_per_user.to_string());
    let interval = Field::new("sync-interval", &current.sync_interval_secs.to_string());
    let error: Signal<Option<&'static str>> = create_signal(None);

    let save = {
        let (env, error) = (env.clone(), error.clone());
        let (host, port, security, user, password, certificate) = (
            host.clone(),
            port.clone(),
            security.clone(),
            user.clone(),
            password.clone(),
            certificate.clone(),
        );
        let (identities, hosts, ports, daily, hourly, max_accounts, interval) = (
            identities.clone(),
            hosts.clone(),
            ports.clone(),
            daily.clone(),
            hourly.clone(),
            max_accounts.clone(),
            interval.clone(),
        );
        move || {
            let number = |f: &Field| f.value_untracked().trim().parse::<u32>().ok();
            let transport: Option<TransportInput> = if host.value_untracked().trim().is_empty() {
                None
            } else {
                match (TransportDraft {
                    host: host.value_untracked(),
                    port: port.value_untracked(),
                    security: security.value_untracked(),
                    username: user.value_untracked(),
                    password: password.value_untracked(),
                    tls_roots_pem: certificate.value_untracked(),
                })
                .validate()
                {
                    Ok(t) => Some(t),
                    Err(why) => {
                        error.set(Some(why.message()));
                        return;
                    }
                }
            };
            let (Some(daily), Some(hourly), Some(max_accounts), Some(interval)) = (
                number(&daily),
                number(&hourly),
                number(&max_accounts),
                number(&interval),
            ) else {
                error.set(Some("the limits must be whole numbers"));
                return;
            };
            error.set(None);
            (env.ask)(Request::SaveSettings(SettingsInput {
                transport,
                identities: Some(parse_identities(&identities.value_untracked())),
                private_hosts: Some(lines(&hosts.value_untracked())),
                extra_ports: Some(numbers(&ports.value_untracked())),
                daily_budget: Some(daily),
                hourly_per_source: Some(hourly),
                max_accounts_per_user: Some(max_accounts),
                sync_interval_secs: Some(interval),
            }));
        }
    };
    let test = {
        let (env, state) = (env.clone(), state.clone());
        move || {
            state.transport_test.set(None);
            (env.ask)(Request::TestTransport);
        }
    };
    let result = {
        let (env, tested) = (env.clone(), state.transport_test.clone());
        View::dynamic(move || {
            tested.get().map_or(View::Empty, |t| {
                p().attr("id", "transport-result")
                    .class(if t.smtp.ok { "mr-ok" } else { "mr-error" })
                    .text(if t.smtp.ok {
                        env.t.t("mr_account_test_ok")
                    } else {
                        format!("{} — {}", env.t.t("mr_account_test_failed"), t.smtp.error)
                    })
                    .into_view()
            })
        })
    };
    let security_options = vec![
        ("tls".to_string(), env.t.t("mr_account_tls")),
        ("starttls".to_string(), env.t.t("mr_account_starttls")),
    ];
    let has_password = t.as_ref().is_some_and(|t| t.has_password);
    form()
        .attr("id", "settings-form")
        .class("mr-card mr-form")
        .on_submit(save)
        .child(h3().text(env.t.t("mr_admin_transport")))
        .child(
            p().class("mr-muted")
                .text(env.t.t("mr_admin_transport_hint")),
        )
        .child(
            div()
                .class("mr-row")
                .child(labelled(
                    "transport-host",
                    &env.t.t("mr_account_host"),
                    text_box(&host),
                ))
                .child(labelled(
                    "transport-port",
                    &env.t.t("mr_account_port"),
                    text_box(&port),
                ))
                .child(labelled(
                    "transport-security",
                    &env.t.t("mr_account_security"),
                    choice(&security, security_options),
                )),
        )
        .child(
            div()
                .class("mr-row")
                .child(labelled(
                    "transport-username",
                    &env.t.t("mr_account_username"),
                    text_box(&user),
                ))
                .child(labelled(
                    "transport-password",
                    &env.t.t(if has_password {
                        "mr_account_password_keep"
                    } else {
                        "mr_account_password"
                    }),
                    secret_box(&password),
                )),
        )
        .child(labelled(
            "transport-certificate",
            &env.t.t("mr_account_certificate"),
            area(&certificate),
        ))
        .child(h3().text(env.t.t("mr_admin_identities")))
        .child(
            p().class("mr-muted")
                .text(env.t.t("mr_admin_identities_hint")),
        )
        .child(labelled(
            "identities",
            &env.t.t("mr_admin_identities"),
            area(&identities),
        ))
        .child(h3().text(env.t.t("mr_admin_hosts")))
        .child(p().class("mr-muted").text(env.t.t("mr_admin_hosts_hint")))
        .child(labelled(
            "private-hosts",
            &env.t.t("mr_admin_private_hosts"),
            area(&hosts),
        ))
        .child(labelled(
            "extra-ports",
            &env.t.t("mr_admin_extra_ports"),
            text_box(&ports),
        ))
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
                    "hourly-limit",
                    &env.t.t("mr_admin_hourly"),
                    text_box(&hourly),
                ))
                .child(labelled(
                    "max-accounts",
                    &env.t.t("mr_admin_max_accounts"),
                    text_box(&max_accounts),
                ))
                .child(labelled(
                    "sync-interval",
                    &env.t.t("mr_admin_sync_interval"),
                    text_box(&interval),
                )),
        )
        .child(form_error(&error))
        .child(result)
        .child(
            div()
                .class("mr-actions")
                .child(
                    button()
                        .attr("type", "button")
                        .attr("id", "test-transport")
                        .class("mr-btn")
                        .on_click(test)
                        .text(env.t.t("mr_admin_test_transport")),
                )
                .child(
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
            .map_or_else(|| waiting(&env), |s| settings_form(&env, &state, &s))
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
        .child(span().class("mr-muted").text(format!(
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

fn mailboxes(env: &Env, state: &State) -> View {
    (env.ask)(Request::AdminAccounts { offset: 0 });
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || {
        let Some(page) = state.admin_accounts.get() else {
            return waiting(&env);
        };
        let rows = page.items.iter().map(|a| {
            let (ask, id, disabled) = (Rc::clone(&env.ask), a.id.clone(), a.disabled);
            let toggle = {
                let (ask, id) = (Rc::clone(&ask), id.clone());
                button()
                    .attr("type", "button")
                    .class("mr-btn mr-small")
                    .attr("data-action", if disabled { "enable" } else { "disable" })
                    .on_click(move || {
                        ask(Request::SetDisabled {
                            id: id.clone(),
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
                let (ask, id) = (Rc::clone(&ask), id);
                let sure = create_signal(false);
                let (shown, ok, plain) = (
                    sure.clone(),
                    env.t.t("mr_account_remove_confirm"),
                    env.t.t("mr_admin_delete"),
                );
                button()
                    .attr("type", "button")
                    .class("mr-btn mr-small mr-danger")
                    .attr("data-action", "delete")
                    .on_click(move || {
                        if sure.get_untracked() {
                            ask(Request::AdminRemoveAccount(id.clone()));
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
                .attr("data-account", a.id.clone())
                .child(element("td").text(a.owner.clone()))
                .child(element("td").text(format!("{} · {}", a.label, a.address)))
                .child(element("td").text(a.imap_host.clone()))
                .child(element("td").text(a.state.clone()))
                .child(element("td").text(a.messages.to_string()))
                .child(element("td").text(a.last_error.clone().unwrap_or_default()))
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
                    .attr("id", "admin-accounts")
                    .class("mr-table")
                    .child(
                        element("thead").child(
                            element("tr").children(
                                [
                                    "mr_admin_owner",
                                    "mr_admin_mailbox",
                                    "mr_admin_server",
                                    "mr_admin_state",
                                    "mr_admin_messages",
                                    "mr_admin_error",
                                    "",
                                ]
                                .into_iter()
                                .map(|k| {
                                    element("th")
                                        .text(if k.is_empty() {
                                            String::new()
                                        } else {
                                            env.t.t(k)
                                        })
                                        .into_view()
                                }),
                            ),
                        ),
                    )
                    .child(element("tbody").children(rows)),
            )
            .child(pager(&env, 0, page.items.len(), page.total, {
                let ask = Rc::clone(&env.ask);
                move |offset| ask(Request::AdminAccounts { offset })
            }))
            .into_view()
    })
}

fn queue(env: &Env, state: &State) -> View {
    let filter = create_signal(String::new());
    (env.ask)(Request::Outbox {
        status: String::new(),
        offset: 0,
    });
    let (env, state) = (env.clone(), state.clone());
    let filters = {
        let (env, filter) = (env.clone(), filter.clone());
        div()
            .class("mr-toolbar")
            .children(["", "queued", "sent", "failed"].into_iter().map(|status| {
                let (ask, filter, current) = (Rc::clone(&env.ask), filter.clone(), filter.clone());
                button()
                    .attr("type", "button")
                    .class("mr-btn")
                    .attr("data-status", status)
                    .class_if("mr-on", move || current.get() == status)
                    .on_click(move || {
                        filter.set(status.to_string());
                        ask(Request::Outbox {
                            status: status.to_string(),
                            offset: 0,
                        });
                    })
                    .text(if status.is_empty() {
                        env.t.t("mr_admin_filter_all")
                    } else {
                        env.t.t(&format!("mr_admin_status_{status}"))
                    })
                    .into_view()
            }))
    };
    let table_view = View::dynamic({
        let (env, state, filter) = (env.clone(), state.clone(), filter);
        move || {
            let Some(page) = state.outbox.get() else {
                return waiting(&env);
            };
            let rows = page.items.iter().map(|item| {
                let (ask, id, queued) = (
                    Rc::clone(&env.ask),
                    item.id.clone(),
                    item.status == "queued",
                );
                let retry = queued.then(|| {
                    let (ask, id) = (Rc::clone(&ask), id.clone());
                    button()
                        .attr("type", "button")
                        .class("mr-btn mr-small")
                        .attr("data-action", "retry")
                        .on_click(move || ask(Request::Retry(id.clone())))
                        .text(env.t.t("mr_admin_retry"))
                });
                let remove = {
                    let ask = Rc::clone(&ask);
                    button()
                        .attr("type", "button")
                        .class("mr-btn mr-small mr-danger")
                        .attr("data-action", "remove")
                        .on_click(move || ask(Request::RemoveQueued(id.clone())))
                        .text(env.t.t("mr_admin_remove"))
                };
                element("tr")
                    .attr("data-queued", item.id.clone())
                    .child(element("td").text(env.stamp(item.created)))
                    .child(element("td").text(format!("{} ({})", item.source, item.kind)))
                    .child(element("td").text(item.recipients.join(", ")))
                    .child(element("td").text(item.subject.clone()))
                    .child(element("td").text(item.status.clone()))
                    .child(element("td").text(item.attempts.to_string()))
                    .child(element("td").text(item.last_error.clone().unwrap_or_default()))
                    .child(element("td").child(retry).child(remove))
                    .into_view()
            });
            let status = filter.get();
            div()
                .child(
                    element("table")
                        .attr("id", "admin-queue")
                        .class("mr-table")
                        .child(
                            element("thead").child(
                                element("tr").children(
                                    [
                                        "mr_admin_when",
                                        "mr_admin_source",
                                        "mr_admin_recipients",
                                        "mr_admin_subject",
                                        "mr_admin_state",
                                        "mr_admin_attempts",
                                        "mr_admin_error",
                                        "",
                                    ]
                                    .into_iter()
                                    .map(|k| {
                                        element("th")
                                            .text(if k.is_empty() {
                                                String::new()
                                            } else {
                                                env.t.t(k)
                                            })
                                            .into_view()
                                    }),
                                ),
                            ),
                        )
                        .child(element("tbody").children(rows)),
                )
                .child(pager(&env, 0, page.items.len(), page.total, {
                    let ask = Rc::clone(&env.ask);
                    move |offset| {
                        ask(Request::Outbox {
                            status: status.clone(),
                            offset,
                        })
                    }
                }))
                .into_view()
        }
    });
    div()
        .class("mr-card")
        .child(filters)
        .child(table_view)
        .into_view()
}

fn audit(env: &Env, state: &State) -> View {
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
                    .child(
                        element("thead").child(
                            element("tr").children(
                                [
                                    "mr_admin_when",
                                    "mr_admin_actor",
                                    "mr_admin_action",
                                    "mr_admin_target",
                                    "mr_admin_detail",
                                ]
                                .into_iter()
                                .map(|k| element("th").text(env.t.t(k)).into_view()),
                            ),
                        ),
                    )
                    .child(element("tbody").children(rows)),
            )
            .child(pager(&env, 0, page.items.len(), page.total, {
                let ask = Rc::clone(&env.ask);
                move |offset| ask(Request::Audit { offset })
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
            "queue" => queue(&env, &state),
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
