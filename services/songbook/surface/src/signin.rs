//! The sign-in: the server, a user name and a password. The password goes to the host once and is not kept.

use std::rc::Rc;

use quench_ui::prelude::*;

/// What the form hands over: the server, the user name and the password.
pub type Submit = Rc<dyn Fn(String, String, String)>;

/// The form. `error` says why the last try failed; `busy` is true while one is being made.
#[must_use]
pub fn form(server: &str, error: Signal<String>, busy: &Signal<bool>, submit: Submit) -> View {
    let (srv, usr, pwd) = (
        create_signal(server.to_string()),
        create_signal(String::new()),
        create_signal(String::new()),
    );
    let (server_now, user_now, password_now) = (srv.clone(), usr.clone(), pwd.clone());
    let working = busy.clone();
    let go = move || {
        if working.get_untracked() {
            return;
        }
        submit(
            server_now.get_untracked().trim().to_string(),
            user_now.get_untracked().trim().to_string(),
            password_now.get_untracked(),
        );
    };
    let again = go.clone();
    let label = busy.clone();
    div()
        .attr("id", "signin")
        .class("sb-signin")
        .child(h1().text("Songbook"))
        .child(p().text("Sign in with your account on the server that runs Songbook."))
        .child(
            input()
                .attr("id", "signin-server")
                .attr("type", "url")
                .attr("placeholder", "Server")
                .attr("value", server.to_string())
                .on_input(move |t| srv.set(t.to_string())),
        )
        .child(
            input()
                .attr("id", "signin-user")
                .attr("type", "text")
                .attr("placeholder", "User name")
                .on_input(move |t| usr.set(t.to_string())),
        )
        .child(
            input()
                .attr("id", "signin-password")
                .attr("type", "password")
                .attr("placeholder", "Password")
                .on_input(move |t| pwd.set(t.to_string()))
                .on_keydown(move |k| {
                    if k.key == "Enter" {
                        again();
                    }
                }),
        )
        .child(
            p().attr("id", "signin-error")
                .class("sb-error")
                .text_fn(move || error.get()),
        )
        .child(
            button()
                .attr("id", "signin-go")
                .attr_fn("aria-busy", move || label.get().to_string())
                .text("Sign in")
                .on_click(go),
        )
        .into_view()
}
