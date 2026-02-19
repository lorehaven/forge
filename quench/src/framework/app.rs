use crate::js::locale::locale_js;
use crate::js::theme::theme_js;
use crate::{Element, Link, PageBuilder, Script, Theme, div, theme_shared};
use strum::IntoEnumIterator;

const FONTAWESOME_CSS: &str =
    "https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.7.2/css/all.min.css";

#[derive(Clone, Debug, Default)]
pub struct AppBuilder {
    title: String,
    links: Vec<Link>,
    scripts: Vec<Script>,
    header: Option<Element>,
    content: Option<Element>,
    footer: Option<Element>,
}

impl AppBuilder {
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

    pub fn header(mut self, value: Element) -> Self {
        self.header = Some(value);
        self
    }

    pub fn page_content(mut self, value: Element) -> Self {
        self.content = Some(value);
        self
    }

    pub fn footer(mut self, value: Element) -> Self {
        self.footer = Some(value);
        self
    }

    pub fn build(self) -> String {
        let mut links = vec![
            Link::new("stylesheet", FONTAWESOME_CSS),
            Link::new("stylesheet", "/assets/css/style.css"),
        ];
        Theme::iter().for_each(|theme| {
            let theme = theme.to_string();
            links.push(Link::new(
                "stylesheet",
                &format!("/assets/css/themes/{theme}.css"),
            ))
        });
        links.extend(self.links);

        let mut scripts = vec![
            Script::new("/assets/js/locale.js"),
            Script::new("/assets/js/theme.js"),
        ];
        scripts.extend(self.scripts);

        let app = div()
            .class("app")
            .child_opt(self.header)
            .child_opt(self.content)
            .child_opt(self.footer);

        PageBuilder::new()
            .title(self.title)
            .links(links)
            .scripts(scripts)
            .content(app)
            .build()
    }
}

pub fn create_asset_files(default_theme: Theme) {
    let _ = std::fs::create_dir_all("dist/assets/css/themes");
    let _ = std::fs::write("dist/assets/css/style.css", theme_shared());
    Theme::iter().for_each(|theme| {
        let theme_str = theme.to_string();
        let _ = std::fs::write(
            format!("dist/assets/css/themes/{theme_str}.css"),
            Theme::theme(theme),
        );
    });
    let _ = std::fs::write("dist/assets/css/themes/light.css", crate::light::theme());
    let _ = std::fs::write("dist/assets/css/themes/dark.css", crate::dark::theme());

    let _ = std::fs::create_dir_all("dist/assets/js");
    let _ = std::fs::write("dist/assets/js/locale.js", locale_js());
    let _ = std::fs::write(
        "dist/assets/js/theme.js",
        theme_js(&default_theme.to_string()),
    );
}
