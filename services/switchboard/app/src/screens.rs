//! The screens, as functions from the state to a view.

use std::rc::Rc;

use quench_ui::i18n::{I18n, Value};
use quench_ui::prelude::*;
use quench_ui::reactive::untrack;
use quench_ui::router::{Router, link};
use switchboard_core::draft::{DEVICES, DTYPES, LAUNCH_QUANTS, SORTS, TASKS};
use switchboard_core::fit::{
    CardFit, FitTone, LaunchFit, card_fit, estimate_rows, filter_options, launch_fit,
    launch_gpu_util,
};
use switchboard_core::model::Tone;
use switchboard_core::{Context, FilterDraft, Gpu, Instance, LaunchDraft, Model};

use crate::live::{Estimates, Request, State};

/// What every screen needs of the app.
#[derive(Clone)]
pub struct Env {
    pub t: Rc<I18n>,
    pub base: String,
    pub ask: Rc<dyn Fn(Request)>,
}

impl Env {
    fn path(&self, rest: &str) -> String {
        format!("{}/ui{rest}", self.base)
    }
}

/// A screen that is built again when the language changes (its texts are plain strings).
fn follow(env: &Env, build: impl Fn() -> View + 'static) -> View {
    let t = Rc::clone(&env.t);
    View::dynamic(move || {
        let _ = t.language();
        untrack(&build)
    })
}

/// A refusal's text: the service answers with a translation key (`api_error_...`) or with words.
#[must_use]
pub fn reason(t: &I18n, text: &str) -> String {
    if text.starts_with("api_error_") {
        t.t(text)
    } else {
        text.to_string()
    }
}

fn waiting(env: &Env) -> View {
    p().attr("id", "waiting")
        .class("fui-empty")
        .text(env.t.t("ui_loading"))
        .into_view()
}

#[must_use]
pub fn not_found(env: &Env) -> View {
    let env = env.clone();
    follow(&env.clone(), move || {
        div()
            .class("sb-page")
            .child(
                p().attr("id", "not-found")
                    .class("fui-empty")
                    .text(env.t.t("ui_not_found")),
            )
            .child(link(&env.path("/home")).text(env.t.t("ui_home_button")))
            .into_view()
    })
}

/// Home and the two screens that are switched on: what goes under the brand in the side panel.
#[must_use]
pub(crate) fn sidebar(env: &Env, state: &State, router: &Router) -> View {
    let (env, caps, router) = (env.clone(), state.caps.clone(), router.clone());
    View::dynamic(move || {
        let here = router.location().path;
        let item = |rest: &str, key: &str, id: &str| {
            let href = env.path(rest);
            let mut item = link(&href)
                .attr("id", id.to_string())
                .class("fui-nav-item")
                .child(forge_shell::icon(match id {
                    "nav-models" => "layers",
                    "nav-vllm" => "server",
                    _ => "house",
                }))
                .child(span().text(env.t.t(key)));
            if here.trim_end_matches('/') == href {
                item = item.attr("aria-current", "page").class("sb-current");
            }
            item
        };
        let mut nav = div().child(item("/home", "ui_home_button", "nav-home"));
        if let Some(caps) = caps.get() {
            if caps.models_dashboard {
                nav = nav.child(item("/models/dashboard", "ui_header_models", "nav-models"));
            }
            if caps.vllm_management {
                nav = nav.child(item("/vllm/manage", "ui_header_vllm", "nav-vllm"));
            }
        }
        nav.into_view()
    })
}

// ── Home ─────────────────────────────────────────────────────────────────────

fn tile(env: &Env, id: &str, rest: &str, title: &str, desc: &str) -> Element {
    link(&env.path(rest))
        .attr("id", id.to_string())
        .class("fui-link-card")
        .child(
            div()
                .class("fui-link-card-body")
                .child(div().class("fui-link-card-title").text(env.t.t(title)))
                .child(div().class("fui-link-card-desc").text(env.t.t(desc))),
        )
        .child(
            div()
                .class("fui-link-card-arrow")
                .child(forge_shell::icon("arrow-right")),
        )
}

/// A card for each screen that is switched on.
pub(crate) fn home(env: &Env, state: &State) -> View {
    let (env, caps) = (env.clone(), state.caps.clone());
    follow(&env.clone(), move || {
        let (env, caps) = (env.clone(), caps.clone());
        div()
            .class("sb-page")
            .child(
                div().child(h1().text(env.t.t("ui_home_title"))).child(
                    p().class("sb-muted")
                        .text(env.t.t("ui_home_group_services")),
                ),
            )
            .child(View::dynamic(move || {
                let Some(caps) = caps.get() else {
                    return waiting(&env);
                };
                if !caps.models_dashboard && !caps.vllm_management {
                    return p()
                        .attr("id", "no-services")
                        .class("fui-empty")
                        .text(env.t.t("ui_home_no_services"))
                        .into_view();
                }
                let mut cards = div().class("fui-card-grid");
                if caps.models_dashboard {
                    cards = cards.child(tile(
                        &env,
                        "tile-models",
                        "/models/dashboard",
                        "ui_service_models_dashboard_title",
                        "ui_service_models_dashboard_desc",
                    ));
                }
                if caps.vllm_management {
                    cards = cards.child(tile(
                        &env,
                        "tile-vllm",
                        "/vllm/manage",
                        "ui_service_vllm_management_title",
                        "ui_service_vllm_management_desc",
                    ));
                }
                cards.into_view()
            }))
            .into_view()
    })
}

// ── Shared pieces ────────────────────────────────────────────────────────────

/// The GPU's name, total and free memory, kept fresh by the host's polling.
fn gpu_box(env: &Env, state: &State) -> View {
    let (env, gpu) = (env.clone(), state.gpu.clone());
    View::dynamic(move || {
        let gpu = gpu.get().filter(Gpu::present);
        let (name, total, free) = gpu.map_or_else(
            || (env.t.t("ui_gpu_unavailable"), "n/a".into(), "n/a".into()),
            |g| {
                (
                    format!("{} {}", env.t.t("ui_gpu_label"), g.name),
                    g.total_gb.to_string(),
                    g.free_gb.to_string(),
                )
            },
        );
        div()
            .attr("id", "gpu-status")
            .class("sb-gpu")
            .child(div().class("gpu-name").text(name))
            .child(
                div()
                    .class("gpu-total")
                    .child(span().text(env.t.t("ui_models_gpu_total")))
                    .child(span().text(format!(" {total} GB"))),
            )
            .child(
                div()
                    .class("gpu-free")
                    .child(span().text(env.t.t("ui_models_gpu_free")))
                    .child(span().text(format!(" {free} GB"))),
            )
            .into_view()
    })
}

/// A window over the page with a title and a close button; clicking outside closes it.
fn modal(id: &str, title: Element, close: Rc<dyn Fn()>, body: Element, env: &Env) -> Element {
    let (shut, away) = (Rc::clone(&close), close);
    div()
        .attr("id", id.to_string())
        .attr("role", "dialog")
        .attr("aria-modal", "true")
        .class("sb-modal")
        .child(
            button()
                .attr("type", "button")
                .attr("aria-label", env.t.t("ui_close"))
                .class("sb-backdrop")
                .on_click(move || away()),
        )
        .child(
            div()
                .class("sb-modal-panel")
                .child(
                    div().class("sb-modal-head").child(h3().child(title)).child(
                        button()
                            .attr("type", "button")
                            .attr("id", format!("{id}-close"))
                            .attr("aria-label", env.t.t("ui_close"))
                            .class("sb-x")
                            .text("×")
                            .on_click(move || shut()),
                    ),
                )
                .child(body),
        )
}

fn tone_class(tone: FitTone) -> &'static str {
    match tone {
        FitTone::Ok => "fit-ok",
        FitTone::Warn => "fit-warn",
        FitTone::No => "fit-no",
    }
}

/// `label: value`, the label bold.
fn labelled(label: String, value: String) -> Element {
    span()
        .child(element("strong").text(label))
        .child(span().text(format!(": {value}")))
}

fn sep() -> Element {
    span().class("fit-separator").text(" | ")
}

/// A `<select>`; `options` are `(value, text)`, `current` is read to mark the chosen one.
fn choice(
    id: &str,
    options: Vec<(String, String)>,
    current: impl Fn() -> String + 'static,
    on_pick: impl Fn(&str) + 'static,
) -> Element {
    let current = Rc::new(current);
    let rows = options.into_iter().map(|(value, text)| {
        let (current, v) = (Rc::clone(&current), value.clone());
        option()
            .attr("value", value)
            .attr_bool("selected", move || current() == v)
            .text(text)
            .into_view()
    });
    select()
        .class("sb-input")
        .attr("id", id.to_string())
        .attr("name", id.to_string())
        .on_input(on_pick)
        .children(rows)
}

fn check(
    id: &str,
    text: String,
    on: impl Fn() -> bool + 'static,
    flip: impl Fn() + 'static,
) -> Element {
    label()
        .class("sb-check")
        .child(
            input()
                .attr("type", "checkbox")
                .attr("id", id.to_string())
                .attr("name", id.to_string())
                .attr_bool("checked", on)
                .on_click(flip),
        )
        .child(span().text(text))
}

// ── Models ───────────────────────────────────────────────────────────────────

fn ask_models(env: &Env, state: &State) {
    (env.ask)(Request::Models(
        state.filter.with_untracked(FilterDraft::body),
    ));
}

/// Changes the filters and asks for the models they leave.
fn refilter(env: &Env, state: &State, change: impl FnOnce(&mut FilterDraft)) {
    state.filter.update(change);
    ask_models(env, state);
}

fn quant_options(env: &Env, filter: &FilterDraft) -> Vec<(String, String)> {
    let mut rows = vec![("ALL".to_string(), env.t.t("ui_models_filter_all_quants"))];
    for q in filter.quants() {
        let key = format!("ui_models_quant_{}", q.to_ascii_lowercase());
        rows.push(((*q).to_string(), env.t.t(&key)));
    }
    rows
}

fn filters(env: &Env, state: &State) -> Element {
    let (e1, e2, e3, e4, e5, e6) = (
        env.clone(),
        env.clone(),
        env.clone(),
        env.clone(),
        env.clone(),
        env.clone(),
    );
    let (s1, s2, s3, s4, s5, s6) = (
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
    );
    let (c1, c2, c3, c4) = (
        state.filter.clone(),
        state.filter.clone(),
        state.filter.clone(),
        state.filter.clone(),
    );
    let source = choice(
        "source",
        vec![
            ("hf".into(), env.t.t("ui_models_tab_hf")),
            ("gguf".into(), env.t.t("ui_models_tab_gguf")),
        ],
        move || c1.with(|f| f.source.clone()),
        move |v| {
            refilter(&e1, &s1, |f| {
                f.source = v.to_string();
                f.quant = "ALL".into();
            });
        },
    );
    let sort = choice(
        "sort",
        SORTS
            .iter()
            .map(|s| ((*s).to_string(), env.t.t(&format!("ui_models_sort_{s}"))))
            .collect(),
        move || c2.with(|f| f.sort.clone()),
        move |v| refilter(&e2, &s2, |f| f.sort = v.to_string()),
    );
    let search = input()
        .attr("id", "search")
        .attr("name", "search")
        .attr("type", "search")
        .class("sb-input")
        .attr("placeholder", env.t.t("ui_models_search_placeholder"))
        .attr_fn("value", {
            let f = state.filter.clone();
            move || f.with(|f| f.search.clone())
        })
        .on_input(move |v| refilter(&e3, &s3, |f| f.search = v.to_string()));
    // The quantisations are the source's own: the select is built again when the source changes.
    let quant = {
        let (env, filter, state) = (e4, c3, s4);
        View::dynamic(move || {
            let source = filter.with(|f| f.source.clone());
            let options = filter.with(|f| quant_options(&env, f));
            let (env, state, current) = (env.clone(), state.clone(), filter.clone());
            let _ = source;
            choice(
                "quant",
                options,
                move || current.with(|f| f.quant.clone()),
                move |v| refilter(&env, &state, |f| f.quant = v.to_string()),
            )
            .into_view()
        })
    };
    let mut contexts = vec![("0".to_string(), env.t.t("ui_models_filter_all_contexts"))];
    contexts.extend(
        Context::SIZES
            .iter()
            .map(|n| (n.to_string(), n.to_string())),
    );
    let context = choice(
        "context",
        contexts,
        move || c4.with(|f| f.context.clone()),
        move |v| refilter(&e5, &s5, |f| f.context = v.to_string()),
    );
    let only = {
        let on = state.filter.clone();
        check(
            "vllm-only",
            env.t.t("ui_models_filter_vllm_only"),
            move || on.with(|f| f.vllm_only),
            move || refilter(&e6, &s6, |f| f.vllm_only = !f.vllm_only),
        )
    };
    form()
        .attr("id", "model-filters")
        .class("sb-filters")
        .on_submit(|| {})
        .child(source)
        .child(sort)
        .child(search)
        .child(quant)
        .child(context)
        .child(only)
}

fn fit_line(env: &Env, fit: &CardFit) -> (String, Element) {
    match fit {
        CardFit::Fits {
            context,
            quant,
            total_gb,
            margin_gb,
            tone,
        } => (
            format!("fit-line {}", tone_class(*tone)),
            div()
                .child(span().text(env.t.t("ui_models_card_fits_yes")))
                .child(sep())
                .child(labelled(
                    env.t.t("ui_models_card_best"),
                    format!("{context} / {quant}"),
                ))
                .child(sep())
                .child(labelled(
                    env.t.t("ui_models_card_vram"),
                    format!("{total_gb:.1} GB"),
                ))
                .child(sep())
                .child(labelled(
                    env.t.t("ui_models_card_margin"),
                    format!("{margin_gb:.2} GB"),
                )),
        ),
        CardFit::TooBig { min_gb } => (
            "fit-line fit-no".into(),
            div()
                .child(span().text(env.t.t("ui_models_card_fits_no")))
                .child(sep())
                .child(labelled(
                    env.t.t("ui_models_card_minimum"),
                    format!("{min_gb:.1} GB"),
                )),
        ),
        CardFit::NoEstimates => (
            "fit-line fit-no".into(),
            div().text(env.t.t("ui_models_card_no_estimates")),
        ),
    }
}

fn meta_item(class: &str, label: String, value: String) -> Element {
    div()
        .class(class.to_string())
        .child(span().text(label))
        .child(span().text(format!(": {value}")))
}

fn model_card(env: &Env, state: &State, model: Model, can_delete: bool) -> View {
    let name = model.name.clone();
    let mut title = div().class("card-title");
    if model.vllm_supported {
        title = title.child(span().class("vllm-badge").text("vLLM"));
    }
    title = title.child(span().class("card-title-text").text(model.name.clone()));
    let mut head = div().class("card-header").child(title);
    if can_delete {
        let (deleting, path) = (state.deleting.clone(), model.path.clone());
        head = head.child(
            button()
                .attr("type", "button")
                .attr("title", env.t.t("ui_models_card_delete_tooltip"))
                .attr("aria-label", env.t.t("ui_models_card_delete_tooltip"))
                .attr("data-delete", model.path.clone())
                .class("sb-icon-btn card-delete")
                .text("🗑")
                .on_click(move || deleting.set(Some((path.clone(), name.clone())))),
        );
    }
    let meta = div()
        .class("card-meta")
        .child(
            div()
                .child(meta_item(
                    "card-meta-params",
                    env.t.t("ui_models_card_params"),
                    format!("{:.1}B", model.params_billion),
                ))
                .child(meta_item(
                    "card-meta-quant",
                    env.t.t("ui_models_card_quant"),
                    model.quant.clone(),
                ))
                .child(meta_item(
                    "card-meta-context",
                    env.t.t("ui_models_card_context"),
                    model.context.to_string(),
                )),
        )
        .child(
            div()
                .child(meta_item(
                    "card-meta-layers",
                    env.t.t("ui_models_card_layers"),
                    model.layers.to_string(),
                ))
                .child(meta_item(
                    "card-meta-hidden",
                    env.t.t("ui_models_card_hidden"),
                    model.hidden_size.to_string(),
                )),
        );
    let fit = {
        let (env, gpu, estimates, model) = (
            env.clone(),
            state.gpu.clone(),
            state.estimates.clone(),
            model.clone(),
        );
        View::dynamic(move || {
            let free = gpu.get().map_or(0.0, |g| g.free_gb);
            let (class, content) = fit_line(&env, &card_fit(&model, free));
            let (estimates, model) = (estimates.clone(), model.clone());
            button()
                .attr("type", "button")
                .class("card-fit")
                .attr("data-estimates", model.path.clone())
                .on_click(move || {
                    estimates.set(Some(Estimates {
                        model: model.clone(),
                        fit: "all".into(),
                        context: "all".into(),
                        quant: "all".into(),
                    }));
                })
                .child(
                    div()
                        .class(class)
                        .child(content)
                        .child(span().class("fit-details-icon").text("📊")),
                )
                .into_view()
        })
    };
    div()
        .class("card")
        .child(head)
        .child(meta)
        .child(fit)
        .child(
            div()
                .class("card-path")
                .attr("title", model.path.clone())
                .text(model.path.clone()),
        )
        .into_view()
}

fn estimates_modal(env: &Env, state: &State) -> View {
    let (env, state) = (env.clone(), state.clone());
    let path = {
        let estimates = state.estimates.clone();
        create_memo(move || estimates.with(|e| e.as_ref().map(|e| e.model.path.clone())))
    };
    View::dynamic(move || {
        if path.get().is_none() {
            return View::Empty;
        }
        let Some(open) = state.estimates.get_untracked() else {
            return View::Empty;
        };
        let model = open.model;
        let (contexts, quants) = filter_options(&model);
        let close: Rc<dyn Fn()> = {
            let estimates = state.estimates.clone();
            Rc::new(move || estimates.set(None))
        };
        let pick = |field: fn(&mut Estimates) -> &mut String| {
            let estimates = state.estimates.clone();
            move |v: &str| {
                estimates.update(|e| {
                    if let Some(e) = e {
                        *field(e) = v.to_string();
                    }
                });
            }
        };
        let read = |field: fn(&Estimates) -> &String| {
            let estimates = state.estimates.clone();
            move || estimates.with(|e| e.as_ref().map(|e| field(e).clone()).unwrap_or_default())
        };
        let fit = choice(
            "estimates-fit",
            vec![
                (
                    "all".into(),
                    env.t.t("ui_models_modal_estimates_filter_all"),
                ),
                (
                    "fit".into(),
                    env.t.t("ui_models_modal_estimates_filter_fits"),
                ),
                (
                    "nofit".into(),
                    env.t.t("ui_models_modal_estimates_filter_nofit"),
                ),
            ],
            read(|e| &e.fit),
            pick(|e| &mut e.fit),
        );
        let mut context_rows = vec![(
            "all".to_string(),
            env.t.t("ui_models_modal_estimates_filter_all_contexts"),
        )];
        context_rows.extend(contexts.into_iter().map(|c| (c.clone(), c)));
        let context = choice(
            "estimates-context",
            context_rows,
            read(|e| &e.context),
            pick(|e| &mut e.context),
        );
        let mut quant_rows = vec![(
            "all".to_string(),
            env.t.t("ui_models_modal_estimates_filter_all_quants"),
        )];
        quant_rows.extend(quants.into_iter().map(|q| (q.clone(), q)));
        let quant = choice(
            "estimates-quant",
            quant_rows,
            read(|e| &e.quant),
            pick(|e| &mut e.quant),
        );
        let rows = {
            let (env, state, model) = (env.clone(), state.clone(), model.clone());
            View::dynamic(move || {
                let free = state.gpu.get().map_or(0.0, |g| g.free_gb);
                let (fit, context, quant) = state
                    .estimates
                    .with(|e| {
                        e.as_ref()
                            .map(|e| (e.fit.clone(), e.context.clone(), e.quant.clone()))
                    })
                    .unwrap_or_default();
                let mut grid = div().attr("id", "estimate-grid").class("estimate-grid");
                for row in estimate_rows(&model, free, &fit, &context, &quant) {
                    grid = grid.child(
                        div()
                            .class(format!("fit-line {}", tone_class(row.tone)))
                            .child(
                                div().child(labelled(
                                    env.t.t("ui_models_card_context"),
                                    row.context,
                                )),
                            )
                            .child(
                                div().child(labelled(env.t.t("ui_models_card_quant"), row.quant)),
                            )
                            .child(div().child(labelled(
                                env.t.t("ui_models_card_vram"),
                                format!("{:.1} GB", row.total_gb),
                            )))
                            .child(div().child(labelled(
                                env.t.t("ui_models_card_margin"),
                                format!("{:.2} GB", row.margin_gb),
                            ))),
                    );
                }
                grid.into_view()
            })
        };
        let body = div()
            .class("sb-modal-body")
            .child(
                div()
                    .class("estimate-filters")
                    .child(fit)
                    .child(context)
                    .child(quant),
            )
            .child(rows);
        modal(
            "estimates-modal",
            span()
                .child(span().text(env.t.t("ui_models_modal_estimates_title")))
                .child(span().text(format!(" - {}", model.name))),
            close,
            body,
            &env,
        )
        .into_view()
    })
}

fn delete_modal(env: &Env, state: &State) -> View {
    let (env, state) = (env.clone(), state.clone());
    let target = state.deleting.clone();
    View::dynamic(move || {
        let Some((path, name)) = target.get() else {
            return View::Empty;
        };
        let close: Rc<dyn Fn()> = {
            let target = state.deleting.clone();
            Rc::new(move || target.set(None))
        };
        let (cancel, confirm) = (Rc::clone(&close), env.ask.clone());
        let body = div()
            .class("sb-modal-body")
            .child(p().text(env.t.t("ui_models_modal_delete_text")))
            .child(div().class("model-to-delete-name").text(name))
            .child(
                div()
                    .class("sb-actions")
                    .child(
                        button()
                            .attr("type", "button")
                            .attr("id", "delete-cancel")
                            .class("sb-btn")
                            .text(env.t.t("ui_common_cancel"))
                            .on_click(move || cancel()),
                    )
                    .child(
                        button()
                            .attr("type", "button")
                            .attr("id", "delete-confirm")
                            .class("sb-btn sb-danger")
                            .text(env.t.t("ui_models_modal_delete_confirm"))
                            .on_click(move || confirm(Request::DeleteModel(path.clone()))),
                    ),
            );
        modal(
            "confirm-delete-modal",
            span().text(env.t.t("ui_models_modal_delete_title")),
            close,
            body,
            &env,
        )
        .into_view()
    })
}

/// The models registry: filters, the GPU, a card per model, and the two windows over it.
pub(crate) fn models(env: &Env, state: &State) -> View {
    let (env, state) = (env.clone(), state.clone());
    follow(&env.clone(), move || {
        let can_delete = {
            let caps = state.caps.clone();
            move || caps.with(|c| c.as_ref().is_some_and(|c| c.can_delete_model))
        };
        let grid = {
            let (env, state, can_delete) = (env.clone(), state.clone(), can_delete.clone());
            View::dynamic(move || {
                let Some(count) = state.models.with(|m| m.as_ref().map(Vec::len)) else {
                    return waiting(&env);
                };
                if count == 0 {
                    return p()
                        .attr("id", "no-models")
                        .class("fui-empty")
                        .text(env.t.t("ui_models_empty"))
                        .into_view();
                }
                let (items, key_delete) = (state.models.clone(), can_delete());
                let (env, state) = (env.clone(), state.clone());
                div()
                    .attr("id", "models-grid")
                    .class("models-grid sb-grid")
                    .child(View::keyed(
                        move || items.with(|m| m.clone().unwrap_or_default()),
                        move |m: &Model| {
                            format!(
                                "{}|{}|{}|{}|{}|{}|{}|{}|{}|{key_delete}",
                                m.path,
                                m.name,
                                m.vllm_supported,
                                m.quant,
                                m.context,
                                m.layers,
                                m.hidden_size,
                                m.params_billion,
                                m.estimates.len()
                            )
                        },
                        move |m| model_card(&env, &state, m, key_delete),
                    ))
                    .into_view()
            })
        };
        div()
            .class("sb-page")
            .child(
                div()
                    .class("sb-toolbar")
                    .child(gpu_box(&env, &state))
                    .child(div().class("sb-grow"))
                    .child(filters(&env, &state)),
            )
            .child(grid)
            .child(estimates_modal(&env, &state))
            .child(delete_modal(&env, &state))
            .into_view()
    })
}

// ── vLLM ─────────────────────────────────────────────────────────────────────

fn status_text(env: &Env, status: &str) -> String {
    let key = format!("ui_vllm_status_{status}");
    let text = env.t.t(&key);
    if text == key {
        status.to_string()
    } else {
        text
    }
}

fn instance_card(env: &Env, state: &State, i: &Instance, can_stop: bool) -> View {
    let cpu = i.is_cpu();
    let (device_text, device_key, device_class) = if cpu {
        ("CPU", "ui_vllm_device_cpu", "badge badge-device badge-cpu")
    } else {
        ("GPU", "ui_vllm_device_gpu", "badge badge-device badge-gpu")
    };
    let mut head = div()
        .class("card-header")
        .child(div().class("card-title").text(i.model.clone()))
        .child(
            span()
                .class(device_class)
                .attr("title", format!("Running on {device_text}"))
                .text(env.t.t(device_key)),
        );
    if can_stop && i.stoppable() {
        let (stopping, id, model) = (state.stopping.clone(), i.id.clone(), i.model.clone());
        head = head.child(
            button()
                .attr("type", "button")
                .attr("title", env.t.t("ui_vllm_stop_tooltip"))
                .attr("aria-label", env.t.t("ui_vllm_stop_tooltip"))
                .attr("data-stop", i.id.clone())
                .class("sb-icon-btn card-delete")
                .text("⏻")
                .on_click(move || stopping.set(Some((id.clone(), model.clone())))),
        );
    }
    let tone = match i.tone() {
        Tone::Failed => "status-failed",
        Tone::Warn => {
            if i.status == "starting" {
                "status-starting"
            } else {
                "status-terminating"
            }
        }
        Tone::Ok => "status-running",
    };
    let item = |label: &str, value: Element| {
        div()
            .class("meta-item")
            .child(span().text(env.t.t(label)))
            .child(span().text(": "))
            .child(value)
    };
    let meta = div()
        .class("card-meta")
        .child(item(
            "ui_vllm_meta_id",
            span().class("instance-id").text(i.id.clone()),
        ))
        .child(item(
            "ui_vllm_meta_namespace",
            span().class("instance-namespace").text(i.namespace.clone()),
        ))
        .child(item(
            "ui_vllm_meta_endpoint",
            span().class("instance-endpoint").text(i.endpoint()),
        ))
        .child(item(
            "ui_vllm_meta_status",
            span()
                .class(format!("instance-status {tone}"))
                .text(status_text(env, &i.status)),
        ))
        .child(item(
            "ui_vllm_meta_started",
            span().class("instance-started").text(i.started()),
        ));
    let fit_class = match i.tone() {
        Tone::Failed => "fit-no",
        Tone::Warn => "fit-warn",
        Tone::Ok => "fit-ok",
    };
    let mut fit = div()
        .class(format!("fit-line {fit_class}"))
        .child(span().text(env.t.t("ui_models_card_quant")))
        .child(span().text(": "))
        .child(span().text(i.quant_text()));
    let mut part = |label: &str, value: String| {
        fit = std::mem::replace(&mut fit, div())
            .child(span().text(" | "))
            .child(span().text(env.t.t(label)))
            .child(span().text(": "))
            .child(span().text(value));
    };
    if let Some(dtype) = &i.dtype {
        part("ui_vllm_form_dtype", dtype.clone());
    }
    if let Some(limit) = &i.limit_mm_per_prompt {
        part("ui_vllm_form_limit_mm", limit.clone());
    }
    if cpu {
        part("ui_vllm_meta_device", "CPU".into());
    } else {
        part("ui_vllm_meta_gpu_util", i.gpu_util_text());
    }
    let mut diag = div().class("instance-diagnostics");
    if let Some(err) = &i.last_error {
        diag = diag.child(div().class("instance-error").text(err.clone()));
    }
    if let Some(log) = &i.log_path {
        diag = diag.child(
            div()
                .class("instance-log-path")
                .attr("title", format!("log: {log}"))
                .text(format!("log: {log}")),
        );
    }
    let mut card = div()
        .class("card")
        .attr("data-instance", i.id.clone())
        .child(head)
        .child(meta)
        .child(div().class("card-fit").child(fit));
    if i.last_error.is_some() || i.log_path.is_some() {
        card = card.child(diag);
    }
    card.into_view()
}

fn instance_key(i: &Instance, can_stop: bool) -> String {
    format!(
        "{}|{}|{}|{}|{}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{}|{:?}|{:?}|{can_stop}",
        i.id,
        i.namespace,
        i.model,
        i.endpoint(),
        i.status,
        i.quantization,
        i.dtype,
        i.limit_mm_per_prompt,
        i.device,
        i.gpu_memory_utilization,
        i.max_model_len,
        i.started_at,
        i.last_error,
        i.log_path,
    )
}

fn stop_modal(env: &Env, state: &State) -> View {
    let (env, state) = (env.clone(), state.clone());
    let target = state.stopping.clone();
    View::dynamic(move || {
        let Some((id, model)) = target.get() else {
            return View::Empty;
        };
        let close: Rc<dyn Fn()> = {
            let target = state.stopping.clone();
            Rc::new(move || target.set(None))
        };
        let (cancel, confirm) = (Rc::clone(&close), env.ask.clone());
        let name = if model.is_empty() {
            env.t.t("ui_vllm_unknown_model")
        } else {
            model
        };
        let body = div()
            .class("sb-modal-body")
            .child(p().text(env.t.t("ui_vllm_stop_modal_text")))
            .child(div().class("model-to-delete-name").text(name))
            .child(
                div()
                    .class("sb-actions")
                    .child(
                        button()
                            .attr("type", "button")
                            .attr("id", "stop-cancel")
                            .class("sb-btn")
                            .text(env.t.t("ui_common_cancel"))
                            .on_click(move || cancel()),
                    )
                    .child(
                        button()
                            .attr("type", "button")
                            .attr("id", "stop-confirm")
                            .class("sb-btn sb-danger")
                            .text(env.t.t("ui_vllm_stop_modal_confirm"))
                            .on_click(move || confirm(Request::Stop(id.clone()))),
                    ),
            );
        modal(
            "confirm-stop-instance-modal",
            span().text(env.t.t("ui_vllm_stop_modal_title")),
            close,
            body,
            &env,
        )
        .into_view()
    })
}

/// Sets the memory share to what the chosen model, quantisation, length and device ask for.
fn recalc(state: &State) {
    let gpu = state.gpu.get_untracked().unwrap_or_default();
    let catalog = state.catalog.get_untracked();
    state.draft.update(|d| {
        let model = catalog.iter().find(|m| m.name == d.model);
        let util = launch_gpu_util(
            model,
            &d.quantization,
            d.max_len(),
            d.typed_gpu_util(),
            true,
            &gpu,
        );
        d.gpu_util = format!("{util:.2}");
    });
}

fn field(env: &Env, id: &str, label: &str, control: Element) -> Element {
    div()
        .class("sb-field")
        .child(
            element("label")
                .attr("for", id.to_string())
                .text(env.t.t(label)),
        )
        .child(control)
}

/// A text or number input bound to a part of the draft.
fn typed(
    state: &State,
    id: &str,
    kind: &str,
    get: fn(&LaunchDraft) -> String,
    set: fn(&mut LaunchDraft, String),
    then: Option<fn(&State)>,
) -> Element {
    let (read, write) = (state.draft.clone(), state.clone());
    input()
        .attr("id", id.to_string())
        .attr("name", id.to_string())
        .attr("type", kind.to_string())
        .class("sb-input")
        .attr_fn("value", move || read.with(get))
        .on_input(move |v| {
            write.draft.update(|d| set(d, v.to_string()));
            if let Some(then) = then {
                then(&write);
            }
        })
}

/// A select bound to a part of the draft.
fn picker(
    state: &State,
    id: &str,
    options: Vec<(String, String)>,
    get: fn(&LaunchDraft) -> String,
    set: fn(&mut LaunchDraft, String),
    then: Option<fn(&State)>,
) -> Element {
    let (read, write) = (state.draft.clone(), state.clone());
    choice(
        id,
        options,
        move || read.with(get),
        move |v| {
            write.draft.update(|d| set(d, v.to_string()));
            if let Some(then) = then {
                then(&write);
            }
        },
    )
}

fn launch_note(env: &Env, state: &State) -> View {
    let (env, state) = (env.clone(), state.clone());
    View::dynamic(move || {
        let fit = launch_fit_now(&state);
        let args: Vec<(&str, Value)> = fit
            .args
            .iter()
            .map(|(k, v)| (*k, Value::Text(v.clone())))
            .collect();
        let icon = match fit.tone {
            FitTone::No => "✖",
            FitTone::Warn => "⚠",
            FitTone::Ok => "✔",
        };
        div()
            .attr("id", "launch-fit-note")
            .class("fit-note")
            .child(
                div()
                    .class(format!("fit-line {}", tone_class(fit.tone)))
                    .attr("data-fit", fit.key)
                    .child(span().class("fit-icon").text(icon))
                    .child(span().text(env.t.tr(fit.key, &args))),
            )
            .into_view()
    })
}

/// Where the launch form stands now, for the note under it and the Launch button.
fn launch_fit_now(state: &State) -> LaunchFit {
    let draft = state.draft.get();
    if draft.is_cpu() {
        return LaunchFit::cpu();
    }
    let gpu = state.gpu.get().unwrap_or_default();
    let catalog = state.catalog.get();
    let model = catalog.iter().find(|m| m.name == draft.model);
    let util = launch_gpu_util(
        model,
        &draft.quantization,
        draft.max_len(),
        draft.typed_gpu_util(),
        false,
        &gpu,
    );
    launch_fit(model, &draft.quantization, draft.max_len(), util, &gpu)
}

#[allow(clippy::too_many_lines)]
fn launch_modal(env: &Env, state: &State) -> View {
    let (env, state) = (env.clone(), state.clone());
    let open = state.launch_open.clone();
    View::dynamic(move || {
        if !open.get() {
            return View::Empty;
        }
        let close: Rc<dyn Fn()> = {
            let open = state.launch_open.clone();
            Rc::new(move || open.set(false))
        };
        let cancel = Rc::clone(&close);
        let namespace_shown = state.draft.with_untracked(|d| d.namespace != "native");
        let model_select = {
            let (read, write, catalog, env2) = (
                state.draft.clone(),
                state.clone(),
                state.catalog.clone(),
                env.clone(),
            );
            let current = read.clone();
            select()
                .class("sb-input")
                .attr("id", "launch-model")
                .attr("name", "model")
                .on_input(move |v| {
                    write.draft.update(|d| d.model = v.to_string());
                    recalc(&write);
                })
                .child(View::dynamic(move || {
                    let mut rows = vec![
                        option()
                            .attr("value", "")
                            .text(env2.t.t("ui_vllm_form_select_model"))
                            .into_view(),
                    ];
                    let names: Vec<String> = catalog.with(|models| {
                        models
                            .iter()
                            .filter(|m| m.vllm_supported)
                            .map(|m| m.name.clone())
                            .collect()
                    });
                    for name in names {
                        let (current, chosen) = (current.clone(), name.clone());
                        rows.push(
                            option()
                                .attr("value", name.clone())
                                .attr_bool("selected", move || current.with(|d| d.model == chosen))
                                .text(name)
                                .into_view(),
                        );
                    }
                    View::Fragment(rows)
                }))
        };
        let quant_options: Vec<(String, String)> =
            std::iter::once(("".to_string(), "auto".to_string()))
                .chain(
                    LAUNCH_QUANTS
                        .iter()
                        .map(|q| ((*q).to_string(), (*q).to_string())),
                )
                .collect();
        let dtype_options: Vec<(String, String)> =
            std::iter::once(("".to_string(), "auto".to_string()))
                .chain(DTYPES.iter().map(|q| ((*q).to_string(), (*q).to_string())))
                .collect();
        let device_options: Vec<(String, String)> = DEVICES
            .iter()
            .map(|(v, l)| ((*v).to_string(), (*l).to_string()))
            .collect();
        let task_options: Vec<(String, String)> = TASKS
            .iter()
            .map(|(v, l)| ((*v).to_string(), (*l).to_string()))
            .collect();
        let gpu_util = {
            let (read, write, cpu) = (state.draft.clone(), state.clone(), state.draft.clone());
            input()
                .attr("id", "launch-gpu-util")
                .attr("name", "gpu_memory_utilization")
                .attr("type", "number")
                .attr("step", "0.05")
                .attr("min", "0.1")
                .attr("max", "1.0")
                .class("sb-input")
                .attr_fn("value", move || read.with(|d| d.gpu_util.clone()))
                .attr_bool("disabled", move || cpu.with(LaunchDraft::is_cpu))
                .on_input(move |v| write.draft.update(|d| d.gpu_util = v.to_string()))
        };
        let prefix = {
            let (on, flip) = (state.draft.clone(), state.draft.clone());
            check(
                "launch-prefix-caching",
                env.t.t("ui_vllm_form_prefix_caching"),
                move || on.with(|d| d.prefix_caching),
                move || flip.update(|d| d.prefix_caching = !d.prefix_caching),
            )
        };
        let tools = {
            let (on, flip) = (state.draft.clone(), state.draft.clone());
            check(
                "launch-enable-tool-calling",
                env.t.t("ui_vllm_form_tool_calling"),
                move || on.with(|d| d.tool_calling),
                move || flip.update(|d| d.tool_calling = !d.tool_calling),
            )
        };
        let error = {
            let error = state.launch_error.clone();
            let env = env.clone();
            View::dynamic(move || {
                error.get().map_or(View::Empty, |why| {
                    p().attr("id", "launch-error")
                        .class("sb-error")
                        .attr("role", "alert")
                        .text(reason(&env.t, &why))
                        .into_view()
                })
            })
        };
        let launch = {
            let (state, ask) = (state.clone(), env.ask.clone());
            let enabled = state.clone();
            button()
                .attr("type", "button")
                .attr("id", "confirm-launch-btn")
                .class("sb-btn sb-primary")
                .attr_bool("disabled", move || {
                    enabled.launching.get() || !launch_fit_now(&enabled).launch_enabled
                })
                .text(env.t.t("ui_vllm_launch_confirm"))
                .on_click(move || {
                    let body = state.draft.get_untracked().to_body();
                    match body {
                        Err(why) => state.launch_error.set(Some(why.message().to_string())),
                        Ok(body) => {
                            state.launch_error.set(None);
                            state.launching.set(true);
                            ask(Request::Launch(body));
                        }
                    }
                })
        };
        let form_body = form()
            .attr("id", "launch-form")
            .on_submit(|| {})
            .child(field(
                &env,
                "launch-model",
                "ui_vllm_form_model",
                model_select,
            ))
            .child(
                div()
                    .class("sb-row")
                    .child(field(
                        &env,
                        "launch-host",
                        "ui_vllm_form_host",
                        typed(
                            &state,
                            "launch-host",
                            "text",
                            |d| d.host.clone(),
                            |d, v| d.host = v,
                            None,
                        ),
                    ))
                    .child(field(
                        &env,
                        "launch-port",
                        "ui_vllm_form_port",
                        typed(
                            &state,
                            "launch-port",
                            "number",
                            |d| d.port.clone(),
                            |d, v| d.port = v,
                            None,
                        ),
                    ))
                    .child(if namespace_shown {
                        field(
                            &env,
                            "launch-namespace",
                            "ui_vllm_form_namespace",
                            typed(
                                &state,
                                "launch-namespace",
                                "text",
                                |d| d.namespace.clone(),
                                |d, v| d.namespace = v,
                                None,
                            ),
                        )
                    } else {
                        div()
                    }),
            )
            .child(
                div()
                    .class("sb-row")
                    .child(field(
                        &env,
                        "launch-quant",
                        "ui_vllm_form_quant",
                        picker(
                            &state,
                            "launch-quant",
                            quant_options,
                            |d| d.quantization.clone(),
                            |d, v| d.quantization = v,
                            Some(recalc),
                        ),
                    ))
                    .child(field(
                        &env,
                        "launch-dtype",
                        "ui_vllm_form_dtype",
                        picker(
                            &state,
                            "launch-dtype",
                            dtype_options,
                            |d| d.dtype.clone(),
                            |d, v| d.dtype = v,
                            None,
                        ),
                    ))
                    .child(field(
                        &env,
                        "launch-device",
                        "ui_vllm_form_device",
                        picker(
                            &state,
                            "launch-device",
                            device_options,
                            |d| d.device.clone(),
                            |d, v| d.device = v,
                            Some(recalc),
                        ),
                    ))
                    .child(field(
                        &env,
                        "launch-max-len",
                        "ui_vllm_form_max_len",
                        typed(
                            &state,
                            "launch-max-len",
                            "number",
                            |d| d.max_model_len.clone(),
                            |d, v| d.max_model_len = v,
                            Some(recalc),
                        )
                        .attr("placeholder", "auto"),
                    ))
                    .child(field(
                        &env,
                        "launch-limit-mm",
                        "ui_vllm_form_limit_mm",
                        typed(
                            &state,
                            "launch-limit-mm",
                            "text",
                            |d| d.limit_mm.clone(),
                            |d, v| d.limit_mm = v,
                            None,
                        )
                        .attr("placeholder", r#"{"image": 4}"#),
                    )),
            )
            .child(
                div()
                    .class("sb-row")
                    .child(field(
                        &env,
                        "launch-gpu-util",
                        "ui_vllm_form_gpu_util",
                        gpu_util,
                    ))
                    .child(div().class("sb-field sb-field-check").child(prefix)),
            )
            .child(
                div()
                    .class("sb-row")
                    .child(field(
                        &env,
                        "launch-task",
                        "ui_vllm_form_task",
                        picker(
                            &state,
                            "launch-task",
                            task_options,
                            |d| d.task.clone(),
                            |d, v| d.task = v,
                            None,
                        ),
                    ))
                    .child(div().class("sb-field sb-field-check").child(tools)),
            )
            .child(launch_note(&env, &state))
            .child(error);
        let body = div()
            .class("sb-modal-body")
            .child(
                p().class("sb-muted")
                    .text(env.t.t("ui_vllm_launch_modal_subtitle")),
            )
            .child(form_body)
            .child(
                div()
                    .class("sb-actions")
                    .child(
                        button()
                            .attr("type", "button")
                            .attr("id", "launch-cancel")
                            .class("sb-btn")
                            .text(env.t.t("ui_common_cancel"))
                            .on_click(move || cancel()),
                    )
                    .child(launch),
            );
        modal(
            "launch-modal",
            span().text(env.t.t("ui_vllm_launch_modal_title")),
            close,
            body,
            &env,
        )
        .into_view()
    })
}

/// The vLLM instances: the GPU, the Launch button, a card per instance, and the two windows over it.
pub(crate) fn vllm(env: &Env, state: &State) -> View {
    let (env, state) = (env.clone(), state.clone());
    follow(&env.clone(), move || {
        let launch_button = {
            let (env, state) = (env.clone(), state.clone());
            View::dynamic(move || {
                if !state
                    .caps
                    .with(|c| c.as_ref().is_some_and(|c| c.can_launch))
                {
                    return View::Empty;
                }
                let state = state.clone();
                button()
                    .attr("type", "button")
                    .attr("id", "launch-instance-action")
                    .class("sb-btn sb-primary toolbar-action")
                    .text(format!("+ {}", env.t.t("ui_vllm_launch_new")))
                    .on_click(move || {
                        let namespace = state
                            .caps
                            .with_untracked(|c| c.as_ref().map(|c| c.namespace.clone()))
                            .unwrap_or_default();
                        state.draft.set(LaunchDraft::new(&namespace));
                        state.launch_error.set(None);
                        state.launching.set(false);
                        state.launch_open.set(true);
                    })
                    .into_view()
            })
        };
        let grid = {
            let (env, state) = (env.clone(), state.clone());
            View::dynamic(move || {
                let Some(count) = state.instances.with(|i| i.as_ref().map(Vec::len)) else {
                    return waiting(&env);
                };
                if count == 0 {
                    return div()
                        .attr("id", "vllm-instances-grid")
                        .class("vllm-instances-grid sb-grid")
                        .child(
                            p().attr("id", "no-instances")
                                .class("fui-empty")
                                .text(env.t.t("ui_vllm_no_instances")),
                        )
                        .into_view();
                }
                let can_stop = state.caps.with(|c| c.as_ref().is_some_and(|c| c.can_stop));
                let items = state.instances.clone();
                let (env, state) = (env.clone(), state.clone());
                div()
                    .attr("id", "vllm-instances-grid")
                    .class("vllm-instances-grid sb-grid")
                    .child(View::keyed(
                        move || items.with(|i| i.clone().unwrap_or_default()),
                        move |i: &Instance| instance_key(i, can_stop),
                        move |i| instance_card(&env, &state, &i, can_stop),
                    ))
                    .into_view()
            })
        };
        div()
            .class("sb-page")
            .child(
                div()
                    .class("sb-toolbar")
                    .child(gpu_box(&env, &state))
                    .child(div().class("sb-grow"))
                    .child(launch_button),
            )
            .child(grid)
            .child(launch_modal(&env, &state))
            .child(stop_modal(&env, &state))
            .into_view()
    })
}
