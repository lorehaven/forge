---
title: Client
parent: Gatehouse
grand_parent: Services
nav_order: 5
---

The browser client is the `gatehouse-web` module (wasm, built from `gatehouse-app` + `gatehouse-core`) loaded by the page `forge-web` serves. The app does no I/O: it asks its host (`Request`) and is told what came (`Msg`); the host maps requests to HTTP calls with the session cookie. All writes are plain HTML form posts to the `/ui/...` routes, which answer with redirects; reads come from the page API (`/api/v1/ui/...`).

## Crates

| Crate | Content |
|---|---|
| `gatehouse-core` | wire model (`Dashboard`, `ServiceCard`, `Account`, `Subscription`, `MfaEnrollment`, `MfaEnrollRequest`, `AdminUsers`, `AdminUserRow`, `AdminUser`, `CatalogService`, `Grant`, `ConfirmEmail`, `UnsubscribeInfo`), `Notice`/`Query`/`Tone` (the messages a redirect's query may carry), `MfaDraft`/`DraftError`, helpers `segment` (percent-encode one path segment), `base_of` (service prefix from a page path), `file_name`, constant `ICONS` |
| `gatehouse-app` | `gatehouse(requests, setup) -> Gatehouse { view, deliver }`; `Request`, `Msg`, `Setup { base, language, history }`, `LANGUAGES`, `STYLE`; the screens |
| `gatehouse-web` | `app()` mounting the app over `fetch`; `Client<T: Transport>` mapping requests; `HttpRequest`, `HttpResponse`, `Transport` |

## Routes (all `{base}/ui/...`)

| Path | Screen | Session |
|---|---|---|
| `/home`, `/home/`, `` and `/` under `/ui` | dashboard of service cards | yes (service redirects otherwise) |
| `/login`, `/login/` | sign-in form (posts `username`, `password`, `redirect`), links to register and forgot-password | no |
| `/login/mfa` | code entry (posts `pending`, `code`, `redirect`) | no |
| `/register`, `/register/` | registration (`username`, `password`, `email`) | no |
| `/forgot-password`, `/forgot-password/` | request a reset (`username`) | no |
| `/reset-password?token=` | choose a new password | no |
| `/resend-verification`, `/resend-verification/` | resend the confirmation link | no |
| `/accept-invite?token=` | choose a password for an invited account | no |
| `/confirm-email?token=` | shows the new address, a button confirms | no |
| `/unsubscribe?token=` | shows the kind of notification, a button ends it | no |
| `/account` | profile (multipart form), password, email, notifications, MFA | yes |
| `/account/mfa/enroll` | QR code, secret, code box | yes |
| `/admin/users`, `/admin/users/` | user list, create form | yes (data needs `gatehouse:read-users`) |
| `/admin/users/:username` | editor: role, permission matrix, template, disable/enable/unlock, MFA reset, invitation, delete | yes |
| anything else | "not found" with a link home | |

The screens treated as public (no sidebar, bare top bar) are those whose first path segment after `/ui/` is one of `login`, `register`, `forgot-password`, `reset-password`, `resend-verification`, `accept-invite`, `confirm-email`, `unsubscribe` (`is_public`).

## Requests between app and host

| `Request` | Host call | Answer `Msg` |
|---|---|---|
| `Dashboard` | `GET /api/v1/ui/dashboard` | `Dashboard` |
| `Account` | `GET /api/v1/ui/account` | `Account` |
| `MfaSecret` | `GET /api/v1/ui/mfa-enroll` | `MfaSecret` |
| `MfaEnable(MfaEnrollRequest)` | `POST /api/v1/ui/mfa-enroll` (JSON) | `MfaEnabled` (app navigates to `/ui/account?ok=mfa_enabled`) or `MfaRefused(key)` |
| `AdminUsers` | `GET /api/v1/ui/admin/users` | `AdminUsers` |
| `AdminUser(name)` | `GET /api/v1/ui/admin/users/{name}` (name percent-encoded) | `AdminUser` |
| `ConfirmEmail(token)` | `GET /api/v1/ui/confirm-email?token=` | `ConfirmEmail`, or `LinkInvalid` on 404 |
| `UnsubscribeInfo(token)` | `GET /api/v1/ui/unsubscribe?token=` | `UnsubscribeInfo`, or `LinkInvalid` on 404 |
| `Unsubscribe(token)` | `POST /ui/unsubscribe?token=` (form `unsubscribe=1`) | `Unsubscribed`, or `LinkInvalid` on 404 |
| `Language(code)` | none (the host keeps it) | none |

Status mapping in the host: 200..299 ok; `401` `SignedOut` (the banner links to `/ui/login?redirect=<current path>`); `403` `Forbidden`; `404` `NotFound`; `400`, `409`, `422` with an `{"error": key}` body become `Failed(key)`; other statuses `Failed("the service answered N")`; a transport error `Failed("cannot reach the service: ...")`. Requests carry `Accept: application/json` and the page's cookies.

## Navigation and chrome

The top bar and side panel come from `forge-shell` (`Bar { brand: Gatehouse, icon: shield, image: {base}/ui/assets/favicon.png, dashboard: false, account: true (false on public screens), languages: en-US, de-DE, es-ES, fr-FR, pl-PL }`). The side panel has three items: services (`/ui/home`, id `nav-services`), users (`/ui/admin/users`, id `nav-users`), account (`/ui/account`, id `nav-account`). The page has the framework's session watcher: `GET /ui/status`, `POST /ui/refresh`.

## Internationalisation

Five locales compiled into the module from `service/i18n/<locale>.ftl`: `en-US`, `de-DE`, `es-ES`, `fr-FR`, `pl-PL` (the language picker order; the fallback is the first, `en-US`). The chosen language is kept by the host (browser storage key `language`) and mirrored in the `qlocale` cookie by `forge-shell`'s script. Keys the service sends back are translation keys: dashboard `ui_service_<name>_title`/`_desc`, errors `ui_admin_error_*`, `ui_account_error_*`, `ui_login_*`, `ui_register_error_*`, `ui_reset_error_*`, notification labels `ui_notification_<template id with . replaced by _>`, plus the bar keys of `forge-shell` (`ui_theme_*`, `ui_palette_*`, `ui_user_menu`, `ui_profile_edit`, `ui_logout`, `ui_home_button`, `ui_dashboard_button`, `locale_label`).

## Styling hooks

Own classes (`gatehouse-app/src/style.css`): `gh-main`, `gh-top`, `gh-public`, `gh-page`, `gh-current`, `gh-hint`, `gh-mono`, `gh-panel`, `gh-panel-title`, `gh-meta`, `gh-auth`, `gh-narrow`, `gh-below`, `gh-form`, `gh-field`, `gh-input`, `gh-btn`, `gh-primary`, `gh-danger`, `gh-back`, `gh-row`, `gh-file*`, `gh-avatar`, `gh-qr`, `gh-section-title`, `gh-check`, `gh-status-row`, `gh-inline`, `gh-rows`, `gh-user-row`, `gh-user-main`, `gh-user-grants`, `gh-username`, plus `home-*` and `admin-container` for the dashboard and administration layout. They sit on `forge-shell`'s `fui-*` vocabulary (see `forge-shell` in the [Libraries](../../libs/index.md) section). Element ids that tests use: `waiting`, `not-found`, `notice`, `problem`, `sign-in`, `sign-out`, `home`, `title`, `dashboard`, `language`, `nav-*`.

## Files served for the client

`/ui/assets/quench.js`, `app.css`, `shell.js`, `favicon.png`, `icons/{name}`, `app.wasm` (see [API](api.md#ui)). The page is served with `Cache-Control: no-store`, a per-response script nonce and `Content-Security-Policy` with `script-src 'self' 'nonce-...' 'wasm-unsafe-eval'` and `style-src 'self'`; assets use `no-cache`.
