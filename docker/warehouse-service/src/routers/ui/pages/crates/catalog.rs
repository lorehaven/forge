use super::storage::{IndexDep, IndexRecord, list_crates, list_versions};
use crate::routers::crates::ops::yank::set_yanked;
use crate::routers::ui::PageQuery;
use crate::routers::ui::common::{
    UiPageKind, is_ui_authenticated, render_page, ui_login_redirect, ui_path,
};
use actix_web::{HttpRequest, HttpResponse, Responder, get, post, web};
use quench_auth::prelude::JwtConfig;
use quench_starter::prelude::with_base_path;
use quench_web::prelude::*;

#[derive(serde::Deserialize)]
pub(in crate::routers::ui::pages) struct CrateActionForm {
    name: String,
    version: String,
}

#[get("/crates/catalog")]
pub(super) async fn crates_index(
    req: HttpRequest,
    query: web::Query<PageQuery>,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !is_ui_authenticated(&req, &config) {
        return ui_login_redirect();
    }
    render_crates_page(query.repo.clone(), query.tag.clone())
}

#[get("/crates/catalog/")]
pub(super) async fn crates_index_slash(
    req: HttpRequest,
    query: web::Query<PageQuery>,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !is_ui_authenticated(&req, &config) {
        return ui_login_redirect();
    }
    render_crates_page(query.repo.clone(), query.tag.clone())
}

#[post("/crates/yank")]
pub(in crate::routers::ui::pages) async fn yank_version(
    req: HttpRequest,
    form: web::Form<CrateActionForm>,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    set_yank_state(req, form, config, true).await
}

#[post("/crates/unyank")]
pub(in crate::routers::ui::pages) async fn unyank_version(
    req: HttpRequest,
    form: web::Form<CrateActionForm>,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    set_yank_state(req, form, config, false).await
}

async fn set_yank_state(
    req: HttpRequest,
    form: web::Form<CrateActionForm>,
    config: web::Data<JwtConfig>,
    yanked: bool,
) -> HttpResponse {
    if !is_ui_authenticated(&req, &config) {
        return ui_login_redirect();
    }

    match set_yanked(&form.name, &form.version, yanked).await {
        Ok(true) => HttpResponse::NoContent()
            .append_header((
                "HX-Redirect",
                with_base_path(&format!(
                    "/ui/crates/catalog?repo={}&tag={}",
                    form.name, form.version
                )),
            ))
            .finish(),
        Ok(false) => HttpResponse::NotFound().body("api_error_crate_version_not_found"),
        Err(msg) => {
            tracing::error!("Failed to change yank state: {}", msg);
            HttpResponse::InternalServerError().body("api_error_internal")
        }
    }
}

fn render_crates_page(
    selected_crate: Option<String>,
    selected_version: Option<String>,
) -> HttpResponse {
    let all_crates = list_crates();

    let krate = selected_crate
        .as_ref()
        .filter(|n| all_crates.iter().any(|c| c == *n))
        .cloned();

    let versions: Vec<IndexRecord> = match krate.as_deref() {
        Some(name) => list_versions(name),
        None => Vec::new(),
    };

    let active_version = selected_version
        .as_ref()
        .filter(|v| versions.iter().any(|r| &r.vers == *v))
        .cloned()
        .or_else(|| {
            versions
                .iter()
                .rev()
                .find(|r| !r.yanked)
                .or_else(|| versions.last())
                .map(|r| r.vers.clone())
        });

    let selected_record = active_version
        .as_ref()
        .and_then(|v| versions.iter().find(|r| &r.vers == v));

    let left = div()
        .class("split-left panel")
        .child(div().class("panel-title").attr("data-i18n", "ui_crates"))
        .child(div().class("tree-scroll").child(render_crate_tree(
            &all_crates,
            krate.as_deref(),
            &versions,
            active_version.as_deref(),
        )));

    let right = div()
        .class("split-right panel")
        .child(render_metadata_panel(krate.as_deref(), selected_record));

    render_page(
        HttpResponse::Ok(),
        content()
            .class("container-fluid py-4")
            .child(div().class("split-view").child(left).child(right)),
        UiPageKind::Crates,
    )
}

fn render_crate_tree(
    crates: &[String],
    selected_crate: Option<&str>,
    versions: &[IndexRecord],
    active_version: Option<&str>,
) -> Element {
    if crates.is_empty() {
        return div().class("empty").attr("data-i18n", "ui_crates_empty");
    }

    let mut tree = ul().class("repo-tree");
    for name in crates {
        tree = tree.child(render_crate_node(
            name,
            selected_crate,
            versions,
            active_version,
        ));
    }
    tree
}

fn render_crate_node(
    name: &str,
    selected_crate: Option<&str>,
    versions: &[IndexRecord],
    active_version: Option<&str>,
) -> Element {
    let item = li();
    let is_selected = Some(name) == selected_crate;
    let has_versions = is_selected && !versions.is_empty();

    let needs_details = is_selected;

    if !needs_details {
        return item.child(
            div()
                .class("tree-folder")
                .child(i().class("fas fa-box mr-2"))
                .child(
                    a().attr(
                        "href",
                        format!("{}?repo={name}", ui_path("/crates/catalog")),
                    )
                    .class("repo-link")
                    .text(name),
                ),
        );
    }

    let mut details = element("details").attr("data-path", name);

    if is_selected {
        details = details.attr("open", "open");
    }

    let summary = element("summary")
        .class("tree-folder")
        .child(i().class("fas fa-box mr-2"))
        .child(
            a().attr(
                "href",
                format!("{}?repo={name}", ui_path("/crates/catalog")),
            )
            .class("repo-link active")
            .text(name),
        );

    details = details.child(summary);

    if has_versions {
        let mut tags_list = ul().class("tag-list");
        for record in versions.iter().rev() {
            let tag_class = if Some(record.vers.as_str()) == active_version {
                "tag-link active"
            } else {
                "tag-link"
            };

            let status_icon = if record.yanked {
                i().class("fas fa-ban mr-2")
                    .attr("style", "color: var(--bs-warning);")
            } else {
                i().class("fas fa-tag mr-2")
            };

            tags_list = tags_list.child(
                li().child(
                    a().attr(
                        "href",
                        format!(
                            "{}?repo={name}&tag={}",
                            ui_path("/crates/catalog"),
                            record.vers
                        ),
                    )
                    .class(tag_class)
                    .child(status_icon)
                    .child(span().text(&record.vers)),
                ),
            );
        }
        details = details.child(tags_list);
    } else if is_selected {
        let mut tags_list = ul().class("tag-list");
        tags_list = tags_list.child(
            li().child(
                span()
                    .class("tag-link")
                    .attr("style", "color: var(--bs-gray-500); cursor: default;")
                    .attr("data-i18n", "ui_empty_no_versions")
                    .text("No versions found"),
            ),
        );
        details = details.child(tags_list);
    }

    item.child(details)
}

fn render_metadata_panel(krate: Option<&str>, record: Option<&IndexRecord>) -> Element {
    let title = match (krate, record) {
        (Some(_), Some(r)) => div()
            .class("panel-title")
            .child(span().attr("data-i18n", "ui_metadata_for"))
            .child(span().text(format!(" {}", r.vers))),
        _ => div().class("panel-title").attr("data-i18n", "ui_metadata"),
    };

    let body = match record {
        None => div()
            .class("empty")
            .attr("data-i18n", "ui_empty_select_version"),
        Some(r) => {
            let mut list = div().class("meta-list");

            list = list
                .child(meta_row("ui_meta_version", &r.vers))
                .child(meta_row_value(
                    "ui_meta_status",
                    if r.yanked {
                        span().attr("data-i18n", "ui_status_yanked").text("yanked")
                    } else {
                        span().attr("data-i18n", "ui_status_active").text("active")
                    },
                ))
                .child(meta_row("ui_meta_checksum", &r.cksum));

            if let Some(rv) = &r.rust_version {
                list = list.child(meta_row("ui_meta_rust_version", rv));
            }
            if let Some(links) = &r.links {
                list = list.child(meta_row("ui_meta_links", links));
            }

            if !r.features.is_empty() {
                let mut all_features: Vec<&str> = r.features.keys().map(String::as_str).collect();
                if let Some(f2) = &r.features2 {
                    for k in f2.keys() {
                        if !all_features.contains(&k.as_str()) {
                            all_features.push(k.as_str());
                        }
                    }
                }
                all_features.sort();
                list = list.child(meta_row("ui_meta_features", &all_features.join(", ")));
            }

            if !r.deps.is_empty() {
                list = list.child(render_deps_section(r));
            }

            let crate_name = krate.unwrap_or("");
            let action_button = if r.yanked {
                form()
                    .class("inline-action-form")
                    .attr("hx-post", ui_path("/crates/unyank"))
                    .attr("hx-swap", "none")
                    .child(
                        input()
                            .attr("type", "hidden")
                            .attr("name", "name")
                            .attr("value", crate_name),
                    )
                    .child(
                        input()
                            .attr("type", "hidden")
                            .attr("name", "version")
                            .attr("value", &r.vers),
                    )
                    .child(
                        button()
                            .class("button-danger-sm")
                            .attr("type", "submit")
                            .attr(
                                "style",
                                "color: var(--bs-warning); border-color: var(--bs-warning);",
                            )
                            .child(i().class("fas fa-undo mr-2"))
                            .child(
                                span()
                                    .attr("data-i18n", "ui_unyank_version")
                                    .text("Unyank Version"),
                            ),
                    )
            } else {
                form()
                    .class("inline-action-form")
                    .attr("hx-post", ui_path("/crates/yank"))
                    .attr("hx-swap", "none")
                    .child(
                        input()
                            .attr("type", "hidden")
                            .attr("name", "name")
                            .attr("value", crate_name),
                    )
                    .child(
                        input()
                            .attr("type", "hidden")
                            .attr("name", "version")
                            .attr("value", &r.vers),
                    )
                    .child(
                        button()
                            .class("button-danger-sm")
                            .attr("type", "submit")
                            .child(i().class("fas fa-ban mr-2"))
                            .child(
                                span()
                                    .attr("data-i18n", "ui_yank_version")
                                    .text("Yank Version"),
                            ),
                    )
            };

            list = list.child(div().class("mt-4").child(action_button));

            list
        }
    };

    div()
        .class("h-100 d-flex flex-column")
        .child(title)
        .child(body)
}

fn render_deps_section(record: &IndexRecord) -> Element {
    let mut normal: Vec<&IndexDep> = Vec::new();
    let mut dev: Vec<&IndexDep> = Vec::new();
    let mut build: Vec<&IndexDep> = Vec::new();

    for dep in &record.deps {
        match dep.kind.as_str() {
            "dev" => dev.push(dep),
            "build" => build.push(dep),
            _ => normal.push(dep),
        }
    }

    let mut section = div().class("meta-deps");

    if !normal.is_empty() {
        section = section.child(deps_group("ui_deps_normal", &normal));
    }
    if !build.is_empty() {
        section = section.child(deps_group("ui_deps_build", &build));
    }
    if !dev.is_empty() {
        section = section.child(deps_group("ui_deps_dev", &dev));
    }

    div()
        .class("meta-row")
        .child(div().class("meta-label").attr("data-i18n", "ui_meta_deps"))
        .child(section)
}

fn deps_group(label_key: &str, deps: &[&IndexDep]) -> Element {
    let mut rows = div().class("deps-group");
    rows = rows.child(div().class("deps-group-label").attr("data-i18n", label_key));
    for dep in deps {
        let display_name = dep
            .package
            .as_deref()
            .map(|pkg| format!("{} (as {})", pkg, dep.name))
            .unwrap_or_else(|| dep.name.clone());

        let mut dep_text = format!("{display_name} {}", dep.req);
        if dep.optional {
            dep_text.push_str(" [optional]");
        }
        if let Some(target) = &dep.target {
            dep_text.push_str(&format!(" [target: {target}]"));
        }

        rows = rows.child(div().class("dep-row mono").text(&dep_text));
    }
    rows
}

fn meta_row(label_key: &str, value: &str) -> Element {
    meta_row_value(label_key, span().text(value))
}

fn meta_row_value(label_key: &str, value: Element) -> Element {
    div()
        .class("meta-row")
        .child(div().class("meta-label").attr("data-i18n", label_key))
        .child(div().class("meta-value mono").child(value))
}
