//! The dialogs over the page: delete a conversation, delete a file, create a project.

use std::rc::Rc;

use quench_ui::form::Field;
use quench_ui::prelude::*;
use sage_core::ProjectDraft;

use super::Env;
use crate::live::{Dialog, Request, State};

fn panel(env: &Env, state: &State, title: String, body: Element, footer: Vec<View>) -> View {
    let close = {
        let state = state.clone();
        move || state.dialog.set(None)
    };
    let _ = env;
    div()
        .attr("id", "dialog")
        .class("fui-modal fui-open")
        .child(
            button()
                .attr("type", "button")
                .class("fui-modal-scrim")
                .attr("aria-label", "close")
                .on_click(close),
        )
        .child(
            div()
                .class("fui-modal-panel")
                .attr("role", "dialog")
                .child(div().class("fui-modal-header").child(h3().text(title)))
                .child(div().class("fui-modal-body").child(body))
                .child(div().class("fui-modal-footer").children(footer)),
        )
        .into_view()
}

fn confirm(
    env: &Env,
    state: &State,
    title_key: &str,
    text_key: &str,
    name: &str,
    on_yes: Rc<dyn Fn()>,
) -> View {
    let cancel = {
        let state = state.clone();
        button()
            .attr("type", "button")
            .attr("id", "dialog-cancel")
            .class("fui-btn fui-btn-secondary")
            .on_click(move || state.dialog.set(None))
            .text(env.t.t("ui_common_cancel"))
            .into_view()
    };
    let yes = button()
        .attr("type", "button")
        .attr("id", "dialog-confirm")
        .class("fui-btn fui-btn-danger")
        .on_click(move || on_yes())
        .text(env.t.t("ui_common_delete"))
        .into_view();
    panel(
        env,
        state,
        env.t.t(title_key),
        div()
            .child(p().text(env.t.t(text_key)))
            .child(p().class("sg-dialog-name").text(name.to_string())),
        vec![cancel, yes],
    )
}

fn new_project(env: &Env, state: &State) -> View {
    let name = Field::new("project-name", "");
    let error: Signal<Option<&'static str>> = create_signal(None);
    let send = {
        let (env, name, error) = (env.clone(), name.clone(), error.clone());
        move || {
            let draft = ProjectDraft {
                name: name.value_untracked(),
            };
            match draft.body() {
                Ok(body) => {
                    error.set(None);
                    (env.ask)(Request::CreateProject(body));
                }
                Err(why) => error.set(Some(why.0)),
            }
        }
    };
    let cancel = {
        let state = state.clone();
        button()
            .attr("type", "button")
            .attr("id", "dialog-cancel")
            .class("fui-btn fui-btn-secondary")
            .on_click(move || state.dialog.set(None))
            .text(env.t.t("ui_common_cancel"))
            .into_view()
    };
    let create = button()
        .attr("type", "submit")
        .attr("id", "dialog-create")
        .attr("form", "new-project-form")
        .class("fui-btn fui-btn-primary")
        .text(env.t.t("ui_projects_create"))
        .into_view();
    let shown = {
        let (env, error) = (env.clone(), error);
        View::dynamic(move || match error.get() {
            Some(key) => p()
                .attr("id", "project-error")
                .class("sg-field-error")
                .text(env.t.t(key))
                .into_view(),
            None => View::Empty,
        })
    };
    panel(
        env,
        state,
        env.t.t("ui_projects_new_title"),
        form()
            .attr("id", "new-project-form")
            .on_submit(send)
            .child(
                label()
                    .attr("for", "project-name")
                    .class("fui-label")
                    .text(env.t.t("ui_projects_name_label")),
            )
            .child(
                name.input()
                    .class("fui-input")
                    .attr("maxlength", "120")
                    .attr("autofocus", "autofocus"),
            )
            .child(shown),
        vec![cancel, create],
    )
}

pub(crate) fn dialogs(env: &Env, state: &State) -> View {
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || match state.dialog.get() {
        None => View::Empty,
        Some(Dialog::DeleteConversation { id, title }) => {
            let ask = Rc::clone(&env.ask);
            confirm(
                &env,
                &state,
                "ui_modal_delete_title",
                "ui_chat_delete_confirm_text",
                &format!("\"{title}\""),
                Rc::new(move || ask(Request::DeleteConversation(id.clone()))),
            )
        }
        Some(Dialog::DeleteFile { id, name }) => {
            let ask = Rc::clone(&env.ask);
            confirm(
                &env,
                &state,
                "ui_modal_delete_title",
                "ui_files_delete_confirm_text",
                &format!("\"{name}\""),
                Rc::new(move || ask(Request::DeleteFile(id.clone()))),
            )
        }
        Some(Dialog::NewProject) => new_project(&env, &state),
    })
}
