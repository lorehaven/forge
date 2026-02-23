use crate::dom::{on_dom_ready, set_select_value, toggle_modal, update_from_select};
use crate::js::locale::available_locales;
use crate::{Element, Theme, div, i, label, nav, option, script, select};
use strum::IntoEnumIterator;

pub fn nav_button() -> Element {
    nav()
        .on_click(&toggle_modal("modal-overlay", "modal-side", "show"))
        .child(i().class("fas").class("fa-grip"))
}

#[derive(Clone, Debug, Default)]
pub struct NavPanelBuilder {
    pub default_theme: Option<Theme>,
    pub default_locale: Option<String>,
    pub supported_themes: Option<Vec<Theme>>,
    pub supported_locales: Option<Vec<String>>,
}

impl NavPanelBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn default_theme(mut self, default_theme: Theme) -> Self {
        self.default_theme = Some(default_theme);
        self
    }

    pub fn default_locale(mut self, default_locale: impl Into<String>) -> Self {
        self.default_locale = Some(default_locale.into());
        self
    }

    pub fn supported_themes(mut self, supported_themes: Vec<Theme>) -> Self {
        self.supported_themes = Some(supported_themes);
        self
    }

    pub fn supported_locales(mut self, supported_locales: Vec<String>) -> Self {
        self.supported_locales = Some(supported_locales);
        self
    }

    pub fn build(self) -> Element {
        let toggle_modal_js = toggle_modal("modal-overlay", "modal-side", "show");
        let on_load_js = on_dom_ready(&[
            set_select_value("locale-select", "getLocale"),
            set_select_value("theme-select", "getTheme"),
        ]);

        div()
            .child(div().class("modal-overlay").on_click(&toggle_modal_js))
            .child(
                div().class("modal-side").child(
                    div()
                        .class("modal-content")
                        .child(label().attr("data-i18n", "locale_label"))
                        .child(self.select_locale())
                        .child(label().attr("data-i18n", "theme_label"))
                        .child(self.select_theme()),
                ),
            )
            .child(script(on_load_js).raw().defer())
    }

    fn select_locale(&self) -> Element {
        let locales = self
            .supported_locales
            .clone()
            .unwrap_or_else(|| available_locales().unwrap_or_else(|_| vec!["en-US".to_string()]));
        let default_locale = match &self.default_locale {
            Some(value) if locales.iter().any(|l| l == value) => value.clone(),
            _ => locales
                .first()
                .cloned()
                .unwrap_or_else(|| "en-US".to_string()),
        };

        let mut element = select()
            .attr("id", "locale-select")
            .attr("value", &default_locale)
            .on_change(&update_from_select("locale-select", "updateLocale"));

        for locale in locales {
            let label = format!("{} {}", locale_flag(&locale), locale)
                .trim()
                .to_string();
            element = element
                .clone()
                .child(option().attr("value", &locale).text(&label));
        }

        element
    }

    fn select_theme(&self) -> Element {
        let themes = self
            .supported_themes
            .clone()
            .unwrap_or_else(|| Theme::iter().collect::<Vec<_>>());
        let mut element = select()
            .attr("id", "theme-select")
            .attr(
                "value",
                &self.default_theme.unwrap_or(Theme::DefaultDark).to_string(),
            )
            .on_change(&update_from_select("theme-select", "updateTheme"));
        themes.into_iter().for_each(|theme| {
            let theme_str = theme.to_string();
            element = element
                .clone()
                .child(option().attr("value", &theme_str).text(&theme_str));
        });
        element
    }
}

fn locale_flag(locale: &str) -> String {
    let region = locale
        .split(['-', '_'])
        .nth(1)
        .unwrap_or("")
        .to_ascii_uppercase();

    if region.len() != 2 || !region.chars().all(|c| c.is_ascii_alphabetic()) {
        return String::new();
    }

    region
        .chars()
        .map(|c| char::from_u32(0x1F1E6 + (c as u32 - 'A' as u32)).unwrap_or(' '))
        .collect::<String>()
}
