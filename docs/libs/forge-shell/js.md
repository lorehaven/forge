---
title: JavaScript hooks
parent: forge-shell
grand_parent: Libraries
nav_order: 3
---

`forge_shell::SCRIPT` (`assets/shell.js`) is one self-invoking function with no exports. A page loads it with a plain `<script src=".../ui/assets/shell.js" data-gatehouse="...">` (`forge-web` adds this tag when the client is `framed`). Every behaviour is delegated from `document`, so elements drawn later by the wasm client work. The client only draws the elements; the script does the rest.

## Cookies and storage

| Name | Written by | Meaning |
|---|---|---|
| cookie `qtheme` | script (`put('qtheme', 'default-' + mode)`) | value contains `light` means light theme, anything else or absent means dark. Max-age 31 536 000 s, `path=/`, `SameSite=Lax` |
| cookie `fui_palette` | script | one of `green teal blue purple rose orange`; otherwise `green` is used |
| cookie `qlocale` | script on a change of `select#language` | the chosen locale |
| cookie `qlocale_chosen` | script, value `1` | set together with `qlocale`: marks that a person chose |
| `localStorage` key `q:language` | script, once at start | copied from cookie `qlocale` when the storage has none yet (storage errors are ignored) |

Theme and palette: `apply()` runs at start and every 1000 ms (`setInterval`), setting `data-fui-theme` (`light` or `dark`) and `data-fui-palette` on `<html>` only when they differ. Cookies written by another tab or service therefore take effect within a second.

## Data attributes and ids the script reacts to

| Hook | Trigger | Effect |
|---|---|---|
| `[data-fui-mode="light"\|"dark"]` | click | writes `qtheme=default-<mode>`, re-applies |
| `[data-fui-palette="<name>"]` | click | writes `fui_palette=<name>`, re-applies |
| `details.fui-dropdown` | click anywhere | every open dropdown except the one clicked in loses `open` |
| any key `Escape` | keydown | closes every open `details.fui-dropdown` |
| `select#language` | change | writes `qlocale` and `qlocale_chosen=1` (the client reacts to the same event through the bar's `on_input`) |
| `img` inside `.fui-avatar` | `load` (capture) | adds class `has-avatar` to the avatar so the picture replaces the icon; a 404 leaves the icon |
| `[data-gh-href]` | at start and on every DOM mutation | sets `href` to `<gatehouse base>` + the attribute value |
| `[data-gh-src]` | same | sets `src` the same way (only when it differs) |
| `[data-drag-id]`, `[data-drag-select]`, `[data-drop-value]` | drag events | see below |
| `[data-copy-target="<id>"]` | click | see below |
| `[data-scroll-to="<id>"]` | click | `document.getElementById(id).scrollIntoView({behavior: 'smooth', block: 'start'})` |
| `input[type=file][data-upload-url]`, `data-upload-fields`, `data-upload-result` | change | see below |

### Gatehouse links

`<gatehouse base>` is the `data-gatehouse` attribute of the script tag (`document.currentScript`), default `/gatehouse`, with trailing slashes removed. A `MutationObserver` on `document.documentElement` (`childList`, `subtree`) re-applies the links after every change.

### Drag and drop

* An element with `[data-drag-id]` is draggable (`cursor: grab`). `dragstart` stores it, sets `text/plain` data to the id, `effectAllowed = 'move'` and adds class `fui-dragging`.
* An element with `[data-drop-value]` is a drop target: while a drag is active, `dragover` is cancelled (allowing the drop, `dropEffect = 'move'`), `dragenter` adds `fui-drop-target`, `dragleave` removes it when leaving to outside the target.
* `drop` removes `fui-drop-target`, finds the `<select>` whose id is the dragged element's `data-drag-select` attribute, and, if its value differs from the target's `data-drop-value`, sets the select's value and dispatches a bubbling `input` event, so a drag does exactly what changing the select does.
* `dragend` removes `fui-dragging` and every `fui-drop-target`.

Example:

```html
<div data-drag-id="card-7" data-drag-select="status-7" draggable="true">Card</div>
<div data-drop-value="done">Done column</div>
<select id="status-7"><option value="todo">todo</option><option value="done">done</option></select>
```

`draggable="true"` must be set by the markup: the script styles `[data-drag-id]` but does not set it.

### Copy

Click on `[data-copy-target]` copies `innerText` of the element with that id through `navigator.clipboard.writeText` (nothing happens when the clipboard API is missing or the id is unknown) and adds class `fui-copied` to the clicked element for 1500 ms.

### Upload

A change event on an `<input type="file" data-upload-url="URL">` (and `files` present):

1. `data-upload-fields` (optional) is parsed as a JSON object of extra form fields (a parse error means none).
2. The input is cleared (`picker.value = ''`).
3. For each chosen file: a `FormData` with the fields, then `file` (with the file name) is sent as `POST` to `URL` with `credentials: 'same-origin'` (`multipart/form-data`).
4. The outcome is written to the element whose id is `data-upload-result`: its `value` becomes the JSON `{"name": <file name>, "status": <HTTP status or 0>, "body": <response text, or the error text when the request failed>}` and a bubbling `input` event is dispatched. Without a result element nothing is reported.

The client cannot read a chosen file itself; this is how a file reaches the service. The upload hook ignores every other `change` (the language picker has its own handler).

## Pitfalls

* The script needs `document.cookie`; with cookies blocked the theme stays dark and green.
* `qtheme` is shared with quench's own scripts (`default-light` / `default-dark`); a value without `light` is read as dark.
* `data-gh-href` values are paths on gatehouse (`/ui/home`, `/ui/account`), not full URLs.
