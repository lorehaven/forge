---
title: Testing
parent: forge-shell
grand_parent: Libraries
nav_order: 6
---

`tests/unit.rs` (`harness = false`, `quench-test`):

* `a_flag_is_the_regions_two_letters`: `flag("pl-PL")`, `flag("en_us")`, `flag("en")`, `flag("zh-Hans")`.
* `the_bar_has_everything_the_old_pages_had`: renders `topbar` on a `MemoryHistory` router at `/gauge/ui/home` inside `quench_ui::create_root` and checks the HTML contains `data-gh-href="/ui/home"`, `id="home"`, `fui-ico-eye`, the brand, the flag option label, the translated theme toggle, `data-fui-mode="dark"`, the account and avatar hooks, the logout link and every palette.
* `the_style_and_script_are_there`: `STYLE` contains `--fui-bg: #0d0e10` and `.fui-ico-eye`; `SCRIPT` contains `qtheme` and `data-drop-value`.
* `the_script_has_the_scroll_and_upload_hooks`: string checks for the scroll and upload hooks and balanced braces and parentheses.

To test a bar in a client: build it inside `quench_ui::create_root`, provide a `Router` (the `link` helper needs one when `spa` is true), and render with `quench_ui::render_to_string`. Run: `cargo test -p forge-shell`.
