use super::storage::{IndexDep, IndexRecord, list_crates, list_versions};
use crate::domain::jwt::JwtConfig;
use crate::routers::ui::PageQuery;
use crate::routers::ui::common::{UiPageKind, is_ui_authenticated, render_page, ui_login_redirect};
use actix_web::{HttpRequest, HttpResponse, Responder, get, web};
use quench::prelude::*;

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

// ---------------------------------------------------------------------------
// Page renderer
// ---------------------------------------------------------------------------

// We reuse the `PageQuery` struct already defined in ui/mod.rs:
//   repo  → selected crate name
//   tag   → selected version string

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

    // Default to latest non-yanked version, then any version
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
        .child(
            div()
                .class("tree-scroll")
                .child(render_crate_list(&all_crates, krate.as_deref())),
        );

    let right = div()
        .class("split-right")
        .child(div().class("right-top").child(render_versions_panel(
            krate.as_deref(),
            &versions,
            active_version.as_deref(),
        )))
        .child(
            div()
                .class("right-bottom")
                .child(render_details_panel(krate.as_deref(), selected_record)),
        );

    render_page(
        HttpResponse::Ok(),
        content()
            .class("container-fluid py-4")
            .child(div().class("split-view").child(left).child(right)),
        UiPageKind::Crates,
    )
}

// ---------------------------------------------------------------------------
// Left panel – crate list
// ---------------------------------------------------------------------------

fn render_crate_list(crates: &[String], selected: Option<&str>) -> Element {
    if crates.is_empty() {
        return div().class("empty").attr("data-i18n", "ui_crates_empty");
    }

    let mut list = ul().class("repo-tree"); // reuse repo-tree styles for identical look
    for name in crates {
        let href = format!("/ui/crates/catalog?repo={name}");
        let class = if Some(name.as_str()) == selected {
            "repo-link active"
        } else {
            "repo-link"
        };
        list = list.child(li().child(a().attr("href", &href).class(class).text(name)));
    }
    list
}

// ---------------------------------------------------------------------------
// Right-top panel – versions table
// ---------------------------------------------------------------------------

fn render_versions_panel(
    krate: Option<&str>,
    versions: &[IndexRecord],
    active_version: Option<&str>,
) -> Element {
    let title = match krate {
        Some(n) => div()
            .class("panel-title")
            .child(span().attr("data-i18n", "ui_versions_for"))
            .child(span().text(&format!(" {n}"))),
        None => div().class("panel-title").attr("data-i18n", "ui_versions"),
    };

    let header = div()
        .class("header")
        .child(div().class("cell").attr("data-i18n", "ui_col_version"))
        .child(div().class("cell").attr("data-i18n", "ui_col_status"))
        .child(div().class("cell").attr("data-i18n", "ui_col_checksum"));

    let mut body = div().class("body");

    if krate.is_none() {
        body = body.child(
            div()
                .class("empty")
                .attr("data-i18n", "ui_empty_select_crate"),
        );
    } else if versions.is_empty() {
        body = body.child(
            div()
                .class("empty")
                .attr("data-i18n", "ui_empty_no_versions"),
        );
    } else {
        // Show newest first
        for record in versions.iter().rev() {
            let Some(crate_name) = krate else { break };
            let href = format!("/ui/crates/catalog?repo={crate_name}&tag={}", record.vers);
            let row_class = if Some(record.vers.as_str()) == active_version {
                "row active"
            } else {
                "row"
            };
            let status_key = if record.yanked {
                "ui_status_yanked"
            } else {
                "ui_status_active"
            };

            let mut row = div()
                .class(row_class)
                .child(
                    div()
                        .class("cell")
                        .child(a().attr("href", &href).class("tag-link").text(&record.vers)),
                )
                .child(
                    div()
                        .class("cell")
                        .child(span().attr("data-i18n", status_key)),
                )
                .child(div().class("cell mono").text(&short_hex(&record.cksum)));

            // Add yank/unyank buttons for yankable versions
            if record.yanked {
                // Add unyank button with Font Awesome icon
                row = row.child(
                    div().class("cell").class("actions").child(
                        i().class("fas fa-undo")
                            .attr("aria-hidden", "true")
                            .attr("data-action", "unyank")
                            .attr("data-crate", crate_name)
                            .attr("data-version", &record.vers)
                            .attr("title", "Unyank version")
                            .attr("role", "button")
                            .attr("aria-label", "Unyank version")
                            .on_click("handleUnyankClick(event)"),
                    ),
                );
            } else {
                // Add yank button with Font Awesome icon
                row = row.child(
                    div().class("cell").class("actions").child(
                        i().class("fas fa-ban")
                            .attr("aria-hidden", "true")
                            .attr("data-action", "yank")
                            .attr("data-crate", crate_name)
                            .attr("data-version", &record.vers)
                            .attr("title", "Yank version")
                            .attr("role", "button")
                            .attr("aria-label", "Yank version")
                            .on_click("handleYankClick(event)"),
                    ),
                );
            }

            body = body.child(row);
        }
    }

    div()
        .class("panel table versions-grid")
        .child(title)
        .child(header)
        .child(body)
}

// ---------------------------------------------------------------------------
// Right-bottom panel – version details
// ---------------------------------------------------------------------------

fn render_details_panel(krate: Option<&str>, record: Option<&IndexRecord>) -> Element {
    let title = match (krate, record) {
        (Some(_), Some(r)) => div()
            .class("panel-title")
            .child(span().attr("data-i18n", "ui_metadata_for"))
            .child(span().text(&format!(" {}", r.vers))),
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
                .child(meta_row(
                    "ui_meta_status",
                    if r.yanked { "yanked" } else { "active" },
                ))
                .child(meta_row("ui_meta_checksum", &r.cksum));

            if let Some(rv) = &r.rust_version {
                list = list.child(meta_row("ui_meta_rust_version", rv));
            }
            if let Some(links) = &r.links {
                list = list.child(meta_row("ui_meta_links", links));
            }

            // Features
            if !r.features.is_empty() {
                let mut all_features: Vec<&str> = r.features.keys().map(String::as_str).collect();
                // Merge features2 keys if present
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

            // Dependencies — grouped by kind
            if !r.deps.is_empty() {
                list = list.child(render_deps_section(r));
            }

            list
        }
    };

    div().class("panel").child(title).child(body)
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn meta_row(label_key: &str, value: &str) -> Element {
    div()
        .class("meta-row")
        .child(div().class("meta-label").attr("data-i18n", label_key))
        .child(div().class("meta-value mono").text(value))
}

fn short_hex(hex: &str) -> String {
    if hex.len() <= 16 {
        return hex.to_string();
    }
    format!("{}…{}", &hex[..8], &hex[hex.len() - 8..])
}
