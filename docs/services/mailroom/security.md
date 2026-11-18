# Mailroom: security assessment

Scope: `services/mailroom` (service, core, app, web), `libs/mail-html`, `libs/mailroom-client`, Gatehouse's `MailroomSender`, the
Foundry migration and the homecloud overlay. Mailroom is the estate's **own** mail: a mailbox per Gatehouse user in Postgres,
people writing to each other, services writing to people. It has no connection to any mail server and stores no credential of
anybody's. That removes the largest attack surface a mail product has (parsing hostile servers' protocols, SSRF through
user-chosen hosts, sealed third-party passwords); what is left is below.

## Assets

1. **The content of people's mail** (text, HTML, attachments).
2. **Who may write as whom**: a message from `gatehouse@<domain>` must really be from Gatehouse.
3. **The services' ability to write to people** (verification and reset messages ride on it).
4. The estate's availability (Postgres space, the process).

## Threat model and controls

| Threat | Control | Where |
|---|---|---|
| Unauthenticated access | `Auth` (realm JWT, bearer or session cookie) over all of `/api/v1`, including unmapped paths (401); health is the only open route | `routers/api/mod.rs` |
| A person reading another's mail (IDOR) | every query is scoped by `owner`; `message_get_owned`, `attachment_data` via the owned message; the owner is the token's lower-cased `sub`, never a request field | `store/messages.rs`, `mailbox.rs` |
| Privilege confusion | `Gate`: reads need any `mailroom` grant, changes need `write`, `/admin` needs `admin`, `/send` is for machine identities and an explicit `send` grant only; a machine (`service` role, a wildcard) can do nothing else, a person can never call `/send`; the gate decides on the raw path so encoding cannot move a path into another class | `routers/api/gate.rs` |
| Administrator reading mail | the panel lists mailboxes (owner, size, last seen) and the services' delivery log; there is no endpoint that returns a message of a person, and the delivery log contains only service mail (a person's mail is never written to it) | `admin.rs` |
| CSRF | state-changing requests carrying a cookie are refused when `Sec-Fetch-Site`/`Origin` say another site; bearer requests are exempt; cookies are `SameSite=Lax` | `gate::SameOrigin` |
| XSS from mail HTML | an allow-list **writer** (not a stripper): known tags and attributes, a safe CSS subset, links only `http(s)`/`mailto`/`tel`, images only `data:`/`cid:` unless the reader allows remote ones; served in a sandboxed iframe (no `allow-scripts`, no `allow-same-origin`) with its own CSP (`default-src 'none'`, `img-src data:`), `nosniff`, `no-referrer`, never cached | `libs/mail-html`, `mailbox::html_policy` |
| XSS in the client | typed `text()`/`attr()` nodes, no `innerHTML`; the unsubscribe link is shown only if `https` | `app/` |
| Hostile attachments | always `application/octet-stream` + `Content-Disposition: attachment` with a header-safe file name, `nosniff`, CSP `sandbox`; names are cleaned (no path, no control characters) | `routers/api/messages.rs`, `outgoing::clean_filename` |
| Header or log injection | there are no mail headers any more: a message is a row. Subject and unsubscribe link reject CR/LF/NUL; recipients are reduced to a valid user name; text with NUL is refused (Postgres cannot store it) | `delivery::check`, `draft::recipient_name` |
| SQL injection | every statement is parameterised; the only built SQL is the column list and `$n` placeholders in `messages_page`; search terms are bound parameters with LIKE escaped | `store/` |
| Mail leaving the estate | nothing can: no SMTP/IMAP client exists. A recipient with another domain is refused with a sentence | `outgoing::names` |
| Spoofing a service | a person's message always carries an empty `source`; a service's carries its token-derived name and the reader shows "Automatic message"; nobody can write to `gatehouse@` (a service has no mailbox unless a person has that user name) | `delivery.rs` |
| A service abusing its credential | per-service hourly and estate daily budgets, idempotency keys, ≤50 recipients, size caps; a service cannot read anything | `outgoing::send_service`, `limits.rs` |
| Spam or flooding between people | per-person hourly send limit, ≤50 recipients, per-mailbox quota (default 500 MB), upload cap (60 MB per person, 60 uploads/hour), attachments ≤20 MB / 10 files | `settings.rs` |
| Upload quota race | the cap check and insert run under a per-person advisory lock inside one transaction (a bare `INSERT … SELECT` is not enough under READ COMMITTED: the test that found it runs four uploads in parallel) | `store/uploads.rs` |
| Unbounded storage | trash emptied after 30 days, unsent uploads after a day, orphaned attachment bytes after an hour, delivery records after 30 days | `worker.rs` |
| Search / paging amplification | ≤8 search terms of ≤100 characters, pages ≤100, change lists ≤200 | `mailbox.rs` |
| Error leakage | database and internal errors become "something went wrong" (details go to the log); data errors (NUL) become 400 | `error.rs` |
| Secrets | none stored. The only secrets are the two Gatehouse client secrets, in the age-encrypted `secrets.yaml` |  |
| Open ports | none. No `hostPort`, no router rule; HTTP only inside the cluster and through the existing HTTPS ingress | homecloud overlay |

## Gatehouse's account links

A person cannot read a mailbox before they can sign in, so the verification, password-reset and invitation links could never be
read there. Gatehouse therefore also writes every message to its log in the logging sender's shape (`visit <link> to ...`), for
the administrator to hand over. These lines contain single-use tokens: anyone who can read Gatehouse's log can take over the
account the token belongs to. The administrator can already do that, but **log aggregation extends the audience**: if Gatehouse's
logs are shipped somewhere wider, set `GATEHOUSE_LOG_ACCOUNT_LINKS=false` and accept that these messages are then unreadable until
a way to hand links over exists. This is the largest residual risk of the design.

## Residual risks (accepted, with the reason)

- **Mailbox identity is the user name.** A new Gatehouse user with the name of a deleted one would inherit that mailbox. Delete
  the mailbox in the panel when deleting the user (the runbook says so).
- **Anyone with a grant can list user names** (`/people` completes names; writing to a missing name says "has no mailbox"). A
  home estate's user list is not secret; a public one would need an allow-list.
- **Quota is soft under concurrency**: two simultaneous deliveries to the same mailbox may both pass the check and exceed it by one
  message. The next delivery is refused.
- **Rate limits are in memory** (per process): a restart resets them. One replica is the deployment.
- **A compromised service credential** can write anything to anybody as that service, within its budget, until revoked.
- **Mail between people is not encrypted at rest** (Postgres holds it as text); an administrator with database access can read it.
  Encrypting rows would need a key the service holds, which protects only against database theft.

## Not covered

`quench-auth`'s token validation and cookie flags, `quench-html`'s parser on pathological input, and Gatehouse's assignment of the
`service` role are relied on as shipped; the framework has its own tests.

## Review

An independent read-only review of the reworked code was run; its findings and what became of them are in the next section.
