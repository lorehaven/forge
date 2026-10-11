//! Untitled UI styled building blocks. Every helper returns a plain `Element`, so a page can keep
//! chaining `.class(..)`, `.attr(..)` and `.child(..)` on the result.
//!
//! Visible text goes through `data-i18n` keys (`t`) unless it is data (`txt`).

use quench_web::prelude::*;

/// Translatable text node: `<span data-i18n="key">`.
pub fn t(key: &str) -> Element {
    span().attr("data-i18n", key)
}

/// Literal (already localized or dynamic) text.
pub fn txt(text: impl ToString) -> Element {
    span().text(text)
}

/// A Font Awesome solid icon by name, e.g. `icon("plus")`.
pub fn icon(name: &str) -> Element {
    i().class("fa-solid").class(format!("fa-{name}"))
}

// ---------------------------------------------------------------- buttons

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    Primary,
    Secondary,
    Tertiary,
    Link,
    Danger,
    /// Destructive but not primary: outlined red.
    DangerSoft,
}

impl Variant {
    fn class(self) -> &'static str {
        match self {
            Self::Primary => "fui-btn-primary",
            Self::Secondary => "fui-btn-secondary",
            Self::Tertiary => "fui-btn-tertiary",
            Self::Link => "fui-btn-link",
            Self::Danger => "fui-btn-danger",
            Self::DangerSoft => "fui-btn-danger-soft",
        }
    }
}

/// `<button type="button">` with a translated label.
pub fn button_el(variant: Variant, label_key: &str) -> Element {
    button()
        .attr("type", "button")
        .class("fui-btn")
        .class(variant.class())
        .child(t(label_key))
}

/// Submit button (`<button type="submit">`).
pub fn submit_button(variant: Variant, label_key: &str) -> Element {
    button_el(variant, label_key).attr("type", "submit")
}

/// A link that looks like a button.
pub fn link_button(variant: Variant, href: &str, label_key: &str) -> Element {
    a().attr("href", href)
        .class("fui-btn")
        .class(variant.class())
        .child(t(label_key))
}

/// Square icon-only button. `label_key` is the screen-reader / tooltip text.
pub fn icon_button(icon_name: &str, label_key: &str) -> Element {
    button()
        .attr("type", "button")
        .class("fui-icon-btn")
        .attr("data-i18n-title", label_key)
        .child(icon(icon_name))
        .child(t(label_key).class("sr-only"))
}

/// Icon-only link.
pub fn icon_link(icon_name: &str, href: &str, label_key: &str) -> Element {
    a().attr("href", href)
        .class("fui-icon-btn")
        .attr("data-i18n-title", label_key)
        .child(icon(icon_name))
        .child(t(label_key).class("sr-only"))
}

// ----------------------------------------------------------------- badges

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Gray,
    Brand,
    Success,
    Warning,
    Error,
    Info,
}

impl Tone {
    fn badge_class(self) -> &'static str {
        match self {
            Self::Gray => "",
            Self::Brand => "fui-badge-brand",
            Self::Success => "fui-badge-success",
            Self::Warning => "fui-badge-warning",
            Self::Error => "fui-badge-error",
            Self::Info => "fui-badge-info",
        }
    }

    fn alert_class(self) -> &'static str {
        match self {
            Self::Gray | Self::Brand => "",
            Self::Success => "fui-alert-success",
            Self::Warning => "fui-alert-warning",
            Self::Error => "fui-alert-error",
            Self::Info => "fui-alert-info",
        }
    }

    fn alert_icon(self) -> &'static str {
        match self {
            Self::Success => "circle-check",
            Self::Warning => "triangle-exclamation",
            Self::Error => "circle-exclamation",
            _ => "circle-info",
        }
    }
}

/// Pill with a literal (data) label.
pub fn badge(tone: Tone, text: impl ToString) -> Element {
    span()
        .class("fui-badge")
        .class(tone.badge_class())
        .text(text)
}

/// Pill with a status dot in front.
pub fn status_badge(tone: Tone, text: impl ToString) -> Element {
    badge_of(tone, txt(text))
}

/// Pill with a translated label and a status dot.
pub fn status_badge_key(tone: Tone, key: &str) -> Element {
    badge_of(tone, t(key))
}

fn badge_of(tone: Tone, label: Element) -> Element {
    span()
        .class("fui-badge")
        .class(tone.badge_class())
        .child(span().class("fui-dot"))
        .child(label)
}

// ------------------------------------------------------------------ cards

/// Bordered surface. `.title_key(..)` / `.action(..)` add a header row.
#[derive(Default)]
pub struct Card {
    title_key: Option<String>,
    description_key: Option<String>,
    actions: Vec<Element>,
    body: Vec<Element>,
    footer: Option<Element>,
    flush: bool,
}

impl Card {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn title_key(mut self, key: &str) -> Self {
        self.title_key = Some(key.to_string());
        self
    }

    pub fn description_key(mut self, key: &str) -> Self {
        self.description_key = Some(key.to_string());
        self
    }

    pub fn action(mut self, el: Element) -> Self {
        self.actions.push(el);
        self
    }

    pub fn body(mut self, el: Element) -> Self {
        self.body.push(el);
        self
    }

    pub fn footer(mut self, el: Element) -> Self {
        self.footer = Some(el);
        self
    }

    /// No body padding - for tables and lists that run edge to edge.
    pub fn flush(mut self) -> Self {
        self.flush = true;
        self
    }

    pub fn build(self) -> Element {
        let mut card = div().class("fui-card");
        if self.title_key.is_some() || !self.actions.is_empty() {
            let mut text = div().class("fui-stack fui-stack-sm");
            if let Some(key) = &self.title_key {
                text = text.child(h3().class("fui-card-title").attr("data-i18n", key));
            }
            if let Some(key) = &self.description_key {
                text = text.child(div().class("fui-card-desc").attr("data-i18n", key));
            }
            let mut actions = div().class("fui-row");
            for action in self.actions {
                actions = actions.child(action);
            }
            card = card.child(div().class("fui-card-header").child(text).child(actions));
        }
        let mut body = div().class("fui-card-body");
        if self.flush {
            body = body.class("fui-flush");
        }
        for el in self.body {
            body = body.child(el);
        }
        card = card.child(body);
        card.child_opt(self.footer.map(|f| div().class("fui-card-footer").child(f)))
    }
}

/// Whole-card link with an icon tile, title, description and chevron - the home page tile.
pub fn link_card(
    icon_name: &str,
    href: &str,
    title: Element,
    description: Option<Element>,
) -> Element {
    a().attr("href", href)
        .class("fui-link-card")
        .child(div().class("fui-link-card-icon").child(icon(icon_name)))
        .child(
            div()
                .class("fui-link-card-body")
                .child(div().class("fui-link-card-title").child(title))
                .child_opt(description.map(|d| div().class("fui-link-card-desc").child(d))),
        )
        .child(icon("chevron-right").class("fui-link-card-arrow"))
}

/// Number with a caption.
pub fn stat(label: Element, value: Element) -> Element {
    div()
        .class("fui-stat")
        .child(div().class("fui-stat-label").child(label))
        .child(div().class("fui-stat-value").child(value))
}

// ------------------------------------------------------------ page header

#[derive(Default)]
pub struct PageHeader {
    title: Option<Element>,
    subtitle: Option<Element>,
    breadcrumbs: Vec<(Element, Option<String>)>,
    actions: Vec<Element>,
}

impl PageHeader {
    pub fn new(title_key: &str) -> Self {
        Self {
            title: Some(t(title_key)),
            ..Default::default()
        }
    }

    /// Dynamic title (a project name, a repository).
    pub fn titled(title: impl ToString) -> Self {
        Self {
            title: Some(txt(title)),
            ..Default::default()
        }
    }

    pub fn subtitle_key(mut self, key: &str) -> Self {
        self.subtitle = Some(t(key));
        self
    }

    pub fn subtitle(mut self, text: impl ToString) -> Self {
        self.subtitle = Some(txt(text));
        self
    }

    /// A crumb; `href: None` marks the current page.
    pub fn crumb(mut self, label: Element, href: Option<&str>) -> Self {
        self.breadcrumbs.push((label, href.map(str::to_string)));
        self
    }

    pub fn action(mut self, el: Element) -> Self {
        self.actions.push(el);
        self
    }

    pub fn build(self) -> Element {
        let mut text = div().class("fui-page-header-text");
        if !self.breadcrumbs.is_empty() {
            let mut crumbs = div().class("fui-breadcrumbs");
            let last = self.breadcrumbs.len() - 1;
            for (idx, (label, href)) in self.breadcrumbs.into_iter().enumerate() {
                crumbs = crumbs.child_opt(match href {
                    Some(href) => Some(a().attr("href", href).child(label)),
                    None => Some(span().class("fui-crumb-current").child(label)),
                });
                if idx != last {
                    crumbs = crumbs.child(icon("chevron-right").attr("aria-hidden", "true"));
                }
            }
            text = text.child(crumbs);
        }
        text = text
            .child_opt(
                self.title
                    .map(|title| h1().class("fui-page-title").child(title)),
            )
            .child_opt(
                self.subtitle
                    .map(|s| p().class("fui-page-subtitle").child(s)),
            );

        let mut actions = div().class("fui-page-actions");
        for action in self.actions {
            actions = actions.child(action);
        }
        div().class("fui-page-header").child(text).child(actions)
    }
}

// ------------------------------------------------------------------ forms

pub fn form_el(method: &str, action: &str) -> Element {
    form()
        .attr("method", method)
        .attr("action", action)
        .class("fui-form")
}

/// Label + control + optional hint, stacked.
pub fn field(label_key: &str, control: Element, hint_key: Option<&str>) -> Element {
    div()
        .class("fui-field")
        .child(label().class("fui-label").attr("data-i18n", label_key))
        .child(control)
        .child_opt(hint_key.map(|k| div().class("fui-hint").attr("data-i18n", k)))
}

/// Like [`field`], but the label's `for` points at `control_id` (set the same id on the control).
pub fn field_for(
    control_id: &str,
    label_key: &str,
    control: Element,
    hint_key: Option<&str>,
) -> Element {
    div()
        .class("fui-field")
        .child(
            label()
                .class("fui-label")
                .attr("for", control_id)
                .attr("data-i18n", label_key),
        )
        .child(control)
        .child_opt(hint_key.map(|k| div().class("fui-hint").attr("data-i18n", k)))
}

pub fn text_input(name: &str, input_type: &str) -> Element {
    input()
        .attr("type", input_type)
        .attr("name", name)
        .attr("id", name)
        .class("fui-input")
}

pub fn select_el(name: &str) -> Element {
    select()
        .attr("name", name)
        .attr("id", name)
        .class("fui-select")
}

pub fn textarea_el(name: &str) -> Element {
    textarea()
        .attr("name", name)
        .attr("id", name)
        .class("fui-textarea")
}

/// Search box with a leading magnifier.
pub fn search_input(name: &str, placeholder_key: &str) -> Element {
    div()
        .class("fui-input-group")
        .child(icon("magnifying-glass"))
        .child(
            input()
                .attr("type", "search")
                .attr("name", name)
                .attr("data-i18n-placeholder", placeholder_key)
                .class("fui-input"),
        )
}

pub fn checkbox_field(name: &str, label_key: &str) -> Element {
    label()
        .class("fui-check")
        .child(input().attr("type", "checkbox").attr("name", name))
        .child(t(label_key))
}

// ------------------------------------------------------------------ tables

/// `<table>` in a horizontally scrolling wrapper. `header_keys` are i18n keys; rows are built by the caller
/// (`tr().child(td()...)`). Pass `stack: true` to collapse rows into label/value pairs on phones.
pub fn table_el(header_keys: &[&str], rows: Vec<Element>, stack: bool) -> Element {
    let mut head = element("tr");
    for key in header_keys {
        head = head.child(element("th").attr("data-i18n", key));
    }
    let mut body = element("tbody");
    for row in rows {
        body = body.child(row);
    }
    let mut table = element("table")
        .class("fui-table")
        .child(element("thead").child(head))
        .child(body);
    if stack {
        table = table.class("fui-table-stack");
    }
    div().class("fui-table-wrap").child(table)
}

pub fn tr_el() -> Element {
    element("tr")
}

pub fn td_el() -> Element {
    element("td")
}

/// `<dl>` of label/value pairs.
pub fn key_values(pairs: Vec<(Element, Element)>) -> Element {
    let mut dl = element("dl").class("fui-kv");
    for (k, v) in pairs {
        dl = dl
            .child(element("dt").child(k))
            .child(element("dd").child(v));
    }
    dl
}

// ----------------------------------------------------------- feedback / empty

/// Inline message with a tone-matched icon.
pub fn alert(tone: Tone, body: Element) -> Element {
    div()
        .class("fui-alert")
        .class(tone.alert_class())
        .attr(
            "role",
            if tone == Tone::Error {
                "alert"
            } else {
                "status"
            },
        )
        .child(icon(tone.alert_icon()))
        .child(div().class("fui-alert-body").child(body))
}

pub fn empty_state(icon_name: &str, title_key: &str, description_key: Option<&str>) -> Element {
    div()
        .class("fui-empty")
        .child(div().class("fui-empty-icon").child(icon(icon_name)))
        .child(div().class("fui-empty-title").attr("data-i18n", title_key))
        .child_opt(description_key.map(|k| div().attr("data-i18n", k)))
}

pub fn progress(percent: u8) -> Element {
    div()
        .class("fui-progress")
        .attr("role", "progressbar")
        .attr("aria-valuenow", percent.min(100))
        .child(span().attr("style", format!("width:{}%", percent.min(100))))
}

pub fn code_block(text: impl ToString) -> Element {
    pre().class("fui-code-block").text(text)
}

// ------------------------------------------------------------------ tabs

/// Link tabs: each `(label_key, href, active)`.
pub fn tabs(items: &[(&str, String, bool)]) -> Element {
    let mut bar = div().class("fui-tabs").attr("role", "tablist");
    for (key, href, active) in items {
        let mut tab = a().attr("href", href).class("fui-tab").child(t(key));
        if *active {
            tab = tab.attr("aria-current", "page");
        }
        bar = bar.child(tab);
    }
    bar
}

// ------------------------------------------------------------------ modal

/// Dialog that is hidden until `open_modal(id)` runs. Slides up as a sheet on phones.
pub fn modal(id: &str, title_key: &str, body: Element, footer: Option<Element>) -> Element {
    div()
        .class("fui-modal")
        .attr("id", id)
        .attr("role", "dialog")
        .attr("aria-modal", "true")
        .child(div().class("fui-modal-scrim").on_click(close_modal(id)))
        .child(
            div()
                .class("fui-modal-panel")
                .child(
                    div()
                        .class("fui-modal-header")
                        .child(h3().attr("data-i18n", title_key))
                        .child(icon_button("xmark", "ui_close").on_click(close_modal(id))),
                )
                .child(div().class("fui-modal-body").child(body))
                .child_opt(footer.map(|f| div().class("fui-modal-footer").child(f))),
        )
}

/// `onclick` code that opens the modal with this id.
pub fn open_modal(id: &str) -> String {
    format!("fuiOpenModal('{id}')")
}

/// `onclick` code that closes the modal with this id.
pub fn close_modal(id: &str) -> String {
    format!("fuiCloseModal('{id}')")
}
