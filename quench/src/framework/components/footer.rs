use crate::{Element, footer, p};

#[derive(Clone, Debug, Default)]
pub struct FooterBuilder {
    label: String,
}

impl FooterBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn label(mut self, value: impl Into<String>) -> Self {
        self.label = value.into();
        self
    }

    pub fn build(self) -> Element {
        footer()
            .class("footer")
            .child(p().attr("data-i18n", &self.label))
    }
}
