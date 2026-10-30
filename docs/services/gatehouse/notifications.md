---
title: Notifications and mail
parent: Gatehouse
grand_parent: Services
nav_order: 8
---

Gatehouse is the estate's only sender of mail to people. Other services never hold SMTP credentials.

## Mail kinds

| Kind (`email::Mail`) | When | Goes to | Link lifetime |
|---|---|---|---|
| Verification | registration, resend | the new address | 24 hours (`/ui/verify?token=`) |
| PasswordReset | forgot-password | the address on file | 1 hour (`/ui/reset-password?token=`) |
| Invite | an account created with an address | that address | 7 days (`/ui/accept-invite?token=`) |
| EmailChange | a person asks to change their address | the new address | 24 hours (`/ui/confirm-email?token=`) |
| EmailChanged | the change was confirmed | the old address, if it was confirmed | none |
| PasswordChanged, MfaEnabled, MfaDisabled | the matching self-service or administrator action | the confirmed address | none |
| Notification | `POST /api/v1/notify` | the confirmed address | unsubscribe link 90 days |

Notification mail carries the headers `List-Unsubscribe: <link>` and `List-Unsubscribe-Post: List-Unsubscribe=One-Click` (RFC 8058); every mail has `Auto-Submitted: auto-generated`.

Security notices are sent only to a **confirmed** address and cannot be turned off. A send failure is logged and never changes the answer of the page.

Text and HTML bodies exist in English, Polish, German, French and Spanish (`en`, `pl`, `de`, `fr`, `es`). The language is the recipient's `preferred_locale`, else the browser's `qlocale` cookie when supported, else English. Links in mail start with `PUBLIC_BASE_URL` (never the request's `Host`).

## Delivery

`SMTP_HOST` set: `SmtpSender` (through `quench-mail`) behind `BudgetedSender`; invalid mail settings stop the process at start. A background check at start verifies the server accepts the login (a failure warns). `SMTP_HOST` unset: `LoggingSender` writes `email(<kind>) to=... user=...: visit <link> ...` lines to the log, not suitable for a deployment real users reach. Settings: [configuration](configuration.md).

## The notification catalog

`POST /api/v1/notify` names a template; wording, translations and variables belong to the catalog (`notify::catalog`):

| Id | Service | On by default | Variables |
|---|---|---|---|
| `conveyor.run.failed` | conveyor | yes | `project`, `run`, `ref` (text), `url` (link) |
| `conveyor.run.succeeded` | conveyor | no | same |
| `conveyor.run.recovered` | conveyor | no | same |

Validation (`400 {"error": ...}`): the template exists (`no such notification: <id>`); no variable beyond the declared ones (`<id> does not take a variable named "x"`); all present (`<id> needs the variable "x"`); not blank; no control characters; at most 300 characters (500 for a link); a link must start with Gatehouse's public origin followed by `/` and contain no whitespace.

Decision order after validation: unknown account (`no_such_user`), no confirmed address (`no_verified_email`), not subscribed (`not_subscribed`), duplicate `dedupe_key` within a day (`duplicate`), per-person limit 20 per hour and per-person-per-template 10 per hour (`rate_limited`). Then the mail is sent: `202 accepted`, or `502 failed` with `retryable`. A request with `requested: true` counts as subscribed for a kind that is off by default; a person's explicit opt-out still wins.

## Subscriptions

Table `auth.notification_prefs` holds only departures from a template's default. The account page (`POST /ui/account/notifications`) sets one checkbox per template named `sub_<template id>`. The unsubscribe link (`/ui/unsubscribe?token=`) turns one kind off without a session; the token is not spent.
