---
title: Lectern
parent: Services
nav_order: 9
has_children: true
---

Lectern shows the documentation books of the estate to anyone who opens its address. It reads the books that `anvil docs release` puts in a Warehouse files storage, keeps the newest release of each in memory, and serves them as the plain web sites they were built as. Nobody signs in: there is no login page, no redirect to Gatehouse, no account menu. The service signs in to Warehouse for itself.

## Crates

| Crate | Path | Version | Role |
|---|---|---|---|
| `lectern` | `services/lectern/service` | 0.1.0 | The server: reading the storage, unpacking releases, serving pages. Binary `lectern-service`. |

There is no `core`, `app` or `web` crate and no WebAssembly client: the pages of a book are the book's own (a Jekyll site with the just-the-docs theme), and the one page Lectern draws, the list of books at `/`, is rendered by the server. It wears the same frame as the other services (below).

## How it works

1. Every `LECTERN_REFRESH_SECS` (60) the service lists the directories at the root of the storage `LECTERN_STORAGE` (`docs`). Each directory is a book; the layout is the one [`anvil docs release`](../../cli/anvil/commands-docs.md) writes:

   ```text
   <book>/latest.json                        manifest of the newest release + archive_path
   <book>/<version>/manifest.json
   <book>/<version>/<book>-<version>.tar.gz  the built site
   ```

2. For a book whose `latest.json` names a version it does not hold, it downloads the archive, checks it against `archive_sha256` and unpacks it in memory (at most 512 MiB). A release that fails any step is logged and the version already served stays in service.
3. A book that disappears from the storage disappears from the service at the next look.
4. Requests are answered from memory. Nothing is written to disk.

## Where a book appears

A book `forge` is served at `<BASE_PATH>/forge/`; the list of books is at `<BASE_PATH>/`.

A book built without a `baseurl` writes every link from the site root (`href="/assets/..."`), which would point outside its place under Lectern. When the manifest has no `baseurl`, Lectern rewrites those links while unpacking, so the same release works here and anywhere else:

| File | What is rewritten |
|---|---|
| HTML | `href`, `src` and `action` attributes of real tags that start with a single `/` (prose that merely quotes `href="/x"` is left alone) |
| CSS | `url(/...)` |
| `search-data.json` | `"url": "/..."` |
| the theme's script | the quoted `/assets/...` paths (the search index and the theme switch) |

Links with a scheme, `//host` links, fragments and relative links are never changed. A book whose manifest has a `baseurl` is served exactly as built: its `baseurl` must be the path Lectern serves it at.

## The front page

The list of books uses `forge-shell`, the frame every forge service shares: its stylesheet (tokens, components, top bar) and its script, served from `/ui/assets/style.css` and `/ui/assets/shell.js`, and the markup of its top bar. So it looks like the other services and follows the same cookies: light or dark mode (`qtheme`), colour palette (`fui_palette`), language (`qlocale`). A book is a card like the ones on the Gatehouse dashboard, with its title, the release being served and the number of files.

What is left out is what Lectern does not have: no account menu, no services-dashboard button, no link to Gatehouse. The language is the cookie's, else the browser's `Accept-Language`, else English; the page is translated into `en-US`, `de-DE`, `es-ES`, `fr-FR` and `pl-PL` (`services/lectern/service/i18n`), and changing the language reloads the page. The page's policy allows its own files only (`default-src 'none'`, scripts and styles from the same origin, images from the same origin and `data:` for the icon masks).

## A book inside Lectern

A page of a book opened in the browser's window is not sent as it is: Lectern answers with its own frame, the **top bar** (home button, brand, a menu of all the books with the one being read marked, language, theme) and below it the page, in an iframe at the same address. So there is always a way back to the list or on to another book. The frame is for navigations only: the iframe asks for the same address and gets the page itself, as does anything that is not a navigation (stylesheets, scripts, images, a download tool).

* The browser says which it is with the `Sec-Fetch-Dest` header (`document` for a navigation, `iframe` for the frame), which browsers send to https and `localhost`. A client that does not send it gets the page alone, without the top bar.
* The address bar follows the page in the frame (a small script on the frame page copies the frame's path, query and anchor into the address with `history.replaceState`), so a link copied from the bar opens the same page, inside the frame, and the tab title is the page's.
* Links that leave the site open in a tab of their own.
* The frame page and the page itself have different validators (`W/"<version>-framed"` and `W/"<version>"`) and both answer `Vary: Sec-Fetch-Dest`.
* The mode and colour chosen in the top bar reach the book at once (it reads the same cookies), and the book hides its own theme menu while framed (see the [forge theme](../../cli/anvil/commands-docs.md)).

The books are shown with the forge theme `anvil docs build` gives them; a book built with `theme = "none"` appears in its own.

Names that are paths of the service itself (`api`, `health`, `ui`) cannot be book names: a directory called so is skipped with a log line.

## Facts

* Default port under foreman: HTTPS 15443, redirect 15080, `BASE_PATH=/lectern` (`foreman.toml`).
* Dependencies: Warehouse (the storage) and Gatehouse (only for the service's token). It starts without either and reports not ready until the storage has been listed once.
* Authentication: none for readers. Only `GET` and `HEAD` are answered (`405` with `Allow: GET, HEAD` otherwise).
* Identity towards Warehouse: the OAuth client `lectern-warehouse` (`client_credentials`, audience `warehouse`), defined in Gatehouse's `config/clients.toml`; its secret is `CLIENT_SECRET_LECTERN_WAREHOUSE` there and `WAREHOUSE_CLIENT_SECRET` here.
* Caching: every file answers `Cache-Control: no-cache` with a weak `ETag` of the release version, so a revalidation is a `304` and a new release is never mixed with files cached from the last one. A page opened in the browser's window and the same page asked for by the frame have different validators and `Vary: Sec-Fetch-Dest`.
* Responses carry `X-Content-Type-Options: nosniff` and a correlation id.
* On the dashboard: Gatehouse shows a card for it to everyone who is signed in (`[services.lectern]` in its catalog has `public = true`, and Gatehouse needs `LECTERN_UI_URL`); there is no grant to hold, so Lectern has no row in the permission editor. The card's picture is `services/lectern/icon.png`.
* Tab and top-bar icon: `services/lectern/icon.png`, served at `/ui/assets/favicon.png`.

## Pages

| Page | Content |
|---|---|
| [API](api.md), [openapi.yaml](openapi.yaml) | The routes. |
| [Configuration](configuration.md) | Environment. |
| [Operations](operations.md) | Running it, health, what goes wrong. |
