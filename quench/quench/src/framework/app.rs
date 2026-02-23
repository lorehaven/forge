use crate::js::locale::{available_locales, locale_js_with_options, validate_locales_exist};
use crate::js::theme::theme_js_with_options;
use crate::{
    Element, FooterBuilder, HeaderBuilder, Link, NavPanelBuilder, PageBuilder, Script, Theme, div,
    theme_shared,
};
use anyhow::Result;
use strum::IntoEnumIterator;

const FONTAWESOME_CSS: &str =
    "https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.7.2/css/all.min.css";

#[derive(Clone, Debug, Default)]
pub struct AppBuilder {
    title: String,
    links: Vec<Link>,
    scripts: Vec<Script>,
    supported_themes: Vec<Theme>,
    header: Option<Element>,
    content: Option<Element>,
    footer: Option<Element>,
}

impl AppBuilder {
    pub fn new() -> Self {
        Self {
            supported_themes: Theme::iter().collect(),
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
        self.supported_themes.iter().for_each(|theme| {
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
            .child(
                div()
                    .class("content")
                    .child(div().class("content-inner").child_opt(self.content)),
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

pub fn create_asset_files(default_theme: Theme) {
    let supported_themes = Theme::iter().collect::<Vec<_>>();
    let supported_locales = available_locales().unwrap_or_default();
    create_asset_files_with_options(default_theme, &supported_themes, &supported_locales);
}

pub fn create_asset_files_with_options(
    default_theme: Theme,
    supported_themes: &[Theme],
    supported_locales: &[String],
) {
    let _ = std::fs::create_dir_all("dist/assets/css/themes");
    let _ = std::fs::write("dist/assets/css/style.css", theme_shared());
    supported_themes.iter().for_each(|theme| {
        let theme_str = theme.to_string();
        let _ = std::fs::write(
            format!("dist/assets/css/themes/{theme_str}.css"),
            Theme::theme(*theme),
        );
    });

    let _ = std::fs::create_dir_all("dist/assets/js");
    let _ = std::fs::write(
        "dist/assets/js/locale.js",
        locale_js_with_options(supported_locales, None),
    );
    let _ = std::fs::write(
        "dist/assets/js/theme.js",
        theme_js_with_options(&default_theme.to_string(), supported_themes),
    );
}

#[derive(Clone, Debug)]
pub struct AppShellBuilder {
    title: String,
    default_theme: Theme,
    default_locale: Option<String>,
    header_label: String,
    footer_label: String,
    with_nav: bool,
    header: Option<Element>,
    footer: Option<Element>,
    links: Vec<Link>,
    scripts: Vec<Script>,
    supported_themes: Option<Vec<Theme>>,
    supported_locales: Option<Vec<String>>,
}

impl Default for AppShellBuilder {
    fn default() -> Self {
        Self {
            title: "Quench".to_string(),
            default_theme: Theme::DefaultDark,
            default_locale: None,
            header_label: "header_label".to_string(),
            footer_label: "footer_label".to_string(),
            with_nav: true,
            header: None,
            footer: None,
            links: Vec::new(),
            scripts: Vec::new(),
            supported_themes: None,
            supported_locales: None,
        }
    }
}

impl AppShellBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn title(mut self, value: impl Into<String>) -> Self {
        self.title = value.into();
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

    pub fn header_label(mut self, value: impl Into<String>) -> Self {
        self.header_label = value.into();
        self
    }

    pub fn footer_label(mut self, value: impl Into<String>) -> Self {
        self.footer_label = value.into();
        self
    }

    pub fn with_nav(mut self, value: bool) -> Self {
        self.with_nav = value;
        self
    }

    pub fn header(mut self, value: Element) -> Self {
        self.header = Some(value);
        self
    }

    pub fn footer(mut self, value: Element) -> Self {
        self.footer = Some(value);
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
        self.supported_themes = Some(value);
        self
    }

    pub fn supported_locales(mut self, value: Vec<String>) -> Self {
        self.supported_locales = Some(value);
        self
    }

    pub fn try_build(self) -> Result<AppShell> {
        let mut supported_themes = self
            .supported_themes
            .clone()
            .unwrap_or_else(|| Theme::iter().collect::<Vec<_>>());
        if supported_themes.is_empty() {
            supported_themes = Theme::iter().collect::<Vec<_>>();
        }
        let effective_default_theme = if supported_themes.contains(&self.default_theme) {
            self.default_theme
        } else {
            supported_themes[0]
        };

        let supported_locales = match self.supported_locales.clone() {
            Some(v) => {
                validate_locales_exist(&v)?;
                v
            }
            None => available_locales().unwrap_or_default(),
        };
        let effective_default_locale = match &self.default_locale {
            Some(locale) if supported_locales.iter().any(|l| l == locale) => Some(locale.clone()),
            _ => supported_locales.first().cloned(),
        };

        let _ = std::fs::create_dir_all("dist/assets/css/themes");
        let _ = std::fs::write("dist/assets/css/style.css", theme_shared());
        supported_themes.iter().for_each(|theme| {
            let theme_str = theme.to_string();
            let _ = std::fs::write(
                format!("dist/assets/css/themes/{theme_str}.css"),
                Theme::theme(*theme),
            );
        });
        let _ = std::fs::create_dir_all("dist/assets/js");
        let _ = std::fs::write(
            "dist/assets/js/locale.js",
            locale_js_with_options(&supported_locales, effective_default_locale.as_deref()),
        );
        let _ = std::fs::write(
            "dist/assets/js/theme.js",
            theme_js_with_options(&effective_default_theme.to_string(), &supported_themes),
        );

        let header = self.header.unwrap_or_else(|| {
            let nav_panel = NavPanelBuilder::new()
                .default_theme(effective_default_theme)
                .default_locale(effective_default_locale.clone().unwrap_or_default())
                .supported_themes(supported_themes.clone())
                .supported_locales(supported_locales.clone())
                .build();
            let mut builder = HeaderBuilder::new().label(self.header_label);
            if self.with_nav {
                builder = builder.with_nav(nav_panel);
            }
            builder.build()
        });

        let footer = self
            .footer
            .unwrap_or_else(|| FooterBuilder::new().label(self.footer_label).build());

        let base = AppBuilder::new()
            .title(self.title)
            .links(self.links)
            .scripts(self.scripts)
            .supported_themes(supported_themes)
            .header(header)
            .footer(footer);

        Ok(AppShell { base })
    }

    pub fn build(self) -> AppShell {
        self.try_build()
            .unwrap_or_else(|err| panic!("failed to build app shell: {err}"))
    }
}

#[derive(Clone, Debug)]
pub struct AppShell {
    base: AppBuilder,
}

impl AppShell {
    pub fn page(&self, content: Element) -> String {
        self.base.clone().page_content(content).build()
    }
}
