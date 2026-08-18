//! Previewing the git credentials conveyor holds - never the token, only
//! the same `preview` fragment the API returns and the same `"read"` grant
//! it enforces (`routers::api::credentials::show_project`/`show_repo`,
//! `routers::api::authz`).
//!
//! Unlike a single scope's `GET .../credentials`, this page shows every
//! credential across the estate at once, so the permission check has to
//! happen per row rather than once for the whole request: a credential the
//! caller cannot read on the API does not appear here either, the same way
//! it would 403 rather than answer with a redacted body there.

use crate::credentials::store::{self as credential_store, CredentialRef};
use crate::domain::{Project, Repo};
use crate::routers::api::authz::granted_project_ids;
use crate::routers::ui::common::{UiPageKind, format, render_page, ui_login_redirect_for};
use crate::routers::ui::pages::repos::project_path;
use crate::scheduler::{projects, repos};
use actix_web::{HttpRequest, HttpResponse, Responder, get, web};
use quench_auth::actix::routers::ui::get_user_from_req;
use quench_auth::prelude::{Claims, JwtConfig};
use quench_db::prelude::Db;
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;
use std::collections::{HashMap, HashSet};

async fn actor(request: &HttpRequest, config: &JwtConfig) -> Option<Claims> {
    get_user_from_req(request, config).await
}

/// The project ids `claims` may read: every project when they hold the
/// blanket `conveyor:read` grant (or auth is disabled), otherwise whatever
/// they hold a resource-scoped grant on plus everything nested beneath it -
/// the read-side mirror of `pages::repos::writable_project_ids`.
async fn readable_project_ids(db: &Db, claims: &Claims, all_projects: &[Project]) -> Vec<String> {
    if claims.can("conveyor", "read") {
        return all_projects.iter().map(|p| p.id.clone()).collect();
    }
    let granted = granted_project_ids(claims, "read");
    projects::descendant_ids(db, &granted)
        .await
        .unwrap_or_default()
}

#[get("/credentials")]
pub(super) async fn list_page(
    request: HttpRequest,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    let Some(claims) = actor(&request, &config).await else {
        return ui_login_redirect_for(&request);
    };
    render_list(&db, &claims).await
}

async fn render_list(db: &Db, claims: &Claims) -> HttpResponse {
    let all_projects = projects::list_all(db).await.unwrap_or_default();
    let all_repos = repos::list(db).await.unwrap_or_default();
    let all_credentials = credential_store::list_all(db).await.unwrap_or_default();

    let readable = readable_project_ids(db, claims, &all_projects).await;
    let readable_set: HashSet<&str> = readable.iter().map(String::as_str).collect();
    let repos_by_id: HashMap<&str, &Repo> = all_repos
        .iter()
        .map(|repo| (repo.id.as_str(), repo))
        .collect();

    let mut rows = div().class("meta-list");
    let mut shown = 0;
    for credential in &all_credentials {
        let Some(scope) = credential_scope(credential, &repos_by_id) else {
            // A repo-scoped row whose repository has since been removed -
            // `ON DELETE CASCADE` means this should not exist, but a row
            // this page cannot place is a row it does not show, not a panic.
            continue;
        };
        if !readable_set.contains(scope.project_id) {
            continue;
        }
        shown += 1;
        rows = rows.child(credential_row(credential, &scope, &all_projects));
    }
    if shown == 0 {
        rows = rows.child(empty_state("ui_credentials_empty"));
    }

    let panel = div()
        .class("panel repos-panel")
        .child(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_credentials_title"),
        )
        .child(rows);

    render_page(
        HttpResponse::Ok(),
        content()
            .class("repos-content")
            .child(div().class("repos-container").child(panel)),
        UiPageKind::Home,
    )
}

/// Where a credential lives, resolved to the project id its readability is
/// judged against - direct for a project-scoped row, via the owning
/// repository for a repo-scoped one.
pub struct Scope<'a> {
    pub project_id: &'a str,
    /// A repo-scoped credential's own repository, for the "repo: owner/name"
    /// label - `None` for a project-scoped one.
    pub repo: Option<&'a Repo>,
}

pub fn credential_scope<'a>(
    credential: &'a CredentialRef,
    repos_by_id: &HashMap<&'a str, &'a Repo>,
) -> Option<Scope<'a>> {
    if let Some(project_id) = credential.project_id.as_deref() {
        return Some(Scope {
            project_id,
            repo: None,
        });
    }
    let repo = *repos_by_id.get(credential.repo_id.as_deref()?)?;
    Some(Scope {
        project_id: &repo.project_id,
        repo: Some(repo),
    })
}

pub fn credential_row(
    credential: &CredentialRef,
    scope: &Scope,
    all_projects: &[Project],
) -> Element {
    let scope_label = match scope.repo {
        Some(repo) => format!("repo: {}/{}", repo.owner, repo.name),
        None => format!("project: {}", project_path(scope.project_id, all_projects)),
    };

    div()
        .class("repos-row")
        .child(
            div()
                .class("repos-row-main")
                .child(span().class("repos-repo-name").text(&credential.name))
                .child(span().class("repos-project-path").text(scope_label)),
        )
        .child(
            div()
                .class("repos-row-meta")
                .child(span().class("muted").text(&credential.kind))
                .child(span().class("mono").text(&credential.username))
                .child(span().class("mono").text(&credential.preview))
                .child(
                    span()
                        .class("muted")
                        .text(format::relative(credential.created_at)),
                ),
        )
}
