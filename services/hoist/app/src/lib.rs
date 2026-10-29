//! The interface. The server renders `app()` into the first response and the browser takes the
//! page over, building the same view.

pub mod icons;
pub mod live;
pub mod public;
pub mod screens;

/// The app's own style on top of the framework's: the body, the bar at the bottom, the catalog's rows.
pub const STYLE: &str = "body{margin:0;font-family:system-ui,sans-serif;background:#0b0b0b;color:#d4d4d4}#hoist{display:flex;flex-direction:column;height:100vh}.q-appbar{flex:none}.hoist-screen{flex:1;min-height:0;display:flex;flex-direction:column}.hoist-scrolls,.hoist-scroll{overflow-y:auto}.hoist-scroll{flex:1;min-height:0}.hoist-searchbar{flex:none}#message{flex:none}.q-appbar{background:#0b0b0b;color:#d4d4d4;border-bottom:0;min-height:64px;padding-left:var(--q-space-4);padding-right:var(--q-space-4)}.q-appbar-title{font-size:1.375rem;font-weight:400;margin-left:0}.q-iconbtn .q-icon{width:24px;height:24px}.q-iconbtn{min-width:48px;min-height:48px;padding:0;color:#d4d4d4;font-size:1.5rem}.q-bottomnav{flex:none;background:#0b0b0b;border-top:0;min-height:80px}.q-bottomnav-item{background:transparent;color:#d4d4d4;border:0;border-radius:0;min-height:80px;padding:0;gap:.25rem;font-weight:500}.q-bottomnav-item[aria-current=page]{box-shadow:none}.q-bottomnav-glyph .q-icon{display:block;width:24px;height:24px}.q-bottomnav-glyph{padding:.25rem 1.25rem;border-radius:999px;font-size:1.25rem}.q-bottomnav-item[aria-current=page] .q-bottomnav-glyph{background:#047857;color:#ffffff}.q-iconbtn{border-radius:999px;transition:background-color .35s}.q-iconbtn:active{background:#2a2a2a;transition:background-color 0s}.q-bottomnav-item:active .q-bottomnav-glyph{background:#2a2a2a}.q-bottomnav-glyph{transition:background-color .35s}.hoist-row-main,.hoist-setting-button{transition:background-color .35s}.hoist-row-main:active,.hoist-setting-button:active{background:#1f1f1f;transition:background-color 0s}button.hoist-action{transition:background-color .35s}button.hoist-action:active{background:#0d3b2e;transition:background-color 0s}.hoist-primary:active,#sign-in:active{background:#065f46}.hoist-primary,#sign-in{transition:background-color .35s}.hoist-loader{position:fixed;left:0;right:0;top:0;bottom:0;display:flex;align-items:center;justify-content:center;gap:.75rem;background:rgba(11,11,11,.6)}.hoist-dot{width:16px;height:16px;border-radius:999px;background:#10b981;opacity:.25;animation-name:hoist-pulse;animation-duration:1.2s;animation-iteration-count:infinite}.hoist-dot-2{animation-delay:.2s}.hoist-dot-3{animation-delay:.4s}@keyframes hoist-pulse{0%{opacity:.25}40%{opacity:1}80%{opacity:.25}100%{opacity:.25}}.q-scrim{background:rgba(0,0,0,.6)}.q-dialog,.q-anim-in .q-dialog{left:1.5rem;right:1.5rem;width:auto;transform:translateY(-50%)}.q-dialog{background:#1f1f1f;color:#d4d4d4;border-radius:1.75rem;padding:1.5rem}.q-modal-title{font-size:1.5rem;font-weight:400}.q-dialog .hoist-note{margin:0}.hoist-danger{color:#fb7185;border-color:#4c1d2a}.hoist-section{margin-bottom:.5rem}.hoist-section-head{display:flex;align-items:center;justify-content:space-between}.hoist-link{min-height:40px;padding:0 .5rem;color:#10b981;background:transparent;border:0}.hoist-description{margin:1rem 0;color:#a3a3a3;white-space:pre-wrap}.hoist-hidden{display:none}.hoist-body{flex:1;min-height:0;display:flex;flex-direction:column;padding:0 var(--q-space-4)}h2{font-size:1.5rem;font-weight:400;margin:1.25rem 0 .25rem}h3{font-size:1.25rem;font-weight:500;margin:1.5rem 0 .75rem}.hoist-toolbar{display:flex;gap:.5rem;align-items:center;padding:.5rem 0}.hoist-actions{display:flex;gap:.5rem;align-items:center}.q-tile{padding:.5rem 0;background:transparent;border-radius:0}.q-tile-trailing{margin-right:.25rem}.hoist-actions button{padding:0 .875rem}button{min-height:40px;padding:0 1.25rem;font:inherit;font-weight:500;color:#059669;background:transparent;border:1px solid #2f3b37;border-radius:999px}.hoist-primary,#sign-in{display:block;width:100%;min-height:48px;margin:1.25rem 0 .5rem;color:#ffffff;background:#047857;border:0}.hoist-package{margin:0 0 1rem;color:#a3a3a3}.hoist-fact{display:flex;gap:1rem;padding:.25rem 0}.hoist-fact-name{min-width:7rem;font-weight:700;color:#d4d4d4}.hoist-fact-value{flex:1;min-width:0}.hoist-chips{display:flex;gap:.5rem;flex-wrap:wrap}.hoist-chip{padding:.4rem .75rem;font-size:.875rem;border:1px solid #d4d4d4;border-radius:.5rem}.hoist-note{margin:.5rem 0 1rem;color:#a3a3a3}.q-textfield .q-textfield-input{padding:0;background:transparent;color:#d4d4d4;border:0;border-bottom:1px solid #525252;border-radius:0}.q-textfield .q-textfield-input:focus-visible{outline:0;border-bottom:2px solid #047857}.q-textfield .q-textfield-label{color:#a3a3a3;font-size:.875rem}.hoist-searchbar{display:flex;align-items:center;margin:.5rem 0 1rem;padding:0 .75rem;border:1px solid #525252;border-radius:.5rem}.hoist-search-icon{width:24px;height:24px;margin-right:.75rem}.hoist-search{flex:1;min-height:0;margin:0;padding:.9rem 0;color:#d4d4d4;font-size:1rem;background:transparent;border:0}.hoist-list{list-style:none;margin:0;padding:0}.hoist-row{display:flex;align-items:center;gap:.5rem;padding:.25rem 0;border-bottom:1px solid #3a3a3a}.hoist-row-main{flex:1;min-width:0;min-height:64px;display:flex;align-items:center;gap:1rem;padding:0;text-align:left;font-weight:400;color:#d4d4d4;background:transparent;border:0;border-radius:0}.hoist-row-icon{width:28px;height:28px}.hoist-row-text{display:flex;flex-direction:column;min-width:0}.hoist-row-title{font-size:1.125rem}.hoist-row-package,.hoist-row-meta{color:#a3a3a3;font-size:.9375rem}.hoist-setting{display:flex;align-items:center;justify-content:space-between;gap:1rem;padding:1rem 0;text-align:left}.hoist-setting-column{flex-direction:column;align-items:stretch;gap:.25rem}.hoist-setting-button{width:100%;min-height:0;font-weight:400;color:#d4d4d4;background:transparent;border:0;border-radius:0}.hoist-setting-text{display:flex;flex-direction:column;flex:1;min-width:0;gap:.25rem}.hoist-setting-title{font-size:1.125rem}.hoist-setting-note{color:#a3a3a3;font-size:.9375rem}.hoist-setting-icon{width:24px;height:24px}.hoist-setting-error{margin:0;color:#e11d48}.hoist-server{padding:.5rem 0;color:#d4d4d4;font-size:1rem;background:transparent;border:0;border-bottom:1px solid #525252;border-radius:0}.hoist-rule{border-top:1px solid #3a3a3a}#message{margin:.5rem var(--q-space-4);color:#a3a3a3}";

use std::collections::BTreeMap;
use std::rc::Rc;

use hoist_core::{Artifact, Entry, Installed, build_entries};
use quench_ui::kit::{NavItem, app_bar, bottom_nav};
use quench_ui::prelude::*;
use screens::{Act, Action, Settings, catalog, settings};

/// A registry's worth of builds for trying the screens before the `http` plugin brings the real one.
#[must_use]
pub fn sample_catalog() -> (Vec<Artifact>, BTreeMap<String, Installed>) {
    let build = |program: &str, label: &str, code: i64, name: &str, size: i64| Artifact {
        program: program.to_string(),
        platform: "linux".to_string(),
        arch: Some("x86_64".to_string()),
        format: "deb".to_string(),
        version_code: code,
        version_name: name.to_string(),
        filename: format!("{program}_{name}.deb"),
        size_bytes: size,
        sha256: "0".repeat(64),
        label: Some(label.to_string()),
        min_sdk_version: None,
        target_sdk_version: None,
        permissions: Vec::new(),
        uploaded_by: "ci".to_string(),
        yanked: false,
        created_at: "2026-10-28T12:00:00Z".to_string(),
    };
    let catalog = vec![
        build("mathom", "Mathom", 12, "1.2.0", 41_758_291),
        build("daybook", "Daybook", 7, "0.7.1", 12_000_000),
        build("palantir", "Palantir", 3, "0.3.0", 88_000_000),
        build("hoist", "Hoist", 1, "0.1.0", 9_500_000),
    ];
    let installed = BTreeMap::from([
        (
            "mathom".to_string(),
            Installed {
                program: "mathom".to_string(),
                version_code: 11,
                version_name: "1.1.0".to_string(),
            },
        ),
        (
            "daybook".to_string(),
            Installed {
                program: "daybook".to_string(),
                version_code: 7,
                version_name: "0.7.1".to_string(),
            },
        ),
    ]);
    (catalog, installed)
}

/// The app: a catalog and the settings, with a bar to switch between them.
#[must_use]
pub fn app() -> View {
    View::lazy(|| {
        let (catalog_data, installed) = sample_catalog();
        let entries = create_signal(build_entries(&catalog_data, &installed, "hoist"));
        let screen = create_signal(0_usize);
        let message = create_signal(String::new());
        let (note, shown) = (message.clone(), message.clone());
        // Until the hosts' `http` and `packages` plugins are wired in, an action only says what it would do.
        let act: Act = {
            let entries = entries.clone();
            Rc::new(move |action: Action| {
                let said = match &action {
                    Action::Install(p) => format!("Installing {p}"),
                    Action::Update(p) => format!("Updating {p}"),
                    Action::Open(p) => format!("Opening {p}"),
                    Action::Details(p) => format!("Details of {p}"),
                    Action::UpdateAll => format!(
                        "Updating {} programs",
                        entries
                            .get()
                            .iter()
                            .filter(
                                |e: &&Entry| e.state() == hoist_core::InstallState::UpdateAvailable
                            )
                            .count()
                    ),
                };
                note.set(said);
            })
        };
        let prefs = Settings::new("https://warehouse.example.org", true);
        let search = create_signal(String::new());
        let saved = message.clone();
        let panels = {
            let (entries, act, screen) = (entries, act, screen.clone());
            View::dynamic(move || {
                if screen.get() == 0 {
                    catalog(&entries, &create_signal(Vec::new()), &act, &search)
                } else {
                    let (checked, out) = (saved.clone(), saved.clone());
                    settings(
                        &prefs,
                        "sample",
                        move || checked.set("Checking".to_string()),
                        move || out.set("Signed out".to_string()),
                    )
                }
            })
        };
        div()
            .attr("id", "hoist")
            .child(app_bar("Hoist", None, Vec::new()))
            .child(div().class("hoist-body").child(panels))
            .child(
                p().attr("id", "message")
                    .attr("role", "status")
                    .text_fn(move || shown.get()),
            )
            .child(bottom_nav(
                "Main",
                vec![
                    NavItem::new("Catalog", "\u{2630}"),
                    NavItem::new("Settings", "\u{2699}"),
                ],
                &screen,
            ))
            .into_view()
    })
}

quench_ui_web::app!(app);
