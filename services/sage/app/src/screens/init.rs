//! The screen that waits for the models: one row per default model, read again until every one runs.

use quench_ui::prelude::*;

use super::Env;
use crate::live::State;

fn presentation(state: &str) -> (&'static str, &'static str) {
    match state {
        "running" => ("running", "ui_init_status_running"),
        "starting" => ("starting", "ui_init_status_starting"),
        "failed" => ("failed", "ui_init_status_failed"),
        "unknown" => ("unknown", "ui_init_status_unknown"),
        _ => ("pending", "ui_init_status_queued"),
    }
}

pub(crate) fn initializing(env: &Env, state: &State) -> View {
    let (env, models) = (env.clone(), state.models.clone());
    let rows = {
        let (env, models) = (env.clone(), models.clone());
        View::dynamic(move || {
            let mut rows = div().attr("id", "model-rows").class("sg-model-rows");
            if let Some(view) = models.get() {
                for model in &view.defaults {
                    let (modifier, key) = presentation(&model.state);
                    let mut name = span().text(model.name.clone());
                    if model.embedding {
                        name = span()
                            .child(name)
                            .child(span().text(format!(" {}", env.t.t("ui_init_embedding_tag"))));
                    }
                    rows = rows.child(
                        div()
                            .class(format!("sg-model-row sg-model-{modifier}"))
                            .attr("data-state", modifier)
                            .child(div().class("sg-model-name").child(name))
                            .child(
                                div()
                                    .class(format!("sg-model-status sg-model-status-{modifier}"))
                                    .text(env.t.t(key)),
                            ),
                    );
                }
            }
            rows.into_view()
        })
    };
    let waiting = {
        let (env, models) = (env.clone(), models);
        View::dynamic(move || {
            // No answer from switchboard: the rows cannot say more than "connecting".
            if models.with(|m| m.as_ref().is_some_and(|m| !m.switchboard_ok)) {
                p().attr("id", "init-waiting")
                    .class("fui-notice fui-notice-error")
                    .text(env.t.t("ui_init_waiting"))
                    .into_view()
            } else {
                View::Empty
            }
        })
    };
    div()
        .class("sg-page")
        .child(
            div()
                .attr("id", "init-card")
                .class("sg-init-card")
                .child(h2().class("sg-init-title").text(env.t.t("ui_init_title")))
                .child(
                    div()
                        .class("sg-init-subtitle")
                        .text(env.t.t("ui_init_subtitle")),
                )
                .child(rows)
                .child(waiting),
        )
        .into_view()
}
