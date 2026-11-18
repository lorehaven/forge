# Mailroom: security assessment

Scope: `libs/mail-client`, `services/mailroom/{core,app,web,service}`, Gatehouse's `MailroomSender`, the homecloud overlay.
Method: threat model (below), attack tests in the suites named in each row, an independent read-only code review (section
"Review"), and the BDD scenarios in `tests/forge-bdd/features/mailroom/`.

## What is at stake

| Asset | Why it matters |
|---|---|
| Passwords of people's mailboxes at other providers | a credential for someone's whole mail life (and password resets everywhere) |
| People's mail | private |
| The estate's sending identity and relay login | spam / phishing from `noreply@…`, relay credit |
| The cluster network | a service that dials user-chosen hosts is an SSRF primitive |
| Gatehouse tokens | machine tokens that may send mail |

## Threats and controls

| # | Threat | Control | Evidence |
|---|---|---|---|
| 1 | **SSRF**: a user adds a mailbox on `10.43.0.1:6443`, `169.254.169.254`, Postgres, Redis, another service's admin port | `netguard`: public addresses only (loopback, private, link-local, CGNAT, documentation, multicast, IPv4-mapped/NAT64/6to4 forms refused), mail ports only (143, 993, 465, 587, 2525 + administrator's), **connect to the addresses that were checked** (no second resolution, so no DNS rebinding), same path for IMAP and SMTP; private hosts only by administrator allow-list; the connection test is rate-limited (12/min/person) | `mail-client` netguard tests, `accounts_tests::internal_hosts_…`, BDD `mailboxes.feature` (5 hosts) |
| 2 | Stored mailbox passwords leak (DB dump, API, logs) | XChaCha20-Poly1305 under a key derived (HKDF) from `MAILROOM_VAULT_KEY`; associated data = account id + owner, so a sealed value copied to another row does not open; never returned by any API (`Account` has no secret field), never in errors (`Auth` Debug is redacted; upstream errors are rewritten to fixed sentences), key rotation via `_PREVIOUS` with re-sealing on use | `pure_tests` vault tests, `api_tests`, BDD "password is not echoed" |
| 3 | Another person reads/changes my mail (IDOR) | every read goes through `*_owned` queries joining `accounts.owner`; ids are random UUIDs but never the only guard; administrators see mailbox **metadata only** (no message endpoint accepts anyone but the owner) | `accounts_tests::nobody_else…`, `mailbox_tests::nobody_else…`, `api_tests` admin case, BDD "Nobody else can read my mail" |
| 4 | **XSS / active content in a stranger's HTML mail** | two layers: (a) server-side allow-list sanitiser writes a *new* document (no script/style/iframe/forms/svg/event handlers, `href` only http(s)/mailto/tel, filtered inline CSS, remote images removed and counted, `data:` images only png/jpeg/gif/webp); (b) shown only inside `<iframe sandbox="allow-popups allow-popups-to-escape-sandbox">` (no scripts, no same-origin) served with its own CSP `default-src 'none'; img-src data:; sandbox …; form-action 'none'; base-uri 'none'`; inline images are inlined as `data:` so the frame needs no network; remote images only on the reader's click | sanitiser XSS corpus tests, `api_tests::the_html_…`, BDD reading scenario |
| 5 | Tracking pixels / read receipts | remote content blocked by default and counted | sanitiser + CSP tests |
| 6 | Malicious attachment (HTML/SVG served inline, path in the name) | always `Content-Type: application/octet-stream`, `Content-Disposition: attachment` (RFC 2231 name), `nosniff`, `CSP: sandbox`; filenames stripped of paths/controls | `pure_tests`, `api_tests::attachments_always_download…` |
| 7 | **Header / command injection** (CRLF in subject, address, header names; IMAP argument injection) | compose builds only from strictly parsed addresses and one-line values; extra headers limited to `List-*`, `Auto-Submitted`, `X-*`; IMAP user strings only via quoted/literal builders, UID sets and flags validated | compose tests, `imap_server_tests::arguments_that_could_inject…`, BDD sending |
| 8 | **Open relay / spoofed sender** via `/send` | `From` must be an administrator-defined identity; machine tokens only; ≤20 recipients, size caps, hourly per-service and daily estate budgets, idempotency keys, `Auto-Submitted`, source recorded; persons cannot call it at all | `outgoing_tests`, `api_tests::each_grant…`, BDD admin/access |
| 9 | A service credential used as a person (privilege escalation: the `service` role is a wildcard) | gate treats `service` tokens as send-only; `send` is not granted by a person's wildcard `admin` | `pure_tests::grants_map…`, BDD "service credential opens nothing else" |
| 10 | **CSRF** (cookie-authenticated browser) | session cookie is `SameSite=Lax`; plus `SameOrigin` middleware refuses state-changing requests with `Sec-Fetch-Site: cross-site` or a foreign `Origin`; bearer-token calls are exempt | `pure_tests`, `api_tests::a_page_from_another_site…` |
| 11 | Hostile mail server / hostile mail (DoS) | IMAP reader limits: line 256 KiB, response 64 MiB, literal ≤ response cap, list depth 32; MIME: depth 12, 1000 parts, 4 MiB text, header caps; sanitiser depth 64 and 2 MiB output; timeouts on every read; sync bounded (300 first look, 500 per run); messages > 40 MB are not opened | `imap_server_tests::endless_lines…`, `message_tests::deep_nesting…`, sanitiser tests |
| 12 | MITM on the mail connection | TLS mandatory (implicit or STARTTLS **before** the password; plaintext refused; leftover buffered bytes after STARTTLS rejected); certificate verified against the real host name even though the checked IP is dialled; a per-mailbox trusted certificate (PEM) for a bridge/private CA, never "accept any" | `imap_server_tests` (starttls order, untrusted cert), smtp tests |
| 13 | Brute force / abuse of the connection test | per-person rate limits (test 12/min, refresh 20/min, send 60/h) | `accounts_tests`, `sync_tests`, `outgoing_tests` |
| 14 | SQL injection | every statement is parameterised; the one dynamic clause (search words) only adds `$n` placeholders, `LIKE` metacharacters escaped | `sync_tests::searching…` (incl. `%`, `_`, `\`, `' OR 1=1`), `api_tests::malformed_requests…` |
| 15 | Lost updates / races | settings changes serialised; queue claim uses `FOR UPDATE SKIP LOCKED`; one sync per mailbox | `outgoing_tests::messages_stuck…`, BDD (parallel scenarios) |
| 16 | Error detail leaks | internal failures logged, caller sees "something went wrong"; SQLSTATE class 22 → 400; upstream text is rewritten or stripped of control characters | `pure_tests::errors_tell_the_caller_little…` |
| 17 | Admin abuse | panel actions are audited (names of changed settings, never values); admins cannot read mail; disabling/deleting a mailbox is logged | `api_tests` admin tests, BDD admin.feature |
| 18 | Inbound attack surface | none: no listening mail port, no router rule; HTTP only behind the existing ingress | overlay |

## Residual risks and decisions to know about

- **No OAuth**: Gmail/Outlook/iCloud need app passwords, which are broader than a scoped token. They are sealed, but a database
  *and* vault-key compromise exposes them. Keep the vault key out of the database backup's reach.
- **An administrator who can change the vault key or the deployment** can read stored passwords: Mailroom is as trusted as its
  operator. The panel itself cannot.
- **Sanitiser is allow-list, not proof**: defence in depth (CSP sandbox, no same-origin) is what protects against a bypass.
- **In-memory rate limits** reset on restart and are per replica (the service is a single replica by design).
- The web app page itself has no CSP of its own yet (the shell's inline bootstrap needs a separate pass).
- `MAILROOM_PRIVATE_HOSTS` widens the SSRF boundary: list only names you control; ports stay restricted to mail ports plus those
  you add.
- Sent-mail copies and cached headers are plaintext in Postgres (disk encryption is the operator's layer).

## Review

A read-only review by a separate reviewer (not the author) found no SQL injection, IDOR, gate bypass or SSRF escape, and these
issues. Fixed means code and a test changed.

| # | Finding | Result |
|---|---|---|
| 1 | Hostile IMAP server streams responses until the pod runs out of memory | **Fixed**: ≤200k untagged responses and a 5-minute deadline per command |
| 2 | Slow-drip server pins the 4 sync slots | **Fixed**: 5-minute command deadline, 15-minute cap per sync run |
| 3 | Admin repoints the stored relay login at their own host and reads the password back by connecting | **Fixed**: changing host, port or user name requires typing the password again; host change is audited; test added |
| 4 | Admin panel showed subjects/recipients of users' sent mail | **Fixed**: only service mail shows them; a person's own rows show no recipients/subject |
| 5 | Flag/move/delete used UIDs without checking UIDVALIDITY (wrong mail changed after a renumbering) | **Fixed**: refused with a conflict and a refresh is queued |
| 6 | Mailroom can be used as a login/port probe against public hosts on mail ports | **Mitigated, not closed**: mail ports only, 12 tests/min/person, 10 mailboxes/person. A global per-destination limit would need shared state; accepted for a single-replica home estate |
| 7 | Upload quota race; uploads buffered in memory | **Fixed**: quota check and insert are one SQL statement; 60 uploads/hour/person (memory per upload stays ≤21 MB) |
| 8 | Unbounded subject/name sizes stored | **Fixed**: truncated (1000/200/320) |
| 9 | Reply flag could target another mailbox's message | **Fixed**: the replied-to message must be in the sending mailbox |
| 10 | Ownership is the token's `sub` (username): a re-used username inherits mailboxes | **Open**: depends on Gatehouse never re-using a deleted username; delete a person's mailboxes when deleting the user |
| 11 | Disabled mailbox could still be changed (login retried) | **Fixed** for change; test/create never touch a stored mailbox |
| 12 | Any service token may send arbitrary content as the estate's address | **Partly**: `X-Mailroom-*` headers are now reserved; per-service budgets bound it. A compromised service can still phish: give each service its own client and revoke it |
| 13 | Auth off means everyone is an admin | **By design for local runs** (`SERVICE_AUTH_ENABLED`); the homecloud config sets it to `true` and the README says it must stay so |

Not covered by the review (assumed from the framework): `quench-mail`'s SMTP limits, `quench-html` on pathological nesting,
`quench-auth` audience enforcement, and Gatehouse's assignment of the `service` role.
