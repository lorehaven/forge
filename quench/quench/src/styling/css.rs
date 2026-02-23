pub struct CssRule {
    selector: String,
    properties: Vec<(String, String)>,
    children: Vec<CssRule>,
}

impl CssRule {
    pub fn new(selector: &str) -> Self {
        Self {
            selector: selector.to_string(),
            properties: Vec::new(),
            children: Vec::new(),
        }
    }

    pub fn property(mut self, name: &str, value: &str) -> Self {
        self.properties.push((name.to_string(), value.to_string()));
        self
    }

    pub fn child(mut self, rule: CssRule) -> Self {
        self.children.push(rule);
        self
    }

    pub fn render(&self) -> String {
        self.render_internal(0)
    }

    fn render_internal(&self, indent: usize) -> String {
        let indent_str = "    ".repeat(indent);
        let inner_indent = "    ".repeat(indent + 1);

        let mut css = String::new();

        css.push_str(&format!("{}{} {{\n", indent_str, self.selector));

        for (name, value) in &self.properties {
            css.push_str(&format!("{}{}: {};\n", inner_indent, name, value));
        }

        for child in &self.children {
            css.push_str(&child.render_internal(indent + 1));
        }

        css.push_str(&format!("{}}}\n", indent_str));
        css
    }
}
