//! Renders every component into a static page for review (`cargo run -p forge-ui --example gallery -- <out-dir>`).
//! Serve `<out-dir>` with any static file server and open `/ui/gallery.html`; resize to check the mobile layout.

use forge_ui::*;
use quench_web::prelude::*;
use std::path::Path;

fn main() -> std::io::Result<()> {
    let out = std::env::args().nth(1).expect("usage: gallery <out-dir>");
    let out = Path::new(&out);
    std::fs::create_dir_all(out.join("i18n"))?;
    for locale in SUPPORTED_LOCALES {
        std::fs::copy(
            concat!(env!("CARGO_MANIFEST_DIR"), "/examples/gallery.ftl"),
            out.join("i18n").join(format!("{locale}.ftl")),
        )?;
    }
    std::env::set_current_dir(out)?;

    let cfg = ShellConfig::new("Gallery", "gallery.css", "layer-group")
        .nav(vec![
            NavSection::new(vec![NavItem::new(
                "house",
                "nav_overview",
                "/ui/gallery.html",
            )]),
            NavSection::titled(
                "nav_group_work",
                vec![
                    NavItem::new("folder-open", "nav_projects", "/ui/projects"),
                    NavItem::new("diagram-project", "nav_pipelines", "/ui/pipelines"),
                    NavItem::new("box-archive", "nav_storage", "/ui/storage"),
                ],
            ),
            NavSection::titled(
                "nav_group_admin",
                vec![NavItem::new("gear", "nav_settings", "/ui/settings")],
            ),
        ])
        .dashboard("/ui/dashboard")
        .profile("/ui/account");
    write_css("gallery.css", &[]);
    let shell = UiShell::new(cfg.clone());

    std::fs::create_dir_all("ui")?;
    std::fs::write("ui/gallery.html", shell.page(body()))?;
    std::fs::write(
        "ui/auth.html",
        UiShell::auth(cfg).page(
            div().class("fui-auth-card").child(
                Card::new()
                    .title_key("g_form")
                    .body(
                        form_el("post", "#")
                            .child(field(
                                "g_name",
                                text_input("name", "text"),
                                Some("g_name_hint"),
                            ))
                            .child(
                                submit_button(Variant::Primary, "g_save").class("fui-btn-block"),
                            ),
                    )
                    .build(),
            ),
        ),
    )?;
    let _ = std::fs::remove_file("ui/assets");
    std::os::unix::fs::symlink("../dist/assets", "ui/assets")?;
    Ok(())
}

fn body() -> Element {
    let runs = vec![
        ("build-api", Tone::Success, "g_ok", "2m 14s"),
        ("deploy-web", Tone::Error, "g_fail", "47s"),
        ("lint-all", Tone::Warning, "g_wait", "-"),
    ]
    .into_iter()
    .map(|(name, tone, key, dur)| {
        tr_el()
            .child(td_el().class("fui-cell-strong").text(name))
            .child(td_el().child(status_badge_key(tone, key)))
            .child(td_el().text(dur))
            .child(
                td_el()
                    .class("fui-cell-actions")
                    .child(button_el(Variant::DangerSoft, "g_delete").class("fui-btn-sm")),
            )
    })
    .collect();

    div()
        .class("fui-stack fui-stack-lg")
        .child(
            PageHeader::new("g_title")
                .subtitle_key("g_subtitle")
                .crumb(t("nav_overview"), Some("/ui/gallery.html"))
                .crumb(t("g_title"), None)
                .action(button_el(Variant::Secondary, "g_export"))
                .action(button_el(Variant::Primary, "g_new").child(icon("plus")))
                .build(),
        )
        .child(
            div()
                .class("fui-grid fui-grid-3")
                .child(stat(t("g_stat_runs"), txt("128")))
                .child(stat(t("g_stat_fail"), txt("3")))
                .child(link_card(
                    "box",
                    "/ui/docker",
                    t("g_card_a"),
                    Some(t("g_card_a_desc")),
                )),
        )
        .child(tabs(&[
            ("g_tab_a", "#a".into(), true),
            ("g_tab_b", "#b".into(), false),
            ("g_tab_c", "#c".into(), false),
        ]))
        .child(
            Card::new()
                .title_key("g_table")
                .action(search_input("q", "g_search"))
                .flush()
                .body(table_el(
                    &[
                        "g_col_name",
                        "g_col_status",
                        "g_col_duration",
                        "g_col_actions",
                    ],
                    runs,
                    true,
                ))
                .build(),
        )
        .child(
            div()
                .class("fui-grid fui-grid-2")
                .child(
                    Card::new()
                        .title_key("g_form")
                        .description_key("g_form_desc")
                        .body(
                            form_el("post", "#")
                                .child(field(
                                    "g_name",
                                    text_input("name", "text"),
                                    Some("g_name_hint"),
                                ))
                                .child(field(
                                    "g_kind",
                                    select_el("kind")
                                        .child(option().text("Service"))
                                        .child(option().text("Library")),
                                    None,
                                ))
                                .child(field("g_notes", textarea_el("notes"), None))
                                .child(checkbox_field("pub", "g_ok"))
                                .child(
                                    div()
                                        .class("fui-form-actions fui-end")
                                        .child(button_el(Variant::Secondary, "g_cancel"))
                                        .child(submit_button(Variant::Primary, "g_save")),
                                ),
                        )
                        .build(),
                )
                .child(
                    div()
                        .class("fui-stack")
                        .child(alert(Tone::Success, t("g_alert_ok")))
                        .child(alert(Tone::Warning, t("g_alert_warn")))
                        .child(alert(Tone::Error, t("g_alert_err")))
                        .child(alert(Tone::Info, t("g_alert_info")))
                        .child(
                            Card::new()
                                .title_key("g_buttons")
                                .body(
                                    div()
                                        .class("fui-row")
                                        .child(button_el(Variant::Primary, "g_save"))
                                        .child(button_el(Variant::Secondary, "g_cancel"))
                                        .child(button_el(Variant::Tertiary, "g_export"))
                                        .child(button_el(Variant::Danger, "g_delete"))
                                        .child(button_el(Variant::Link, "g_new"))
                                        .child(icon_button("gear", "nav_settings")),
                                )
                                .body(
                                    div()
                                        .class("fui-row")
                                        .child(badge(Tone::Gray, "gray"))
                                        .child(badge(Tone::Brand, "brand"))
                                        .child(badge(Tone::Success, "success"))
                                        .child(badge(Tone::Warning, "warning"))
                                        .child(badge(Tone::Error, "error"))
                                        .child(badge(Tone::Info, "info")),
                                )
                                .body(progress(64))
                                .build(),
                        ),
                ),
        )
        .child(
            Card::new()
                .body(empty_state(
                    "folder-open",
                    "g_empty_title",
                    Some("g_empty_desc"),
                ))
                .build(),
        )
        .child(button_el(Variant::Secondary, "g_open_modal").on_click(open_modal("demo")))
        .child(modal(
            "demo",
            "g_modal_title",
            p().attr("data-i18n", "g_modal_body"),
            Some(
                div()
                    .class("fui-row")
                    .child(button_el(Variant::Secondary, "g_cancel").on_click(close_modal("demo")))
                    .child(button_el(Variant::Danger, "g_delete")),
            ),
        ))
}
