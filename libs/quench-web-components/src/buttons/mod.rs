use quench_web::{Element, button};

#[derive(Clone, Copy, Debug)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Danger,
    Success,
    Warning,
    Outline,
}

impl ButtonVariant {
    pub fn class_name(&self) -> &'static str {
        match self {
            ButtonVariant::Primary => "btn btn-primary",
            ButtonVariant::Secondary => "btn btn-secondary",
            ButtonVariant::Danger => "btn btn-danger",
            ButtonVariant::Success => "btn btn-success",
            ButtonVariant::Warning => "btn btn-warning",
            ButtonVariant::Outline => "btn btn-outline",
        }
    }
}

/// Builder for styled button elements with various variants
#[derive(Clone, Debug, Default)]
pub struct ButtonBuilder {
    text: String,
    variant: Option<ButtonVariant>,
    disabled: bool,
    button_type: String,
    id: Option<String>,
}

impl ButtonBuilder {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            button_type: "button".to_string(),
            ..Default::default()
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = Some(variant);
        self
    }

    pub fn primary(mut self) -> Self {
        self.variant = Some(ButtonVariant::Primary);
        self
    }

    pub fn secondary(mut self) -> Self {
        self.variant = Some(ButtonVariant::Secondary);
        self
    }

    pub fn danger(mut self) -> Self {
        self.variant = Some(ButtonVariant::Danger);
        self
    }

    pub fn success(mut self) -> Self {
        self.variant = Some(ButtonVariant::Success);
        self
    }

    pub fn warning(mut self) -> Self {
        self.variant = Some(ButtonVariant::Warning);
        self
    }

    pub fn outline(mut self) -> Self {
        self.variant = Some(ButtonVariant::Outline);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn button_type(mut self, button_type: impl Into<String>) -> Self {
        self.button_type = button_type.into();
        self
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn build(self) -> Element {
        let mut btn = button().attr("type", &self.button_type).text(&self.text);

        if let Some(variant) = self.variant {
            btn = btn.class(variant.class_name());
        } else {
            btn = btn.class("btn");
        }

        if self.disabled {
            btn = btn.attr("disabled", "disabled");
        }

        if let Some(id) = self.id {
            btn = btn.attr("id", &id);
        }

        btn
    }
}

/// Helper functions for common button styles
pub fn primary_button(text: impl Into<String>) -> Element {
    ButtonBuilder::new(text).primary().build()
}

pub fn secondary_button(text: impl Into<String>) -> Element {
    ButtonBuilder::new(text).secondary().build()
}

pub fn danger_button(text: impl Into<String>) -> Element {
    ButtonBuilder::new(text).danger().build()
}

pub fn success_button(text: impl Into<String>) -> Element {
    ButtonBuilder::new(text).success().build()
}

pub fn warning_button(text: impl Into<String>) -> Element {
    ButtonBuilder::new(text).warning().build()
}

pub fn outline_button(text: impl Into<String>) -> Element {
    ButtonBuilder::new(text).outline().build()
}
