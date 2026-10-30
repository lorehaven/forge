---
title: CSS source
parent: forge-shell
grand_parent: Libraries
nav_order: 5
---

The stylesheet `forge_shell::STYLE` verbatim, in cascade order: `tokens.css` (see [tokens](tokens.md)), `base.css`, `components.css`, `shell.css`. Embedded `data:` SVG URIs of the icon masks are abbreviated to `url("data:image/svg+xml,…")`; everything else is exact. Comments are the authors' own.

## base.css

```css
/* forge-ui base - reset, typography and the quench overrides every service needs. */

*, *::before, *::after { box-sizing: border-box; }

html { -webkit-text-size-adjust: 100%; text-size-adjust: 100%; }
html, body { background: var(--fui-bg); height: 100%; }
body { margin: 0; font-family: var(--fui-font); font-size: 0.875rem; line-height: 1.43; color: var(--fui-text); -webkit-font-smoothing: antialiased; }

/* quench paints every element with its shell colour and font; take text colour back to inherit */
.fui-app *, .fui-auth * { font-family: inherit; color: inherit; }
.fui-app, .fui-auth { font-family: var(--fui-font); color: var(--fui-text); }
.fui-app i[class*="fa-"], .fui-auth i[class*="fa-"] { font-family: "Font Awesome 6 Free", "Font Awesome 6 Brands"; }

.fui-app h1, .fui-app h2, .fui-app h3, .fui-app h4 { margin: 0; color: var(--fui-text); font-weight: 600; letter-spacing: -0.01em; }
.fui-app h1 { font-size: 1.875rem; line-height: 2.375rem; }
.fui-app h2 { font-size: 1.5rem; line-height: 2rem; }
.fui-app h3 { font-size: 1.125rem; line-height: 1.75rem; font-weight: 600; }
.fui-app h4 { font-size: 1rem; line-height: 1.5rem; }
/* :where keeps this reset below any class a service gives a paragraph (a margin on `.cv-hint` must win) */
:where(.fui-app) p { margin: 0; }
.fui-app a { color: var(--fui-link); text-decoration: none; }
/* no link underlines on hover anywhere (cards, titles, rows, tabs, nav): the hover shows in the colour; .fui-underline opts in */
.fui-app a:hover { text-decoration: none; }
.fui-app a.fui-underline:hover { text-decoration: underline; }

/* text is not selected by accident while tapping or dragging; what people copy opts back in with .fui-selectable (inputs, code, logs, errors are selectable by default) */
:where(html, body, .fui-app, .fui-auth) { -webkit-user-select: none; user-select: none; }
.fui-app input, .fui-app textarea, .fui-app [contenteditable], .fui-app [contenteditable] *, .fui-app pre, .fui-app code, .fui-app samp, .fui-app kbd,
.fui-app .fui-selectable, .fui-app .fui-selectable *, .fui-app .fui-code-block, .fui-app .fui-alert, .fui-app .fui-notice, .fui-app .fui-field-error,
.fui-auth input, .fui-auth textarea { -webkit-user-select: text; user-select: text; }
.fui-app code, .fui-app pre, .fui-mono { font-family: var(--fui-mono); font-size: 0.8125rem; }
.fui-app hr { border: 0; border-top: 1px solid var(--fui-border); margin: 1rem 0; width: 100%; }

.fui-app :focus-visible, .fui-auth :focus-visible { outline: 2px solid var(--fui-brand-500); outline-offset: 2px; }

/* quench's global form rule (column flex, gap, full width) stays for plain forms; .fui-form sets its own gap */
.fui-app form.fui-form { gap: 1.25rem; }

.sr-only { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; border: 0; }

/* quench shell chrome is not used; neutralise whatever wrapper it still emits */
.app.q-shell-app { display: block; height: 100dvh; width: 100%; min-width: 0; overflow: hidden; background: var(--fui-bg); }
.app > .content, .app > .content > .content-inner { padding: 0; margin: 0; display: block; min-height: 0; height: 100%; width: 100%; overflow: visible; background: transparent; }

* { scrollbar-width: thin; scrollbar-color: var(--fui-border-strong) transparent; }
*::-webkit-scrollbar { width: 0.5rem; height: 0.5rem; }
*::-webkit-scrollbar-track { background: transparent; }
*::-webkit-scrollbar-thumb { background-color: var(--fui-border-strong); border-radius: 0.5rem; border: 0; }

@media (prefers-reduced-motion: reduce) { * { transition: none !important; animation: none !important; } }

/* ---- compat for markup that still uses quench's bare elements (htmx fragments, `.button`) ----
   :where() keeps these at the lowest specificity so every .fui-* class and service rule beats them. */
:where(.fui-app, .fui-auth) button, :where(.fui-app, .fui-auth) a.button { display: inline-flex; align-items: center; justify-content: center; gap: 0.5rem; min-height: 2.5rem; padding: 0.5rem 1rem; border: 1px solid var(--fui-border-strong); border-radius: var(--fui-radius-md); background: var(--fui-surface); color: var(--fui-text-secondary); font: inherit; font-size: 0.875rem; font-weight: 600; line-height: 1.25rem; text-decoration: none; cursor: pointer; box-shadow: var(--fui-shadow-xs); transition: background-color .15s, border-color .15s; }
:where(.fui-app, .fui-auth) button:hover, :where(.fui-app, .fui-auth) a.button:hover { background: var(--fui-surface-hover); color: var(--fui-text); }
:where(.fui-app, .fui-auth) button:disabled { opacity: .5; cursor: not-allowed; }
:where(.fui-app, .fui-auth) .button.primary, :where(.fui-app, .fui-auth) button.primary { background: var(--fui-brand-solid); border-color: var(--fui-brand-solid); color: #fff; }
:where(.fui-app, .fui-auth) .button.primary:hover, :where(.fui-app, .fui-auth) button.primary:hover { background: var(--fui-brand-solid-hover); color: #fff; }
:where(.fui-app, .fui-auth) .button.delete, :where(.fui-app, .fui-auth) .button.danger, :where(.fui-app, .fui-auth) button.danger { background: var(--fui-error-600); border-color: var(--fui-error-600); color: #fff; }
:where(.fui-app, .fui-auth) .button.delete:hover, :where(.fui-app, .fui-auth) .button.danger:hover, :where(.fui-app, .fui-auth) button.danger:hover { background: var(--fui-error-700); color: #fff; }
:where(.fui-app, .fui-auth) input:not([type="checkbox"]):not([type="radio"]):not([type="file"]):not([type="range"]), :where(.fui-app, .fui-auth) select, :where(.fui-app, .fui-auth) textarea { min-height: 2.5rem; padding: 0.5rem 0.75rem; border: 1px solid var(--fui-border-strong); border-radius: var(--fui-radius-md); background-color: var(--fui-surface); color: var(--fui-text); font: inherit; font-size: 0.9375rem; box-shadow: var(--fui-shadow-xs); }
:where(.fui-app, .fui-auth) textarea { min-height: 5rem; }
:where(.fui-app, .fui-auth) input:focus, :where(.fui-app, .fui-auth) select:focus, :where(.fui-app, .fui-auth) textarea:focus { outline: none; border-color: var(--fui-brand-400); box-shadow: 0 0 0 4px var(--fui-focus); }
:where(.fui-app, .fui-auth) option { background: var(--fui-surface); color: var(--fui-text); }

```

## components.css

```css
/* forge-ui components - Untitled UI buttons, badges, cards, forms, tables, alerts, modals, tabs. */

/* ================= buttons ================= */
.fui-btn { display: inline-flex; align-items: center; justify-content: center; gap: 0.5rem; min-height: 2.5rem; padding: 0.5rem 1rem; border: 1px solid transparent; border-radius: var(--fui-radius-md); font: inherit; font-size: 0.875rem; font-weight: 600; line-height: 1.25rem; cursor: pointer; text-decoration: none !important; white-space: nowrap; box-shadow: var(--fui-shadow-xs); transition: background-color .15s, border-color .15s, box-shadow .15s, color .15s; -webkit-tap-highlight-color: transparent; }
.fui-btn:focus-visible { outline: none; box-shadow: 0 0 0 4px var(--fui-focus); }
.fui-btn[disabled], .fui-btn[aria-disabled="true"] { opacity: .5; cursor: not-allowed; pointer-events: none; }
.fui-btn i { color: inherit !important; font-size: 0.95em; }
.fui-btn-sm { min-height: 2.25rem; padding: 0.4rem 0.75rem; }
.fui-btn-lg { min-height: 2.75rem; padding: 0.625rem 1.125rem; font-size: 1rem; }
.fui-btn-block { width: 100%; }

.fui-btn-primary { background: var(--fui-brand-solid); color: #fff !important; border-color: var(--fui-brand-solid); }
.fui-btn-primary:hover { background: var(--fui-brand-solid-hover); border-color: var(--fui-brand-solid-hover); }
.fui-btn-secondary { background: var(--fui-surface); color: var(--fui-text-secondary) !important; border-color: var(--fui-border-strong); }
.fui-btn-secondary:hover { background: var(--fui-surface-hover); color: var(--fui-text) !important; }
.fui-btn-tertiary { background: transparent; color: var(--fui-text-tertiary) !important; box-shadow: none; }
.fui-btn-tertiary:hover { background: var(--fui-surface-hover); color: var(--fui-text) !important; }
.fui-btn-link { background: transparent; color: var(--fui-link) !important; box-shadow: none; padding-left: 0.25rem; padding-right: 0.25rem; min-height: 0; }
.fui-btn-link:hover { text-decoration: underline !important; }
.fui-btn-danger { background: var(--fui-error-600); color: #fff !important; border-color: var(--fui-error-600); }
.fui-btn-danger:hover { background: var(--fui-error-700); border-color: var(--fui-error-700); }
.fui-btn-danger:focus-visible { box-shadow: 0 0 0 4px rgba(240, 68, 56, .3); }
.fui-btn-danger-soft { background: var(--fui-surface); color: var(--fui-error-soft-text) !important; border-color: var(--fui-error-soft-border); }
.fui-btn-danger-soft:hover { background: var(--fui-error-soft-bg); }

.fui-icon-btn { display: inline-flex; align-items: center; justify-content: center; width: 2.5rem; height: 2.5rem; padding: 0; border: 0; border-radius: var(--fui-radius-md); background: transparent; color: var(--fui-text-tertiary) !important; cursor: pointer; font-size: 1.05rem; transition: background-color .15s, color .15s; list-style: none; text-decoration: none !important; -webkit-tap-highlight-color: transparent; }
.fui-icon-btn i { color: inherit !important; }
.fui-icon-btn:hover { background: var(--fui-surface-hover); color: var(--fui-text) !important; }
.fui-icon-btn::-webkit-details-marker { display: none; }

/* ================= badges ================= */
.fui-badge { display: inline-flex; align-items: center; gap: 0.375rem; padding: 0.125rem 0.625rem; border-radius: 999px; border: 1px solid var(--fui-gray-soft-border); background: var(--fui-gray-soft-bg); color: var(--fui-gray-soft-text) !important; font-size: 0.75rem; font-weight: 500; line-height: 1.125rem; white-space: nowrap; }
.fui-badge i { font-size: 0.7em; color: inherit !important; }
.fui-badge-brand { background: var(--fui-brand-soft-bg); color: var(--fui-brand-soft-text) !important; border-color: transparent; }
.fui-badge-success { background: var(--fui-success-soft-bg); color: var(--fui-success-soft-text) !important; border-color: var(--fui-success-soft-border); }
.fui-badge-warning { background: var(--fui-warning-soft-bg); color: var(--fui-warning-soft-text) !important; border-color: var(--fui-warning-soft-border); }
.fui-badge-error { background: var(--fui-error-soft-bg); color: var(--fui-error-soft-text) !important; border-color: var(--fui-error-soft-border); }
.fui-badge-info { background: var(--fui-blue-soft-bg); color: var(--fui-blue-soft-text) !important; border-color: var(--fui-blue-soft-border); }
.fui-dot { width: 0.5rem; height: 0.5rem; border-radius: 50%; background: currentColor; flex: 0 0 auto; }

/* ================= cards ================= */
.fui-card { display: flex; flex-direction: column; min-width: 0; background: var(--fui-surface); border: 1px solid var(--fui-border); border-radius: var(--fui-radius-xl); box-shadow: var(--fui-shadow-xs); overflow: hidden; }
.fui-card-header { display: flex; align-items: center; justify-content: space-between; gap: 0.75rem; flex-wrap: wrap; padding: 1.125rem 1.5rem; border-bottom: 1px solid var(--fui-border); }
.fui-card-title { font-size: 1rem !important; line-height: 1.5rem; font-weight: 600; margin: 0; }
.fui-card-desc { color: var(--fui-text-tertiary); font-size: 0.875rem; }
.fui-card-body { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; min-width: 0; }
.fui-card-body.fui-flush { padding: 0; gap: 0; }
.fui-card-footer { display: flex; align-items: center; justify-content: flex-end; gap: 0.75rem; flex-wrap: wrap; padding: 1rem 1.5rem; border-top: 1px solid var(--fui-border); }

.fui-link-card { display: flex; align-items: center; gap: 1rem; padding: 1.25rem 1.5rem; background: var(--fui-surface); border: 1px solid var(--fui-border); border-radius: var(--fui-radius-xl); box-shadow: var(--fui-shadow-xs); color: inherit !important; text-decoration: none !important; transition: border-color .15s, box-shadow .15s, background-color .15s; min-width: 0; }
.fui-link-card:hover { border-color: var(--fui-brand-400); box-shadow: var(--fui-shadow-md); }
.fui-link-card-stack { flex-direction: column; align-items: stretch; gap: 0.5rem; }
.fui-link-card-icon { flex: 0 0 auto; display: inline-flex; align-items: center; justify-content: center; width: 2.75rem; height: 2.75rem; border-radius: var(--fui-radius-lg); background: var(--fui-brand-soft-bg); color: var(--fui-brand-soft-text); font-size: 1.125rem; }
.fui-link-card-icon i { color: inherit !important; }
.fui-link-card-img { flex: 0 0 auto; display: block; width: 2.75rem; height: 2.75rem; border-radius: 0.625rem; }
.fui-link-card-body { flex: 1 1 auto; min-width: 0; display: flex; flex-direction: column; gap: 0.25rem; }
.fui-link-card-title { font-weight: 600; font-size: 1rem; color: var(--fui-text); overflow-wrap: anywhere; }
.fui-link-card-desc { color: var(--fui-text-tertiary); font-size: 0.875rem; overflow-wrap: anywhere; }
.fui-link-card-arrow { flex: 0 0 auto; display: flex; color: var(--fui-text-tertiary) !important; }
.fui-link-card-arrow .fui-ico { width: 1.75rem; height: 1.75rem; }
.fui-link-card:hover .fui-link-card-arrow { color: var(--fui-brand-soft-text) !important; }
/* the grid every card page uses */
.fui-card-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(min(100%, 20rem), 1fr)); gap: 1.25rem; }

.fui-stat { display: flex; flex-direction: column; gap: 0.5rem; padding: 1.25rem 1.5rem; background: var(--fui-surface); border: 1px solid var(--fui-border); border-radius: var(--fui-radius-xl); box-shadow: var(--fui-shadow-xs); }
.fui-stat-label { color: var(--fui-text-tertiary); font-size: 0.875rem; font-weight: 500; }
.fui-stat-value { font-size: 1.875rem; line-height: 2.375rem; font-weight: 600; color: var(--fui-text); }

/* ================= forms ================= */
.fui-form { display: flex; flex-direction: column; gap: 1.25rem; width: 100%; }
.fui-field { display: flex; flex-direction: column; gap: 0.375rem; min-width: 0; }
.fui-label { font-size: 0.875rem; font-weight: 500; color: var(--fui-text-secondary); margin: 0; }
.fui-hint { font-size: 0.8125rem; color: var(--fui-text-tertiary); }
.fui-field-error { font-size: 0.8125rem; color: var(--fui-error-soft-text); }
.fui-input, .fui-select, .fui-textarea, .fui-app input[type="text"].fui-input, .fui-app input[type="password"].fui-input, .fui-app input[type="email"].fui-input, .fui-app input[type="search"].fui-input { width: 100%; min-height: 2.75rem; padding: 0.625rem 0.875rem; border: 1px solid var(--fui-border-strong); border-radius: var(--fui-radius-md); background: var(--fui-surface); color: var(--fui-text) !important; font: inherit; font-size: 1rem; line-height: 1.5rem; box-shadow: var(--fui-shadow-xs); transition: border-color .15s, box-shadow .15s; appearance: none; -webkit-appearance: none; }
.fui-textarea { min-height: 6rem; resize: vertical; }
.fui-select { padding-right: 2.5rem; background-image: url("data:image/svg+xml,…"); background-repeat: no-repeat; background-position: right 0.75rem center; background-size: 1.25rem; }
.fui-input::placeholder, .fui-textarea::placeholder { color: var(--fui-text-tertiary); }
.fui-input:focus, .fui-select:focus, .fui-textarea:focus { outline: none; border-color: var(--fui-brand-400); box-shadow: 0 0 0 4px var(--fui-focus); }
.fui-input[disabled], .fui-select[disabled], .fui-textarea[disabled] { background: var(--fui-surface-2); color: var(--fui-text-disabled) !important; cursor: not-allowed; }
.fui-input-invalid { border-color: var(--fui-error-500); }
.fui-input-sm, .fui-select-sm { min-height: 2.25rem; padding: 0.375rem 0.75rem; font-size: 0.875rem; }
.fui-select-sm { padding-right: 2.25rem; }
.fui-input-group { position: relative; display: flex; align-items: center; min-width: 0; }
.fui-input-group > i { position: absolute; left: 0.875rem; color: var(--fui-text-tertiary) !important; pointer-events: none; }
.fui-app .fui-input-group > input.fui-input[type] { padding-left: 2.5rem; }
.fui-check { display: inline-flex; align-items: center; gap: 0.625rem; min-height: 1.5rem; cursor: pointer; font-size: 0.9375rem; color: var(--fui-text-secondary); }
.fui-check input { width: 1.125rem; height: 1.125rem; margin: 0; accent-color: var(--fui-brand-600); flex: 0 0 auto; }
.fui-form-actions { display: flex; gap: 0.75rem; flex-wrap: wrap; }
.fui-form-actions.fui-end { justify-content: flex-end; }
.fui-toolbar { display: flex; align-items: center; gap: 0.75rem; flex-wrap: wrap; }
.fui-toolbar .fui-input-group { flex: 1 1 14rem; max-width: 24rem; }

/* ================= tables ================= */
.fui-table-wrap { width: 100%; overflow-x: auto; -webkit-overflow-scrolling: touch; }
.fui-table { width: 100%; border-collapse: separate; border-spacing: 0; font-size: 0.875rem; }
.fui-table th { text-align: left; padding: 0.75rem 1.5rem; background: var(--fui-surface-2); color: var(--fui-text-tertiary) !important; font-size: 0.75rem; font-weight: 500; line-height: 1.125rem; border-bottom: 1px solid var(--fui-border); white-space: nowrap; }
.fui-table td { padding: 1rem 1.5rem; border-bottom: 1px solid var(--fui-border); color: var(--fui-text-secondary); vertical-align: middle; }
.fui-table tbody tr:last-child td { border-bottom: 0; }
.fui-table tbody tr:hover td { background: var(--fui-surface-hover); }
.fui-table td.fui-cell-strong, .fui-table td.fui-cell-strong a { color: var(--fui-text) !important; font-weight: 500; }
.fui-table td.fui-cell-actions { text-align: right; white-space: nowrap; }
.fui-table .fui-cell-wrap { white-space: normal; overflow-wrap: anywhere; }
.fui-table .fui-cell-mono { font-family: var(--fui-mono); font-size: 0.8125rem; }
@media (max-width: 639.98px) {
  /* stacked rows: opt in with .fui-table-stack and data-label on each td */
  .fui-table-stack thead { display: none; }
  .fui-table-stack, .fui-table-stack tbody, .fui-table-stack tr, .fui-table-stack td { display: block; width: 100%; }
  .fui-table-stack tr { padding: 0.75rem 1rem; border-bottom: 1px solid var(--fui-border); }
  .fui-table-stack tbody tr:last-child { border-bottom: 0; }
  .fui-table-stack td { display: flex; justify-content: space-between; gap: 1rem; padding: 0.375rem 0; border: 0; text-align: right; }
  .fui-table-stack td::before { content: attr(data-label); color: var(--fui-text-tertiary); font-weight: 500; text-align: left; flex: 0 0 auto; }
  .fui-table-stack td.fui-cell-actions { justify-content: flex-end; }
  .fui-table-stack td:not([data-label])::before { content: none; }
  .fui-table th, .fui-table td { padding-left: 1rem; padding-right: 1rem; }
}

/* key/value list */
.fui-kv { display: grid; grid-template-columns: minmax(7rem, 14rem) minmax(0, 1fr); gap: 0.75rem 1.5rem; margin: 0; }
.fui-kv dt { color: var(--fui-text-tertiary); font-weight: 500; }
.fui-kv dd { margin: 0; color: var(--fui-text); overflow-wrap: anywhere; }
@media (max-width: 639.98px) { .fui-kv { grid-template-columns: 1fr; gap: 0.125rem; } .fui-kv dd { margin-bottom: 0.75rem; } }

/* ================= alerts / empty ================= */
.fui-alert { display: flex; align-items: flex-start; gap: 0.75rem; padding: 0.875rem 1rem; border-radius: var(--fui-radius-lg); border: 1px solid var(--fui-gray-soft-border); background: var(--fui-gray-soft-bg); color: var(--fui-gray-soft-text); font-size: 0.875rem; }
.fui-alert > i { margin-top: 0.15rem; color: inherit !important; }
.fui-alert-body { min-width: 0; overflow-wrap: anywhere; }
.fui-alert-success { background: var(--fui-success-soft-bg); border-color: var(--fui-success-soft-border); color: var(--fui-success-soft-text); }
.fui-alert-warning { background: var(--fui-warning-soft-bg); border-color: var(--fui-warning-soft-border); color: var(--fui-warning-soft-text); }
.fui-alert-error { background: var(--fui-error-soft-bg); border-color: var(--fui-error-soft-border); color: var(--fui-error-soft-text); }
.fui-alert-info { background: var(--fui-blue-soft-bg); border-color: var(--fui-blue-soft-border); color: var(--fui-blue-soft-text); }
.fui-alert * { color: inherit; }

.fui-app .fui-empty, .fui-empty { display: flex; flex-direction: column; align-items: center; text-align: center; gap: 0.75rem; padding: 3rem 1.5rem; color: var(--fui-text-tertiary); }
/* a line that says there is nothing (here), a loading line, a "not found": padded, never flush against its neighbours (use on a p or div) */
.fui-app .fui-empty { margin: 0; color: var(--fui-text-tertiary); }
.fui-app .fui-empty-sm { padding: 1.25rem 1rem; }
/* a banner above the content: .fui-notice-error / -ok / -info (default neutral) */
.fui-app .fui-notice { display: block; margin: 0 0 1rem; padding: 0.75rem 1rem; border-radius: var(--fui-radius-lg); border: 1px solid var(--fui-gray-soft-border); background: var(--fui-gray-soft-bg); font-size: 0.875rem; overflow-wrap: anywhere; }
.fui-app .fui-notice-error { background: var(--fui-error-soft-bg); border-color: var(--fui-error-soft-border); color: var(--fui-error-soft-text); }
.fui-app .fui-notice-ok { background: var(--fui-success-soft-bg); border-color: var(--fui-success-soft-border); color: var(--fui-success-soft-text); }
.fui-app .fui-notice-info { background: var(--fui-blue-soft-bg); border-color: var(--fui-blue-soft-border); color: var(--fui-blue-soft-text); }
.fui-app .fui-notice a { text-decoration: underline; }
.fui-empty-icon { display: inline-flex; align-items: center; justify-content: center; width: 3rem; height: 3rem; border-radius: 50%; background: var(--fui-surface-2); border: 1px solid var(--fui-border); font-size: 1.25rem; }
.fui-empty-title { color: var(--fui-text); font-weight: 600; font-size: 1rem; }

/* ================= modal ================= */
.fui-modal { position: fixed; inset: 0; z-index: 100; display: none; align-items: center; justify-content: center; padding: 1rem; }
.fui-modal.fui-open { display: flex; }
.fui-modal-scrim { position: absolute; inset: 0; background: var(--fui-scrim); }
.fui-modal-panel { position: relative; width: 100%; max-width: 32rem; max-height: calc(100dvh - 2rem); display: flex; flex-direction: column; background: var(--fui-surface); border: 1px solid var(--fui-border); border-radius: var(--fui-radius-xl); box-shadow: var(--fui-shadow-xl); overflow: hidden; }
.fui-modal-panel.fui-modal-lg { max-width: 48rem; }
.fui-modal-header { display: flex; align-items: flex-start; justify-content: space-between; gap: 1rem; padding: 1.25rem 1.5rem 0.75rem; }
.fui-modal-body { padding: 0.5rem 1.5rem 1.25rem; overflow-y: auto; }
.fui-modal-footer { display: flex; gap: 0.75rem; justify-content: flex-end; padding: 1rem 1.5rem; border-top: 1px solid var(--fui-border); }
@media (max-width: 639.98px) {
  .fui-modal { align-items: flex-end; padding: 0; }
  .fui-modal-panel, .fui-modal-panel.fui-modal-lg { max-width: none; border-radius: var(--fui-radius-2xl) var(--fui-radius-2xl) 0 0; max-height: 92dvh; }
  .fui-modal-footer { flex-direction: column-reverse; } .fui-modal-footer > .fui-btn { width: 100%; }
}

/* ================= tabs ================= */
.fui-tabs { display: flex; gap: 0.25rem; border-bottom: 1px solid var(--fui-border); overflow-x: auto; scrollbar-width: none; }
.fui-tabs::-webkit-scrollbar { display: none; }
.fui-tab { padding: 0.625rem 1rem; font-weight: 600; font-size: 0.9375rem; color: var(--fui-text-tertiary) !important; border-bottom: 2px solid transparent; margin-bottom: -1px; white-space: nowrap; text-decoration: none !important; background: transparent; border-top: 0; border-left: 0; border-right: 0; cursor: pointer; font-family: inherit; }
.fui-tab:hover { color: var(--fui-text) !important; }
.fui-tab[aria-current="page"], .fui-tab[aria-selected="true"] { color: var(--fui-brand-soft-text) !important; border-bottom-color: var(--fui-brand-500); }

/* ================= dropdown / user menu (details-based) ================= */
.fui-dropdown { position: relative; }
.fui-dropdown > summary { list-style: none; cursor: pointer; }
.fui-dropdown > summary::-webkit-details-marker { display: none; }
.fui-dropdown-menu { position: absolute; right: 0; top: calc(100% + 0.5rem); z-index: 60; min-width: 13rem; padding: 0.25rem; display: flex; flex-direction: column; background: var(--fui-surface); border: 1px solid var(--fui-border); border-radius: var(--fui-radius-lg); box-shadow: var(--fui-shadow-md); }
.fui-dropdown-up .fui-dropdown-menu { top: auto; bottom: calc(100% + 0.5rem); left: 0; right: auto; }
.fui-dropdown-item { display: flex; align-items: center; gap: 0.75rem; min-height: 2.5rem; padding: 0.5rem 0.75rem; border-radius: var(--fui-radius-sm); color: var(--fui-text-secondary) !important; font-weight: 500; text-decoration: none !important; background: transparent; border: 0; font: inherit; cursor: pointer; text-align: left; width: 100%; }
.fui-dropdown-item i { width: 1.1rem; text-align: center; color: var(--fui-text-tertiary) !important; }
.fui-dropdown-item:hover { background: var(--fui-surface-hover); color: var(--fui-text) !important; }

.fui-avatar { display: inline-flex; align-items: center; justify-content: center; width: 2.5rem; height: 2.5rem; border-radius: 50%; overflow: hidden; background: var(--fui-brand-soft-bg); color: var(--fui-brand-soft-text); border: 1px solid var(--fui-border); font-size: 1rem; position: relative; flex: 0 0 auto; }
.fui-avatar i { color: inherit !important; }
.fui-avatar img { display: none; position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
.fui-avatar.has-avatar img { display: block; }
.fui-avatar.has-avatar i { display: none; }

/* quench's locale switch, restyled */
.fui-app .locale-switch, .fui-auth .locale-switch { display: inline-flex; align-items: center; }
.fui-app .locale-switch select, .fui-auth .locale-switch select { min-height: 2.25rem; padding: 0.25rem 2rem 0.25rem 0.625rem; border: 1px solid var(--fui-border-strong); border-radius: var(--fui-radius-md); background-color: var(--fui-surface); color: var(--fui-text-secondary) !important; font: inherit; font-size: 0.8125rem; font-weight: 500; box-shadow: var(--fui-shadow-xs); width: auto; }

/* misc */
.fui-muted { color: var(--fui-text-tertiary); }
.fui-code-block { margin: 0; padding: 1rem; background: var(--fui-bg-sunken); border: 1px solid var(--fui-border); border-radius: var(--fui-radius-md); overflow-x: auto; white-space: pre; color: var(--fui-text-secondary); }
.fui-progress { height: 0.5rem; border-radius: 999px; background: var(--fui-surface-2); overflow: hidden; }
.fui-progress > span { display: block; height: 100%; background: var(--fui-brand-500); border-radius: inherit; }
.fui-pagination { display: flex; align-items: center; justify-content: space-between; gap: 0.75rem; padding: 0.75rem 1.5rem; border-top: 1px solid var(--fui-border); }

/* theme menu: mode (light/dark) and palette swatches */
.fui-theme-menu { min-width: 15rem; padding: .75rem; gap: .75rem; }
.fui-theme-label { font-size: .75rem; font-weight: 600; color: var(--fui-text-tertiary); margin-bottom: .375rem; }
.fui-theme-modes { display: grid; grid-template-columns: 1fr 1fr; gap: .5rem; }
.fui-theme-mode { display: inline-flex; align-items: center; justify-content: center; gap: .5rem; min-height: 2.5rem; padding: .375rem .75rem; border: 1px solid var(--fui-border-strong); border-radius: var(--fui-radius-md); background: var(--fui-surface); color: var(--fui-text-secondary) !important; font: inherit; font-size: .875rem; font-weight: 600; cursor: pointer; box-shadow: none; }
.fui-theme-mode:hover { background: var(--fui-surface-hover); }
html[data-fui-theme="dark"] .fui-theme-mode[data-mode="dark"], html[data-fui-theme="light"] .fui-theme-mode[data-mode="light"] { border-color: var(--fui-brand-500); background: var(--fui-brand-soft-bg); color: var(--fui-brand-soft-text) !important; }
.fui-theme-palettes { display: flex; flex-wrap: wrap; gap: .625rem; }
.fui-swatch { width: 2rem; height: 2rem; padding: 0; border-radius: 50%; border: 2px solid transparent; background: var(--sw); cursor: pointer; box-shadow: inset 0 0 0 2px var(--fui-surface); min-height: 0; }
.fui-swatch:hover { background: var(--sw); transform: scale(1.08); }
.fui-swatch[data-palette="green"] { --sw: #17b26a; } .fui-swatch[data-palette="teal"] { --sw: #15b79e; } .fui-swatch[data-palette="blue"] { --sw: #2e90fa; }
.fui-swatch[data-palette="purple"] { --sw: #9e77ed; } .fui-swatch[data-palette="rose"] { --sw: #f63d68; } .fui-swatch[data-palette="orange"] { --sw: #fb6514; }
html[data-fui-palette="green"] .fui-swatch[data-palette="green"], html[data-fui-palette="teal"] .fui-swatch[data-palette="teal"], html[data-fui-palette="blue"] .fui-swatch[data-palette="blue"], html[data-fui-palette="purple"] .fui-swatch[data-palette="purple"], html[data-fui-palette="rose"] .fui-swatch[data-palette="rose"], html[data-fui-palette="orange"] .fui-swatch[data-palette="orange"] { border-color: var(--fui-text); }
.fui-topbar-actions .fui-avatar { cursor: pointer; width: 2.25rem; height: 2.25rem; }

```

## shell.css

```css
/* forge-shell: the top bar of the browser clients, and the icons (masks: the page's policy allows no font from elsewhere). */
.fui-client { min-height: 100vh; display: flex; flex-direction: row; background: var(--fui-bg); }
.fui-client-col { flex: 1 1 auto; min-width: 0; display: flex; flex-direction: column; }
.fui-sidebar { flex: 0 0 var(--fui-sidebar-w); width: var(--fui-sidebar-w); position: sticky; top: 0; height: 100vh; display: flex; flex-direction: column; background: var(--fui-surface); border-right: 1px solid var(--fui-border); min-height: 0; }
.fui-sidebar-head { display: flex; align-items: center; gap: 0.75rem; height: var(--fui-topbar-h); padding: 0 1.25rem; flex: 0 0 auto; }
.fui-nav { flex: 1 1 auto; overflow-y: auto; padding: 0.5rem 1rem 1rem; display: flex; flex-direction: column; gap: 0.25rem; }
.fui-nav:empty { display: none; }
.fui-nav-item { display: flex; align-items: center; gap: 0.75rem; min-height: 2.5rem; padding: 0.5rem 0.75rem; border-radius: var(--fui-radius-md); color: var(--fui-text-secondary) !important; font-weight: 600; font-size: 0.9375rem; text-decoration: none !important; transition: background-color .15s, color .15s; }
.fui-nav-item:hover { background: var(--fui-surface-hover); color: var(--fui-text) !important; }
.fui-nav-item[aria-current="page"] { background: var(--fui-surface-2); color: var(--fui-text) !important; }
.fui-nav-sub { display: flex; flex-direction: column; gap: 0.125rem; margin: 0 0 0.25rem 1.75rem; padding-left: 0.5rem; border-left: 1px solid var(--fui-border); }
.fui-nav-subitem { display: block; padding: 0.375rem 0.75rem; border-radius: var(--fui-radius-md); color: var(--fui-text-tertiary) !important; font-size: 0.875rem; font-weight: 500; text-decoration: none !important; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; transition: background-color .15s, color .15s; }
.fui-nav-subitem:hover { background: var(--fui-surface-hover); color: var(--fui-text) !important; }
.fui-nav-subitem[aria-current="page"] { background: var(--fui-surface-2); color: var(--fui-text) !important; }
.fui-brand-image { background: transparent; box-shadow: none; }
.fui-brand-image img { width: 2rem; height: 2rem; border-radius: var(--fui-radius-md); display: block; }
@media (min-width: 1024px) { .fui-hide-desktop { display: none !important; } }
@media (max-width: 1023.98px) {
  .fui-client { flex-direction: column; }
  .fui-sidebar { position: static; width: auto; height: auto; flex: 0 0 auto; border-right: 0; border-bottom: 1px solid var(--fui-border); }
  .fui-sidebar-head { display: none; }
}
.fui-topbar { flex: 0 0 auto; display: flex; align-items: center; gap: 0.75rem; height: var(--fui-topbar-h); padding: 0 1.5rem; background: var(--fui-bg); border-bottom: 1px solid var(--fui-border); position: relative; z-index: 30; }
.fui-topbar-title { font-size: 1rem; font-weight: 600; color: var(--fui-text); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; margin: 0; }
.fui-topbar-spacer { flex: 1 1 auto; }
.fui-topbar-actions { display: flex; align-items: center; gap: 0.5rem; }
.fui-topbar-nav { display: flex; align-items: center; gap: 0.25rem; }
.fui-brand { display: inline-flex; align-items: center; gap: 0.625rem; min-width: 0; font-size: 1.0625rem; font-weight: 600; color: var(--fui-text) !important; text-decoration: none !important; }
.fui-brand-mark { flex: 0 0 auto; display: inline-flex; align-items: center; justify-content: center; width: 2rem; height: 2rem; border-radius: var(--fui-radius-md); background: var(--fui-brand-solid); color: #fff !important; font-size: 0.95rem; box-shadow: inset 0 0 0 1px rgba(255,255,255,0.12), var(--fui-shadow-xs); }
.fui-brand-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.fui-client-main { flex: 1 1 auto; min-height: 0; }
@media (max-width: 639.98px) { :root { --fui-topbar-h: 3.5rem; } .fui-topbar { padding: 0 1rem; } .fui-topbar-nav { display: none; } }

[data-drag-id] { cursor: grab; }
.fui-dragging { opacity: .5; }
.fui-drop-target { outline: 2px dashed var(--fui-brand-500); outline-offset: -2px; }

.fui-ico { display: inline-block; width: 1.15em; height: 1.15em; background-color: currentColor; -webkit-mask: var(--ico) center / contain no-repeat; mask: var(--ico) center / contain no-repeat; }
.fui-brand-mark .fui-ico { color: #fff; width: 1.2rem; height: 1.2rem; }
.fui-icon-btn .fui-ico, .fui-dropdown-item .fui-ico, .fui-avatar .fui-ico { color: inherit; }
.fui-dropdown-item .fui-ico { margin-right: 0; color: var(--fui-text-tertiary); }
.fui-avatar img { display: none; }
.fui-ico-grid { --ico: url("data:image/svg+xml,…"); }
.fui-ico-house { --ico: url("data:image/svg+xml,…"); }
.fui-ico-palette { --ico: url("data:image/svg+xml,…"); }
.fui-ico-sun { --ico: url("data:image/svg+xml,…"); }
.fui-ico-moon { --ico: url("data:image/svg+xml,…"); }
.fui-ico-user { --ico: url("data:image/svg+xml,…"); }
.fui-ico-logout { --ico: url("data:image/svg+xml,…"); }
.fui-ico-eye { --ico: url("data:image/svg+xml,…"); }
.fui-ico-list-check { --ico: url("data:image/svg+xml,…"); }
.fui-dropdown-item { justify-content: flex-start; }

/* Icons of the side panels and the dashboards. */
.fui-ico-users { --ico: url("data:image/svg+xml,…"); }
.fui-ico-key { --ico: url("data:image/svg+xml,…"); }
.fui-ico-box { --ico: url("data:image/svg+xml,…"); }
.fui-ico-clock { --ico: url("data:image/svg+xml,…"); }
.fui-ico-folder { --ico: url("data:image/svg+xml,…"); }
.fui-ico-archive { --ico: url("data:image/svg+xml,…"); }
.fui-ico-server { --ico: url("data:image/svg+xml,…"); }
.fui-ico-cpu { --ico: url("data:image/svg+xml,…"); }
.fui-ico-layers { --ico: url("data:image/svg+xml,…"); }
.fui-ico-columns { --ico: url("data:image/svg+xml,…"); }
.fui-ico-database { --ico: url("data:image/svg+xml,…"); }
.fui-ico-message { --ico: url("data:image/svg+xml,…"); }
.fui-ico-plus { --ico: url("data:image/svg+xml,…"); }
.fui-ico-list { --ico: url("data:image/svg+xml,…"); }
.fui-ico-file { --ico: url("data:image/svg+xml,…"); }
.fui-ico-shield { --ico: url("data:image/svg+xml,…"); }
.fui-ico-arrow-right { --ico: url("data:image/svg+xml,…"); }
.fui-ico-activity { --ico: url("data:image/svg+xml,…"); }
.fui-ico-terminal { --ico: url("data:image/svg+xml,…"); }
.fui-nav-item .fui-ico { width: 1.25rem; height: 1.25rem; flex: 0 0 1.25rem; color: var(--fui-text-tertiary); }
.fui-nav-item:hover .fui-ico, .fui-nav-item[aria-current="page"] .fui-ico { color: var(--fui-brand-soft-text); }

```
