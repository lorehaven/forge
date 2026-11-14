//! Untitled UI for the terminal.
//!
//! The design tokens of the forge web and mobile apps (`forge-shell/assets/tokens.css`) as RGB values, a small
//! styled-line model that renders to truecolor, 256-colour or plain text, and the widgets a full-screen tool needs:
//! badges, buttons, keyboard chips, cards and alerts.

pub mod color;
pub mod console;
pub mod line;
pub mod theme;
pub mod widgets;

pub use color::{ColorLevel, Rgb};
pub use line::{Line, Style, fill_screen};
pub use theme::{Mode, Palette, Soft, Theme, Tone};
