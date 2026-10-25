# forge-sectest

Black-box security scanner for the forge services. It reads every route the services declare
(`#[get("/...")]` and friends, ~480 today, including the shared `quench-starter` routes), then attacks
each service from outside.

```sh
# what is the attack surface?
forge-sectest census --quench ../quench

# scan a running estate (local stack, BDD harness, staging)
forge-sectest scan --quench ../quench \
    --target warehouse=http://127.0.0.1:8080 --target gatehouse=http://127.0.0.1:8081
```

## Checks

| check       | what it does                                                                                      |
|-------------|---------------------------------------------------------------------------------------------------|
| `auth`      | Requests every route with no credentials. Anything but 401/403 must be in the reviewed baseline. |
| `tokens`    | Forged credentials on every protected route: `alg:none`, guessable HS256 secrets, attacker-signed EdDSA, `kid` games, default basic auth, forged cookies/identity headers. A single 2xx is **critical**. |
| `traversal` | Encodings of `../` in every path parameter of every GET route; probes for `.git`, `.env`, etc.    |
| `headers`   | nosniff, framing, HSTS, cookie flags, version disclosure, `TRACE`, Host-header injection, 404 leaks. |
| `cors`      | Reflected / wildcard origins, credentialed CORS, approved hostile preflights.                     |
| `injection` | Hostile query strings on every GET (crash / leak / reflected XSS); malformed bodies under `--active`. |
| `protocol`  | Raw-socket request smuggling (CL.TE, duplicate CL, obfuscated TE), bare LF, NUL, folding, oversized heads, pipelining after errors. `--slow` adds a slowloris hold. |
| `throttle`  | `--active` only: a burst of wrong logins on every login-shaped route must hit a 429.             |

## Public-route baseline

The first run lists every anonymous route as a finding. Review them, then freeze the list:

```sh
forge-sectest scan --quench ../quench --target ... --write-baseline tests/forge-sectest/public-routes.txt
```

From then on a *new* anonymous route (a router someone forgot to wrap in `Auth`) fails the scan.
`scripts/security.fish` picks the file up automatically.

## Safety model

* **Read-only by default.** Mutating verbs, credential bursts and malformed write bodies need `--active`.
* **Sentinel identifiers** (`00000000-...`, `0.0.0-sectest`) that cannot name real data.
* **Refuses non-private targets** (anything outside loopback / RFC 1918 / `.svc` / `.cluster.local`)
  unless `--allow-remote` is passed. Only use that for systems you own.
* **Rate limited** (`--rps`, default 20). No flooding and no DoS tests by design.
* **An unreachable target is an error**, never a clean report; >50% transport failures is a finding.

`--active` against anything that holds data you care about: run it against the BDD harness or a staging
copy first. If a route turns out to be unprotected, the request that proved it has already been sent.

## Trusting the scanner

`cargo test -p forge-sectest` runs the scanner against a deliberately vulnerable app (must flag
every axis), a hardened app (must report nothing above `low`), a desync-prone raw server, and the real
`quench-http` server (every protocol probe must be refused). The route census is cross-checked against an
independent count of route attributes so a new attribute style cannot silently hide routes.

## Limits

* Not coverage-guided and not a replacement for review: it finds missing auth, bad parsing and leaks, not
  business-logic flaws. IDOR / cross-tenant authorisation needs two valid identities (`--token` runs the
  traversal/injection/CORS checks authenticated; two-user authz is not built yet).
* A 404 on a route with parameters is treated as "not routed" (the sentinel id does not exist), so it
  cannot distinguish "auth ran first" from "route missing".
* Proxy in front? Protocol results then describe proxy + service together.

## The nightly gate (what conveyor runs)

`scripts/security.fish` and the `security` stage of `.conveyor.toml` run the scanner as a **test run**, so the
result is a `quench-test` report conveyor shows as the Security card (see `docs/docker/conveyor-service.md`).
One case per stage:

* `tests/gate.rs` (`--features gate`): `cargo deny`, `cargo audit` (advisories appear without any code
  changing, which is why this runs nightly), unsafe code confined to `scripts/unsafe-allowlist.txt`, no
  secrets in the project's files (git's view: tracked or new, not ignored; reviewed exceptions in
  `scripts/secrets-allowlist.txt`), and the scanner's own self-test.
* `tests/live.rs` (`--features gate`): one case per check against `FORGE_SECTEST_TARGETS`
  (`name=URL`, space separated), configured by `FORGE_SECTEST_TOKEN`, `_ACTIVE`, `_FAIL_ON` (default `high`),
  `_BASELINE`, `_RPS`. With no targets every case **fails**: a nightly that scanned nothing is not a pass.
  Every finding is also written to `target/security-findings.json`.
* `tests/scanners.rs` (always on): the gate's scanners shown known-bad input.

`src/gate.rs` is a copy of `quench-security`'s scanners (that crate is unpublished and forge builds against
published quench crates); keep the two in step.
