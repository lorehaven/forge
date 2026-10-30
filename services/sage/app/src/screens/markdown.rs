//! The markdown subset of an answer drawn as elements (see `sage_core::markdown`): text stays text.

use quench_ui::prelude::*;
use sage_core::markdown::{Block, Inline, parse};

use super::Env;

fn inlines(runs: &[Inline]) -> Vec<View> {
    runs.iter().map(inline).collect()
}

fn inline(run: &Inline) -> View {
    match run {
        Inline::Text(t) => View::text(t.clone()),
        Inline::Code(c) => element("code")
            .class("sg-inline-code")
            .text(c.clone())
            .into_view(),
        Inline::Strong(inner) => element("strong").children(inlines(inner)).into_view(),
        Inline::Em(inner) => element("em").children(inlines(inner)).into_view(),
        Inline::Link { href, text } => a()
            .attr("href", href.clone())
            .attr("target", "_blank")
            .attr("rel", "noopener noreferrer")
            .children(inlines(text))
            .into_view(),
    }
}

fn block(env: &Env, prefix: &str, index: usize, b: &Block) -> View {
    match b {
        Block::Paragraph(runs) => p().children(inlines(runs)).into_view(),
        Block::Heading(level, runs) => {
            let tag = format!("h{}", (*level).clamp(1, 6));
            element(&tag).children(inlines(runs)).into_view()
        }
        Block::List { ordered, items } => {
            let mut list = element(if *ordered { "ol" } else { "ul" });
            for item in items {
                list = list.child(li().children(inlines(item)));
            }
            list.into_view()
        }
        Block::Table { head, rows } => {
            let mut header = element("tr");
            for cell in head {
                header = header.child(element("th").children(inlines(cell)));
            }
            let mut body = element("tbody");
            for row in rows {
                let mut tr = element("tr");
                for cell in row {
                    tr = tr.child(element("td").children(inlines(cell)));
                }
                body = body.child(tr);
            }
            div()
                .class("sg-table-wrap")
                .child(
                    element("table")
                        .child(element("thead").child(header))
                        .child(body),
                )
                .into_view()
        }
        Block::Rule => element("hr").into_view(),
        Block::Code { lang, code } => {
            let id = format!("code-{prefix}-{index}");
            div()
                .class("sg-code")
                .child(
                    div()
                        .class("sg-code-head")
                        .child(span().class("sg-code-lang").text(lang.clone()))
                        .child(
                            button()
                                .attr("type", "button")
                                .class("sg-copy")
                                .attr("data-copy-target", id.clone())
                                .text(env.t.t("ui_code_copy")),
                        ),
                )
                .child(pre().child(element("code").attr("id", id).text(code.clone())))
                .into_view()
        }
        Block::Tool {
            name,
            error,
            paragraphs,
        } => {
            let mut body = div().class("sg-tool-body");
            for paragraph in paragraphs {
                body = body.child(p().text(paragraph.clone()));
            }
            div()
                .class(if *error {
                    "sg-tool sg-tool-error"
                } else {
                    "sg-tool"
                })
                .child(div().class("sg-tool-name").text(name.clone()))
                .child(body)
                .into_view()
        }
    }
}

/// The message text as blocks. `prefix` keeps the ids of its code blocks apart from other messages'.
pub(crate) fn render(env: &Env, prefix: &str, text: &str) -> Vec<View> {
    parse(text)
        .iter()
        .enumerate()
        .map(|(i, b)| block(env, prefix, i, b))
        .collect()
}
