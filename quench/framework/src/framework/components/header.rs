use crate::{Element, div, header, nav_button, h2};

#[derive(Clone, Debug, Default)]
pub struct HeaderBuilder {
    label: String,
    with_nav: bool,
    nav_panel: Option<Element>,
}

impl HeaderBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn label(mut self, value: impl Into<String>) -> Self {
        self.label = value.into();
        self
    }

    pub fn with_nav(mut self, value: Element) -> Self {
        self.with_nav = true;
        self.nav_panel = Some(value);
        self
    }

    pub fn build(self) -> Element {
        header().child(
            div()
                .class("left-panel")
                .child_opt(self.with_nav.then(nav_button))
                .child(h2().attr("data-i18n", &self.label))
                .child_opt(self.nav_panel),
        )
    }
}
