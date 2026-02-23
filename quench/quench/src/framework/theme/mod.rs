use crate::styles::common::{content, elements, footer, header, modal, root};
use crate::styling::css::CssRule;
use std::fmt::{Display, Formatter};
use strum_macros::EnumIter;

pub mod bootstrap_dark;
pub mod bootstrap_light;
pub mod default_dark;
pub mod default_light;

pub trait ThemeSpec {
    fn colors() -> Vec<CssRule>;

    fn render() -> String {
        Self::colors()
            .into_iter()
            .map(|rule| rule.render())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumIter)]
pub enum Theme {
    DefaultDark,
    DefaultLight,
    BootstrapDark,
    BootstrapLight,
}

impl Display for Theme {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Theme::DefaultDark => write!(f, "default-dark"),
            Theme::DefaultLight => write!(f, "default-light"),
            Theme::BootstrapDark => write!(f, "bootstrap-dark"),
            Theme::BootstrapLight => write!(f, "bootstrap-light"),
        }
    }
}

impl Theme {
    pub fn theme(theme: Self) -> String {
        match theme {
            Theme::DefaultDark => default_dark::DefaultDarkTheme::render(),
            Theme::DefaultLight => default_light::DefaultLightTheme::render(),
            Theme::BootstrapDark => bootstrap_dark::BootstrapDarkTheme::render(),
            Theme::BootstrapLight => bootstrap_light::BootstrapLightTheme::render(),
        }
    }
}

pub fn theme_shared() -> String {
    vec![root(), header(), content(), footer(), elements(), modal()]
        .into_iter()
        .flatten()
        .map(|rule| rule.render())
        .collect::<Vec<_>>()
        .join("\n")
}
