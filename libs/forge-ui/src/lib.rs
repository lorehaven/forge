//! Shared UI for the forge web services: Untitled UI tokens, a responsive shell (sidebar on desktop,
//! drawer on mobile), and components. Services add only page-specific CSS on top via [`write_css`].

mod assets;
pub mod components;
mod scripts;
mod shell;

pub use assets::{stylesheet, stylesheet_with, write_css, write_css_with, write_stylesheet};
pub use components::*;
pub use scripts::LOCALE_CHOSEN_COOKIE;
pub use shell::{
    NavChild, NavItem, NavSection, PALETTES, PageWidth, Sidebar, SUPPORTED_LOCALES, ShellConfig, UiShell,
    gatehouse_dashboard_url, gatehouse_profile_url, menu_item, supported_locales, theme_menu,
    user_menu,
};
