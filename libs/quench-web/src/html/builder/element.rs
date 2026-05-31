use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct Element {
    tag: String,
    attributes: HashMap<String, String>,
    children: Vec<Element>,
    text_content: Option<String>,
    onclick: Option<String>,
    onchange: Option<String>,
    raw: bool,
    defer: bool,
}

impl Element {
    pub(crate) fn new(tag: impl ToString) -> Self {
        Self {
            tag: tag.to_string(),
            attributes: HashMap::new(),
            children: Vec::new(),
            text_content: None,
            onclick: None,
            onchange: None,
            raw: false,
            defer: false,
        }
    }

    pub fn attr(mut self, key: impl ToString, value: impl ToString) -> Self {
        self.attributes.insert(key.to_string(), value.to_string());
        self
    }

    pub fn class(mut self, class: impl ToString) -> Self {
        let mut classes = self
            .attributes
            .get("class")
            .unwrap_or(&String::new())
            .to_string();
        if !classes.is_empty() {
            classes.push(' ');
        }
        classes.push_str(class.to_string().as_str());
        self.attributes.insert("class".to_string(), classes);
        self
    }

    pub fn text(mut self, text: impl ToString) -> Self {
        self.text_content = Some(text.to_string());
        self
    }

    pub fn child(mut self, child: Element) -> Self {
        self.children.push(child);
        self
    }

    pub fn child_opt(mut self, child: Option<Element>) -> Self {
        if let Some(child) = child {
            self.children.push(child);
        }
        self
    }

    pub fn on_click(mut self, js_code: impl ToString) -> Self {
        self.onclick = Some(js_code.to_string());
        self
    }

    pub fn on_change(mut self, js_code: impl ToString) -> Self {
        self.onchange = Some(js_code.to_string());
        self
    }

    pub fn raw(mut self) -> Self {
        self.raw = true;
        self
    }

    pub fn defer(mut self) -> Self {
        self.defer = true;
        self
    }

    pub fn render(&self) -> String {
        let mut html = format!("<{}", self.tag);

        for (key, value) in &self.attributes {
            html.push_str(&format!(" {}=\"{}\"", key, html_escape(value)));
        }

        if let Some(onclick) = &self.onclick {
            html.push_str(&format!(" onclick=\"{}\"", html_escape(onclick)));
        }

        if let Some(onchange) = &self.onchange {
            html.push_str(&format!(" onchange=\"{}\"", html_escape(onchange)));
        }

        if self.defer {
            html.push_str(" defer");
        }

        html.push('>');
        if let Some(text) = &self.text_content {
            if self.raw {
                // Insert as-is, no escaping
                html.push_str(text);
            } else {
                html.push_str(&html_escape(text));
            }
        }

        for child in &self.children {
            html.push_str(&child.render());
        }

        html.push_str(&format!("</{}>", self.tag));
        html
    }
}

// Helper function to escape HTML
pub(crate) fn html_escape(s: &str) -> String {
    s.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace("\"", "&quot;")
        .replace("'", "&#39;")
}
