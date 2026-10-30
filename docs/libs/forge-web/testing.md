---
title: Testing
parent: forge-web
grand_parent: Libraries
nav_order: 3
---

`tests/unit.rs` (`harness = false`) writes throwaway directories under the system temp dir (named with the process id) and calls `page_from` / `module_from`:

* the page starts the module with a strict policy (nonce, `'wasm-unsafe-eval'`, no `unsafe-inline` in `style-src`, `cache-control: no-store`, title, the three asset paths);
* every page has a nonce of its own (two policies differ);
* without the module the page is `404` with "not installed" and `module_from` is `404`;
* the module is served as `application/wasm` with the bytes intact; script and style content types; unframed style equals the client's CSS, framed style starts with `forge_shell::STYLE`; the shell script equals `forge_shell::SCRIPT`; `favicon()` is `404` without an icon and `image/png` with one; the page links the icon and `/ui/assets/shell.js`.

Run: `cargo test -p forge-web`.
