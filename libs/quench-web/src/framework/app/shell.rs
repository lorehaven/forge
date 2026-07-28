use super::assets::create_asset_files_with_options;
use super::i18n::{available_locales, validate_locales_exist};
use crate::{
    AppBuilder, Element, FooterBuilder, HeaderBuilder, Link, NavPanelBuilder, Script, Theme,
};
use anyhow::Result;
use strum::IntoEnumIterator;

#[derive(Clone, Debug)]
pub struct AppShellBuilder {
    pub(crate) title: String,
    pub(crate) default_theme: Theme,
    pub(crate) default_locale: Option<String>,
    pub(crate) header_label: String,
    pub(crate) footer_label: String,
    pub(crate) with_nav: bool,
    pub(crate) with_header: bool,
    pub(crate) header: Option<Element>,
    pub(crate) footer: Option<Element>,
    pub(crate) links: Vec<Link>,
    pub(crate) scripts: Vec<Script>,
    pub(crate) supported_themes: Option<Vec<Theme>>,
    pub(crate) supported_locales: Option<Vec<String>>,
    pub(crate) resources_prefix: String,
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
            with_header: true,
            header: None,
            footer: None,
            links: Vec::new(),
            scripts: Vec::new(),
            supported_themes: None,
            supported_locales: None,
            resources_prefix: String::new(),
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

    /// Drops the top bar entirely. A page that carries its own chrome - the
    /// login card, for one - has nothing to put in it.
    pub fn with_header(mut self, value: bool) -> Self {
        self.with_header = value;
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

    pub fn resources_prefix(mut self, value: String) -> Self {
        self.resources_prefix = value;
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

        create_asset_files_with_options(
            effective_default_theme,
            &supported_themes,
            &supported_locales,
            &self.resources_prefix,
        );

        let header = self.with_header.then(|| {
            self.header.unwrap_or_else(|| {
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
            })
        });

        let footer = self
            .footer
            .unwrap_or_else(|| FooterBuilder::new().label(self.footer_label).build());

        let mut base = AppBuilder::new()
            .title(self.title)
            .links(self.links)
            .scripts(self.scripts)
            .supported_themes(supported_themes)
            .supported_locales(supported_locales)
            .default_theme(effective_default_theme)
            .default_locale(effective_default_locale.clone().unwrap_or_default())
            .footer(footer)
            .resources_prefix(self.resources_prefix);

        if let Some(header) = header {
            base = base.header(header);
        }

        Ok(AppShell { base })
    }

    pub fn build(self) -> AppShell {
        self.try_build()
            .unwrap_or_else(|err| panic!("failed to build app shell: {err}"))
    }
}

#[derive(Clone, Debug)]
pub struct AppShell {
    pub(crate) base: AppBuilder,
}

impl AppShell {
    pub fn page(&self, content: Element) -> String {
        self.base.clone().page_content(content).build()
    }
}
