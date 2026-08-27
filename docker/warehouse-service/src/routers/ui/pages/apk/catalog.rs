use crate::domain::apk::ApkVersion;
use crate::routers::apk::ops::yank::set_yanked;
use crate::routers::ui::PageQuery;
use crate::routers::ui::authz::{require_manage, ui_claims};
use crate::routers::ui::common::{
    UiPageKind, is_ui_authenticated, render_page, ui_login_redirect, ui_path,
};
use actix_web::{HttpRequest, HttpResponse, Responder, get, post, web};
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::{Crud, Db};
use quench_starter::prelude::with_base_path;
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;
use std::collections::BTreeMap;

#[derive(serde::Deserialize)]
pub struct ApkActionForm {
    pub package: String,
    pub version_code: i64,
}

// ---------------------------------------------------------------------------
// GET /ui/apk/catalog
// ---------------------------------------------------------------------------

#[get("/apk/catalog")]
pub async fn apk_catalog(
    req: HttpRequest,
    query: web::Query<PageQuery>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    render(&req, query, &config, &db).await
}

#[get("/apk/catalog/")]
pub async fn apk_catalog_slash(
    req: HttpRequest,
    query: web::Query<PageQuery>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    render(&req, query, &config, &db).await
}

async fn render(
    req: &HttpRequest,
    query: web::Query<PageQuery>,
    config: &JwtConfig,
    db: &Db,
) -> HttpResponse {
    if !is_ui_authenticated(req, config).await {
        return ui_login_redirect();
    }

    let can_manage = ui_claims(req, config)
        .await
        .is_some_and(|claims| crate::routers::ui::authz::can_manage(&claims));

    // A disabled feature or an unreachable database renders an empty catalog,
    // the same non-answer the JSON API gives an unauthorised caller.
    let versions = if crate::routers::apk_enabled() {
        db.repository::<ApkVersion>()
            .list()
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let selected_code = query.tag.as_deref().and_then(|t| t.parse::<i64>().ok());
    render_apk_page(&versions, query.repo.as_deref(), selected_code, can_manage)
}

// ---------------------------------------------------------------------------
// POST /ui/apk/yank  |  /ui/apk/unyank
// ---------------------------------------------------------------------------

#[post("/apk/yank")]
pub async fn yank_version(
    req: HttpRequest,
    form: web::Form<ApkActionForm>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    set_yank_state(req, form, config, db, true).await
}

#[post("/apk/unyank")]
pub async fn unyank_version(
    req: HttpRequest,
    form: web::Form<ApkActionForm>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    set_yank_state(req, form, config, db, false).await
}

async fn set_yank_state(
    req: HttpRequest,
    form: web::Form<ApkActionForm>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    yanked: bool,
) -> HttpResponse {
    if let Err(response) = require_manage(&req, &config).await {
        return response;
    }

    if !crate::routers::apk_enabled() {
        return HttpResponse::NotFound().body("api_error_apk_disabled");
    }

    // `set_yanked` owns the read-modify-write against the catalog; we only
    // reinterpret its outcome as an htmx navigation instead of a JSON body.
    let outcome = set_yanked(&db, (form.package.clone(), form.version_code), yanked).await;
    if !outcome.status().is_success() {
        return outcome;
    }

    HttpResponse::NoContent()
        .append_header((
            "HX-Redirect",
            with_base_path(&format!(
                "/ui/apk/catalog?repo={}&tag={}",
                form.package, form.version_code
            )),
        ))
        .finish()
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

pub fn render_apk_page(
    versions: &[ApkVersion],
    selected_pkg: Option<&str>,
    selected_code: Option<i64>,
    can_manage: bool,
) -> HttpResponse {
    // package name -> its versions, newest `version_code` first.
    let mut by_package: BTreeMap<&str, Vec<&ApkVersion>> = BTreeMap::new();
    for version in versions {
        by_package
            .entry(version.package_name.as_str())
            .or_default()
            .push(version);
    }
    for list in by_package.values_mut() {
        list.sort_by_key(|v| std::cmp::Reverse(v.version_code));
    }

    let package = selected_pkg.filter(|name| by_package.contains_key(*name));

    let package_versions: &[&ApkVersion] = package
        .and_then(|name| by_package.get(name))
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    let selected = selected_code
        .and_then(|code| package_versions.iter().find(|v| v.version_code == code))
        .or_else(|| package_versions.first())
        .copied();

    let left = div()
        .class("split-left panel")
        .child(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_apk_packages"),
        )
        .child(div().class("tree-scroll").child(render_package_tree(
            &by_package,
            package,
            selected,
        )));

    let right = div()
        .class("split-right panel")
        .child(render_metadata_panel(package, selected, can_manage));

    render_page(
        HttpResponse::Ok(),
        content()
            .class("container-fluid py-4")
            .child(div().class("split-view").child(left).child(right)),
        UiPageKind::Apk,
    )
}

fn render_package_tree(
    by_package: &BTreeMap<&str, Vec<&ApkVersion>>,
    selected_pkg: Option<&str>,
    selected: Option<&ApkVersion>,
) -> Element {
    if by_package.is_empty() {
        return empty_state("ui_apk_empty");
    }

    let mut tree = ul().class("repo-tree");
    for (name, versions) in by_package {
        tree = tree.child(render_package_node(name, versions, selected_pkg, selected));
    }
    tree
}

fn render_package_node(
    name: &str,
    versions: &[&ApkVersion],
    selected_pkg: Option<&str>,
    selected: Option<&ApkVersion>,
) -> Element {
    let item = li();
    let is_selected = Some(name) == selected_pkg;

    if !is_selected {
        return item.child(
            div()
                .class("tree-folder")
                .child(i().class("fas fa-mobile-screen mr-2"))
                .child(
                    a().attr("href", format!("{}?repo={name}", ui_path("/apk/catalog")))
                        .class("repo-link")
                        .text(name),
                ),
        );
    }

    let summary = element("summary")
        .class("tree-folder")
        .child(i().class("fas fa-mobile-screen mr-2"))
        .child(
            a().attr("href", format!("{}?repo={name}", ui_path("/apk/catalog")))
                .class("repo-link active")
                .text(name),
        );

    let mut details = element("details")
        .attr("data-path", name)
        .attr("open", "open")
        .child(summary);

    if versions.is_empty() {
        return item.child(details);
    }

    let selected_code = selected.map(|v| v.version_code);
    let mut list = ul().class("tag-list");
    for version in versions {
        let link_class = if Some(version.version_code) == selected_code {
            "tag-link active"
        } else {
            "tag-link"
        };
        let icon = if version.yanked {
            i().class("fas fa-ban mr-2")
                .attr("style", "color: var(--bs-warning);")
        } else {
            i().class("fas fa-tag mr-2")
        };
        list = list.child(
            li().child(
                a().attr(
                    "href",
                    format!(
                        "{}?repo={name}&tag={}",
                        ui_path("/apk/catalog"),
                        version.version_code
                    ),
                )
                .class(link_class)
                .child(icon)
                .child(span().text(format!(
                    "{} ({})",
                    version.version_name, version.version_code
                ))),
            ),
        );
    }
    details = details.child(list);

    item.child(details)
}

fn render_metadata_panel(
    package: Option<&str>,
    selected: Option<&ApkVersion>,
    can_manage: bool,
) -> Element {
    let title = match selected {
        Some(v) => div()
            .class("panel-title")
            .child(span().attr("data-i18n", "ui_metadata_for"))
            .child(span().text(format!(" {} ({})", v.version_name, v.version_code))),
        None => div().class("panel-title").attr("data-i18n", "ui_metadata"),
    };

    let body = match selected {
        None => empty_state("ui_apk_empty_select_version"),
        Some(v) => {
            let mut list = div()
                .class("meta-list")
                .child(meta_row("ui_apk_meta_package", &v.package_name))
                .child(meta_row("ui_apk_meta_version_name", &v.version_name))
                .child(meta_row(
                    "ui_apk_meta_version_code",
                    &v.version_code.to_string(),
                ))
                .child(meta_row_value(
                    "ui_meta_status",
                    if v.yanked {
                        span().attr("data-i18n", "ui_status_yanked").text("yanked")
                    } else {
                        span().attr("data-i18n", "ui_status_active").text("active")
                    },
                ));

            if let Some(label) = &v.label {
                list = list.child(meta_row("ui_apk_meta_label", label));
            }
            if let Some(min_sdk) = v.min_sdk_version {
                list = list.child(meta_row("ui_apk_meta_min_sdk", &min_sdk.to_string()));
            }
            if let Some(target_sdk) = v.target_sdk_version {
                list = list.child(meta_row("ui_apk_meta_target_sdk", &target_sdk.to_string()));
            }
            list = list
                .child(meta_row(
                    "ui_apk_meta_size",
                    &format!("{} bytes", v.size_bytes),
                ))
                .child(meta_row("ui_meta_checksum", &v.sha256))
                .child(meta_row("ui_apk_meta_uploaded_by", &v.uploaded_by));

            if !v.permissions.0.is_empty() {
                list = list.child(meta_row(
                    "ui_apk_meta_permissions",
                    &v.permissions.0.join(", "),
                ));
            }

            if can_manage {
                list = list.child(
                    div()
                        .class("mt-4")
                        .child(yank_form(package.unwrap_or(""), v)),
                );
            }

            list
        }
    };

    div()
        .class("h-100 d-flex flex-column")
        .child(title)
        .child(body)
}

fn yank_form(package: &str, version: &ApkVersion) -> Element {
    let (action, icon, label_key, label_text) = if version.yanked {
        ("/apk/unyank", "fas fa-undo mr-2", "ui_apk_unyank", "Unyank")
    } else {
        ("/apk/yank", "fas fa-ban mr-2", "ui_apk_yank", "Yank")
    };

    let mut submit = button()
        .class("button-danger-sm")
        .attr("type", "submit")
        .child(i().class(icon))
        .child(span().attr("data-i18n", label_key).text(label_text));
    if version.yanked {
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
                .attr("name", "package")
                .attr("value", package),
        )
        .child(
            input()
                .attr("type", "hidden")
                .attr("name", "version_code")
                .attr("value", version.version_code.to_string()),
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
