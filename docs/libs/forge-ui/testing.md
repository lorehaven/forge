---
title: Testing
parent: forge-ui
grand_parent: Libraries
nav_order: 5
---

`tests/components_tests.rs` (`[[test]] name = "components_tests"`, `harness = false`) renders components and checks the HTML: user menu (profile and logout links, avatar swap via `has-avatar`), stylesheet contents (tokens, both themes, the `@media (max-width: 1023.98px)` rule, service rules appended last), button variants to classes, `data-i18n` keys, alert role, page header crumbs and actions, modal wiring (`open_modal`/`close_modal` strings), stacked tables.

The gallery example renders every component to static HTML for visual review:

```bash
cargo run -p forge-ui --example gallery -- /tmp/gallery
# serve /tmp/gallery with any static server and open /ui/gallery.html (and /ui/auth.html)
```

It copies `examples/gallery.ftl` to `i18n/<locale>.ftl` for each of `SUPPORTED_LOCALES`. Run the tests with `cargo test -p forge-ui`.
