---
title: Design tokens
parent: forge-shell
grand_parent: Libraries
nav_order: 4
---

`assets/tokens.css` of `forge-shell` is byte-identical to the one in `forge-ui` (checked with `diff`). Semantic tokens (`--fui-*`) are what components use; the raw scales (`--fui-gray-*`, `--fui-brand-*`, `--fui-error-*`, ...) only feed them.

## Theme and palette selection

| Selector | Effect |
|---|---|
| `:root, html[data-fui-theme="dark"]` | dark theme; **the default** when the attribute is absent |
| `html[data-fui-theme="light"]` | light theme |
| `html[data-fui-palette="green"\|"purple"\|"blue"\|"teal"\|"orange"\|"rose"]` | replaces the whole `--fui-brand-25 ... --fui-brand-900` scale; without the attribute the base `:root` scale is green |

The attributes are set on `<html>` by [`shell.js`](js.md) from the cookies `qtheme` (contains `light` means light, anything else dark) and `fui_palette` (one of `PALETTES`, else `green`).

## Token groups

| Group | Names |
|---|---|
| Fonts | `--fui-font` (Inter, system fallbacks), `--fui-mono` |
| Raw gray | `--fui-gray-25` ... `--fui-gray-950` (25, 50, 100..900, 950) |
| Brand scale | `--fui-brand-25`, `-50`, `-100` ... `-900` (swapped by palette) |
| Status scales | `--fui-error-{50,200,500,600,700}`, `--fui-warning-{...}`, `--fui-success-{...}`, `--fui-blue-{...}` |
| Radii | `--fui-radius-xs` 4px, `-sm` 6px, `-md` 8px, `-lg` 10px, `-xl` 12px, `-2xl` 16px |
| Sizes | `--fui-sidebar-w` 18.5rem, `--fui-topbar-h` 4rem (3.5rem below 640px), `--fui-content-max` 80rem, `--fui-tap` 2.75rem |
| Surfaces | `--fui-bg`, `--fui-bg-sunken`, `--fui-surface`, `--fui-surface-2`, `--fui-surface-hover` |
| Borders | `--fui-border`, `--fui-border-strong` |
| Text | `--fui-text`, `--fui-text-secondary`, `--fui-text-tertiary`, `--fui-text-disabled`, `--fui-link` |
| Brand roles | `--fui-focus`, `--fui-brand-solid`, `--fui-brand-solid-hover`, `--fui-brand-soft-bg`, `--fui-brand-soft-text` |
| Soft tones | `--fui-{error,warning,success,blue,gray}-soft-{bg,text,border}` (brand has `-bg` and `-text`) |
| Shadows | `--fui-shadow-xs`, `--fui-shadow-md`, `--fui-shadow-xl` |
| Scrim | `--fui-scrim` |
| Legacy bridges | `--bs-gray-50` ... `--bs-gray-950` (scale inverted per theme), `--q-shell-*` (follow the semantic tokens), `--bs-danger`, `--bs-warning` |

## Source

```css
/* forge-ui design tokens - Untitled UI palette, type scale, radii and shadows.
   Semantic tokens (--fui-*) are what components use; the raw scale only feeds them.
   Theme is selected by html[data-fui-theme="dark|light"], kept in sync with quench's
   `qtheme` cookie by the theme script. Dark is the default when the attribute is absent. */

:root {
  --fui-font: "Inter", ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
  --fui-mono: ui-monospace, "SF Mono", "JetBrains Mono", Menlo, Consolas, monospace;

  /* Untitled UI "gray modern" */
  --fui-gray-25: #fcfcfd; --fui-gray-50: #f9fafb; --fui-gray-100: #f2f4f7; --fui-gray-200: #eaecf0;
  --fui-gray-300: #d0d5dd; --fui-gray-400: #98a2b3; --fui-gray-500: #667085; --fui-gray-600: #475467;
  --fui-gray-700: #344054; --fui-gray-800: #182230; --fui-gray-900: #101828; --fui-gray-950: #0c111d;

  /* Brand: green by default; html[data-fui-palette] swaps the whole scale (see palettes below) */
  --fui-brand-25: #f6fef9; --fui-brand-50: #ecfdf3; --fui-brand-100: #dcfae6; --fui-brand-200: #abefc6;
  --fui-brand-300: #75e0a7; --fui-brand-400: #47cd89; --fui-brand-500: #17b26a; --fui-brand-600: #079455;
  --fui-brand-700: #067647; --fui-brand-800: #085d3a; --fui-brand-900: #074d31;

  --fui-error-50: #fef3f2; --fui-error-200: #fecdca; --fui-error-500: #f04438; --fui-error-600: #d92d20; --fui-error-700: #b42318;
  --fui-warning-50: #fffaeb; --fui-warning-200: #fedf89; --fui-warning-500: #f79009; --fui-warning-600: #dc6803; --fui-warning-700: #b54708;
  --fui-success-50: #ecfdf3; --fui-success-200: #abefc6; --fui-success-500: #17b26a; --fui-success-600: #079455; --fui-success-700: #067647;
  --fui-blue-50: #eff8ff; --fui-blue-200: #b2ddff; --fui-blue-500: #2e90fa; --fui-blue-600: #1570ef; --fui-blue-700: #175cd3;

  --fui-radius-xs: 4px; --fui-radius-sm: 6px; --fui-radius-md: 8px; --fui-radius-lg: 10px; --fui-radius-xl: 12px; --fui-radius-2xl: 16px;

  --fui-sidebar-w: 18.5rem;
  --fui-topbar-h: 4rem;
  --fui-content-max: 80rem;
  --fui-tap: 2.75rem; /* minimum touch target */
}

/* ---------- dark (default) ---------- */
:root, html[data-fui-theme="dark"] {
  color-scheme: dark;
  --fui-bg: #0d0e10;
  --fui-bg-sunken: #08090a;
  --fui-surface: #141517;
  --fui-surface-2: #1b1c1f;
  --fui-surface-hover: #212326;
  --fui-border: #232528;
  --fui-border-strong: #36383d;
  --fui-text: #f4f4f5;
  --fui-text-secondary: #cecfd2;
  --fui-text-tertiary: #94969c;
  --fui-text-disabled: #61646c;
  --fui-link: var(--fui-brand-300);
  --fui-focus: color-mix(in srgb, var(--fui-brand-500) 35%, transparent);
  --fui-brand-solid: var(--fui-brand-600);
  --fui-brand-solid-hover: var(--fui-brand-500);
  --fui-brand-soft-bg: color-mix(in srgb, var(--fui-brand-500) 16%, transparent);
  --fui-brand-soft-text: var(--fui-brand-300);
  --fui-error-soft-bg: rgba(240, 68, 56, 0.14);   --fui-error-soft-text: #fda29b;   --fui-error-soft-border: rgba(240, 68, 56, 0.35);
  --fui-warning-soft-bg: rgba(247, 144, 9, 0.14);  --fui-warning-soft-text: #fec84b;  --fui-warning-soft-border: rgba(247, 144, 9, 0.35);
  --fui-success-soft-bg: rgba(23, 178, 106, 0.14); --fui-success-soft-text: #75e0a7;  --fui-success-soft-border: rgba(23, 178, 106, 0.35);
  --fui-blue-soft-bg: rgba(46, 144, 250, 0.14);    --fui-blue-soft-text: #84caff;     --fui-blue-soft-border: rgba(46, 144, 250, 0.35);
  --fui-gray-soft-bg: rgba(255, 255, 255, 0.06);   --fui-gray-soft-text: #cecfd2;     --fui-gray-soft-border: rgba(255, 255, 255, 0.12);
  --fui-shadow-xs: 0 1px 2px rgba(0, 0, 0, 0.4);
  --fui-shadow-md: 0 4px 8px -2px rgba(0, 0, 0, 0.5), 0 2px 4px -2px rgba(0, 0, 0, 0.4);
  --fui-shadow-xl: 0 20px 24px -4px rgba(0, 0, 0, 0.55), 0 8px 8px -4px rgba(0, 0, 0, 0.4);
  --fui-scrim: rgba(3, 4, 5, 0.72);

  /* Legacy bridge: quench's scale runs 50 = strongest text ... 950 = page background. */
  --bs-gray-50: #f7f7f8; --bs-gray-100: #ececee; --bs-gray-200: #d7d9dc; --bs-gray-300: #b4b7bd; --bs-gray-400: #8b8e96;
  --bs-gray-500: #666a72; --bs-gray-600: #40434a; --bs-gray-700: #2a2c30; --bs-gray-800: #1b1c1f; --bs-gray-900: #141517; --bs-gray-950: #0d0e10;
}

/* ---------- light ---------- */
html[data-fui-theme="light"] {
  color-scheme: light;
  --fui-bg: var(--fui-gray-50);
  --fui-bg-sunken: var(--fui-gray-100);
  --fui-surface: #ffffff;
  --fui-surface-2: var(--fui-gray-50);
  --fui-surface-hover: var(--fui-gray-50);
  --fui-border: var(--fui-gray-200);
  --fui-border-strong: var(--fui-gray-300);
  --fui-text: var(--fui-gray-900);
  --fui-text-secondary: var(--fui-gray-700);
  --fui-text-tertiary: var(--fui-gray-500);
  --fui-text-disabled: var(--fui-gray-400);
  --fui-link: var(--fui-brand-700);
  --fui-focus: color-mix(in srgb, var(--fui-brand-500) 28%, transparent);
  --fui-brand-solid: var(--fui-brand-600);
  --fui-brand-solid-hover: var(--fui-brand-700);
  --fui-brand-soft-bg: var(--fui-brand-50);
  --fui-brand-soft-text: var(--fui-brand-700);
  --fui-error-soft-bg: var(--fui-error-50);     --fui-error-soft-text: var(--fui-error-700);     --fui-error-soft-border: var(--fui-error-200);
  --fui-warning-soft-bg: var(--fui-warning-50); --fui-warning-soft-text: var(--fui-warning-700); --fui-warning-soft-border: var(--fui-warning-200);
  --fui-success-soft-bg: var(--fui-success-50); --fui-success-soft-text: var(--fui-success-700); --fui-success-soft-border: var(--fui-success-200);
  --fui-blue-soft-bg: var(--fui-blue-50);       --fui-blue-soft-text: var(--fui-blue-700);       --fui-blue-soft-border: var(--fui-blue-200);
  --fui-gray-soft-bg: var(--fui-gray-50);       --fui-gray-soft-text: var(--fui-gray-700);       --fui-gray-soft-border: var(--fui-gray-200);
  --fui-shadow-xs: 0 1px 2px rgba(16, 24, 40, 0.05);
  --fui-shadow-md: 0 4px 8px -2px rgba(16, 24, 40, 0.1), 0 2px 4px -2px rgba(16, 24, 40, 0.06);
  --fui-shadow-xl: 0 20px 24px -4px rgba(16, 24, 40, 0.08), 0 8px 8px -4px rgba(16, 24, 40, 0.03);
  --fui-scrim: rgba(12, 17, 29, 0.55);

  --bs-gray-50: #101828; --bs-gray-100: #182230; --bs-gray-200: #344054; --bs-gray-300: #475467; --bs-gray-400: #667085;
  --bs-gray-500: #98a2b3; --bs-gray-600: #d0d5dd; --bs-gray-700: #eaecf0; --bs-gray-800: #f2f4f7; --bs-gray-900: #f9fafb; --bs-gray-950: #ffffff;
}

/* quench's shell variables follow the semantic tokens so unmigrated markup matches */
:root {
  --q-shell-font: var(--fui-font);
  --q-shell-text: var(--fui-text-secondary);
  --q-shell-text-muted: var(--fui-text-tertiary);
  --q-shell-accent: var(--fui-brand-600);
  --q-shell-panel-radius: var(--fui-radius-md);
  --q-shell-panel-border: 1px solid var(--fui-border);
  --q-shell-panel-shadow: var(--fui-shadow-xs);
  --q-shell-panel-bg: var(--fui-surface);
  --q-shell-panel-bg-soft: var(--fui-bg);
  --q-shell-panel-bg-strong: var(--fui-surface);
  --bs-danger: var(--fui-error-600);
  --bs-warning: var(--fui-warning-500);
}

/* ---------- palettes: brand scale only; modes above apply to every palette ---------- */
html[data-fui-palette="green"] {
  --fui-brand-25: #f6fef9; --fui-brand-50: #ecfdf3; --fui-brand-100: #dcfae6; --fui-brand-200: #abefc6;
  --fui-brand-300: #75e0a7; --fui-brand-400: #47cd89; --fui-brand-500: #17b26a; --fui-brand-600: #079455;
  --fui-brand-700: #067647; --fui-brand-800: #085d3a; --fui-brand-900: #074d31;
}
html[data-fui-palette="purple"] {
  --fui-brand-25: #fcfaff; --fui-brand-50: #f9f5ff; --fui-brand-100: #f4ebff; --fui-brand-200: #e9d7fe;
  --fui-brand-300: #d6bbfb; --fui-brand-400: #b692f6; --fui-brand-500: #9e77ed; --fui-brand-600: #7f56d9;
  --fui-brand-700: #6941c6; --fui-brand-800: #53389e; --fui-brand-900: #42307d;
}
html[data-fui-palette="blue"] {
  --fui-brand-25: #f5faff; --fui-brand-50: #eff8ff; --fui-brand-100: #d1e9ff; --fui-brand-200: #b2ddff;
  --fui-brand-300: #84caff; --fui-brand-400: #53b1fd; --fui-brand-500: #2e90fa; --fui-brand-600: #1570ef;
  --fui-brand-700: #175cd3; --fui-brand-800: #1849a9; --fui-brand-900: #194185;
}
html[data-fui-palette="teal"] {
  --fui-brand-25: #f6fefc; --fui-brand-50: #f0fdf9; --fui-brand-100: #ccfbef; --fui-brand-200: #99f6e0;
  --fui-brand-300: #5fe9d0; --fui-brand-400: #2ed3b7; --fui-brand-500: #15b79e; --fui-brand-600: #0e9384;
  --fui-brand-700: #107569; --fui-brand-800: #125d56; --fui-brand-900: #134e48;
}
html[data-fui-palette="orange"] {
  --fui-brand-25: #fefaf5; --fui-brand-50: #fff6ed; --fui-brand-100: #ffead5; --fui-brand-200: #fddcab;
  --fui-brand-300: #feb273; --fui-brand-400: #fd853a; --fui-brand-500: #fb6514; --fui-brand-600: #ec4a0a;
  --fui-brand-700: #c4320a; --fui-brand-800: #9c2a10; --fui-brand-900: #7e2410;
}
html[data-fui-palette="rose"] {
  --fui-brand-25: #fff5f6; --fui-brand-50: #fff1f3; --fui-brand-100: #ffe4e8; --fui-brand-200: #fecdd6;
  --fui-brand-300: #fea3b4; --fui-brand-400: #fd6f8e; --fui-brand-500: #f63d68; --fui-brand-600: #e31b54;
  --fui-brand-700: #c01048; --fui-brand-800: #a11043; --fui-brand-900: #89123e;
}

```
