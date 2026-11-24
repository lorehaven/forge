---
title: Mailroom
parent: Services
nav_order: 17
has_children: true
permalink: /services/mailroom/
---

# Mailroom

The estate's **own** mail. Every Gatehouse user has a mailbox, people write to each other in a web mail client, forge services
write to people through it, and an administrator's panel manages the whole. It replaces Stalwart and Roundcube.

**Nothing leaves the cluster.** Mailroom is not a mail server and not a mail client of anybody else's server: it opens no
connection to any mail provider, relay or SMTP/IMAP server, and takes no mail in from outside. Mail to an outside address is
refused with a clear message; mail from outside cannot arrive. No port is opened in the router. Mail lives in Postgres.

| Part | Where | What |
|---|---|---|
| `mail-html` | `libs/mail-html` | the HTML sanitiser (allow-list) and HTML-to-text, for showing a message safely |
| `mailroom-client` | `libs/mailroom-client` | how a service writes to a person: machine token + `POST /api/v1/send` |
| `mailroom-core/app/web` | `services/mailroom/` | wire model and forms, quench-ui screens, wasm client |
| `mailroom` | `services/mailroom/service` | the server (`mailroom-service`) |

## Addresses and folders

A person's address is `<user name>@<MAILROOM_DOMAIN>` (the Gatehouse user name; only lower-case letters, digits, `.`, `-`, `_` can have a mailbox, and the names of the estate's services are reserved). A mailbox has four folders:
**inbox**, **archive**, **sent** and **trash** (emptied after 30 days). A change can name a folder (`all_in`) instead of messages: read everything in it, empty the trash (up to 5000 at a time). A message to three people is stored once per mailbox, with the
attachment bytes shared. A mailbox is made the first time the person opens Mailroom or a service first writes to them; a person
cannot write to someone who has neither (they have to sign in once). Writing `ann` and `ann@<domain>` is the same; any other
domain is refused. Quota per mailbox: 500 MB by default (panel).

## Drafts, muted senders, conversations

What a person is writing is kept as a **draft** (`/api/v1/drafts`): the recipients as typed, the subject, the text and the message it
answers, at most 100 per person; attachments are not kept. Sending a message written in a draft removes it (`draft` in the send
body). A **muted** sender (`/api/v1/mutes`) has their mail put in the archive already read; it is each person's own choice. A
message's **conversation** (`/api/v1/messages/{id}/thread`) is found by the message-id links in the headers, not by the subject. The
**signature** (`PUT /api/v1/signature`) is added by the client to the end of the messages a person starts.

## People

`mailroom:read` opens your mailbox, `mailroom:write` sends and changes mail, `mailroom:admin` is the panel (settings, mailboxes
**as metadata only** - owner, size, last seen -, the services' delivery log, audit log). `mailroom:send` is for machine identities.
A service's own credential (the `service` role) can call `/send` and nothing else; a person can never call `/send`. An
administrator cannot read anybody's mail, and the panel never lists mail between people.

## Services writing to people

`mailroom_client::Client::new(&Config::from_env()?)?.send(&SendRequest{ to: vec!["ann".into()], subject, text, html, .. })` with a
machine client in Gatehouse (`<service>-mailroom`, audience `mailroom`). The sender is the service itself
(`gatehouse@<domain>`, shown as automated; nobody can reply to it). Hourly per-service and daily estate budgets apply, and an
`idempotency_key` makes a retry harmless. Delivery is immediate and all-or-nothing per message.

Gatehouse (verification, reset, invitations, security notices, `/notify`) uses `MailroomSender` when `MAILROOM_URL` is set. It
addresses the person by **user name**, not by the address stored in the account. The four messages that carry a link (verification,
password reset, invitation, email change) are **not** put in any mailbox (a person cannot read one before they can sign in, and
a reset link in the account's own mailbox would hand the account to whoever holds a live session): they are written to Gatehouse's
log for the administrator to hand over (`GATEHOUSE_LOG_ACCOUNT_LINKS`, default on). Notices and notifications are delivered.

## Configuration (environment)

| Variable | Meaning |
|---|---|
| `DATABASE_URL`, `DB_SCHEMA` | Postgres (schema `mailroom`, installed by Foundry module `mailroom`) |
| `MAILROOM_DOMAIN` | the part after the `@` (default `mailroom.local`) |
| `MAILROOM_MAINTENANCE_SECS`, `MAILROOM_BACKGROUND` | sweep cadence (3600), worker on/off |
| `MAILROOM_TRUSTED_ORIGINS` | extra browser origins allowed to call the API with a session cookie |

There are no passwords, keys or relay settings: the only secrets are the two Gatehouse client secrets.

## Testing

`cargo test -p mail-html -p mailroom-core -p mailroom-app -p mailroom-web -p mailroom-client`; the service tests need Postgres:
`MAILROOM_TEST_DATABASE_URL=postgres://… cargo test -p mailroom`. BDD: `MAILROOM_BDD_DATABASE_URL=… cargo run -p forge-bdd --
--service mailroom` (people writing to each other, the services' mail, Gatehouse → Mailroom flows). See [security.md](security.md).

## Known limits

Local mail only (by design); no drafts folder, folders other than the three, threads view, address book or mail rules; no push
(the client refreshes when asked and on navigation); HTML mail is shown without its own CSS (inline style subset only); the Android
app is not built yet.
