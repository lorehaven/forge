//! The person's own account: profile and picture, address, notifications, password and MFA; and enrolling an authenticator.
//! The forms are posted by the browser to `/ui/account...`, which redirects back here with `?ok=` or `?err=`.

use gatehouse_core::{Account, MfaDraft, file_name, notice};
use quench_ui::form::Field;
use quench_ui::prelude::*;
use quench_ui::router::{RouteContext, link};

use super::{Env, hint, panel, post_form, query, submit, waiting};
use crate::live::{Problem, Request, State};

pub(crate) fn account(env: &Env, state: &State, ctx: &RouteContext) -> View {
    let flash = notice::account(&query(ctx));
    state.account.set(None);
    (env.ask)(Request::Account);
    let (env, account) = (env.clone(), state.account.clone());
    View::dynamic(move || {
        account.get().map_or_else(
            || waiting(&env),
            |account| {
                div()
                    .class("admin-container")
                    .child(super::notice(&env, flash))
                    .child(profile_panel(&env, &account))
                    .child(email_panel(&env, &account))
                    .child(notifications_panel(&env, &account))
                    .child(password_panel(&env))
                    .child(mfa_panel(&env, &account))
                    .into_view()
            },
        )
    })
}

/// A label on the left, the control on the right.
fn row(env: &Env, id: &str, key: &str, control: Element) -> Element {
    div()
        .class("gh-row")
        .child(label().attr("for", id.to_string()).text(env.t.t(key)))
        .child(control)
}

fn text_row(env: &Env, id: &str, key: &str, value: Option<&str>) -> Element {
    row(
        env,
        id,
        key,
        input()
            .class("gh-input")
            .attr("type", "text")
            .attr("id", id.to_string())
            .attr("name", id.to_string())
            .attr("value", value.unwrap_or_default().to_string()),
    )
}

fn password_row(env: &Env, id: &str, key: &str, autocomplete: &str) -> Element {
    row(
        env,
        id,
        key,
        input()
            .class("gh-input")
            .attr("type", "password")
            .attr("id", id.to_string())
            .attr("name", id.to_string())
            .attr("autocomplete", autocomplete.to_string())
            .attr("required", "required"),
    )
}

/// A choice from a fixed list, blank meaning "not set". A stored value that is no longer in the list stays selectable, so
/// opening the page never silently drops it.
fn choice_row(
    env: &Env,
    id: &str,
    key: &str,
    choices: &[String],
    current: Option<&str>,
) -> Element {
    let current = current.filter(|c| !c.is_empty());
    let mut all: Vec<&str> = choices.iter().map(String::as_str).collect();
    if let Some(value) = current.filter(|c| !all.contains(c)) {
        all.insert(0, value);
    }
    let mut control = select()
        .class("gh-input")
        .attr("id", id.to_string())
        .attr("name", id.to_string())
        .child(option().attr("value", "").text("-"));
    for choice in all {
        let selected = current == Some(choice);
        control = control.child(
            option()
                .attr("value", choice.to_string())
                .attr_bool("selected", move || selected)
                .text(choice.to_string()),
        );
    }
    row(env, id, key, control)
}

/// The native file input is hidden: a button (its label) and a read-only box with the chosen file's name stand in for it.
fn file_picker(env: &Env) -> Element {
    let chosen = create_signal(String::new());
    let show = chosen.clone();
    div()
        .class("gh-file")
        .child(
            input()
                .class("gh-file-native")
                .attr("type", "file")
                .attr("id", "avatar_file")
                .attr("name", "avatar_file")
                .attr("accept", "image/png,image/jpeg,image/gif,image/webp")
                .on_change(move |value| chosen.set(file_name(value).to_string())),
        )
        .child(
            label()
                .class("gh-file-button")
                .attr("for", "avatar_file")
                .text(env.t.t("ui_account_avatar_choose")),
        )
        .child(
            input()
                .class("gh-input gh-file-name")
                .attr("type", "text")
                .attr("readonly", "readonly")
                .attr("tabindex", "-1")
                .attr_fn("value", move || show.get()),
        )
}

fn profile_panel(env: &Env, account: &Account) -> Element {
    let mut form = post_form(env.path("/account"))
        .attr("id", "profile-form")
        .attr("method", "post")
        .attr("enctype", "multipart/form-data")
        .child(text_row(
            env,
            "display_name",
            "ui_account_display_name",
            account.display_name.as_deref(),
        ));
    if account.has_avatar {
        form = form.child(
            div().class("gh-row").child(
                img()
                    .class("gh-avatar")
                    .attr("id", "avatar")
                    .attr("src", env.path("/account/avatar"))
                    .attr("alt", "")
                    .attr("width", "64")
                    .attr("height", "64"),
            ),
        );
    }
    form = form
        .child(text_row(
            env,
            "avatar_url",
            "ui_account_avatar_url",
            account.avatar_url.as_deref(),
        ))
        .child(row(
            env,
            "avatar_file",
            "ui_account_avatar_file",
            file_picker(env),
        ))
        .child(hint(env, "ui_account_avatar_hint"));
    if account.has_avatar {
        form = form.child(row(
            env,
            "avatar_remove",
            "ui_account_avatar_remove",
            input()
                .attr("type", "checkbox")
                .attr("id", "avatar_remove")
                .attr("name", "avatar_remove")
                .attr("value", "1"),
        ));
    }
    form = form
        .child(text_row(
            env,
            "title",
            "ui_account_title_field",
            account.title.as_deref(),
        ))
        .child(choice_row(
            env,
            "timezone",
            "ui_account_timezone",
            &account.timezones,
            account.timezone.as_deref(),
        ))
        .child(choice_row(
            env,
            "preferred_locale",
            "ui_account_preferred_locale",
            &account.locales,
            account.preferred_locale.as_deref(),
        ))
        .child(submit(env, "ui_account_save"));
    panel(env, "ui_account_profile_title", form)
}

/// The address on file (and whether it is confirmed), and the form to change it.
fn email_panel(env: &Env, account: &Account) -> Element {
    let current = account
        .email
        .as_deref()
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map_or_else(
            || div().child(hint(env, "ui_account_email_none")),
            |address| {
                let status = if account.email_verified {
                    "ui_account_email_confirmed"
                } else {
                    "ui_account_email_unconfirmed"
                };
                div()
                    .class("gh-status-row")
                    .child(span().attr("id", "current-email").text(address.to_string()))
                    .child(span().class("gh-hint").text(env.t.t(status)))
            },
        );
    let form = post_form(env.path("/account/email"))
        .attr("id", "email-form")
        .child(row(
            env,
            "email",
            "ui_account_email_new",
            input()
                .class("gh-input")
                .attr("type", "email")
                .attr("id", "email")
                .attr("name", "email")
                .attr("autocomplete", "email")
                .attr("required", "required"),
        ))
        .child(password_row(
            env,
            "email_current_password",
            "ui_account_current_password",
            "current-password",
        ))
        .child(hint(env, "ui_account_email_hint"))
        .child(submit(env, "ui_account_email_submit"));
    panel(
        env,
        "ui_account_email_title",
        div().child(current).child(form),
    )
}

/// A checkbox per kind of service notification, grouped by service. The account security mail is not in this list because it
/// cannot be turned off.
fn notifications_panel(env: &Env, account: &Account) -> Element {
    let mut form = post_form(env.path("/account/notifications")).attr("id", "notifications-form");
    let mut last = "";
    for sub in &account.subscriptions {
        if sub.service != last {
            last = &sub.service;
            form = form.child(
                div()
                    .class("gh-section-title")
                    .text(env.t.t(&format!("ui_notification_service_{}", sub.service))),
            );
        }
        let name = format!("sub_{}", sub.id);
        let wanted = sub.wanted;
        form = form.child(
            div()
                .class("gh-check")
                .child(
                    input()
                        .attr("type", "checkbox")
                        .attr("id", name.clone())
                        .attr("name", name.clone())
                        .attr_bool("checked", move || wanted),
                )
                .child(label().attr("for", name).text(env.t.t(&sub.label_key))),
        );
    }
    form = form
        .child(hint(env, "ui_account_notifications_hint"))
        .child(submit(env, "ui_account_notifications_save"));
    let mut body = div();
    if !account.has_notice_address {
        body = body.child(hint(env, "ui_account_notifications_no_address"));
    }
    panel(env, "ui_account_notifications_title", body.child(form))
}

fn password_panel(env: &Env) -> Element {
    let form = post_form(env.path("/account/password"))
        .attr("id", "password-form")
        .child(password_row(
            env,
            "current_password",
            "ui_account_current_password",
            "current-password",
        ))
        .child(password_row(
            env,
            "new_password",
            "ui_account_new_password",
            "new-password",
        ))
        .child(password_row(
            env,
            "confirm_password",
            "ui_account_confirm_password",
            "new-password",
        ))
        .child(submit(env, "ui_account_password_change"));
    panel(env, "ui_account_password_title", form)
}

/// Whether MFA is on, and the way to flip it.
fn mfa_panel(env: &Env, account: &Account) -> Element {
    let body = if account.mfa_enabled {
        div().child(hint(env, "ui_account_mfa_enabled")).child(
            post_form(env.path("/account/mfa/disable"))
                .attr("id", "mfa-disable-form")
                .child(submit(env, "ui_account_mfa_disable")),
        )
    } else {
        div().child(hint(env, "ui_account_mfa_disabled")).child(
            link(&env.path("/account/mfa/enroll"))
                .attr("id", "mfa-enable")
                .class("gh-btn")
                .text(env.t.t("ui_account_mfa_enable")),
        )
    };
    panel(env, "ui_account_mfa_title", body)
}

/// Enrolling an authenticator: scan the code (or type the secret), then give the first code it shows. The code is checked
/// through the API so that a wrong one leaves the same secret on the page; the secret never travels in a URL.
pub(crate) fn mfa_enroll(env: &Env, state: &State, _ctx: &RouteContext) -> View {
    state.enrollment.set(None);
    (env.ask)(Request::MfaSecret);
    let draft_error: Signal<Option<&'static str>> = create_signal(None);
    let code = Field::new("code", "");
    let (env, enrollment, refused, problem) = (
        env.clone(),
        state.enrollment.clone(),
        state.refused.clone(),
        state.problem.clone(),
    );
    View::dynamic(move || {
        // A person who is not signed in sees the banner; there is nothing to enrol with.
        if matches!(problem.get(), Some(Problem::SignedOut)) {
            return View::Empty;
        }
        let Some(enrollment) = enrollment.get() else {
            return waiting(&env);
        };
        let send = {
            let (env, code, secret, draft_error) = (
                env.clone(),
                code.clone(),
                enrollment.secret.clone(),
                draft_error.clone(),
            );
            move || {
                let draft = MfaDraft {
                    code: code.value_untracked(),
                };
                match draft.validate(&secret) {
                    Ok(body) => {
                        draft_error.set(None);
                        (env.ask)(Request::MfaEnable(body));
                    }
                    Err(why) => draft_error.set(Some(why.key())),
                }
            }
        };
        let (shown, wrong) = (draft_error.clone(), refused.clone());
        let mut form = form()
            .attr("id", "mfa-enroll-form")
            .class("gh-form")
            .on_submit(send)
            .child(hint(&env, "ui_account_mfa_enroll_hint"));
        if let Some(qr) = enrollment.qr.clone() {
            form = form.child(
                div().class("gh-qr").child(
                    img()
                        .attr("id", "qr")
                        .attr("src", qr)
                        .attr("alt", "QR code")
                        .attr("width", "200")
                        .attr("height", "200"),
                ),
            );
        }
        form = form
            .child(p().class("gh-mono").text(enrollment.uri.clone()))
            .child(
                label()
                    .attr("for", "mfa-secret")
                    .text(env.t.t("ui_account_mfa_secret")),
            )
            .child(
                span()
                    .attr("id", "mfa-secret")
                    .class("gh-mono")
                    .text(enrollment.secret.clone()),
            )
            .child(row(
                &env,
                "code",
                "ui_account_mfa_code",
                code.input()
                    .class("gh-input")
                    .attr("inputmode", "numeric")
                    .attr("autocomplete", "one-time-code"),
            ))
            .child(submit(&env, "ui_account_mfa_verify"));
        let message = {
            let env = env.clone();
            View::dynamic(move || {
                match (shown.get(), wrong.get()) {
                    (Some(key), _) => Some(key.to_string()),
                    (None, Some(key)) => Some(key),
                    _ => None,
                }
                .map_or(View::Empty, |key| {
                    p().attr("id", "mfa-error")
                        .class("gh-notice error")
                        .attr("role", "alert")
                        .text(env.t.t(&key))
                        .into_view()
                })
            })
        };
        div()
            .class("admin-container")
            .child(panel(
                &env,
                "ui_account_mfa_enroll_title",
                form.child(message),
            ))
            .into_view()
    })
}
