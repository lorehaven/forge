use crate::js::locale::available_locales;
use crate::{Element, Theme, div, i, label, nav, option, script, select};
use strum::IntoEnumIterator;

const TOGGLE_SHOW_MODAL: &str = r#"
const overlay = document.getElementsByClassName('modal-overlay');
const sidemodal = document.getElementsByClassName('modal-side');
if (overlay.length === 1 && sidemodal.length) {
    overlay[0].classList.toggle('show');
    sidemodal[0].classList.toggle('show');
}
"#;

const UPDATE_LOCALE: &str = r#"
const selected = document.getElementById('locale-select').value;
updateLocale(selected);
"#;

const UPDATE_THEME: &str = r#"
const selected = document.getElementById('theme-select').value;
updateTheme(selected);
"#;

const SET_ON_LOAD: &str = r#"
document.addEventListener("DOMContentLoaded", () => {
    const localeSelect = document.getElementById('locale-select');
    localeSelect.value = getLocale();

    const themeSelect = document.getElementById('theme-select');
    themeSelect.value = getTheme();
});
"#;

pub fn nav_button() -> Element {
    nav()
        .on_click(TOGGLE_SHOW_MODAL)
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
        div()
            .child(div().class("modal-overlay").on_click(TOGGLE_SHOW_MODAL))
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
            .child(script(SET_ON_LOAD.to_string()).raw().defer())
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
            .on_change(UPDATE_LOCALE);

        for locale in locales {
            let label = format!("{} {}", locale_flag(&locale), locale).trim().to_string();
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
            .on_change(UPDATE_THEME);
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
