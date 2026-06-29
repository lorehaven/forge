use quench_web::{Element, div, i, span};

#[derive(Clone, Copy, Debug)]
pub enum StatusLevel {
    Info,
    Success,
    Warning,
    Error,
}

impl StatusLevel {
    pub fn class_name(&self) -> &'static str {
        match self {
            StatusLevel::Info => "status-info",
            StatusLevel::Success => "status-success",
            StatusLevel::Warning => "status-warning",
            StatusLevel::Error => "status-error",
        }
    }

    pub fn icon_class(&self) -> &'static str {
        match self {
            StatusLevel::Info => "fas fa-info-circle",
            StatusLevel::Success => "fas fa-check-circle",
            StatusLevel::Warning => "fas fa-exclamation-circle",
            StatusLevel::Error => "fas fa-times-circle",
        }
    }
}

/// Status badge for displaying status information
#[derive(Clone, Debug, Default)]
pub struct StatusBadge {
    text: String,
    level: Option<StatusLevel>,
    with_icon: bool,
}

impl StatusBadge {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Default::default()
        }
    }

    pub fn level(mut self, level: StatusLevel) -> Self {
        self.level = Some(level);
        self
    }

    pub fn info(mut self) -> Self {
        self.level = Some(StatusLevel::Info);
        self
    }

    pub fn success(mut self) -> Self {
        self.level = Some(StatusLevel::Success);
        self
    }

    pub fn warning(mut self) -> Self {
        self.level = Some(StatusLevel::Warning);
        self
    }

    pub fn error(mut self) -> Self {
        self.level = Some(StatusLevel::Error);
        self
    }

    pub fn with_icon(mut self, with_icon: bool) -> Self {
        self.with_icon = with_icon;
        self
    }

    pub fn build(self) -> Element {
        let mut badge = span().class("status-badge");

        if let Some(level) = self.level {
            badge = badge.class(level.class_name());

            if self.with_icon {
                badge = badge.child(i().class(level.icon_class()));
            }
        }

        badge.text(&self.text)
    }
}

/// Alert box for displaying messages with status levels
#[derive(Clone, Debug, Default)]
pub struct AlertBox {
    message: String,
    level: Option<StatusLevel>,
    closeable: bool,
}

impl AlertBox {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            ..Default::default()
        }
    }

    pub fn level(mut self, level: StatusLevel) -> Self {
        self.level = Some(level);
        self
    }

    pub fn info(mut self) -> Self {
        self.level = Some(StatusLevel::Info);
        self
    }

    pub fn success(mut self) -> Self {
        self.level = Some(StatusLevel::Success);
        self
    }

    pub fn warning(mut self) -> Self {
        self.level = Some(StatusLevel::Warning);
        self
    }

    pub fn error(mut self) -> Self {
        self.level = Some(StatusLevel::Error);
        self
    }

    pub fn closeable(mut self, closeable: bool) -> Self {
        self.closeable = closeable;
        self
    }

    pub fn build(self) -> Element {
        let mut alert = div().class("alert");

        if let Some(level) = self.level {
            alert = alert.class(format!("alert-{}", level.class_name()));
        }

        alert = alert.child(span().text(&self.message));

        if self.closeable {
            alert = alert.child(
                span()
                    .class("alert-close")
                    .attr("role", "button")
                    .attr("aria-label", "Close alert")
                    .child(i().class("fas").class("fa-times")),
            );
        }

        alert
    }
}

/// Helper functions for status displays
pub fn success_badge(text: impl Into<String>) -> Element {
    StatusBadge::new(text).success().with_icon(true).build()
}

pub fn warning_badge(text: impl Into<String>) -> Element {
    StatusBadge::new(text).warning().with_icon(true).build()
}

pub fn error_badge(text: impl Into<String>) -> Element {
    StatusBadge::new(text).error().with_icon(true).build()
}

pub fn info_alert(message: impl Into<String>) -> Element {
    AlertBox::new(message).info().build()
}

pub fn success_alert(message: impl Into<String>) -> Element {
    AlertBox::new(message).success().build()
}

pub fn warning_alert(message: impl Into<String>) -> Element {
    AlertBox::new(message).warning().build()
}

pub fn error_alert(message: impl Into<String>) -> Element {
    AlertBox::new(message).error().build()
}
