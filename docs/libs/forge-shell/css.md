---
title: CSS vocabulary
parent: forge-shell
grand_parent: Libraries
nav_order: 2
---

The class vocabulary of `forge_shell::STYLE`. All classes are prefixed `fui-`; state is carried by attributes (`aria-current="page"`, `[open]`, `[disabled]`, `data-*`) and by a few state classes (`fui-open`, `fui-dragging`, `fui-drop-target`, `has-avatar`). Values come from [tokens](tokens.md); every rule is in [CSS source](css-source.md).

## Roots

| Class | Use |
|---|---|
| `fui-app` | root of a signed-in page. `base.css` scopes typography, headings (`h1` 1.875rem, `h2` 1.5rem, `h3` 1.125rem, `h4` 1rem; weight 600), links (colour `--fui-link`, no hover underline) and the form/button compat rules to it |
| `fui-auth` | root of a signed-out page (login); same scoping as `fui-app` |
| `fui-client` | the flex row of a client: side panel + column. Below 1024px it stacks vertically |
| `fui-client-col` | the column beside the side panel (top bar above, content below) |
| `fui-client-main` | the scrolling content area |
| `fui-underline` | on an `a` inside `fui-app`: underline on hover (otherwise never) |
| `fui-selectable` | text selection allowed (by default `user-select: none` on `html, body, .fui-app, .fui-auth`; inputs, textareas, `pre`, `code`, `samp`, `kbd`, contenteditable, `.fui-code-block`, `.fui-alert`, `.fui-notice`, `.fui-field-error` are selectable already) |
| `fui-mono` | monospace (`--fui-mono`, 0.8125rem); `code` and `pre` inside `fui-app` are monospace already |
| `fui-muted` | `--fui-text-tertiary` colour |
| `sr-only` | visually hidden, available to assistive technology |
| `fui-hide-desktop` | `display: none !important` from 1024px up |

## Frame

| Class | Use |
|---|---|
| `fui-sidebar` | side panel: sticky, `--fui-sidebar-w` wide on desktop; static and full width below 1024px (its head hidden) |
| `fui-sidebar-head` | brand row, `--fui-topbar-h` high |
| `fui-nav` | the navigation list (`display: none` when empty) |
| `fui-nav-item` | a navigation link; `aria-current="page"` marks the current one. May contain an icon (`fui-ico`, 1.25rem) |
| `fui-nav-sub`, `fui-nav-subitem` | nested list under an item; `aria-current="page"` on the sub item |
| `fui-topbar` | the top bar (height `--fui-topbar-h`, bottom border). Below 640px the nav part (`fui-topbar-nav`) is hidden |
| `fui-topbar-nav`, `fui-topbar-spacer`, `fui-topbar-actions`, `fui-topbar-title` | left button group, flexible spacer, right action group, text title |
| `fui-brand`, `fui-brand-mark`, `fui-brand-name`, `fui-brand-image` | the brand link; the 2rem square mark in the brand colour; the name (ellipsised); the variant of the mark that holds an image instead of a glyph |
| `fui-icon-btn` | 2.5rem square icon button (button, link or `summary`) |
| `fui-avatar` | round account button; `has-avatar` (added by script when the picture loaded) hides the icon and shows the `img` |
| `fui-dropdown` | `details` based menu; `fui-dropdown-menu` the panel (absolute, right aligned), `fui-dropdown-item` an entry; `fui-dropdown-up` on an ancestor of the menu (for example the `details`) opens it upward, left aligned |
| `fui-theme-menu`, `fui-theme-label`, `fui-theme-modes`, `fui-theme-mode`, `fui-theme-palettes`, `fui-swatch` | the theme menu parts; `.fui-theme-mode` and `.fui-swatch` highlight when `html[data-fui-theme]` / `html[data-fui-palette]` matches their `data-mode` / `data-palette` |
| `locale-switch` | wrapper of the language `select` (restyled quench class) |

## Components

| Class | Use |
|---|---|
| Buttons: `fui-btn` with `fui-btn-primary`, `-secondary`, `-tertiary`, `-link`, `-danger`, `-danger-soft`; size `fui-btn-sm`, `fui-btn-lg`; `fui-btn-block` (full width) | `[disabled]` and `[aria-disabled="true"]` dim the button and disable pointer events |
| Badges: `fui-badge` with `fui-badge-brand`, `-success`, `-warning`, `-error`, `-info` (no modifier is grey); `fui-dot` the status dot | |
| Cards: `fui-card`, `fui-card-header`, `fui-card-title`, `fui-card-desc`, `fui-card-body` (+ `fui-flush` for no padding), `fui-card-footer` | |
| Link cards: `fui-link-card` (an `a`), `fui-link-card-icon` (tile with icon), `fui-link-card-img` (image instead), `fui-link-card-body`, `-title`, `-desc`, `-arrow`; `fui-link-card-stack` stacks the content vertically; `fui-card-grid` is the auto-fill grid of cards | |
| Stats: `fui-stat`, `fui-stat-label`, `fui-stat-value` | |
| Forms: `fui-form`, `fui-field`, `fui-label`, `fui-hint`, `fui-field-error`, `fui-input`, `fui-select`, `fui-textarea` (+ `fui-input-sm`, `fui-select-sm`, `fui-input-invalid`), `fui-input-group` (a leading `i` child is positioned inside the input), `fui-check` (label wrapping a checkbox), `fui-form-actions` (+ `fui-end` right-aligned), `fui-toolbar` | |
| Tables: `fui-table-wrap` (horizontal scroll), `fui-table`, cell classes `fui-cell-strong`, `fui-cell-actions`, `fui-cell-wrap`, `fui-cell-mono`; `fui-table-stack` (below 640px rows become label/value pairs using each `td`'s `data-label`) | |
| Key/value list: `fui-kv` on a `dl` (two columns, one below 640px) | |
| Alerts: `fui-alert` with `fui-alert-success`, `-warning`, `-error`, `-info`; `fui-alert-body` | |
| Notices (inside `fui-app`): `fui-notice` with `fui-notice-error`, `-ok`, `-info` | a banner above the content |
| Empty state: `fui-empty`, `fui-empty-icon`, `fui-empty-title`; `fui-empty-sm` (compact, inside `fui-app`) | |
| Modal: `fui-modal` (`fui-open` shows it), `fui-modal-scrim`, `fui-modal-panel` (+ `fui-modal-lg`), `fui-modal-header`, `fui-modal-body`, `fui-modal-footer`; below 640px it is a bottom sheet | |
| Tabs: `fui-tabs`, `fui-tab` (`aria-current="page"` or `aria-selected="true"` marks the active one) | |
| Misc: `fui-code-block` (a `pre`), `fui-progress` (a `span` child sets the width), `fui-pagination` | |
| Drag and drop: `fui-dragging` (on the dragged `[data-drag-id]` element), `fui-drop-target` (on a hovered `[data-drop-value]`) | see [js](js.md#drag-and-drop) |
| Copy feedback: `fui-copied` | added to a `[data-copy-target]` element for 1.5 s (no rule in the stylesheet; styled by services) |

Compat rules in `base.css`: bare `button` and `a.button` inside `fui-app`/`fui-auth` get button styling at the lowest specificity (`:where`), with `.primary`, `.delete`, `.danger` variants; `.app.q-shell-app` and `.app > .content` neutralise quench's shell wrapper.

## Icons

`<i class="fui-ico fui-ico-NAME">` is a 1.15em box painted with `currentColor` through a CSS mask (`--ico`), because the page's content security policy allows no font from elsewhere. Defined names (28): `grid`, `house`, `palette`, `sun`, `moon`, `user`, `logout`, `eye`, `list-check`, `users`, `key`, `box`, `clock`, `folder`, `archive`, `server`, `cpu`, `layers`, `columns`, `database`, `message`, `plus`, `list`, `file`, `shield`, `arrow-right`, `activity`, `terminal`.

## Class index (generated from the stylesheet)

Every class selector that occurs in `base.css`, `components.css` and `shell.css`, grouped by prefix:

* `.app`
* `.button`
* `.content`, `.content-inner`
* `.danger`
* `.delete`
* `.fui-alert`, `.fui-alert-body`, `.fui-alert-error`, `.fui-alert-info`, `.fui-alert-success`, `.fui-alert-warning`
* `.fui-app`
* `.fui-auth`
* `.fui-avatar`
* `.fui-badge`, `.fui-badge-brand`, `.fui-badge-error`, `.fui-badge-info`, `.fui-badge-success`, `.fui-badge-warning`
* `.fui-brand`, `.fui-brand-image`, `.fui-brand-mark`, `.fui-brand-name`
* `.fui-btn`, `.fui-btn-block`, `.fui-btn-danger`, `.fui-btn-danger-soft`, `.fui-btn-lg`, `.fui-btn-link`, `.fui-btn-primary`, `.fui-btn-secondary`, `.fui-btn-sm`, `.fui-btn-tertiary`
* `.fui-card`, `.fui-card-body`, `.fui-card-desc`, `.fui-card-footer`, `.fui-card-grid`, `.fui-card-header`, `.fui-card-title`
* `.fui-cell-actions`, `.fui-cell-mono`, `.fui-cell-strong`, `.fui-cell-wrap`
* `.fui-check`
* `.fui-client`, `.fui-client-col`, `.fui-client-main`
* `.fui-code-block`
* `.fui-dot`
* `.fui-dragging`
* `.fui-drop-target`
* `.fui-dropdown`, `.fui-dropdown-item`, `.fui-dropdown-menu`, `.fui-dropdown-up`
* `.fui-empty`, `.fui-empty-icon`, `.fui-empty-sm`, `.fui-empty-title`
* `.fui-end`
* `.fui-field`, `.fui-field-error`
* `.fui-flush`
* `.fui-form`, `.fui-form-actions`
* `.fui-hide-desktop`
* `.fui-hint`
* `.fui-ico`, `.fui-ico-activity`, `.fui-ico-archive`, `.fui-ico-arrow-right`, `.fui-ico-box`, `.fui-ico-clock`, `.fui-ico-columns`, `.fui-ico-cpu`, `.fui-ico-database`, `.fui-ico-eye`, `.fui-ico-file`, `.fui-ico-folder`, `.fui-ico-grid`, `.fui-ico-house`, `.fui-ico-key`, `.fui-ico-layers`, `.fui-ico-list`, `.fui-ico-list-check`, `.fui-ico-logout`, `.fui-ico-message`, `.fui-ico-moon`, `.fui-ico-palette`, `.fui-ico-plus`, `.fui-ico-server`, `.fui-ico-shield`, `.fui-ico-sun`, `.fui-ico-terminal`, `.fui-ico-user`, `.fui-ico-users`
* `.fui-icon-btn`
* `.fui-input`, `.fui-input-group`, `.fui-input-invalid`, `.fui-input-sm`
* `.fui-kv`
* `.fui-label`
* `.fui-link-card`, `.fui-link-card-arrow`, `.fui-link-card-body`, `.fui-link-card-desc`, `.fui-link-card-icon`, `.fui-link-card-img`, `.fui-link-card-stack`, `.fui-link-card-title`
* `.fui-modal`, `.fui-modal-body`, `.fui-modal-footer`, `.fui-modal-header`, `.fui-modal-lg`, `.fui-modal-panel`, `.fui-modal-scrim`
* `.fui-mono`
* `.fui-muted`
* `.fui-nav`, `.fui-nav-item`, `.fui-nav-sub`, `.fui-nav-subitem`
* `.fui-notice`, `.fui-notice-error`, `.fui-notice-info`, `.fui-notice-ok`
* `.fui-open`
* `.fui-pagination`
* `.fui-progress`
* `.fui-select`, `.fui-select-sm`
* `.fui-selectable`
* `.fui-sidebar`, `.fui-sidebar-head`
* `.fui-stat`, `.fui-stat-label`, `.fui-stat-value`
* `.fui-swatch`
* `.fui-tab`
* `.fui-table`, `.fui-table-stack`, `.fui-table-wrap`
* `.fui-tabs`
* `.fui-textarea`
* `.fui-theme-label`, `.fui-theme-menu`, `.fui-theme-mode`, `.fui-theme-modes`, `.fui-theme-palettes`
* `.fui-toolbar`
* `.fui-topbar`, `.fui-topbar-actions`, `.fui-topbar-nav`, `.fui-topbar-spacer`, `.fui-topbar-title`
* `.fui-underline`
* `.has-avatar`
* `.locale-switch`
* `.primary`
* `.q-shell-app`
* `.sr-only`
