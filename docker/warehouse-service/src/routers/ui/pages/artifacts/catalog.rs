use crate::domain::artifact::ArtifactVersion;
use crate::routers::artifacts::ops::yank::set_yanked;
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
pub struct ArtifactActionForm {
    pub program: String,
    pub platform: String,
    pub version_code: i64,
}

// ---------------------------------------------------------------------------
// GET /ui/artifacts/catalog
// ---------------------------------------------------------------------------

#[get("/artifacts/catalog")]
pub async fn artifacts_catalog(
    req: HttpRequest,
    query: web::Query<PageQuery>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    render(&req, query, &config, &db).await
}

#[get("/artifacts/catalog/")]
pub async fn artifacts_catalog_slash(
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
    let versions = if crate::routers::artifacts_enabled() {
        db.repository::<ArtifactVersion>()
            .list()
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let selected_code = query.tag.as_deref().and_then(|t| t.parse::<i64>().ok());
    render_artifacts_page(
        &versions,
        query.repo.as_deref(),
        query.platform.as_deref(),
        selected_code,
        can_manage,
    )
}

// ---------------------------------------------------------------------------
// POST /ui/artifacts/yank  |  /ui/artifacts/unyank
// ---------------------------------------------------------------------------

#[post("/artifacts/yank")]
pub async fn yank_version(
    req: HttpRequest,
    form: web::Form<ArtifactActionForm>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    set_yank_state(req, form, config, db, true).await
}

#[post("/artifacts/unyank")]
pub async fn unyank_version(
    req: HttpRequest,
    form: web::Form<ArtifactActionForm>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
) -> impl Responder {
    set_yank_state(req, form, config, db, false).await
}

async fn set_yank_state(
    req: HttpRequest,
    form: web::Form<ArtifactActionForm>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    yanked: bool,
) -> HttpResponse {
    if let Err(response) = require_manage(&req, &config).await {
        return response;
    }

    if !crate::routers::artifacts_enabled() {
        return HttpResponse::NotFound().body("api_error_artifacts_disabled");
    }

    let outcome = set_yanked(
        &db,
        &form.program,
        &form.platform,
        form.version_code,
        yanked,
    )
    .await;
    if !outcome.status().is_success() {
        return outcome;
    }

    HttpResponse::NoContent()
        .append_header((
            "HX-Redirect",
            with_base_path(&format!(
                "/ui/artifacts/catalog?repo={}&platform={}&tag={}",
                form.program, form.platform, form.version_code
            )),
        ))
        .finish()
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// program -> platform -> its versions, newest `version_code` first.
type Tree<'a> = BTreeMap<&'a str, BTreeMap<&'a str, Vec<&'a ArtifactVersion>>>;

pub fn render_artifacts_page(
    versions: &[ArtifactVersion],
    selected_program: Option<&str>,
    selected_platform: Option<&str>,
    selected_code: Option<i64>,
    can_manage: bool,
) -> HttpResponse {
    let mut tree: Tree = BTreeMap::new();
    for version in versions {
        tree.entry(version.program.as_str())
            .or_default()
            .entry(version.platform.as_str())
            .or_default()
            .push(version);
    }
    for platforms in tree.values_mut() {
        for list in platforms.values_mut() {
            list.sort_by_key(|v| std::cmp::Reverse(v.version_code));
        }
    }

    let program = selected_program.filter(|name| tree.contains_key(*name));
    let platforms = program.and_then(|name| tree.get(name));
    let platform = selected_platform
        .filter(|p| platforms.is_some_and(|m| m.contains_key(*p)))
        .or_else(|| platforms.and_then(|m| m.keys().next().copied()));

    let version_list: &[&ArtifactVersion] = platform
        .and_then(|p| platforms.and_then(|m| m.get(p)))
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    let selected = selected_code
        .and_then(|code| version_list.iter().find(|v| v.version_code == code))
        .or_else(|| version_list.first())
        .copied();

    let left = div()
        .class("split-left panel")
        .child(
            div()
                .class("panel-title")
                .attr("data-i18n", "ui_artifact_programs"),
        )
        .child(
            div()
                .class("tree-scroll")
                .child(render_tree(&tree, program, platform, selected)),
        );

    let right = div()
        .class("split-right panel")
        .child(render_metadata_panel(
            program, platform, selected, can_manage,
        ));

    render_page(
        HttpResponse::Ok(),
        content()
            .class("container-fluid py-4")
            .child(div().class("split-view").child(left).child(right)),
        UiPageKind::Artifacts,
    )
}

fn platform_icon(platform: &str) -> &'static str {
    match platform {
        "android" => "fab fa-android",
        "linux" => "fab fa-linux",
        "windows" => "fab fa-windows",
        "macos" => "fab fa-apple",
        _ => "fas fa-cube",
    }
}

fn render_tree(
    tree: &Tree,
    selected_program: Option<&str>,
    selected_platform: Option<&str>,
    selected: Option<&ArtifactVersion>,
) -> Element {
    if tree.is_empty() {
        return empty_state("ui_artifact_empty");
    }

    let mut list = ul().class("repo-tree");
    for (program, platforms) in tree {
        list = list.child(render_program_node(
            program,
            platforms,
            selected_program,
            selected_platform,
            selected,
        ));
    }
    list
}

fn render_program_node(
    program: &str,
    platforms: &BTreeMap<&str, Vec<&ArtifactVersion>>,
    selected_program: Option<&str>,
    selected_platform: Option<&str>,
    selected: Option<&ArtifactVersion>,
) -> Element {
    let item = li();
    let is_selected = Some(program) == selected_program;
    let href = format!("{}?repo={program}", ui_path("/artifacts/catalog"));

    if !is_selected {
        return item.child(
            div()
                .class("tree-folder")
                .child(i().class("fas fa-cube mr-2"))
                .child(a().attr("href", href).class("repo-link").text(program)),
        );
    }

    let summary = element("summary")
        .class("tree-folder")
        .child(i().class("fas fa-cube mr-2"))
        .child(
            a().attr("href", href)
                .class("repo-link active")
                .text(program),
        );

    let mut details = element("details")
        .attr("data-path", program)
        .attr("open", "open")
        .child(summary);

    for (platform, versions) in platforms {
        details = details.child(render_platform_node(
            program,
            platform,
            versions,
            selected_platform,
            selected,
        ));
    }

    item.child(details)
}

fn render_platform_node(
    program: &str,
    platform: &str,
    versions: &[&ArtifactVersion],
    selected_platform: Option<&str>,
    selected: Option<&ArtifactVersion>,
) -> Element {
    let is_selected = Some(platform) == selected_platform;
    let href = format!(
        "{}?repo={program}&platform={platform}",
        ui_path("/artifacts/catalog")
    );

    let summary = element("summary")
        .class("tree-folder")
        .child(i().class(format!("{} mr-2", platform_icon(platform))))
        .child(
            a().attr("href", href)
                .class(if is_selected {
                    "repo-link active"
                } else {
                    "repo-link"
                })
                .text(platform),
        );

    let mut details = element("details")
        .attr("data-path", platform)
        .child(summary);
    if is_selected {
        details = details.attr("open", "open");
    }
    if !is_selected {
        return details;
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
                        "{}?repo={program}&platform={platform}&tag={}",
                        ui_path("/artifacts/catalog"),
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

    details.child(list)
}

fn render_metadata_panel(
    program: Option<&str>,
    platform: Option<&str>,
    selected: Option<&ArtifactVersion>,
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
        None => empty_state("ui_artifact_empty_select_version"),
        Some(v) => {
            let mut list = div()
                .class("meta-list")
                .child(meta_row("ui_artifact_meta_program", &v.program))
                .child(meta_row("ui_artifact_meta_platform", &v.platform))
                .child(meta_row("ui_artifact_meta_format", &v.format))
                .child(meta_row("ui_artifact_meta_version_name", &v.version_name))
                .child(meta_row(
                    "ui_artifact_meta_version_code",
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

            if let Some(arch) = &v.arch {
                list = list.child(meta_row("ui_artifact_meta_arch", arch));
            }
            if let Some(label) = &v.label {
                list = list.child(meta_row("ui_artifact_meta_label", label));
            }
            if let Some(min_sdk) = v.metadata.0.min_sdk_version {
                list = list.child(meta_row("ui_artifact_meta_min_sdk", &min_sdk.to_string()));
            }
            if let Some(target_sdk) = v.metadata.0.target_sdk_version {
                list = list.child(meta_row(
                    "ui_artifact_meta_target_sdk",
                    &target_sdk.to_string(),
                ));
            }
            list = list
                .child(meta_row("ui_artifact_meta_filename", &v.filename))
                .child(meta_row(
                    "ui_artifact_meta_size",
                    &format!("{} bytes", v.size_bytes),
                ))
                .child(meta_row("ui_meta_checksum", &v.sha256))
                .child(meta_row("ui_artifact_meta_uploaded_by", &v.uploaded_by));

            if !v.metadata.0.permissions.is_empty() {
                list = list.child(meta_row(
                    "ui_artifact_meta_permissions",
                    &v.metadata.0.permissions.join(", "),
                ));
            }

            if can_manage {
                list = list.child(div().class("mt-4").child(yank_form(
                    program.unwrap_or(""),
                    platform.unwrap_or(""),
                    v,
                )));
            }

            list
        }
    };

    div()
        .class("h-100 d-flex flex-column")
        .child(title)
        .child(body)
}

fn yank_form(program: &str, platform: &str, version: &ArtifactVersion) -> Element {
    let (action, icon, label_key, label_text) = if version.yanked {
        (
            "/artifacts/unyank",
            "fas fa-undo mr-2",
            "ui_artifact_unyank",
            "Unyank",
        )
    } else {
        (
            "/artifacts/yank",
            "fas fa-ban mr-2",
            "ui_artifact_yank",
            "Yank",
        )
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
                .attr("name", "program")
                .attr("value", program),
        )
        .child(
            input()
                .attr("type", "hidden")
                .attr("name", "platform")
                .attr("value", platform),
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
