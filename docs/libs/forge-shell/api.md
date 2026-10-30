---
title: API reference
parent: forge-shell
grand_parent: Libraries
nav_order: 1
---

Everything is in `src/lib.rs`. Imports used: `quench_ui::i18n::I18n`, `quench_ui::prelude::*`, `quench_ui::router::link`. No `Send`/`Sync` bounds: the bar types hold `Rc` and are meant for the single-threaded browser runtime.

## Constants

```rust
pub const STYLE: &str
```
`tokens.css`, `base.css`, `components.css`, `shell.css` concatenated with a `\n` between each (`concat!(include_str!(..), "\n", ...)`). See [CSS source](css-source.md).

```rust
pub const SCRIPT: &str
```
`assets/shell.js`. See [JavaScript hooks](js.md).

```rust
pub const PALETTES: [&str; 6] = ["green", "teal", "blue", "purple", "rose", "orange"];
```
Palettes in menu order. Matches `PALETTES` in `shell.js` and the `html[data-fui-palette]` rules of `tokens.css`.

## Types

### `Brand`

```rust
#[derive(Debug, Clone, Copy)]
pub struct Brand { pub name: &'static str, pub icon: &'static str }
```
`name` is shown next to the mark. `icon` is a `fui-ico-*` name without the prefix (`eye`, `list-check`, ...; the 28 names are in [CSS vocabulary](css.md#icons)).

### `Bar`

```rust
#[derive(Clone)]
pub struct Bar {
    pub brand: Brand,
    pub image: Option<String>,
    pub home: String,
    pub spa: bool,
    pub account: bool,
    pub brand_in_bar: bool,
    pub logout: String,
    pub on_logout: Option<Rc<dyn Fn()>>,
    pub dashboard: bool,
    pub languages: &'static [&'static str],
    pub on_language: Rc<dyn Fn(&str)>,
}
```

| Field | Meaning |
|---|---|
| `brand` | name and icon |
| `image` | the service's icon as the page serves it (`{base}/ui/assets/favicon.png`); `Some` replaces the glyph mark with an `<img>` (`fui-brand-mark fui-brand-image`) |
| `home` | the service's home screen |
| `spa` | `true`: `home` is a route of the client and links are drawn with `quench_ui::router::link`; `false`: plain `<a href>` (a page of its own) |
| `account` | whether the account menu (edit profile, sign out) is drawn; a client that signs in by itself has none |
| `brand_in_bar` | `true`: the brand is in the top bar on wide screens too (class `fui-brand`); `false`: `fui-brand fui-hide-desktop` (the side panel shows it) |
| `logout` | where signing out starts (a full page load): the `href` of the sign-out link |
| `on_logout` | when `Some`, the menu entry is a `<button id="sign-out">` that calls this instead of a link |
| `dashboard` | whether the services-dashboard button is drawn (gatehouse is the dashboard: `false` there) |
| `languages` | locales to pick from; the first is the fallback; empty hides the picker |
| `on_language` | called with the locale code after `I18n::set_language(code)` |

## Functions

```rust
#[must_use] pub fn icon(name: &str) -> Element
```
`<i class="fui-ico fui-ico-{name}">`: an icon drawn as a CSS mask painted with the text colour. An unknown name renders an empty (invisible) box because no `--ico` is defined.

```rust
#[must_use] pub fn flag(locale: &str) -> String
```
The region part of `locale` (second component when split on `-` or `_`), as regional-indicator characters: `flag("pl-PL")` is the Polish flag emoji, `flag("en_us")` the US one. Empty string when there is no region, it is not exactly two characters, or not all ASCII letters (`flag("en")`, `flag("zh-Hans")` give `""`).

```rust
#[must_use] pub fn brand_head(bar: &Bar) -> Element
```
`div.fui-sidebar-head > div.fui-brand > (span.fui-brand-mark, span.fui-brand-name)`: the brand alone, for a client that draws its own side panel.

```rust
#[must_use] pub fn sidebar(bar: &Bar, nav: Vec<View>) -> Element
```
`<aside class="fui-sidebar">` with the same head, then `<nav class="fui-nav" aria-label="Main">` holding `nav`.

```rust
#[must_use] pub fn topbar(bar: &Bar, t: &Rc<I18n>) -> Element
```
`<header class="fui-topbar">` containing, in order:

1. `div.fui-topbar-nav`: when `bar.dashboard`, `a#dashboard.fui-icon-btn[data-gh-href="/ui/home"][href="/gatehouse/ui/home"]` (icon `grid`, title `ui_dashboard_button`); always `a#home.fui-icon-btn` to `bar.home` (icon `house`, title `ui_home_button`).
2. `a#title` to `bar.home`, class `fui-brand` (plus ` fui-hide-desktop` unless `brand_in_bar`), with the mark and `span.fui-brand-name`.
3. `div.fui-topbar-spacer`.
4. `div.fui-topbar-actions`: when `languages` is non-empty `div.locale-switch > select#language` (`aria-label` = `t.t("locale_label")`, one `<option>` per code labelled `"<flag> <code>"` trimmed, selected when `t.language() == code`; on input it calls `t.set_language(code)` then `on_language(code)`); the theme menu (`details.fui-dropdown`: summary `fui-icon-btn` with icon `palette`; light/dark buttons `.fui-theme-mode[data-mode][data-fui-mode]` with icons `sun`/`moon`; six `.fui-swatch[data-palette][data-fui-palette]` buttons); and when `bar.account` the user menu.
5. User menu: `details.fui-dropdown` with `summary.fui-avatar` (icon `user` plus `<img data-gh-src="/ui/account/avatar">`), and items: `a.fui-dropdown-item[data-gh-href="/ui/account"][href="/gatehouse/ui/account"]` (title key `ui_profile_edit`), then sign-out as above (`ui_logout`).

The `href` values with `/gatehouse/...` are fallbacks; [`shell.js`](js.md#gatehouse-links) rewrites every `[data-gh-href]` to `<gatehouse base>` + the attribute value. Build the bar where the language is read, so a language change re-renders it.

## i18n keys

The bar reads these keys from the `I18n` catalogue (all must exist in each locale; `tests/unit.rs` defines `EN`):

`locale_label`, `ui_dashboard_button`, `ui_home_button`, `ui_theme_toggle`, `ui_theme_mode`, `ui_theme_light`, `ui_theme_dark`, `ui_theme_palette`, `ui_palette_green`, `ui_palette_teal`, `ui_palette_blue`, `ui_palette_purple`, `ui_palette_rose`, `ui_palette_orange`, `ui_user_menu`, `ui_profile_edit`, `ui_logout`.
