use crate::Element;
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, RcDom};

#[derive(Clone, Debug, Default)]
pub struct Link {
    pub rel: String,
    pub href: String,
}

impl Link {
    pub fn new(rel: &str, href: &str) -> Self {
        Self {
            rel: rel.to_string(),
            href: href.to_string(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Script {
    pub src: String,
}

impl Script {
    pub fn new(src: &str) -> Self {
        Self {
            src: src.to_string(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PageBuilder {
    title: String,
    links: Vec<Link>,
    scripts: Vec<Script>,
    content: Option<Element>,
}

impl PageBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn title(mut self, value: impl Into<String>) -> Self {
        self.title = value.into();
        self
    }

    pub fn links(mut self, value: Vec<Link>) -> Self {
        self.links = value;
        self
    }

    pub fn scripts(mut self, value: Vec<Script>) -> Self {
        self.scripts = value;
        self
    }

    pub fn content(mut self, value: Element) -> Self {
        self.content = Some(value);
        self
    }

    pub fn build(self) -> String {
        let links = self
            .links
            .into_iter()
            .map(|link| format!("<link rel=\"{}\" href=\"{}\"></link>", link.rel, link.href))
            .collect::<Vec<_>>()
            .join("\n");
        let scripts = self
            .scripts
            .into_iter()
            .map(|script| format!("<script src=\"{}\" defer></script>", script.src))
            .collect::<Vec<_>>()
            .join("\n");

        let html_string = format!(
            r#"<!DOCTYPE html>
<html>
    <head>
        <meta charset="utf-8">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <link rel="icon" href="assets/favicon.ico"></link>
        {links}
        {scripts}
        <title>{}</title>
    </head>
    <body>{}</body>
</html>"#,
            self.title,
            self.content.unwrap().render()
        );

        pretty_print_html(&html_string)
    }
}

fn pretty_html_string(node: &Handle, indent: usize) -> String {
    match &node.data {
        markup5ever_rcdom::NodeData::Document => node
            .children
            .borrow()
            .iter()
            .map(|child| pretty_html_string(child, indent))
            .collect(),
        markup5ever_rcdom::NodeData::Text { contents } => {
            let contents_ref = contents.borrow();
            let text = contents_ref.trim();
            if text.is_empty() {
                "".to_string()
            } else {
                format!("{}{}\n", " ".repeat(indent), text)
            }
        }
        markup5ever_rcdom::NodeData::Element { name, attrs, .. } => {
            let attrs_string: String = attrs
                .borrow()
                .iter()
                .map(|attr| format!(" {}=\"{}\"", attr.name.local, attr.value))
                .collect();

            let mut s = format!("{}<{}{}>\n", " ".repeat(indent), name.local, attrs_string);

            // Recurse into children
            for child in node.children.borrow().iter() {
                s.push_str(&pretty_html_string(child, indent + 4));
            }

            s.push_str(&format!("{}{}</{}>\n", " ".repeat(indent), "", name.local));
            s
        }
        _ => "".to_string(),
    }
}

pub fn pretty_print_html(html_string: &str) -> String {
    let dom: RcDom = parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut html_string.as_bytes())
        .unwrap();

    pretty_html_string(&dom.document, 0)
}
