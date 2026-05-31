use super::i18n::available_locales;
use super::scripts::{locale_script, theme_script};
use crate::{Element, Link, PageBuilder, Script, Theme, div};
use strum::IntoEnumIterator;

const FONTAWESOME_CSS: &str =
    "https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.7.2/css/all.min.css";

const HTMX_SCRIPT: &str = "https://cdn.jsdelivr.net/npm/htmx.org@2.0.10/dist/htmx.min.js";
const HTMX_SSE_SCRIPT: &str = "https://cdn.jsdelivr.net/npm/htmx-ext-sse@2.2.4/dist/sse.min.js";

#[derive(Clone, Debug)]
pub struct AppBuilder {
    pub(crate) title: String,
    pub(crate) links: Vec<Link>,
    pub(crate) scripts: Vec<Script>,
    pub(crate) supported_themes: Vec<Theme>,
    pub(crate) supported_locales: Vec<String>,
    pub(crate) default_theme: Theme,
    pub(crate) default_locale: Option<String>,
    pub(crate) header: Option<Element>,
    pub(crate) content: Option<Element>,
    pub(crate) footer: Option<Element>,
    pub(crate) resources_prefix: String,
}

impl AppBuilder {
    pub fn new() -> Self {
        Self {
            supported_themes: Theme::iter().collect(),
            default_theme: Theme::DefaultDark,
            ..Self::default()
        }
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

    pub fn supported_themes(mut self, value: Vec<Theme>) -> Self {
        self.supported_themes = value;
        self
    }

    pub fn supported_locales(mut self, value: Vec<String>) -> Self {
        self.supported_locales = value;
        self
    }

    pub fn default_theme(mut self, value: Theme) -> Self {
        self.default_theme = value;
        self
    }

    pub fn default_locale(mut self, value: impl Into<String>) -> Self {
        self.default_locale = Some(value.into());
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

    pub fn resources_prefix(mut self, value: String) -> Self {
        self.resources_prefix = value;
        self
    }

    pub fn build(self) -> String {
        let active_theme = self.default_theme.to_string();
        let supported_locales = if self.supported_locales.is_empty() {
            available_locales().unwrap_or_default()
        } else {
            self.supported_locales.clone()
        };
        let mut scripts = vec![
            Script::inline(
                "window.global = window.global || window; window.global.global = window.global;",
            ),
            Script::new(HTMX_SCRIPT).immediate(),
            Script::new(HTMX_SSE_SCRIPT).immediate(),
            Script::new(&format!(
                "{}/assets/js/translations.js",
                self.resources_prefix
            ))
            .immediate(),
            locale_script(&supported_locales, self.default_locale.as_deref()),
            theme_script(
                &active_theme,
                &self.supported_themes,
                &self.resources_prefix,
            ),
        ];
        scripts.extend(self.scripts);

        let mut links = vec![
            Link::new(
                "icon",
                &format!("{}/assets/favicon.png", self.resources_prefix),
            ),
            Link::new("stylesheet", FONTAWESOME_CSS),
            Link::new(
                "stylesheet",
                &format!("{}/assets/css/style.css", self.resources_prefix),
            ),
            Link::new(
                "stylesheet",
                &format!(
                    "{}/assets/css/themes/{active_theme}.css",
                    self.resources_prefix
                ),
            )
            .attr("id", "theme-link"),
        ];
        self.supported_themes.iter().for_each(|theme| {
            let theme = theme.to_string();
            if theme == active_theme {
                return;
            }
            links.push(
                Link::new(
                    "preload",
                    &format!("{}/assets/css/themes/{theme}.css", self.resources_prefix),
                )
                .attr("as", "style"),
            )
        });
        links.extend(self.links);

        let app = div()
            .class("app")
            .class("q-shell-app")
            .child_opt(self.header)
            .child(
                div().class("content").class("q-shell-content").child(
                    div()
                        .class("content-inner")
                        .class("q-shell-content-inner")
                        .child_opt(self.content),
                ),
            )
            .child_opt(self.footer);

        PageBuilder::new()
            .title(self.title)
            .links(links)
            .scripts(scripts)
            .content(app)
            .build()
    }
}

impl Default for AppBuilder {
    fn default() -> Self {
        Self {
            title: String::new(),
            links: Vec::new(),
            scripts: Vec::new(),
            supported_themes: Vec::new(),
            supported_locales: Vec::new(),
            default_theme: Theme::DefaultDark,
            default_locale: None,
            header: None,
            content: None,
            footer: None,
            resources_prefix: String::new(),
        }
    }
}
