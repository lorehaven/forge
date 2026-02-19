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
}

impl NavPanelBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn default_theme(mut self, default_theme: Theme) -> Self {
        self.default_theme = Some(default_theme);
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
        select()
            .attr("id", "locale-select")
            .attr("value", "en-US")
            .on_change(UPDATE_LOCALE)
            .child(option().attr("value", "en-US").text("🇬🇧 English"))
            .child(option().attr("value", "pl-PL").text("🇵🇱 Polski"))
    }

    fn select_theme(&self) -> Element {
        let mut element = select()
            .attr("id", "theme-select")
            .attr(
                "value",
                &self.default_theme.unwrap_or(Theme::Dark).to_string(),
            )
            .on_change(UPDATE_THEME);
        Theme::iter().for_each(|theme| {
            let theme_str = theme.to_string();
            element = element
                .clone()
                .child(option().attr("value", &theme_str).text(&theme_str));
        });
        element
    }
}
