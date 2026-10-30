---
title: CSS vocabulary
parent: forge-ui
grand_parent: Libraries
nav_order: 2
---

`forge-ui` and [forge-shell](../forge-shell/css.md) share most of the `fui-*` vocabulary (buttons, badges, cards, forms, tables, alerts, modals, tabs, dropdowns, theme menu, base typography, tokens). This page lists what is specific to `forge-ui`. Source: [CSS source](css-source.md). Token reference: [tokens](../forge-shell/tokens.md).

## Differences from the forge-shell stylesheet

| In forge-ui only | In forge-shell only |
|---|---|
| `layout.css` classes below; `legacy.css` bridge; `fui-nav-section`, `fui-nav-heading`, `fui-nav-badge`, `fui-sidebar-close`, `fui-sidebar-foot` | `fui-client*`, `fui-card-grid`, `fui-link-card-stack`, `fui-link-card-img`, `fui-notice*`, `fui-empty-sm`, `fui-brand-image`, the `fui-ico*` masks, drag and drop classes |
| icons are Font Awesome (`<i class="fa-solid fa-NAME">`) | icons are CSS masks (`fui-ico`) |
| `body` has `user-select: auto` | `body` has no such declaration (selection control is in `base.css` for `fui-app`) |

(Derived by `diff` of the files; consult the source pages for exact rules.)

## Frame (`layout.css`)

| Class | Use |
|---|---|
| `fui-app` | `position: fixed; inset: 0; display: flex`: the signed-in root |
| `fui-sidebar` | left column, `--fui-sidebar-w`; below 1024px an off-canvas drawer (`transform: translateX(-102%)`, shown by `html.fui-drawer-open`) |
| `fui-sidebar-head`, `fui-sidebar-close` (hidden on desktop), `fui-sidebar-foot` | brand row, close button of the drawer, footer area with top border |
| `fui-nav`, `fui-nav-section`, `fui-nav-heading`, `fui-nav-item`, `fui-nav-sub`, `fui-nav-subitem`, `fui-nav-badge` | navigation; `aria-current="page"` marks the current item |
| `fui-main`, `fui-topbar`, `fui-topbar-title`, `fui-topbar-spacer`, `fui-topbar-actions`, `fui-topbar-nav`, `fui-menu-button` | main column and its top bar; the menu button shows below 1024px; the title is hidden below 480px |
| `fui-content` | scrolling area (`overflow-y: auto`); `overflow: hidden` when its direct child is `fui-page-fill` |
| `fui-page`, `fui-page-wide`, `fui-page-fill` | centred page (max `--fui-content-max`), no max width, full-bleed with no padding |
| `fui-scrim`, state `html.fui-drawer-open` | dimming layer behind the open drawer (`z-index: 35`) |
| `fui-page-header`, `fui-page-header-text`, `fui-page-title`, `fui-page-subtitle`, `fui-page-actions`, `fui-breadcrumbs`, `fui-crumb-current` | page heading block |
| `fui-stack` (+ `-sm`, `-lg`), `fui-row` (+ `fui-row-between`), `fui-grow`, `fui-grid` (+ `fui-grid-2`, `fui-grid-3`), `fui-split` | layout helpers; `fui-split` is a two-pane grid (one column below 1024px) |
| `fui-auth`, `fui-auth-bar`, `fui-auth-body`, `fui-auth-card` (+ `fui-auth-wide`), `fui-auth-heading` | signed-out layout (no sidebar) |
| `fui-hide-desktop`, `fui-hide-mobile` | hidden from 1024px up / hidden below 640px inside `fui-topbar-actions` |

Breakpoints: `1023.98px` (drawer, one-column splits), `639.98px` (tight paddings, stacked tables, bottom-sheet modals, `--fui-topbar-h: 3.5rem`), `479.98px` (top bar title hidden).

## Legacy bridge (`legacy.css`)

Same names as quench-starter's shared classes, forge-ui look: `panel`, `panel-title`, `page-header`, `split-view`, `split-right`, `meta-list`, `empty`, `page`, `home-content`, `home-container`, `home-header`, `home-sections`, `home-section`, `home-section-title`, `home-subtitle`, `home-grid`, `home-card` (+ `-body`, `-title`, `-desc`, `-arrow`), `login-layout`, `login-panel`, `login-bar`, `login-brand`, `login-credentials`. New code should use the components instead.

## Class index (generated)

Every class selector in `base.css`, `layout.css`, `components.css` and `legacy.css`:

* `.app`
* `.button`
* `.content`, `.content-inner`
* `.danger`
* `.delete`
* `.empty`
* `.fui-alert`, `.fui-alert-body`, `.fui-alert-error`, `.fui-alert-info`, `.fui-alert-success`, `.fui-alert-warning`
* `.fui-app`
* `.fui-auth`, `.fui-auth-bar`, `.fui-auth-body`, `.fui-auth-card`, `.fui-auth-heading`, `.fui-auth-wide`
* `.fui-avatar`
* `.fui-badge`, `.fui-badge-brand`, `.fui-badge-error`, `.fui-badge-info`, `.fui-badge-success`, `.fui-badge-warning`
* `.fui-brand`, `.fui-brand-mark`, `.fui-brand-name`
* `.fui-breadcrumbs`
* `.fui-btn`, `.fui-btn-block`, `.fui-btn-danger`, `.fui-btn-danger-soft`, `.fui-btn-lg`, `.fui-btn-link`, `.fui-btn-primary`, `.fui-btn-secondary`, `.fui-btn-sm`, `.fui-btn-tertiary`
* `.fui-card`, `.fui-card-body`, `.fui-card-desc`, `.fui-card-footer`, `.fui-card-header`, `.fui-card-title`
* `.fui-cell-actions`, `.fui-cell-mono`, `.fui-cell-strong`, `.fui-cell-wrap`
* `.fui-check`
* `.fui-code-block`
* `.fui-content`
* `.fui-crumb-current`
* `.fui-dot`
* `.fui-drawer-open`
* `.fui-dropdown`, `.fui-dropdown-item`, `.fui-dropdown-menu`, `.fui-dropdown-up`
* `.fui-empty`, `.fui-empty-icon`, `.fui-empty-title`
* `.fui-end`
* `.fui-field`, `.fui-field-error`
* `.fui-flush`
* `.fui-form`, `.fui-form-actions`
* `.fui-grid`, `.fui-grid-2`, `.fui-grid-3`
* `.fui-grow`
* `.fui-hide-desktop`, `.fui-hide-mobile`
* `.fui-hint`
* `.fui-icon-btn`
* `.fui-input`, `.fui-input-group`, `.fui-input-invalid`, `.fui-input-sm`
* `.fui-kv`
* `.fui-label`
* `.fui-link-card`, `.fui-link-card-arrow`, `.fui-link-card-body`, `.fui-link-card-desc`, `.fui-link-card-icon`, `.fui-link-card-title`
* `.fui-main`
* `.fui-menu-button`
* `.fui-modal`, `.fui-modal-body`, `.fui-modal-footer`, `.fui-modal-header`, `.fui-modal-lg`, `.fui-modal-panel`, `.fui-modal-scrim`
* `.fui-mono`
* `.fui-muted`
* `.fui-nav`, `.fui-nav-badge`, `.fui-nav-heading`, `.fui-nav-item`, `.fui-nav-section`, `.fui-nav-sub`, `.fui-nav-subitem`
* `.fui-open`
* `.fui-page`, `.fui-page-actions`, `.fui-page-fill`, `.fui-page-header`, `.fui-page-header-text`, `.fui-page-subtitle`, `.fui-page-title`, `.fui-page-wide`
* `.fui-pagination`
* `.fui-progress`
* `.fui-row`, `.fui-row-between`
* `.fui-scrim`
* `.fui-select`, `.fui-select-sm`
* `.fui-sidebar`, `.fui-sidebar-close`, `.fui-sidebar-foot`, `.fui-sidebar-head`
* `.fui-split`
* `.fui-stack`, `.fui-stack-lg`, `.fui-stack-sm`
* `.fui-stat`, `.fui-stat-label`, `.fui-stat-value`
* `.fui-swatch`
* `.fui-tab`
* `.fui-table`, `.fui-table-stack`, `.fui-table-wrap`
* `.fui-tabs`
* `.fui-textarea`
* `.fui-theme-label`, `.fui-theme-menu`, `.fui-theme-mode`, `.fui-theme-modes`, `.fui-theme-palettes`
* `.fui-toolbar`
* `.fui-topbar`, `.fui-topbar-actions`, `.fui-topbar-nav`, `.fui-topbar-spacer`, `.fui-topbar-title`
* `.has-avatar`
* `.home-card`, `.home-card-arrow`, `.home-card-body`, `.home-card-desc`, `.home-card-title`, `.home-container`, `.home-content`, `.home-grid`, `.home-header`, `.home-section`, `.home-section-title`, `.home-sections`, `.home-subtitle`
* `.locale-switch`
* `.login-bar`, `.login-brand`, `.login-credentials`, `.login-layout`, `.login-panel`
* `.meta-list`
* `.page`, `.page-header`
* `.panel`, `.panel-title`
* `.primary`
* `.q-shell-app`
* `.split-right`, `.split-view`
* `.sr-only`
