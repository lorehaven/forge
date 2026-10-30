---
title: API reference
parent: forge-ui
grand_parent: Libraries
nav_order: 1
---

Modules: `assets` (private; functions re-exported), `components` (public, glob re-exported), `scripts` (private; one constant re-exported), `shell` (private; items re-exported).

```rust
pub use assets::{stylesheet, stylesheet_with, write_css, write_css_with, write_stylesheet};
pub use components::*;
pub use scripts::LOCALE_CHOSEN_COOKIE;
pub use shell::{NavChild, NavItem, NavSection, PALETTES, PageWidth, SUPPORTED_LOCALES, ShellConfig,
                Sidebar, UiShell, gatehouse_dashboard_url, gatehouse_profile_url, menu_item,
                supported_locales, theme_menu, user_menu};
```

All types own their data; none are documented as `Send`/`Sync`-sensitive. Text shown to people goes through `data-i18n` keys (`t(key)`) unless it is data (`txt`).

## Stylesheet functions (`assets`)

The library CSS is `tokens.css`, `base.css`, `layout.css`, `components.css`, `legacy.css` joined with `\n`.

| Signature | Description |
|---|---|
| `pub fn stylesheet(rules: &[CssRule]) -> String` | library CSS, then each rule rendered (`CssRule::render`) after a `\n` |
| `pub fn stylesheet_with(extra_css: &str, rules: &[CssRule]) -> String` | library CSS, `\n`, `extra_css`, then the rules |
| `pub fn write_css(file_name: &str, rules: &[CssRule])` | creates `dist/assets/css` and writes `stylesheet(rules)` to `dist/assets/css/<file_name>`; I/O errors are ignored |
| `pub fn write_css_with(file_name: &str, extra_css: &str)` | same with `stylesheet_with(extra_css, &[])` |
| `pub fn write_stylesheet(file_name: &str, css: &str)` | writes an assembled stylesheet to `dist/assets/css/<file_name>` |

## Constants

| Item | Value |
|---|---|
| `LOCALE_CHOSEN_COOKIE` | `"qlocale_chosen"`: set when someone picks a language in the switch (quench's own script writes `qlocale` on first load, so that cookie alone cannot say whether anyone chose) |
| `SUPPORTED_LOCALES: [&str; 5]` | `["en-US", "pl-PL", "es-ES", "de-DE", "fr-FR"]` |
| `PALETTES: [&str; 6]` | `["green", "teal", "blue", "purple", "rose", "orange"]` |

`pub fn supported_locales() -> Vec<String>` returns `SUPPORTED_LOCALES` as owned strings.

## Components (`components`)

All return `Element` (chain `.class`, `.attr`, `.child` on the result) unless noted.

### Text and icons

| Function | Output |
|---|---|
| `t(key: &str)` | `<span data-i18n="key">` |
| `txt(text: impl ToString)` | `<span>` with literal text |
| `icon(name: &str)` | `<i class="fa-solid fa-NAME">` (Font Awesome solid, name without `fa-`) |

### Buttons

`pub enum Variant { Primary, Secondary, Tertiary, Link, Danger, DangerSoft }` (`Clone, Copy, Debug, PartialEq, Eq`) maps to `fui-btn-primary`, `-secondary`, `-tertiary`, `-link`, `-danger`, `-danger-soft`.

| Function | Output |
|---|---|
| `button_el(variant, label_key)` | `<button type="button" class="fui-btn fui-btn-X">` with `t(label_key)` |
| `submit_button(variant, label_key)` | same with `type="submit"` |
| `link_button(variant, href, label_key)` | `<a href class="fui-btn fui-btn-X">` |
| `icon_button(icon_name, label_key)` | `<button type="button" class="fui-icon-btn" data-i18n-title="label_key">` with the icon and `t(label_key).class("sr-only")` |
| `icon_link(icon_name, href, label_key)` | same as an `<a>` |

### Badges and tones

`pub enum Tone { Gray, Brand, Success, Warning, Error, Info }` (`Clone, Copy, Debug, PartialEq, Eq`). Badge classes: Gray none, `fui-badge-brand`, `-success`, `-warning`, `-error`, `-info`. Alert classes: Gray and Brand none, `fui-alert-success`, `-warning`, `-error`, `-info`. Alert icons: Success `circle-check`, Warning `triangle-exclamation`, Error `circle-exclamation`, others `circle-info`.

| Function | Output |
|---|---|
| `badge(tone, text)` | `span.fui-badge[.fui-badge-X]` with text |
| `status_badge(tone, text)` | badge with a leading `span.fui-dot` and literal text |
| `status_badge_key(tone, key)` | same with a translated label |

### Cards and stats

`pub struct Card` (`Default`): `Card::new()`, `.title_key(&str)`, `.description_key(&str)`, `.action(Element)` (repeatable, header right side), `.body(Element)` (repeatable), `.footer(Element)`, `.flush()` (no body padding), `.build() -> Element`. Structure: `div.fui-card > [div.fui-card-header (title h3.fui-card-title[data-i18n], div.fui-card-desc, div.fui-row of actions) when a title key or action exists] + div.fui-card-body[.fui-flush] + [div.fui-card-footer]`.

| Function | Output |
|---|---|
| `link_card(icon_name, href, title: Element, description: Option<Element>)` | `a.fui-link-card > div.fui-link-card-icon > icon; div.fui-link-card-body > (div.fui-link-card-title, div.fui-link-card-desc?); icon("chevron-right").fui-link-card-arrow` |
| `stat(label: Element, value: Element)` | `div.fui-stat > div.fui-stat-label + div.fui-stat-value` |

### Page header

`pub struct PageHeader` (`Default`): `PageHeader::new(title_key)`, `PageHeader::titled(title)` (literal), `.subtitle_key(&str)`, `.subtitle(text)`, `.crumb(label: Element, href: Option<&str>)` (`None` marks the current page: `span.fui-crumb-current`; separators are `chevron-right` icons with `aria-hidden`), `.action(Element)`, `.build()`. Output: `div.fui-page-header > div.fui-page-header-text (div.fui-breadcrumbs?, h1.fui-page-title, p.fui-page-subtitle?) + div.fui-page-actions`.

### Forms

| Function | Output |
|---|---|
| `form_el(method, action)` | `<form method action class="fui-form">` |
| `field(label_key, control, hint_key: Option<&str>)` | `div.fui-field > label.fui-label[data-i18n] + control + div.fui-hint?` |
| `field_for(control_id, label_key, control, hint_key)` | same with `label[for=control_id]` |
| `text_input(name, input_type)` | `<input type name id=name class="fui-input">` |
| `select_el(name)` | `<select name id class="fui-select">` |
| `textarea_el(name)` | `<textarea name id class="fui-textarea">` |
| `search_input(name, placeholder_key)` | `div.fui-input-group > icon("magnifying-glass") + input[type=search].fui-input[data-i18n-placeholder]` |
| `checkbox_field(name, label_key)` | `label.fui-check > input[type=checkbox][name] + t(label_key)` |

### Tables and lists

| Function | Output |
|---|---|
| `table_el(header_keys: &[&str], rows: Vec<Element>, stack: bool) -> Element` | `div.fui-table-wrap > table.fui-table[.fui-table-stack]` with `<th data-i18n>` per key and the rows in `<tbody>`; `stack` collapses rows to label/value pairs below 640px (labels filled by the shell script) |
| `tr_el()`, `td_el()` | `<tr>`, `<td>` |
| `key_values(pairs: Vec<(Element, Element)>)` | `dl.fui-kv` with `dt`/`dd` |

### Feedback and misc

| Function | Output |
|---|---|
| `alert(tone, body)` | `div.fui-alert[.fui-alert-X][role]` (`role="alert"` for Error, `status` otherwise) with the tone icon and `div.fui-alert-body` |
| `empty_state(icon_name, title_key, description_key: Option<&str>)` | `div.fui-empty > div.fui-empty-icon, div.fui-empty-title[data-i18n], div[data-i18n]?` |
| `progress(percent: u8)` | `div.fui-progress[role=progressbar][aria-valuenow]` with `span[style="width:N%"]`; `N = percent.min(100)` |
| `code_block(text)` | `<pre class="fui-code-block">` |
| `tabs(items: &[(&str, String, bool)])` | `div.fui-tabs[role=tablist]` of `a.fui-tab` (label key, href, active); the active one gets `aria-current="page"` |
| `modal(id, title_key, body, footer: Option<Element>)` | `div.fui-modal#id[role=dialog][aria-modal=true]` with scrim, panel, header (title and a close icon button using key `ui_close`), body, footer |
| `open_modal(id) -> String` | `fuiOpenModal('ID')` (an `onclick` body) |
| `close_modal(id) -> String` | `fuiCloseModal('ID')` |

## Shell (`shell`)

### Navigation types

| Type | Fields / constructors |
|---|---|
| `NavItem` (`Clone, Debug`) | `icon: String` (Font Awesome name), `label_key: String`, `href: String` (absolute path; the item whose href is the longest prefix of the URL is highlighted); `NavItem::new(icon: &str, label_key: &str, href: impl Into<String>)` |
| `NavChild` (`Clone, Debug`) | `label: String` (literal, localised), `href: String`, `active: bool`; `NavChild::new(label, href, active)`; a link nested under a `NavItem`, known only at request time |
| `NavSection` (`Clone, Debug, Default`) | `heading_key: Option<String>`, `items: Vec<NavItem>`; `NavSection::new(items)`, `NavSection::titled(heading_key, items)` |
| `Sidebar` (`Clone, Debug, Default`) | `children: Vec<(String, Vec<NavChild>)>` (keyed by the parent item's `href`), `nav: Option<Element>`; `.under(parent_href, children)`, `.replacing_nav(Element)` (replaces the configured navigation entirely) |

### `ShellConfig`

`#[derive(Clone, Debug)]` with pub fields `title: String` (tab title), `css_file: String` (name used with `write_css`), `brand_name: String`, `brand_icon: String` (Font Awesome icon in the mark), `nav: Vec<NavSection>`, `profile_href: Option<String>` (`None` leaves the account menu out), `dashboard_href: Option<String>` (`None` leaves the dashboard icon out).

`ShellConfig::new(brand_name: &str, css_file: &str, brand_icon: &str)` sets `title = brand_name`, empty nav, no profile or dashboard. Builders: `.dashboard(href)`, `.nav(sections)`, `.profile(href)`.

### `PageWidth`

`#[derive(Clone, Copy, Debug)] pub enum PageWidth { Normal, Wide, Fill }`: classes `` (none), `fui-page-wide`, `fui-page-fill` on `div.fui-page`.

### `UiShell`

`#[derive(Clone, Debug)]`. Built once per service (typically in a `LazyLock`).

| Method | Description |
|---|---|
| `UiShell::new(cfg: ShellConfig) -> Self` | the signed-in frame: sidebar, top bar, scrolling content |
| `UiShell::auth(cfg: ShellConfig) -> Self` | the signed-out frame: `div.fui-auth` with `div.fui-auth-bar` (brand, locale switch, theme menu) and `div.fui-auth-body` |
| `page(&self, content: Element) -> String` | full HTML document, `PageWidth::Normal` |
| `page_wide(&self, content)` | no max-width cap |
| `page_fill(&self, content)` | content fills the area below the top bar and scrolls its own panes |
| `page_in(&self, content, width: PageWidth, sidebar: &Sidebar) -> String` | with per-request sidebar additions |

Building a shell (private `build`) configures `quench-web`'s `AppShellBuilder`: title; `supported_locales` = `SUPPORTED_LOCALES`; default theme dark, supported themes dark and light; links: preconnect to `https://fonts.gstatic.com` (crossorigin), the Inter stylesheet from `fonts.googleapis.com` (weights 400 to 700), and the service stylesheet at `ui_asset_path("/css/<css_file>")`; scripts: the four inline scripts (see [scripts](scripts.md)); no built-in nav or header; a hidden footer; `resources_prefix(ui_path(""))`. It then **writes `dist/assets/favicon.png`** (the embedded forge icon) replacing the one `quench-web` wrote; the result of the write is ignored.

Structure of the signed-in page: `div.fui-app > aside.fui-sidebar#fui-sidebar` (head with brand link to `ui_path("/home")` and a close button calling `fuiCloseDrawer()`; `nav.fui-nav[aria-label=Main]` with `div.fui-nav-section` groups, optional `div.fui-nav-heading`, `a.fui-nav-item` and `div.fui-nav-sub > a.fui-nav-subitem`), `div.fui-scrim` (click closes the drawer), `div.fui-main > header.fui-topbar + main.fui-content#fui-content > div.fui-page`. The top bar holds the menu button (`fuiOpenDrawer()`, class `fui-menu-button`, `aria-controls="fui-sidebar"`), `div.fui-topbar-nav` (dashboard icon link when configured, home icon link to `ui_path("/home")`), a title shown below desktop width, a spacer, and actions (locale switch, theme menu, user menu when `profile_href` is set; logout link `ui_path("/logout")`).

### Free functions

| Function | Description |
|---|---|
| `theme_menu() -> Element` | `details.fui-dropdown` with mode buttons (`onclick="fuiSetTheme('light'\|'dark')"`) and palette swatches (`onclick="fuiSetPalette('<name>')"`) |
| `user_menu(profile_href: &str, logout_href: &str) -> Element` | avatar dropdown; the picture is `<profile_href without trailing slash>/avatar`, loaded hidden, adds `has-avatar` to its parent on load |
| `menu_item(href, icon_name, key) -> Element` | `a.fui-dropdown-item` |
| `gatehouse_profile_url() -> String` | `GATEHOUSE_URL` (trailing `/` trimmed, default empty) + `/ui/account` |
| `gatehouse_dashboard_url() -> String` | `GATEHOUSE_URL` + `/ui/home` |

I18n keys used by the shell: `ui_menu_open`, `ui_menu_close`, `ui_dashboard_button`, `ui_home_button`, `ui_theme_toggle`, `ui_theme_mode`, `ui_theme_light`, `ui_theme_dark`, `ui_theme_palette`, `ui_palette_<name>`, `ui_user_menu`, `ui_profile_edit`, `ui_logout`, `ui_close`.
