use crate::Element;
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, RcDom};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub struct Link {
    pub rel: String,
    pub href: String,
    pub attrs: BTreeMap<String, String>,
}

impl Link {
    pub fn new(rel: &str, href: &str) -> Self {
        Self {
            rel: rel.to_string(),
            href: href.to_string(),
            attrs: BTreeMap::new(),
        }
    }

    pub fn attr(mut self, key: &str, value: &str) -> Self {
        self.attrs.insert(key.to_string(), value.to_string());
        self
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
    head_link_static_attrs: BTreeMap<String, String>,
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

    pub fn head_link_static_attr(mut self, key: &str, value: &str) -> Self {
        self.head_link_static_attrs
            .insert(key.to_string(), value.to_string());
        self
    }

    pub fn head_link_static_attrs(mut self, value: BTreeMap<String, String>) -> Self {
        self.head_link_static_attrs.extend(value);
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
        let mut head_links = vec![Link::new("icon", "assets/favicon.ico")];
        head_links.extend(self.links);
        let links = self.head_link_static_attrs;
        let links = head_links
            .into_iter()
            .map(|link| render_head_link(link, &links))
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

fn render_head_link(link: Link, static_attrs: &BTreeMap<String, String>) -> String {
    let mut attrs = static_attrs.clone();
    attrs.extend(link.attrs);
    attrs.insert("rel".to_string(), link.rel);
    attrs.insert("href".to_string(), link.href);

    let attrs = attrs
        .into_iter()
        .map(|(key, value)| format!("{key}=\"{value}\""))
        .collect::<Vec<_>>()
        .join(" ");
    format!("<link {attrs}></link>")
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
