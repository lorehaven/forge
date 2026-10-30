---
title: Scripts
parent: forge-ui
grand_parent: Libraries
nav_order: 3
---

`UiShell` injects four inline scripts into every page, in this order. They are progressive: without JavaScript the sidebar is hidden on mobile and the theme stays dark.

| Order | Constant (private) | What it does |
|---|---|---|
| 1 | `THEME_SYNC` (runs in `<head>`, before first paint) | mirrors the `qtheme` cookie onto `html[data-fui-theme]` (`light` if the cookie value contains `light`, else `dark`) and the `fui_palette` cookie onto `html[data-fui-palette]` (default `green`); re-applies on the `themeChanged` event and every 1000 ms. Defines `fuiSetTheme(mode)` (writes `qtheme=default-<mode>`), `fuiSetPalette(name)` (ignores unknown names; writes `fui_palette`), `fuiToggleTheme()` |
| 2 | `MARK_LOCALE_CHOSEN` | on the `localeChanged` event (quench dispatches it only from the switch) sets cookie `qlocale_chosen=1` (max-age 1 year, `path=/`, `SameSite=Lax`) |
| 3 | `SERIALIZE_REFRESH` | wraps quench's global `tryRefresh` so tabs of one origin take turns (Web Locks lock `forge-session-refresh`): the waiting tab first re-reads `SESSION_STATUS_URL` (`credentials: same-origin`, `cache: no-store`) and, if the response JSON has `authenticated === true`, returns `true` without refreshing; otherwise it calls the original. Does nothing where `navigator.locks`, `tryRefresh` or `SESSION_STATUS_URL` is missing. Purpose: refresh tokens are single use, so simultaneous refreshes from several tabs would log all but the first out |
| 4 | `SHELL_JS` | drawer, modals, dropdown dismissal, active nav item, stacked table labels |

## Global functions defined

| Function | Effect |
|---|---|
| `fuiSetTheme(mode)`, `fuiSetPalette(name)`, `fuiToggleTheme()` | see above |
| `fuiOpenDrawer()` / `fuiCloseDrawer()` | add / remove class `fui-drawer-open` on `<html>` |
| `fuiOpenModal(id)` / `fuiCloseModal(id)` | add / remove class `fui-open` on the element with that id |

## Behaviour of `SHELL_JS`

* `Escape`: closes the drawer, every `.fui-modal.fui-open`, and every open `details.fui-dropdown`.
* Click anywhere: open dropdowns other than the one clicked in are closed; a click on `.fui-nav-item` or `.fui-nav-subitem` closes the drawer.
* `resize`: the drawer closes when `window.innerWidth >= 1024`.
* Active item: the `.fui-nav-item[href]` whose href (query stripped, trailing slashes removed) equals the path or is a path prefix followed by `/`, the longest wins, gets `aria-current="page"`; others lose it. Runs at `DOMContentLoaded`.
* Stacked tables: for `table.fui-table-stack`, each `td` in `tbody` gets `data-label` from the matching `thead th` text. Runs at init, 700 ms later, after `localeChanged` (100 ms later) and after `htmx:afterSwap`.
