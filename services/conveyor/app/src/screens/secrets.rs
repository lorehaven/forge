//! The secrets the caller may see - names, who set them and when, never a value - and, for whoever may write, setting,
//! overwriting and removing them.
//!
//! Replace overwrites a value and nothing else: a secret's name is what the pipeline files refer to, so a rename is a
//! removal and a new secret, done as two separate steps.

use std::rc::Rc;

use conveyor_core::tree::project_path;
use conveyor_core::{Access, Repo, SecretDraft, SecretRef};
use quench_ui::form::Field;
use quench_ui::prelude::*;

use super::{Env, labelled, nothing, panel, waiting};
use crate::live::{Notice, Request, State};

/// Where a secret lives, as the request names it: `None` is the whole estate.
type Scope = Option<String>;

/// Whether the caller may change the secrets of `scope`.
fn can_write(access: Option<&Access>, repos: &[Repo], scope: &Scope) -> bool {
    let Some(access) = access else {
        return false;
    };
    scope.as_ref().map_or(access.global_write, |id| {
        repos
            .iter()
            .find(|r| &r.id == id)
            .is_some_and(|r| access.can_write(&r.project_id))
    })
}

fn password(field: &Field) -> Element {
    field
        .input()
        .class("cv-input")
        .attr("type", "password")
        .attr("autocomplete", "new-password")
}

pub fn secrets(env: &Env, state: &State) -> View {
    div()
        .attr("id", "secrets")
        .class("cv-page cv-narrow")
        .child(list_panel(env, state))
        .child(add_panel(env, state))
        .into_view()
}

// ── The list ─────────────────────────────────────────────────────────────────

fn list_panel(env: &Env, state: &State) -> View {
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || {
        let body = state.secrets.with(|found| {
            let Some(found) = found.as_ref() else {
                return waiting(&env);
            };
            if found.is_empty() {
                return nothing(&env, "ui_secrets_empty", "no-secrets").into_view();
            }
            let access = state.access.get();
            let repos = state.repos.get().unwrap_or_default();
            let projects = state.projects.get().unwrap_or_default();

            // Estate-wide first, then each repository that has some, in the order the repositories come.
            let mut groups: Vec<(Scope, String)> = Vec::new();
            if found.iter().any(|s| s.repo_id.is_none()) {
                groups.push((None, env.t.t("ui_secrets_global")));
            }
            for repo in &repos {
                if found.iter().any(|s| s.repo_id.as_deref() == Some(&repo.id)) {
                    groups.push((Some(repo.id.clone()), repo.slug()));
                }
            }
            let mut page = div().attr("id", "secret-groups").class("cv-meta-list");
            if !groups
                .iter()
                .any(|(scope, _)| can_write(access.as_ref(), &repos, scope))
            {
                page = page.child(
                    p().attr("id", "view-only")
                        .class("cv-hint")
                        .text(env.t.t("ui_secrets_view_only")),
                );
            }
            for (scope, title) in groups {
                let writable = can_write(access.as_ref(), &repos, &scope);
                let mut group = div().class("cv-secret-group").attr(
                    "data-scope",
                    scope.clone().unwrap_or_else(|| "global".to_string()),
                );
                let mut heading = div()
                    .class("cv-row-main")
                    .child(span().class("cv-repo-name").text(title));
                if let Some(repo) = scope
                    .as_deref()
                    .and_then(|id| repos.iter().find(|r| r.id == id))
                {
                    heading = heading.child(
                        span()
                            .class("cv-project-path")
                            .text(project_path(&repo.project_id, &projects)),
                    );
                }
                // A `cv-row` carries the padding; the heading is not a row of its own kind, so it borrows one.
                group = group.child(div().class("cv-row cv-secret-head").child(heading));
                for secret in found.iter().filter(|s| s.repo_id == scope) {
                    group = group.child(row(&env, &state, secret, writable));
                }
                page = page.child(group);
            }
            page.into_view()
        });
        panel(div().text(env.t.t("ui_secrets_title")))
            .child(
                p().attr("id", "secrets-hint")
                    .class("cv-hint")
                    .text(env.t.t("ui_secrets_hint")),
            )
            .child(body)
            .into_view()
    })
}

/// One secret: its facts, and for a writer the new value, Replace and Remove (which asks twice).
fn row(env: &Env, state: &State, secret: &SecretRef, writable: bool) -> Element {
    let scope: Scope = secret.repo_id.clone();
    let mut row = div()
        .class("cv-row")
        .attr("data-secret", secret.id.clone())
        .child(
            div()
                .class("cv-row-main")
                .child(span().class("cv-mono").text(secret.name.clone()))
                .child(span().class("cv-muted").text("••••••")),
        )
        .child(
            div()
                .class("cv-row-meta")
                .child(span().class("cv-muted").text(secret.created_by.clone()))
                .child(span().class("cv-muted").text(env.since(secret.updated_at))),
        );
    if !writable {
        return row;
    }

    // A fresh field per row; the list is built again when the secrets are read again, which empties it.
    let value = Field::new(&format!("replace-{}", secret.id), "");
    let replace = {
        let (ask, notice, scope, name, value) = (
            Rc::clone(&env.ask),
            state.notice.clone(),
            scope.clone(),
            secret.name.clone(),
            value.clone(),
        );
        move || {
            let draft = SecretDraft {
                name: name.clone(),
                value: value.value_untracked(),
            };
            match draft.to_set() {
                Ok(body) => ask(Request::SetSecret {
                    repo: scope.clone(),
                    name: draft.name(),
                    value: body.value,
                }),
                Err(why) => notice.set(Some(Notice::ErrKey(why.key().to_string()))),
            }
        }
    };
    let armed = create_signal(false);
    let remove = {
        let (ask, scope, name, armed) = (
            Rc::clone(&env.ask),
            scope,
            secret.name.clone(),
            armed.clone(),
        );
        move || {
            if armed.get_untracked() {
                ask(Request::DeleteSecret {
                    repo: scope.clone(),
                    name: name.clone(),
                });
            } else {
                armed.set(true);
            }
        }
    };
    let (ask_text, label_remove, label_confirm) = (
        env.t.t("ui_secrets_value_placeholder"),
        env.t.t("ui_secrets_remove"),
        env.t.t("ui_secrets_confirm_remove"),
    );
    let remove_label = armed;
    row = row
        .child(
            password(&value)
                .attr("placeholder", ask_text)
                .attr("aria-label", secret.name.clone()),
        )
        .child(
            button()
                .attr("type", "button")
                .attr("id", format!("replace-btn-{}", secret.id))
                .class("cv-btn cv-primary")
                .text(env.t.t("ui_secrets_replace"))
                .on_click(replace),
        )
        .child(
            button()
                .attr("type", "button")
                .attr("id", format!("remove-{}", secret.id))
                .class("cv-btn cv-delete")
                .child(View::dynamic(move || {
                    let text = if remove_label.get() {
                        label_confirm.clone()
                    } else {
                        label_remove.clone()
                    };
                    span().text(text).into_view()
                }))
                .on_click(remove),
        );
    row
}

// ── Add ──────────────────────────────────────────────────────────────────────

/// Omitted when there is nowhere the caller may write a secret.
#[allow(
    clippy::too_many_lines,
    reason = "one function builds the add panel, every field wired to its signal"
)]
fn add_panel(env: &Env, state: &State) -> View {
    let (env, state) = (env.clone(), state.clone());
    let scope = Field::new("new-secret-scope", "");
    let name = Field::new("new-secret-name", "");
    let value = Field::new("new-secret-value", "");

    // A secret that was saved leaves an empty form.
    {
        let notice = state.notice.clone();
        let fields = [name.clone(), value.clone()];
        create_effect(move || {
            if matches!(notice.get(), Some(Notice::Ok(key)) if key == "ui_secrets_ok_set") {
                untrack(|| {
                    for field in &fields {
                        field.reset("");
                    }
                });
            }
        });
    }

    View::dynamic(move || {
        let Some(access) = state.access.get() else {
            return View::Empty;
        };
        let repos = state.repos.get().unwrap_or_default();
        let mut options: Vec<(String, String)> = Vec::new();
        if access.global_write {
            options.push(("global".to_string(), env.t.t("ui_secrets_global")));
        }
        for repo in &repos {
            if access.can_write(&repo.project_id) {
                options.push((repo.id.clone(), repo.slug()));
            }
        }
        let Some(first) = options.first().map(|(id, _)| id.clone()) else {
            return View::Empty;
        };
        if scope.value_untracked().is_empty()
            || !options.iter().any(|(id, _)| *id == scope.value_untracked())
        {
            scope.set(&first);
        }

        let pick = scope.clone();
        let choices = options.into_iter().map(|(value, text)| {
            let (field, v) = (scope.clone(), value.clone());
            option()
                .attr("value", value)
                .attr_bool("selected", move || field.value() == v)
                .text(text)
                .into_view()
        });
        let select = select()
            .class("cv-input")
            .attr("id", "new-secret-scope")
            .attr("name", "new-secret-scope")
            .on_input(move |v| pick.set(v))
            .children(choices);

        let send = {
            let (ask, notice) = (Rc::clone(&env.ask), state.notice.clone());
            let (scope, name, value) = (scope.clone(), name.clone(), value.clone());
            move || {
                let draft = SecretDraft {
                    name: name.value_untracked(),
                    value: value.value_untracked(),
                };
                match draft.to_set() {
                    Ok(body) => {
                        notice.set(None);
                        let chosen = scope.value_untracked();
                        ask(Request::SetSecret {
                            repo: (chosen != "global").then_some(chosen),
                            name: draft.name(),
                            value: body.value,
                        });
                    }
                    Err(why) => notice.set(Some(Notice::ErrKey(why.key().to_string()))),
                }
            }
        };
        let form = form()
            .attr("id", "new-secret")
            .class("cv-form")
            .on_submit(send)
            .child(labelled(
                "new-secret-scope",
                &env.t.t("ui_secrets_scope"),
                select,
            ))
            .child(labelled(
                "new-secret-name",
                &env.t.t("ui_secrets_name"),
                name.input()
                    .class("cv-input")
                    .attr("placeholder", "MY_SECRET"),
            ))
            .child(labelled(
                "new-secret-value",
                &env.t.t("ui_secrets_value"),
                password(&value),
            ))
            .child(
                div().class("cv-actions-row").child(
                    button()
                        .attr("type", "submit")
                        .attr("id", "create-secret")
                        .class("cv-btn cv-primary")
                        .text(env.t.t("ui_secrets_add")),
                ),
            );
        panel(div().text(env.t.t("ui_secrets_add_title")))
            .attr("id", "add-secret")
            .child(div().class("cv-meta-list").child(form))
            .into_view()
    })
}
