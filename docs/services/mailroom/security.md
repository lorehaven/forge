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
| Mailbox identity | a mailbox is the user name **exactly as Gatehouse spells it, lower-case only** (Gatehouse's names are case-sensitive: `Alice` and `alice` are two people). A name with capitals, `@` or `+` gets no mailbox and no mail. A service names recipients exactly (no lower-casing, no addresses) so a name can never be turned into somebody else's; a person typing `Ann` reaches `ann` | `mailbox::open_mailbox`, `outgoing::service_names` |
| Header or log injection | there are no mail headers any more: a message is a row. Subject and unsubscribe link reject CR/LF/NUL; recipients are reduced to a valid user name; text with NUL is refused (Postgres cannot store it) | `delivery::check`, `draft::recipient_name` |
| SQL injection | every statement is parameterised; the only built SQL is the column list and `$n` placeholders in `messages_page`; search terms are bound parameters with LIKE escaped | `store/` |
| Mail leaving the estate | nothing can: no SMTP/IMAP client exists. A recipient with another domain is refused with a sentence | `outgoing::names` |
| Spoofing a service | a person's message always carries an empty `source`; a service's carries its token-derived name and the reader shows "Automatic message"; the estate's service names (`gatehouse`, `sage`, ..., `postmaster`, `noreply`) can have no mailbox, so nobody can send as or be mistaken for one | `delivery::RESERVED` |
| A service abusing its credential | per-service hourly and estate daily budgets, idempotency keys, ≤50 recipients, size caps; a service cannot read anything | `outgoing::send_service`, `limits.rs` |
| Spam or flooding | per-person and per-service rates count **recipients** (fifty recipients are fifty deliveries); ≤50 recipients (checked before anything else), per-mailbox quota (default 500 MB) that **includes the sender's own `sent` copy** and is rechecked under a per-mailbox lock inside the delivery transaction; upload cap (60 MB per person, 60 uploads/hour); attachments ≤20 MB / 10 files, weighed before they are read | `outgoing.rs`, `store/messages.rs` |
| HTML that stalls the reader | the sanitiser is thorough rather than fast: 400 000 nested `<div>`s did not finish in ten minutes (found by testing it). A service's HTML is limited to 20 000 tags nested ≤128 deep at delivery | `delivery::html_shape_ok` |
| Upload quota race | the cap check and insert run under a per-person advisory lock inside one transaction (a bare `INSERT … SELECT` is not enough under READ COMMITTED: the test that found it runs four uploads in parallel) | `store/uploads.rs` |
| Unbounded storage | trash emptied after 30 days, unsent uploads after a day, orphaned attachment bytes after an hour, delivery records after 30 days | `worker.rs` |
| Search / paging amplification | ≤8 search terms of ≤100 characters, pages ≤100, change lists ≤200 | `mailbox.rs` |
| Error leakage | database and internal errors become "something went wrong" (details go to the log); data errors (NUL) become 400 | `error.rs` |
| Secrets | none stored. The only secrets are the two Gatehouse client secrets, in the age-encrypted `secrets.yaml` |  |
| Open ports | none. No `hostPort`, no router rule; HTTP only inside the cluster and through the existing HTTPS ingress | homecloud overlay |

## Gatehouse's account links

The four Gatehouse messages that carry a link granting something (verification, password reset, invitation, email change) are
**not delivered to Mailroom at all**: a person cannot read a mailbox before they can sign in, and a reset link in an account's own
mailbox would let whoever holds a live session of that account (a stolen cookie, XSS elsewhere in the estate) set a new password
and keep the account. They are written to Gatehouse's log (`visit <link> to ...`) for the administrator to hand over. Those
lines contain single-use tokens: anyone who can read Gatehouse's log can take over the account the token belongs to. The
administrator can already do that, but **log aggregation extends the audience** (the observability overlay ships logs to Loki): keep
`visit ` lines out of that pipeline, or accept the wider audience while a link is live. `GATEHOUSE_LOG_ACCOUNT_LINKS=false`
turns them off, and then these four messages fail with an error. This is the largest residual risk of the design.

## Residual risks (accepted, with the reason)

- **Mailbox identity is the user name.** A new Gatehouse user with the name of a deleted one would inherit that mailbox. Delete
  the mailbox in the panel when deleting the user (the runbook says so).
- **Anyone with a grant can list user names** (`/people` completes names; writing to a missing name says "has no mailbox"). A
  home estate's user list is not secret; a public one would need an allow-list.
- **A mailbox can be filled by others**: any person with `write` may send up to their hourly limit of 1 MB messages to a victim
  until the quota is hit, after which Gatehouse's notices to that person fail. There is no block list; the administrator can
  lower the rate or delete the mailbox. Services have no priority over people.
- **A service can create mailboxes** for any valid name (up to its hourly budget); the panel lists and deletes them.
- **Rate limits are in memory** (per process): a restart resets them. One replica is the deployment.
- **A compromised service credential** can write anything to anybody as that service, within its budget, until revoked.
- **Mail between people is not encrypted at rest** (Postgres holds it as text); an administrator with database access can read it.
  Encrypting rows would need a key the service holds, which protects only against database theft.

## Not covered

`quench-auth`'s token validation and cookie flags, `quench-html`'s parser on pathological input, and Gatehouse's assignment of the
`service` role are relied on as shipped; the framework has its own tests.

## Review

A read-only review by a separate reviewer (not the author) of the reworked code found no SQL injection, no IDOR in mail reads,
no gate bypass, no CSRF or XSS path, and these problems. "Fixed" means code and a test changed.

| # | Finding | Result |
|---|---|---|
| 1 | Mailroom lower-cased the user name, Gatehouse's names are case-sensitive: a user `Alice` with a grant would read `alice`'s inbox, reset links included | **Fixed**: the owner is the exact name; a name with capitals has no mailbox; a service must name recipients exactly |
| 2 | A reset link in the account's own mailbox turns a hijacked session into permanent takeover | **Fixed**: the four link messages are never delivered to a mailbox (log hand-over only) |
| 3 | Links logged at info level are shipped to Loki | **Mitigated, documented**: this is the cost of hand-over; see above and the runbook. Opt-out `GATEHOUSE_LOG_ACCOUNT_LINKS=false` |
| 4 | CPU/memory DoS by unbounded recipient and attachment lists before validation; 60 MB of uploads read before the cap check | **Fixed**: ≤50 recipients and ≤10 attachment ids checked first; attachment size is summed in SQL before any bytes are read |
| 5 | The sender's `sent` copy bypassed the quota; the quota check was not atomic | **Fixed**: the author's mailbox is checked; every mailbox is locked and rechecked inside the transaction (test: five parallel deliveries into a 1 MB mailbox deliver exactly two) |
| 6 | Mailbox bombing; budgets counted calls, not recipients; a retry was charged | **Partly fixed**: limits count recipients and a retry is not charged. **Accepted**: no block list, no priority for Gatehouse, no cap on mailboxes a service may create |
| 7 | Mailbox identity reuse after a user is deleted | **Open, documented**: delete the mailbox with the user (runbook). Gatehouse does not tell Mailroom |
| 8 | A Gatehouse user named `alice@ennor.ddns.net` injects Gatehouse-authored mail into `alice` | **Fixed**: a service's recipient list takes names only, no addresses |
| 9 | A person named like a service could send as it | **Fixed**: service names are reserved and have no mailbox |
| 10 | `/people` enumerates mailboxes | **Mitigated**: an empty prefix lists nothing. Accepted for a home estate |
| 11 | Deeply nested HTML | **Real, worse than the review thought**: not a stack overflow but a hang (400 000 nested elements did not finish in ten minutes). **Fixed**: bounded at delivery (≤20 000 tags, ≤128 deep) with a test |
| 12 | Deployment has no `securityContext`/NetworkPolicy; `remove_upload` skipped the mailbox check | **Fixed** (`remove_upload`); the rest **open**: no other service in the overlay sets them either |

Not covered by the review (assumed from the framework): the cookie flags and claim semantics of `quench-auth`, and the
parsers' own limits beyond the one above.
