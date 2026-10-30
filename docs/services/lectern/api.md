---
title: API
parent: Lectern
grand_parent: Services
nav_order: 1
---

Lectern answers `GET` and `HEAD` and needs no credentials. Paths are relative to the base path (`BASE_PATH`). The same description is in [openapi.yaml](openapi.yaml).

## Routes

| Route | Answer |
|---|---|
| `GET /` | An HTML page listing the books as cards: title, release and number of files, each linked to its book. The language, mode and palette follow the `qlocale`, `qtheme` and `fui_palette` cookies, else `Accept-Language`. |
| `GET /ui/assets/style.css`, `/ui/assets/shell.js`, `/ui/assets/lectern.js`, `/ui/assets/frame.js`, `/ui/assets/favicon.png` | The pages' files: forge's stylesheet with Lectern's rules, forge's shell script, the script that reloads on a language change, the script of the frame page (address bar and title), the icon. |
| `GET /api/v1/books` | `200` JSON array, one object per book: `name`, `title`, `version`, `built`, `files`, `url`. |
| `GET /<book>` | `301` to `/<book>/`. |
| `GET /<book>/` | The book's `index.html`. |
| `GET /<book>/<path>` | The file at `<path>` in the release, with a content type from its extension. A directory without its trailing slash is a `301` to the slash. A page (HTML) asked for by a navigation (`Sec-Fetch-Dest: document`) comes in Lectern's frame instead: the top bar and the page in an iframe at the same address; see [A book inside Lectern](index.md#a-book-inside-lectern). |
| `GET /health`, `/health/live`, `/health/ready` | See [Operations](operations.md#health). |

Anything else is `404` (the book's own `404.html` for a missing page of a known book), a method other than `GET` and `HEAD` is `405` with `Allow: GET, HEAD`.

## Conditional requests

Every file of a release carries the weak `ETag` `W/"<version>"` (`W/"<version>-framed"` for the frame page) and `Cache-Control: no-cache`. A request with `If-None-Match` naming it is answered `304`.

## Example

```text
$ curl https://ennor.ddns.net/lectern/api/v1/books
[{"name":"forge","title":"Forge","version":"2026-10-30","built":"2026-10-30T20:20:33.641773049Z","files":224,"url":"/lectern/forge/"}]
```
