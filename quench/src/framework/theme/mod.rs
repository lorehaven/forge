use crate::styles::common::{content, elements, footer, header, modal, root};
use std::fmt::{Display, Formatter};
use strum_macros::EnumIter;

pub mod dark;
pub mod light;

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Theme {
    Dark,
    Light,
}

impl Display for Theme {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Theme::Dark => write!(f, "dark"),
            Theme::Light => write!(f, "light"),
        }
    }
}

impl Theme {
    pub fn theme(theme: Self) -> String {
        match theme {
            Theme::Dark => dark::theme(),
            Theme::Light => light::theme(),
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
