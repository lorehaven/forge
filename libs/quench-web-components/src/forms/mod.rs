use quench_web::{Element, div, input, label, option, select, textarea};

/// Builder for a form input field with label and optional validation helpers
#[derive(Clone, Debug, Default)]
pub struct FormInput {
    name: String,
    label: Option<String>,
    input_type: String,
    placeholder: Option<String>,
    required: bool,
    value: Option<String>,
}

impl FormInput {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            input_type: "text".to_string(),
            ..Default::default()
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn input_type(mut self, input_type: impl Into<String>) -> Self {
        self.input_type = input_type.into();
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    pub fn build(self) -> Element {
        let mut input_elem = input()
            .attr("type", &self.input_type)
            .attr("name", &self.name)
            .attr("id", &self.name);

        if let Some(placeholder) = self.placeholder {
            input_elem = input_elem.attr("placeholder", &placeholder);
        }

        if let Some(value) = self.value {
            input_elem = input_elem.attr("value", &value);
        }

        if self.required {
            input_elem = input_elem.attr("required", "required");
        }

        if let Some(label_text) = self.label {
            div()
                .class("form-group")
                .child(label().attr("for", &self.name).text(label_text))
                .child(input_elem)
        } else {
            input_elem
        }
    }
}

/// Builder for a form select field with options
#[derive(Clone, Debug, Default)]
pub struct FormSelect {
    name: String,
    label: Option<String>,
    options: Vec<(String, String)>,
    selected: Option<String>,
    required: bool,
}

impl FormSelect {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn option(mut self, value: impl Into<String>, label: impl Into<String>) -> Self {
        self.options.push((value.into(), label.into()));
        self
    }

    pub fn options(mut self, options: Vec<(String, String)>) -> Self {
        self.options = options;
        self
    }

    pub fn selected(mut self, value: impl Into<String>) -> Self {
        self.selected = Some(value.into());
        self
    }

    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    pub fn build(self) -> Element {
        let mut select_elem = select().attr("name", &self.name).attr("id", &self.name);

        if self.required {
            select_elem = select_elem.attr("required", "required");
        }

        for (value, label) in self.options {
            let mut opt = option().attr("value", &value).text(&label);

            if let Some(ref selected) = self.selected
                && selected == &value
            {
                opt = opt.attr("selected", "selected");
            }

            select_elem = select_elem.clone().child(opt);
        }

        if let Some(label_text) = self.label {
            div()
                .class("form-group")
                .child(label().attr("for", &self.name).text(label_text))
                .child(select_elem)
        } else {
            select_elem
        }
    }
}

/// Builder for a textarea form field
#[derive(Clone, Debug, Default)]
pub struct FormTextarea {
    name: String,
    label: Option<String>,
    placeholder: Option<String>,
    rows: Option<usize>,
    cols: Option<usize>,
    required: bool,
    value: Option<String>,
}

impl FormTextarea {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn rows(mut self, rows: usize) -> Self {
        self.rows = Some(rows);
        self
    }

    pub fn cols(mut self, cols: usize) -> Self {
        self.cols = Some(cols);
        self
    }

    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    pub fn build(self) -> Element {
        let mut textarea_elem = textarea().attr("name", &self.name).attr("id", &self.name);

        if let Some(rows) = self.rows {
            textarea_elem = textarea_elem.attr("rows", rows.to_string());
        }

        if let Some(cols) = self.cols {
            textarea_elem = textarea_elem.attr("cols", cols.to_string());
        }

        if let Some(placeholder) = self.placeholder {
            textarea_elem = textarea_elem.attr("placeholder", &placeholder);
        }

        if self.required {
            textarea_elem = textarea_elem.attr("required", "required");
        }

        if let Some(value) = self.value {
            textarea_elem = textarea_elem.text(&value);
        }

        if let Some(label_text) = self.label {
            div()
                .class("form-group")
                .child(label().attr("for", &self.name).text(label_text))
                .child(textarea_elem)
        } else {
            textarea_elem
        }
    }
}
