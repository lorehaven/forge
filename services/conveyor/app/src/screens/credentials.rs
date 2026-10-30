//! The credentials the caller may read: never the token.

use std::collections::HashMap;

use conveyor_core::Repo;
use conveyor_core::tree::project_path;
use quench_ui::prelude::*;

use super::{Env, nothing, panel, waiting};
use crate::live::State;

pub(crate) fn credentials(env: &Env, state: &State) -> View {
    let (inner_env, state) = (env.clone(), state.clone());
    let body = View::dynamic(move || {
        let env = &inner_env;
        let Some(list) = state.creds.get() else {
            return waiting(env);
        };
        if list.is_empty() {
            return nothing(env, "ui_credentials_empty", "no-credentials").into_view();
        }
        let projects = state.projects.get().unwrap_or_default();
        let repos = state.repos.get().unwrap_or_default();
        let by_id: HashMap<&str, &Repo> = repos.iter().map(|r| (r.id.as_str(), r)).collect();
        let mut rows = div().attr("id", "credentials").class("cv-meta-list");
        for credential in &list {
            let scope = match (&credential.project_id, &credential.repo_id) {
                (Some(project), _) => format!("project: {}", project_path(project, &projects)),
                (None, Some(repo)) => by_id.get(repo.as_str()).map_or_else(
                    || "repo: ?".to_string(),
                    |repo| format!("repo: {}/{}", repo.owner, repo.name),
                ),
                (None, None) => String::new(),
            };
            rows = rows.child(
                div()
                    .class("cv-row")
                    .attr("data-credential", credential.id.clone())
                    .child(
                        div()
                            .class("cv-row-main")
                            .child(span().class("cv-repo-name").text(credential.name.clone()))
                            .child(span().class("cv-project-path").text(scope)),
                    )
                    .child(
                        div()
                            .class("cv-row-meta")
                            .child(span().class("cv-muted").text(credential.kind.clone()))
                            .child(span().class("cv-mono").text(credential.username.clone()))
                            .child(span().class("cv-mono").text(credential.preview.clone()))
                            .child(
                                span()
                                    .class("cv-muted")
                                    .text(env.since(credential.created_at)),
                            ),
                    ),
            );
        }
        rows.into_view()
    });
    div()
        .class("cv-page cv-narrow")
        .child(panel(div().text(env.t.t("ui_credentials_title"))).child(body))
        .into_view()
}
