//! The screens that need no session: sign-in, registration, the password reset, and what the links in emails open. Each form
//! is posted by the browser to the service's route of the same name; what comes back is a redirect to the next page with a
//! notice in its query.

use gatehouse_core::notice;
use quench_ui::prelude::*;
use quench_ui::router::RouteContext;

use super::{Env, hidden, hint, input_row, post_form, query, submit, waiting};
use crate::live::{Request, State};

/// The centred panel every one of these screens sits in.
fn card(env: &Env, title_key: &str, body: Element) -> View {
    div()
        .class("gh-auth")
        .child(
            div()
                .class("gh-panel gh-narrow")
                .child(div().class("gh-panel-title").text(env.t.t(title_key)))
                .child(div().class("gh-meta").child(body)),
        )
        .into_view()
}

fn plain_link(env: &Env, rest: &str, key: &str, id: &str) -> Element {
    a().class("gh-hint")
        .attr("id", id.to_string())
        .attr("href", env.path(rest))
        .text(env.t.t(key))
}

/// The same-origin address the person was heading for, which the sign-in carries through (the service checks it again).
fn redirect_field(form: Element, target: Option<&str>) -> Element {
    match target.filter(|t| !t.is_empty()) {
        Some(target) => form.child(hidden("redirect", target)),
        None => form,
    }
}

pub(crate) fn login(env: &Env, _state: &State, ctx: &RouteContext) -> View {
    let q = query(ctx);
    let mut form = post_form(env.path("/login"))
        .attr("id", "login-form")
        .child(input_row(
            env,
            "username",
            "ui_login_username",
            "text",
            "username",
        ))
        .child(input_row(
            env,
            "password",
            "ui_login_password",
            "password",
            "current-password",
        ))
        .child(submit(env, "ui_login_submit"));
    form = redirect_field(form, q.get("redirect"));
    form = form.child(super::notice(env, notice::login(&q)));
    if notice::login_offers_resend(&q) {
        form = form.child(plain_link(
            env,
            "/resend-verification",
            "ui_login_resend_link",
            "resend-link",
        ));
    }
    form = form
        .child(plain_link(
            env,
            "/forgot-password",
            "ui_login_forgot_password",
            "forgot-link",
        ))
        .child(plain_link(
            env,
            "/register",
            "ui_login_register",
            "register-link",
        ));
    card(env, "ui_login_sign_in", form)
}

pub(crate) fn login_mfa(env: &Env, _state: &State, ctx: &RouteContext) -> View {
    let q = query(ctx);
    let form = post_form(env.path("/login/mfa"))
        .attr("id", "mfa-form")
        .child(hidden("pending", q.get("pending").unwrap_or_default()))
        .child(
            input_row(env, "code", "ui_login_mfa_code", "text", "one-time-code")
                .attr("inputmode", "numeric"),
        )
        .child(hint(env, "ui_login_mfa_hint"))
        .child(submit(env, "ui_login_mfa_submit"));
    let form = redirect_field(form, q.get("redirect"));
    let form = form.child(super::notice(env, notice::login_mfa(&q)));
    card(env, "ui_login_mfa_title", form)
}

pub(crate) fn register(env: &Env, _state: &State, ctx: &RouteContext) -> View {
    let q = query(ctx);
    let form = post_form(env.path("/register"))
        .attr("id", "register-form")
        .child(input_row(
            env,
            "username",
            "ui_login_username",
            "text",
            "username",
        ))
        .child(input_row(
            env,
            "email",
            "ui_register_email",
            "email",
            "email",
        ))
        .child(input_row(
            env,
            "password",
            "ui_login_password",
            "password",
            "new-password",
        ))
        .child(submit(env, "ui_register_submit"))
        .child(super::notice(env, notice::register(&q)));
    div()
        .child(card(env, "ui_register_title", form))
        .child(div().class("gh-below").child(plain_link(
            env,
            "/login",
            "ui_register_have_account",
            "login-link",
        )))
        .into_view()
}

pub(crate) fn forgot_password(env: &Env, _state: &State, _ctx: &RouteContext) -> View {
    let form = post_form(env.path("/forgot-password"))
        .attr("id", "forgot-form")
        .child(input_row(
            env,
            "username",
            "ui_login_username",
            "text",
            "username",
        ))
        .child(hint(env, "ui_forgot_password_hint"))
        .child(submit(env, "ui_forgot_password_submit"));
    card(env, "ui_forgot_password_title", form)
}

pub(crate) fn reset_password(env: &Env, _state: &State, ctx: &RouteContext) -> View {
    let q = query(ctx);
    let form = post_form(env.path("/reset-password"))
        .attr("id", "reset-form")
        .child(hidden("token", q.get("token").unwrap_or_default()))
        .child(input_row(
            env,
            "password",
            "ui_reset_new_password",
            "password",
            "new-password",
        ))
        .child(submit(env, "ui_reset_submit"))
        .child(super::notice(env, notice::password_page(&q)));
    card(env, "ui_reset_title", form)
}

pub(crate) fn resend(env: &Env, _state: &State, _ctx: &RouteContext) -> View {
    let form = post_form(env.path("/resend-verification"))
        .attr("id", "resend-form")
        .child(input_row(
            env,
            "username",
            "ui_login_username",
            "text",
            "username",
        ))
        .child(hint(env, "ui_resend_hint"))
        .child(submit(env, "ui_resend_submit"));
    card(env, "ui_resend_title", form)
}

pub(crate) fn accept_invite(env: &Env, _state: &State, ctx: &RouteContext) -> View {
    let q = query(ctx);
    let form = post_form(env.path("/accept-invite"))
        .attr("id", "invite-form")
        .child(hidden("token", q.get("token").unwrap_or_default()))
        .child(input_row(
            env,
            "password",
            "ui_reset_new_password",
            "password",
            "new-password",
        ))
        .child(hint(env, "ui_invite_hint"))
        .child(submit(env, "ui_invite_submit"))
        .child(super::notice(env, notice::password_page(&q)));
    card(env, "ui_invite_title", form)
}

/// The page the link for a new address opens: it shows the address, a button changes it.
pub(crate) fn confirm_email(env: &Env, state: &State, ctx: &RouteContext) -> View {
    let token = query(ctx).get("token").unwrap_or_default().to_string();
    state.confirm.set(None);
    state.invalid_link.set(false);
    (env.ask)(Request::ConfirmEmail(token.clone()));
    let (env, confirm, invalid) = (
        env.clone(),
        state.confirm.clone(),
        state.invalid_link.clone(),
    );
    View::dynamic(move || {
        if invalid.get() {
            return card(
                &env,
                "ui_confirm_email_title",
                div()
                    .child(
                        p().attr("id", "invalid-link")
                            .text(env.t.t("ui_login_confirm_email_invalid")),
                    )
                    .child(plain_link(
                        &env,
                        "/login",
                        "ui_register_have_account",
                        "login-link",
                    )),
            );
        }
        let Some(confirm) = confirm.get() else {
            return waiting(&env);
        };
        let form = post_form(env.path("/confirm-email"))
            .attr("id", "confirm-form")
            .child(hidden("token", &token))
            .child(hint(&env, "ui_confirm_email_hint"))
            .child(p().attr("id", "new-email").text(confirm.new_email))
            .child(submit(&env, "ui_confirm_email_submit"));
        card(&env, "ui_confirm_email_title", form)
    })
}

/// The link in a notification: opening it only asks; the button ends that kind of notification.
pub(crate) fn unsubscribe(env: &Env, state: &State, ctx: &RouteContext) -> View {
    let token = query(ctx).get("token").unwrap_or_default().to_string();
    state.unsubscribe.set(None);
    state.unsubscribed.set(false);
    state.invalid_link.set(false);
    (env.ask)(Request::UnsubscribeInfo(token.clone()));
    let (env, info, done, invalid) = (
        env.clone(),
        state.unsubscribe.clone(),
        state.unsubscribed.clone(),
        state.invalid_link.clone(),
    );
    View::dynamic(move || {
        let message = |id: &str, key: &str| {
            card(
                &env,
                "ui_unsubscribe_title",
                div().child(p().attr("id", id.to_string()).text(env.t.t(key))),
            )
        };
        if invalid.get() {
            return message("invalid-link", "ui_unsubscribe_invalid");
        }
        if done.get() {
            return message("done", "ui_unsubscribe_done");
        }
        let Some(info) = info.get() else {
            return waiting(&env);
        };
        let ask = std::rc::Rc::clone(&env.ask);
        let token = token.clone();
        card(
            &env,
            "ui_unsubscribe_title",
            div()
                .class("gh-form")
                .child(hint(&env, "ui_unsubscribe_hint"))
                .child(p().attr("id", "kind").text(env.t.t(&info.label_key)))
                .child(
                    button()
                        .attr("type", "button")
                        .attr("id", "unsubscribe")
                        .class("gh-btn gh-primary")
                        .on_click(move || ask(Request::Unsubscribe(token.clone())))
                        .text(env.t.t("ui_unsubscribe_submit")),
                ),
        )
    })
}
