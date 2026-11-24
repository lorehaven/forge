# forge-sectest
Black-box security scanner for the forge services.

## What it is
`forge-sectest` reads every route the services declare (`#[get("/...")]` and friends, ~480 today, including the shared `quench-starter` routes), then attacks each service from outside: missing auth, forged tokens, path traversal, header and CORS mistakes, hostile input and HTTP request smuggling.

## Use cases
- Census the attack surface of the estate.
- Scan a running estate (local stack, BDD harness, staging) and fail on new anonymous routes.
- Run as the nightly security gate that Conveyor shows as the Security card.

## Example
```sh
# what is the attack surface?
forge-sectest census --quench ../quench

# scan a running estate (local stack, BDD harness, staging)
forge-sectest scan --quench ../quench \
    --target warehouse=http://127.0.0.1:8080 --target gatehouse=http://127.0.0.1:8081
```

## Details

### Checks

| check       | what it does                                                                                      |
|-------------|---------------------------------------------------------------------------------------------------|
| `auth`      | Requests every route with no credentials. Anything but 401/403 must be in the reviewed baseline. |
| `tokens`    | Forged credentials on every protected route: `alg:none`, guessable HS256 secrets, attacker-signed EdDSA, `kid` games, default basic auth, forged cookies/identity headers. A single 2xx is **critical**. |
| `authz`     | Acts as several identities that differ in one way (another service's audience, no grant, read-only, a second user) and looks for what a valid token still gets through. Needs `--authz FILE` and a gatehouse in test mode. |
| `fuzz`      | Reads each service's `docs/services/<name>/openapi.yaml`, builds a valid request per operation and changes one thing at a time (wrong type, boundary value, hostile string, missing required field, extra field, a body that is not JSON, hostile query strings). Needs `--fuzz` (or `--checks fuzz`); mutating operations need `--active`. |
| `traversal` | Encodings of `../` in every path parameter of every GET route; probes for `.git`, `.env`, etc.    |
| `headers`   | nosniff, framing, HSTS, cookie flags, version disclosure, `TRACE`, Host-header injection, 404 leaks. |
| `cors`      | Reflected / wildcard origins, credentialed CORS, approved hostile preflights.                     |
| `injection` | Hostile query strings on every GET (crash / leak / reflected XSS); malformed bodies under `--active`. |
| `protocol`  | Raw-socket request smuggling (CL.TE, duplicate CL, obfuscated TE), bare LF, NUL, folding, oversized heads, pipelining after errors. `--slow` adds a slowloris hold. |
| `throttle`  | `--active` only: a burst of wrong logins on every login-shaped route must hit a 429.             |

### Authorisation (`authz`)

The other checks show the auth wall exists. `authz` looks behind it. Identities are minted from gatehouse's test endpoint
(`POST /api/v1/test/token`, mounted only with `GATEHOUSE_TEST_MODE`), from the grants `authz.toml` names for each service, so it
runs against the BDD stack or a staging copy, never production:

```sh
forge-sectest scan --checks authz --active --insecure \
    --authz tests/forge-sectest/authz.toml --mint http://127.0.0.1:5443/gatehouse \
    --target daybook=http://127.0.0.1:12443/daybook --target workbench=http://127.0.0.1:10443/workbench
```

* **audience** - an administrator's token issued for *another* service, on every protected route: must be refused. (High; Critical-class if it changes data.)
* **no grant** - a valid token for the service with no scope, on every protected route: must be refused, except routes listed under `open` in the spec. Reads that need only a login are one Low note per service, because most services document it that way (gauge: "checks no `gauge:read` grant itself"); set `reads = "grant"` on a service to hold it to its grants. Mutating routes are always individual High findings.
* **read-only** (`--active`) - a reader on every mutating route: must be refused.
* **ownership / IDOR** (`--active`) - each `[[resource]]` is one user's object (`create`, with `{canary}` in the body, and the id found at `id_at`). The other user must be refused `read`/`write` requests, must not see it in a `list`, and the owner's copy must come through untouched. A recipe that cannot create or read back its own object is reported (Medium): a refusal means nothing if the object never existed.

A probe answered 400/404/405/422/502-504 says nothing about permission, so it is counted in an info note, never passed. A route with an
`{id}` is mostly 404 for the sentinel id: the ownership recipes are what cover those.

#### In the daily BDD run

`tests/forge-bdd/features/<service>/authz.feature` runs this check against every service the suite starts, with
`tests/forge-sectest/authz.toml` (so it lands in the same report as the rest of the suite and the BDD card). With
`FORGE_BDD_DATABASE_URL` set the ownership recipes for daybook, workbench and mailroom have real tables to work on. The step prints
the notes it did not fail on (low and info), so a run that decided nothing is visible. Recipes exist for daybook, workbench (per-project
grants), sage (projects) and mailroom (messages); conveyor's routes still answer 503 on the in-memory store the suite gives it.

### Fuzzing from the `OpenAPI` documents (`fuzz`)

```sh
forge-sectest scan --fuzz --active --insecure --forge . \
    --authz tests/forge-sectest/authz.toml --mint http://127.0.0.1:5443/gatehouse \
    --target daybook=http://127.0.0.1:12443/daybook
```

A schema says what a service expects, so a request that is valid except in one place is what gets past the first validation and meets the
code behind it. Requests are made as a signed-in user (`--token`, or one minted from the `authz` spec). The mutations are fixed, so a run
can be repeated; `--fuzz-budget N` caps the requests per operation (default 150).

**Ids.** A body fuzzed against a route whose `{id}` names nothing only ever meets a 404, so each path parameter is given a real value, tried in
this order: a `[[bind]]` in `authz.toml` (a fixed `value`, or a `create` request and where the id is in its answer); an object it made
earlier; a new one made through the collection's `POST` (or a nested one, `/projects/{id}/issues` for `/issues/{id}`; needs `--active`); the
first member of the collection's own `GET` list; the example the document gives for the parameter; last, a well-formed stand-in that names
nothing. Operations left on a stand-in are still fuzzed (crashes and leaks are judged, what they accept is not) and the report lists them,
so a gap is visible: add a `[[bind]]` to close it.

**Path parameters are fuzzed too**: each in turn is given hostile values (a NUL, a 10 000-character string, `..`, `%`, a SQL fragment),
because that is where an id reaches a query.

What it reports: a 500 (High when the reply also leaks), a stack trace or database error in any reply, a stall of 5s or more, a dropped
connection, a service that stopped answering (Critical), unescaped HTML reflected, and a privileged field (`is_admin`, `roles`, `owner`) the
client could set. Accepting a wrong type or a missing required field is a Low note, one per operation. An operation whose own valid request
answers 5xx (a feature the deployment lacks) is not fuzzed, and the note says so.

In the daily BDD run: `features/<service>/fuzz.feature` (`@fuzz @serial`, since it writes to the service), for every service the suite
starts, gatehouse included. Gatehouse is fuzzed as an administrator (that is where its parsing is) but only ever against one throwaway
account, `sectest-victim`: its `{username}` is bound to it, because the first member of the user list is `admin`, and operations whose
path ends in `delete` or `rotate` are not fuzzed, like a `DELETE`.

Objects the fuzzer cannot make itself (it speaks JSON and reads no event streams) are seeded by the harness and handed over as `[[bind]]`
values: a sage conversation, message and file (a conversation exists only once a turn was streamed to its end), a warehouse crate and
build, gauge's host, a mailroom message. What is still fuzzed against a stand-in id is listed in the report.

An ownership recipe that only means something with tables (`requires = "database"` in `authz.toml`) is left out when the suite has no
Postgres (`FORGE_BDD_DATABASE_URL`).

Also in the daily run: `features/<service>/scan.feature` (`@scan @serial`) runs the checks above that need no spec (`auth`, `tokens`,
`traversal`, `headers`, `cors`, `injection`, `protocol`, `throttle`) against every service, behind the auth wall too, with the reviewed
`public-routes.txt` as the baseline. It fails on high and above; the rest is printed. The nightly runs the same checks against the deployed
estate; this is the same scan before the deploy.

#### Sign-in attacks

`tests/forge-bdd/features/gatehouse/attacks.feature` (`@security`) attacks gatehouse's flows and passes when the defence holds: a session
id planted before sign-in, a spent refresh token replayed (at most the one recovery the grace window promises, never a new
credential), an unknown user against a wrong password (same answer, same time), other spellings of a name against the lockout, a reset
that must end the sessions it was meant to and be good once, and a reset email built from a forged `Host`. Not asserted, because the
design accepts it: the owner's refresh token keeps working after a replay inside the grace window.

### Public-route baseline

The first run lists every anonymous route as a finding. Review them, then freeze the list:

```sh
forge-sectest scan --quench ../quench --target ... --write-baseline tests/forge-sectest/public-routes.txt
```

From then on a *new* anonymous route (a router someone forgot to wrap in `Auth`) fails the scan.
`scripts/security.fish` picks the file up automatically.

### Safety model

* **Read-only by default.** Mutating verbs, credential bursts and malformed write bodies need `--active`.
* **Sentinel identifiers** (`00000000-...`, `0.0.0-sectest`) that cannot name real data.
* **Refuses non-private targets** (anything outside loopback / RFC 1918 / `.svc` / `.cluster.local`)
  unless `--allow-remote` is passed. Only use that for systems you own.
* **Rate limited** (`--rps`, default 20). No flooding and no DoS tests by design.
* **An unreachable target is an error**, never a clean report; >50% transport failures is a finding.

`--active` against anything that holds data you care about: run it against the BDD harness or a staging
copy first. If a route turns out to be unprotected, the request that proved it has already been sent.

### Trusting the scanner

`cargo test -p forge-sectest` runs the scanner against a deliberately vulnerable app (must flag
every axis), a hardened app (must report nothing above `low`), a desync-prone raw server, and the real
`quench-http` server (every protocol probe must be refused). The route census is cross-checked against an
independent count of route attributes so a new attribute style cannot silently hide routes.

### Limits

* Not coverage-guided and not a replacement for review: it finds missing auth, bad parsing and leaks, not
  business-logic flaws. Authorisation is covered by `authz` (see above) for the services and objects `authz.toml`
  describes; a service or object with no recipe has only the generic audience / grant / read-only probes. Only daybook tasks
  have an ownership recipe so far, and conveyor's routes cannot be judged on the in-memory BDD stack (503: its queue needs Postgres).
* A 404 on a route with parameters is treated as "not routed" (the sentinel id does not exist), so it
  cannot distinguish "auth ran first" from "route missing".
* Proxy in front? Protocol results then describe proxy + service together.

### The nightly gate (what conveyor runs)

`scripts/security.fish` and the `security` stage of `.conveyor.toml` run the scanner as a **test run**, so the
result is a `quench-test` report conveyor shows as the Security card (see `docs/services/conveyor/scans.md`).
One case per stage:

* `tests/gate.rs` (`--features gate`): `cargo deny`, `cargo audit` (advisories appear without any code
  changing, which is why this runs nightly), unsafe code confined to `scripts/unsafe-allowlist.txt`, no
  secrets in the project's files (git's view: tracked or new, not ignored; reviewed exceptions in
  `scripts/secrets-allowlist.txt`), and the scanner's own self-test.
* `tests/live.rs` (`--features gate`): one case per check (the `authz` case only with `--features authz-live`, which also needs `FORGE_SECTEST_AUTHZ`; against production there are no identities to mint, so the case is absent and the card does not count it as passed) against `FORGE_SECTEST_TARGETS`
  (`name=URL`, space separated), configured by `FORGE_SECTEST_TOKEN`, `_ACTIVE`, `_FAIL_ON` (default `high`),
  `_BASELINE`, `_RPS`. With no targets every case **fails**: a nightly that scanned nothing is not a pass.
  Every finding is also written to `target/security-findings.json`.
* `tests/scanners.rs` (always on): the gate's scanners shown known-bad input.

`src/gate.rs` is a copy of `quench-security`'s scanners (that crate is unpublished and forge builds against
published quench crates); keep the two in step.
