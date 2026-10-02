use crate::domain::rivet::RivetPackage;
use crate::routers::rivets::ops::yank::set_yanked;
use crate::routers::ui::PageQuery;
use crate::routers::ui::authz::{ManageGate, OptionalUiClaims, can_manage};
use crate::routers::ui::common::{PageAuth, UiPageKind, render_page, ui_login_redirect, ui_path};
use quench_db::prelude::{Crud, Db};
use quench_http::prelude::{Form, Inject, Query, Response, get, http::StatusCode, post};
use quench_starter::common::routes::with_base_path;
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;
use std::collections::BTreeMap;

#[derive(serde::Deserialize)]
pub struct RivetActionForm {
    pub name: String,
    pub version: String,
}

// --- GET /ui/rivets/catalog ---

#[get("/ui/rivets/catalog")]
pub async fn rivets_catalog(
    auth: PageAuth,
    claims: OptionalUiClaims,
    Query(query): Query<PageQuery>,
    Inject(db): Inject<Db>,
) -> Response {
    render(auth, claims, query, &db).await
}

#[get("/ui/rivets/catalog/")]
pub async fn rivets_catalog_slash(
    auth: PageAuth,
    claims: OptionalUiClaims,
    Query(query): Query<PageQuery>,
    Inject(db): Inject<Db>,
) -> Response {
    render(auth, claims, query, &db).await
}

async fn render(
    PageAuth(authenticated): PageAuth,
    OptionalUiClaims(claims): OptionalUiClaims,
    query: PageQuery,
    db: &Db,
) -> Response {
    if !authenticated {
        return ui_login_redirect();
    }

    let can_manage = claims.is_some_and(|claims| can_manage(&claims));

    // A disabled feature or an unreachable database renders an empty catalog,
    // the same non-answer the JSON API gives an unauthorised caller.
    let packages = if crate::routers::rivets_enabled() {
        db.repository::<RivetPackage>()
            .list()
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    render_rivets_page(
        &packages,
        query.repo.as_deref(),
        query.tag.as_deref(),
        can_manage,
    )
}

// --- POST /ui/rivets/yank | /ui/rivets/unyank ---

#[post("/ui/rivets/yank")]
pub async fn yank_package(
    gate: ManageGate,
    Form(form): Form<RivetActionForm>,
    Inject(db): Inject<Db>,
) -> Response {
    set_yank_state(gate, form, &db, true).await
}

#[post("/ui/rivets/unyank")]
pub async fn unyank_package(
    gate: ManageGate,
    Form(form): Form<RivetActionForm>,
    Inject(db): Inject<Db>,
) -> Response {
    set_yank_state(gate, form, &db, false).await
}

async fn set_yank_state(
    gate: ManageGate,
    form: RivetActionForm,
    db: &Db,
    yanked: bool,
) -> Response {
    if let Err(response) = gate.or_response() {
        return response;
    }

    if !crate::routers::rivets_enabled() {
        return Response::text(StatusCode::NOT_FOUND, "api_error_rivets_disabled");
    }

    let outcome = set_yanked(db, &form.name, &form.version, yanked).await;
    if !outcome.status().is_success() {
        return outcome;
    }

    Response::new(StatusCode::NO_CONTENT).header(
        "HX-Redirect",
        with_base_path(&format!(
            "/ui/rivets/catalog?repo={}&tag={}",
            urlencoding::encode(&form.name),
            urlencoding::encode(&form.version)
        )),
    )
}

// --- Rendering ---

/// name -> its versions, newest first.
type Tree<'a> = BTreeMap<&'a str, Vec<&'a RivetPackage>>;

pub fn render_rivets_page(
    packages: &[RivetPackage],
    selected_name: Option<&str>,
    selected_version: Option<&str>,
    can_manage: bool,
) -> Response {
    let mut tree: Tree = BTreeMap::new();
    for package in packages {
        tree.entry(package.name.as_str()).or_default().push(package);
    }
    // Newest first by semver precedence; a row whose version will not parse sorts last.
    for versions in tree.values_mut() {
        versions.sort_by_key(|p| std::cmp::Reverse(p.semver()));
    }

    let name = selected_name.filter(|name| tree.contains_key(*name));
    let version_list: &[&RivetPackage] = name
        .and_then(|name| tree.get(name))
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    // Newest *offerable* first: the page opens on what `latest` would give, not on a yanked row.
    let selected = selected_version
        .and_then(|version| version_list.iter().find(|p| p.version == version))
        .or_else(|| version_list.iter().find(|p| !p.yanked))
        .or_else(|| version_list.first())
        .copied();

    let left = div()
        .class("split-left panel")
        .child(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_rivet_packages"),
        )
        .child(
            div()
                .class("tree-scroll")
                .child(render_tree(&tree, name, selected)),
        );

    let right = div()
        .class("split-right panel")
        .child(render_metadata_panel(selected, can_manage));

    render_page(
        StatusCode::OK,
        content()
            .class("container-fluid py-4")
            .child(div().class("split-view").child(left).child(right)),
        UiPageKind::Rivets,
    )
}

fn catalog_href(query: &str) -> String {
    format!("{}?{query}", ui_path("/rivets/catalog"))
}

fn render_tree(
    tree: &Tree,
    selected_name: Option<&str>,
    selected: Option<&RivetPackage>,
) -> Element {
    if tree.is_empty() {
        return empty_state("ui_rivet_empty");
    }

    let mut list = ul().class("repo-tree");
    for (name, versions) in tree {
        list = list.child(render_package_node(name, versions, selected_name, selected));
    }
    list
}

fn render_package_node(
    name: &str,
    versions: &[&RivetPackage],
    selected_name: Option<&str>,
    selected: Option<&RivetPackage>,
) -> Element {
    let item = li();
    let href = catalog_href(&format!("repo={}", urlencoding::encode(name)));

    if Some(name) != selected_name {
        return item.child(
            div()
                .class("tree-folder")
                .child(i().class("fas fa-box mr-2"))
                .child(a().attr("href", href).class("repo-link").text(name)),
        );
    }

    let summary = element("summary")
        .class("tree-folder")
        .child(i().class("fas fa-box mr-2"))
        .child(a().attr("href", href).class("repo-link active").text(name));

    let selected_version = selected.map(|p| p.version.as_str());
    let mut list = ul().class("tag-list");
    for package in versions {
        let link_class = if Some(package.version.as_str()) == selected_version {
            "tag-link active"
        } else {
            "tag-link"
        };
        let icon = if package.yanked {
            i().class("fas fa-ban mr-2")
                .attr("style", "color: var(--bs-warning);")
        } else {
            i().class("fas fa-tag mr-2")
        };
        list = list.child(
            li().child(
                a().attr(
                    "href",
                    catalog_href(&format!(
                        "repo={}&tag={}",
                        urlencoding::encode(name),
                        urlencoding::encode(&package.version)
                    )),
                )
                .class(link_class)
                .child(icon)
                .child(span().text(package.version.clone())),
            ),
        );
    }

    item.child(
        element("details")
            .attr("data-path", name)
            .attr("open", "open")
            .child(summary)
            .child(list),
    )
}

fn render_metadata_panel(selected: Option<&RivetPackage>, can_manage: bool) -> Element {
    let title = match selected {
        Some(p) => div()
            .class("panel-title")
            .child(span().attr("data-i18n", "ui_metadata_for"))
            .child(span().text(format!(" {} {}", p.name, p.version))),
        None => div().class("panel-title").attr("data-i18n", "ui_metadata"),
    };

    let body = match selected {
        None => empty_state("ui_rivet_empty_select_version"),
        Some(p) => {
            let mut list = div()
                .class("meta-list")
                .child(meta_row("ui_rivet_meta_name", &p.name))
                .child(meta_row("ui_rivet_meta_version", &p.version))
                .child(meta_row_value(
                    "ui_meta_status",
                    if p.yanked {
                        span().attr("data-i18n", "ui_status_yanked").text("yanked")
                    } else {
                        span().attr("data-i18n", "ui_status_active").text("active")
                    },
                ));

            if let Some(description) = &p.description {
                list = list.child(meta_row("ui_rivet_meta_description", description));
            }
            if let Some(namespace) = &p.namespace {
                list = list.child(meta_row("ui_rivet_meta_namespace", namespace));
            }
            if let Some(riveter) = p.manifest.0["requires"]["riveter"].as_str() {
                list = list.child(meta_row("ui_rivet_meta_requires_riveter", riveter));
            }
            if let Some(required) = p.manifest.0["requires"]["packages"].as_array() {
                let joined: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
                if !joined.is_empty() {
                    list = list.child(meta_row(
                        "ui_rivet_meta_requires_packages",
                        &joined.join(", "),
                    ));
                }
            }
            if let Some(meta) = p.manifest.0["meta"].as_object() {
                for (key, value) in meta {
                    let text = value
                        .as_str()
                        .map_or_else(|| value.to_string(), str::to_string);
                    list = list.child(meta_row_label(key, &text));
                }
            }

            list = list
                .child(meta_row(
                    "ui_artifact_meta_size",
                    &format!("{} bytes", p.size_bytes),
                ))
                .child(meta_row("ui_meta_checksum", &p.sha256))
                .child(meta_row("ui_rivet_meta_uploaded_by", &p.uploaded_by))
                .child(meta_row(
                    "ui_rivet_meta_published",
                    &p.created_at.format("%Y-%m-%d %H:%M UTC").to_string(),
                ))
                .child(meta_row(
                    "ui_rivet_meta_install",
                    &format!("riveter install {}@{}", p.name, p.version),
                ));

            if can_manage {
                list = list.child(div().class("mt-4").child(yank_form(p)));
            }

            list
        }
    };

    div()
        .class("h-100 d-flex flex-column")
        .child(title)
        .child(body)
}

fn yank_form(package: &RivetPackage) -> Element {
    let (action, icon, label_key, label_text) = if package.yanked {
        (
            "/rivets/unyank",
            "fas fa-undo mr-2",
            "ui_rivet_unyank",
            "Unyank",
        )
    } else {
        ("/rivets/yank", "fas fa-ban mr-2", "ui_rivet_yank", "Yank")
    };

    let mut submit = button()
        .class("button-danger-sm")
        .attr("type", "submit")
        .child(i().class(icon))
        .child(span().attr("data-i18n", label_key).text(label_text));
    if package.yanked {
        submit = submit.attr(
            "style",
            "color: var(--bs-warning); border-color: var(--bs-warning);",
        );
    }

    form()
        .class("inline-action-form")
        .attr("hx-post", ui_path(action))
        .attr("hx-swap", "none")
        .child(
            input()
                .attr("type", "hidden")
                .attr("name", "name")
                .attr("value", package.name.clone()),
        )
        .child(
            input()
                .attr("type", "hidden")
                .attr("name", "version")
                .attr("value", package.version.clone()),
        )
        .child(submit)
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

/// A row whose label is the package's own `[meta]` key, which has no translation.
fn meta_row_label(label: &str, value: &str) -> Element {
    div()
        .class("meta-row")
        .child(div().class("meta-label").text(label))
        .child(div().class("meta-value mono").child(span().text(value)))
}

pub fn register_routes() {
    let _ = rivets_catalog as fn(_, _, _, _) -> _;
    let _ = rivets_catalog_slash as fn(_, _, _, _) -> _;
    let _ = yank_package as fn(_, _, _) -> _;
    let _ = unyank_package as fn(_, _, _) -> _;
}
