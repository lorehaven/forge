# Quench Web Components

`quench-web-components` (crate `quench_web_components`) is a small library of reusable, higher-level UI builders (buttons, cards/panels/containers, form fields, status badges/alerts) layered on top of `quench-web`'s `Element` builder. Its own README is a short pointer to this page. As of this writing it is a workspace member (`libs/quench-web-components`) with a real dependency on `quench-web`, but **no other crate in the workspace depends on it** — it isn't wired into the root `Cargo.toml`'s `[workspace.dependencies]` and nothing under `docker/`, `cli/`, or `examples/` references it. Its `components::nav`, `HeaderBuilder`, and `FooterBuilder` are near-duplicates of the ones that already live in `quench-web::framework::components` — that overlap is worth resolving before adopting this crate anywhere.

## Public API / Key Types

Reachable via `quench_web_components::prelude::*` (a subset) or the crate root:

- `buttons` — `ButtonVariant` (`Primary`, `Secondary`, `Danger`, `Success`, `Warning`, `Outline`), `ButtonBuilder` (`.variant()`/`.primary()`/`.secondary()`/etc., `.disabled()`, `.button_type()`, `.id()`, `.build() -> Element`), and helpers `primary_button`, `secondary_button`, `danger_button`, `success_button`, `warning_button`, `outline_button`.
- `containers` — `Card` (title/content/footer/`.compact()`), `Panel` (title/content/`.bordered()`/`.highlighted()`), `Container` (`.fluid()`/`.centered()`), plus helpers `card`, `compact_card`, `panel`, `highlighted_panel`.
- `forms` — `FormInput`, `FormSelect`, `FormTextarea` builders, each producing a labeled `.form-group` wrapper `Element` when `.label()` is set, or a bare field otherwise.
- `status` — `StatusLevel` (`Info`, `Success`, `Warning`, `Error`), `StatusBadge` (`.with_icon()`), `AlertBox` (`.closeable()`), plus helpers `success_badge`, `warning_badge`, `error_badge`, `info_alert`, `success_alert`, `warning_alert`, `error_alert`.
- `components` — `HeaderBuilder`, `FooterBuilder`, `NavPanelBuilder`, `nav_button()` — functionally identical to `quench-web`'s own versions, built directly against `quench_web::{Element, div, ...}` rather than re-exporting them.
- `dom` — re-exports of `quench_web::dom`'s `on_dom_ready`, `toggle_modal`, `update_from_select`, `set_select_value` (used internally by `NavPanelBuilder`; no additional behavior on top of `quench-web`'s copies).

## Testing

No test files exist under `libs/quench-web-components`.

## Usage example

```rust
use quench_web_components::containers::Card;
use quench_web_components::status::success_badge;

fn build_widget() -> quench_web::Element {
    Card::new()
        .title("Deployment")
        .content(success_badge("healthy"))
        .build()
}
```

[Home](../README.md)
