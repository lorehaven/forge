//! The person's mailboxes: what is there, how each is doing, and the form to add or change one.

use mailroom_core::draft::AccountDraft;
use mailroom_core::{Account, Preset};
use quench_ui::form::Field;
use quench_ui::prelude::*;
use std::rc::Rc;

use super::{Env, area, choice, form_error, labelled, secret_box, text_box, waiting};
use crate::live::{Request, State};

fn state_badge(env: &Env, account: &Account) -> Element {
    let (key, tone) = match account.state.as_str() {
        "ok" => ("mr_account_state_ok", "green"),
        "error" => ("mr_account_state_error", "red"),
        "disabled" => ("mr_account_state_disabled", "amber"),
        _ => ("mr_account_state_syncing", "blue"),
    };
    super::badge(&env.t.t(key), tone)
}

fn row(env: &Env, state: &State, editing: &Signal<Option<Account>>, account: &Account) -> View {
    let (ask, id) = (Rc::clone(&env.ask), account.id.clone());
    let refresh = {
        let (ask, id) = (Rc::clone(&ask), id.clone());
        button()
            .attr("type", "button")
            .class("mr-btn mr-small")
            .attr("data-action", "refresh")
            .on_click(move || ask(Request::Refresh(id.clone())))
            .text(env.t.t("mr_account_refresh"))
    };
    let edit = {
        let (editing, account) = (editing.clone(), account.clone());
        button()
            .attr("type", "button")
            .class("mr-btn mr-small")
            .attr("data-action", "edit")
            .on_click(move || editing.set(Some(account.clone())))
            .text(env.t.t("mr_account_edit"))
    };
    let remove = {
        let (ask, id) = (Rc::clone(&ask), id);
        let confirm = create_signal(false);
        let (shown, label) = (confirm.clone(), env.t.t("mr_account_remove_confirm"));
        let plain = env.t.t("mr_account_remove");
        button()
            .attr("type", "button")
            .class("mr-btn mr-small mr-danger")
            .attr("data-action", "remove")
            .on_click(move || {
                if confirm.get_untracked() {
                    ask(Request::RemoveAccount(id.clone()));
                } else {
                    confirm.set(true);
                }
            })
            .text_fn(move || {
                if shown.get() {
                    label.clone()
                } else {
                    plain.clone()
                }
            })
    };
    let error = account
        .last_error
        .clone()
        .map(|e| p().class("mr-error").attr("data-error", "account").text(e));
    let synced = account
        .last_sync
        .map(|t| format!("{} {}", env.t.t("mr_account_last_sync"), env.stamp(t)))
        .unwrap_or_default();
    let _ = state;
    div()
        .class("mr-card mr-account")
        .attr("data-account", account.id.clone())
        .child(
            div()
                .class("mr-head")
                .child(h3().text(format!("{} · {}", account.label, account.address)))
                .child(state_badge(env, account)),
        )
        .child(p().class("mr-muted").text(format!(
            "{} · IMAP {}:{} · SMTP {}:{} · {} {}",
            account.username,
            account.imap_host,
            account.imap_port,
            account.smtp_host,
            account.smtp_port,
            account.unread,
            env.t.t("mr_account_unread")
        )))
        .child(p().class("mr-muted").text(synced))
        .child(error)
        .child(
            div()
                .class("mr-actions")
                .child(refresh)
                .child(edit)
                .child(remove),
        )
        .into_view()
}

/// The values of a preset, to start the form from.
fn apply(preset: &Preset, fields: &Fields) {
    fields.imap_host.set(&preset.imap_host);
    fields.imap_port.set(&preset.imap_port.to_string());
    fields.imap_security.set(&preset.imap_security);
    fields.smtp_host.set(&preset.smtp_host);
    fields.smtp_port.set(&preset.smtp_port.to_string());
    fields.smtp_security.set(&preset.smtp_security);
    fields
        .save_sent
        .set(if preset.save_sent { "yes" } else { "no" });
}

#[derive(Clone)]
struct Fields {
    label: Field,
    address: Field,
    username: Field,
    password: Field,
    imap_host: Field,
    imap_port: Field,
    imap_security: Field,
    smtp_host: Field,
    smtp_port: Field,
    smtp_security: Field,
    signature: Field,
    certificate: Field,
    save_sent: Field,
    preset: Field,
}

impl Fields {
    fn new(from: Option<&Account>) -> Self {
        let text = |name: &str, value: &str| Field::new(name, value);
        Self {
            label: text("account-label", from.map_or("", |a| a.label.as_str())),
            address: text("account-address", from.map_or("", |a| a.address.as_str())),
            username: text("account-username", from.map_or("", |a| a.username.as_str())),
            password: text("account-password", ""),
            imap_host: text(
                "account-imap-host",
                from.map_or("", |a| a.imap_host.as_str()),
            ),
            imap_port: text(
                "account-imap-port",
                &from.map_or_else(|| "993".to_string(), |a| a.imap_port.to_string()),
            ),
            imap_security: text(
                "account-imap-security",
                from.map_or("tls", |a| a.imap_security.as_str()),
            ),
            smtp_host: text(
                "account-smtp-host",
                from.map_or("", |a| a.smtp_host.as_str()),
            ),
            smtp_port: text(
                "account-smtp-port",
                &from.map_or_else(|| "465".to_string(), |a| a.smtp_port.to_string()),
            ),
            smtp_security: text(
                "account-smtp-security",
                from.map_or("tls", |a| a.smtp_security.as_str()),
            ),
            signature: text(
                "account-signature",
                from.map_or("", |a| a.signature.as_str()),
            ),
            certificate: text("account-certificate", ""),
            save_sent: text(
                "account-save-sent",
                if from.is_none_or(|a| a.save_sent) {
                    "yes"
                } else {
                    "no"
                },
            ),
            preset: text("account-preset", ""),
        }
    }

    fn draft(&self) -> AccountDraft {
        AccountDraft {
            label: self.label.value_untracked(),
            address: self.address.value_untracked(),
            username: self.username.value_untracked(),
            password: self.password.value_untracked(),
            imap_host: self.imap_host.value_untracked(),
            imap_port: self.imap_port.value_untracked(),
            imap_security: self.imap_security.value_untracked(),
            smtp_host: self.smtp_host.value_untracked(),
            smtp_port: self.smtp_port.value_untracked(),
            smtp_security: self.smtp_security.value_untracked(),
            signature: self.signature.value_untracked(),
            tls_roots_pem: self.certificate.value_untracked(),
            save_sent: self.save_sent.value_untracked() == "yes",
        }
    }
}

fn security_options(env: &Env) -> Vec<(String, String)> {
    vec![
        ("tls".into(), env.t.t("mr_account_tls")),
        ("starttls".into(), env.t.t("mr_account_starttls")),
    ]
}

/// The add / change form. `editing` is the mailbox being changed, if any.
fn form_view(env: &Env, state: &State, editing: &Signal<Option<Account>>) -> View {
    let (env, state, editing) = (env.clone(), state.clone(), editing.clone());
    View::dynamic(move || -> View {
        let existing = editing.get();
        let fields = Fields::new(existing.as_ref());
        let error: Signal<Option<&'static str>> = create_signal(None);
        let note = existing.as_ref().map(|a| a.id.clone());

        let draft_of = {
            let (fields, error) = (fields.clone(), error.clone());
            let changing = existing.is_some();
            move || match fields.draft().validate(changing) {
                Ok(input) => {
                    error.set(None);
                    Some(input)
                }
                Err(why) => {
                    error.set(Some(why.message()));
                    None
                }
            }
        };
        let save = {
            let (env, draft_of, editing, id) =
                (env.clone(), draft_of.clone(), editing.clone(), note.clone());
            move || {
                if let Some(input) = draft_of() {
                    match &id {
                        Some(id) => (env.ask)(Request::UpdateAccount {
                            id: id.clone(),
                            input,
                        }),
                        None => (env.ask)(Request::AddAccount(input)),
                    }
                    editing.set(None);
                }
            }
        };
        let test = {
            let (env, draft_of, state) = (env.clone(), draft_of, state.clone());
            move || {
                if let Some(mut input) = draft_of() {
                    // The password is needed to try a login; a change that leaves it blank cannot be tried from here.
                    if input.password.is_empty() {
                        input.password = String::new();
                    }
                    state.tested.set(None);
                    (env.ask)(Request::TestAccount(input));
                }
            }
        };
        let presets = state.presets.get();
        let preset_choice = {
            let (fields, known) = (fields.clone(), presets.clone());
            let mut options = vec![(String::new(), env.t.t("mr_account_preset_other"))];
            options.extend(known.iter().map(|p| (p.id.clone(), p.name.clone())));
            let pick = fields.preset.clone();
            choice(&pick, options).on_input(move |value| {
                if let Some(preset) = known.iter().find(|p| p.id == value) {
                    apply(preset, &fields);
                }
            })
        };
        let note_text = {
            let (preset, presets) = (fields.preset.clone(), presets);
            View::dynamic(move || {
                let id = preset.value();
                presets
                    .iter()
                    .find(|p| p.id == id)
                    .map_or(View::Empty, |p| {
                        if p.note.is_empty() {
                            View::Empty
                        } else {
                            p_note(&p.note)
                        }
                    })
            })
        };
        let result = {
            let (env, tested) = (env.clone(), state.tested.clone());
            View::dynamic(move || {
                tested.get().map_or(View::Empty, |t| {
                    let line = |key: &str, step: &mailroom_core::Step| {
                        p().class(if step.ok { "mr-ok" } else { "mr-error" })
                            .attr("data-test", key.to_string())
                            .text(if step.ok {
                                format!("{key}: {}", env.t.t("mr_account_test_ok"))
                            } else {
                                format!(
                                    "{key}: {} — {}",
                                    env.t.t("mr_account_test_failed"),
                                    step.error
                                )
                            })
                    };
                    div()
                        .attr("id", "test-result")
                        .child(line("IMAP", &t.imap))
                        .child(line("SMTP", &t.smtp))
                        .into_view()
                })
            })
        };
        let title = if existing.is_some() {
            env.t.t("mr_account_edit")
        } else {
            env.t.t("mr_account_add")
        };
        let cancel = existing.is_some().then(|| {
            let editing = editing.clone();
            button()
                .attr("type", "button")
                .class("mr-btn")
                .attr("id", "cancel-edit")
                .on_click(move || editing.set(None))
                .text(env.t.t("ui_cancel"))
        });
        form()
            .attr("id", "account-form")
            .class("mr-card mr-form")
            .on_submit(save)
            .child(h3().text(title))
            .child(labelled(
                "account-preset",
                &env.t.t("mr_account_preset"),
                preset_choice,
            ))
            .child(note_text)
            .child(
                div()
                    .class("mr-row")
                    .child(labelled(
                        "account-label",
                        &env.t.t("mr_account_label"),
                        text_box(&fields.label),
                    ))
                    .child(labelled(
                        "account-address",
                        &env.t.t("mr_account_address"),
                        text_box(&fields.address).attr("type", "email"),
                    )),
            )
            .child(
                div()
                    .class("mr-row")
                    .child(labelled(
                        "account-username",
                        &env.t.t("mr_account_username"),
                        text_box(&fields.username),
                    ))
                    .child(labelled(
                        "account-password",
                        &env.t.t(if existing.is_some() {
                            "mr_account_password_keep"
                        } else {
                            "mr_account_password"
                        }),
                        secret_box(&fields.password),
                    )),
            )
            .child(
                div()
                    .class("mr-row")
                    .child(labelled(
                        "account-imap-host",
                        &format!("IMAP · {}", env.t.t("mr_account_host")),
                        text_box(&fields.imap_host),
                    ))
                    .child(labelled(
                        "account-imap-port",
                        &env.t.t("mr_account_port"),
                        text_box(&fields.imap_port),
                    ))
                    .child(labelled(
                        "account-imap-security",
                        &env.t.t("mr_account_security"),
                        choice(&fields.imap_security, security_options(&env)),
                    )),
            )
            .child(
                div()
                    .class("mr-row")
                    .child(labelled(
                        "account-smtp-host",
                        &format!("SMTP · {}", env.t.t("mr_account_host")),
                        text_box(&fields.smtp_host),
                    ))
                    .child(labelled(
                        "account-smtp-port",
                        &env.t.t("mr_account_port"),
                        text_box(&fields.smtp_port),
                    ))
                    .child(labelled(
                        "account-smtp-security",
                        &env.t.t("mr_account_security"),
                        choice(&fields.smtp_security, security_options(&env)),
                    )),
            )
            .child(labelled(
                "account-signature",
                &env.t.t("mr_account_signature"),
                area(&fields.signature),
            ))
            .child(labelled(
                "account-certificate",
                &env.t.t("mr_account_certificate"),
                area(&fields.certificate),
            ))
            .child(labelled(
                "account-save-sent",
                &env.t.t("mr_account_save_sent"),
                choice(
                    &fields.save_sent,
                    vec![
                        ("yes".into(), env.t.t("mr_yes")),
                        ("no".into(), env.t.t("mr_no")),
                    ],
                ),
            ))
            .child(form_error(&error))
            .child(result)
            .child(
                div()
                    .class("mr-actions")
                    .child(cancel)
                    .child(
                        button()
                            .attr("type", "button")
                            .attr("id", "test-account")
                            .class("mr-btn")
                            .on_click(test)
                            .text(env.t.t("mr_account_test")),
                    )
                    .child(
                        button()
                            .attr("type", "submit")
                            .attr("id", "save-account")
                            .class("mr-btn mr-primary")
                            .text(env.t.t("mr_account_save")),
                    ),
            )
            .into_view()
    })
}

fn p_note(text: &str) -> View {
    p().class("mr-muted mr-note")
        .attr("id", "preset-note")
        .text(text.to_string())
        .into_view()
}

/// The mailboxes page.
#[must_use]
pub(crate) fn page(env: &Env, state: &State) -> View {
    let (env, state) = (env.clone(), state.clone());
    let editing: Signal<Option<Account>> = create_signal(None);
    let list = {
        let (env, state, editing) = (env.clone(), state.clone(), editing.clone());
        move || -> View {
            let Some(accounts) = state.accounts.get() else {
                return waiting(&env);
            };
            if accounts.is_empty() {
                return p()
                    .attr("id", "no-accounts")
                    .class("fui-empty")
                    .text(env.t.t("mr_accounts_empty"))
                    .into_view();
            }
            div()
                .class("mr-stack")
                .children(accounts.iter().map(|a| row(&env, &state, &editing, a)))
                .into_view()
        }
    };
    div()
        .class("mr-page")
        .child(
            h1().attr("id", "accounts-title")
                .text(env.t.t("mr_accounts_title")),
        )
        .child(View::dynamic(list))
        .child(form_view(&env, &state, &editing))
        .into_view()
}
