# Mailroom

The estate's mail: a web mail client over **any** IMAP/SMTP provider, an Android preview app, an administrator's panel, and the
one place forge services send mail from. It replaces Stalwart and Roundcube.

**It is a client, not a mail server.** Nothing is exposed in the router: Mailroom opens connections *out* (to people's providers
and to the estate's relay) and keeps a cache in Postgres. The consequence is deliberate: mail addressed to `@ennor.ddns.net` is no
longer received (see the homecloud runbook for moving what you still want).

| Part | Where | What |
|---|---|---|
| `mail-client` | `libs/mail-client` | IMAP client, MIME parser/composer, HTML sanitiser, outbound address policy, provider presets |
| `mail-testkit` | `libs/mail-testkit` | in-process IMAP/SMTP servers (tests, BDD, `--example serve` for a browser demo) |
| `mailroom-client` | `libs/mailroom-client` | how a service sends mail: machine token + `POST /api/v1/send` |
| `mailroom-core/app/web` | `services/mailroom/` | wire model, quench-ui screens, wasm client |
| `mailroom` | `services/mailroom/service` | the server (`mailroom-service`) |

## People

`mailroom:read` opens your mailboxes, `mailroom:write` adds mailboxes and sends from them, `mailroom:admin` is the panel (the sending
server, sender addresses, host policy, queue, everyone's mailboxes **as metadata only**, audit log). `mailroom:send` is for machine
identities. A service's own credential (the `service` role) can call `/send` and nothing else; a person can never call `/send`.

Providers: Gmail, Outlook, iCloud, Yahoo, Fastmail, GMX, Zoho, WP, Onet, o2, Interia have presets; anything else is typed in.
Gmail/Outlook/iCloud need an *app password* (no OAuth yet). Proton needs Proton Mail Bridge (an administrator allows its host).

## Services sending mail

Gatehouse (verification, reset, invitations, security notices, `/notify`) uses `MailroomSender` when `MAILROOM_URL` is set. Any other
service: `mailroom_client::Client::new(&Config::from_env()?)?.send(&SendRequest{..})` with a machine client in Gatehouse
(`<service>-mailroom`, audience `mailroom`). `From` must be one of the administrator's sender identities; hourly per-service and
daily estate budgets apply; an `idempotency_key` makes a retry harmless; the queue retries transient failures (1 min, 5 min,
30 min, 2 h, 6 h) and gives up after six.

## Configuration (environment)

| Variable | Meaning |
|---|---|
| `DATABASE_URL`, `DB_SCHEMA` | Postgres (schema `mailroom`, installed by Foundry module `mailroom`) |
| `MAILROOM_VAULT_KEY` (+ `_PREVIOUS`) | seals mailbox passwords, 32+ chars; required (dev: `MAILROOM_ALLOW_EPHEMERAL_KEY=true`) |
| `MAILROOM_SMTP_HOST/PORT/SECURITY/USERNAME/PASSWORD`, `MAILROOM_FROM` | estate relay + default sender, **seeded once** into settings |
| `MAILROOM_PRIVATE_HOSTS`, `MAILROOM_EXTRA_PORTS` | private hosts / ports users may use (added to the panel's lists) |
| `MAILROOM_SYNC_SECS`, `MAILROOM_INITIAL_MESSAGES`, `MAILROOM_BACKGROUND` | sync cadence, first-look size (300), workers on/off |
| `MAILROOM_TRUSTED_ORIGINS` | extra browser origins allowed to call the API with a session cookie |

## Testing

`cargo test -p mail-client -p mail-testkit -p mailroom-core -p mailroom-app -p mailroom-web -p mailroom-client`;
the service tests need Postgres: `MAILROOM_TEST_DATABASE_URL=postgres://… cargo test -p mailroom`. BDD:
`MAILROOM_BDD_DATABASE_URL=… cargo run -p forge-bdd -- --service mailroom` (47 scenarios, including real Gatehouse → Mailroom →
SMTP). With that variable set, the gatehouse suite also sends its mail through Mailroom. See [security.md](security.md).

## Known limits

No OAuth2/XOAUTH2 for SMTP (IMAP supports the mechanism); no IMAP IDLE push (polling every 2 minutes); no drafts folder sync; no
S/MIME/PGP; the Android app is a read-only preview; HTML mail is shown without its own CSS (inline style subset only).
