---
title: Scans
parent: Conveyor
grand_parent: Services
nav_order: 10
---

The scan feature turns what a repository's runs already did into six check cards. Nothing here starts a scan or runs a tool: it is a read of step output and stored artifacts (`src/scan/`, `src/artifacts/`). Routes: `GET /api/v1/scan`, `GET /api/v1/repos/{id}/scan`, `.../scan/findings`, `.../scan/{category}/report` (see [API](api.md)).

## The six checks

Order everywhere (`ScanSummary::KINDS`, `conveyor_core::scan::CHECKS`), with the chip letter of the front page:

| `kind` | Chip | Title key | Source |
|---|---|---|---|
| `lint` | L | `ui_scan_lint_title` | a step whose command starts with `lint` (`{ anvil = "lint ... --json" }`) |
| `machete` | M | `ui_scan_machete_title` | a step whose command starts with `machete` |
| `audit` | A | `ui_scan_audit_title` | a step whose command starts with `audit` |
| `coverage` | C | `ui_scan_coverage_title` | a step whose command contains `llvm-cov` (checked first; for example `cargo llvm-cov report`) |
| `tests` | T | `ui_scan_tests_title` | the `summary.json` artifact of a job that ran `quench-test` tests |
| `security` | S | `ui_scan_security_title` | the `security-summary.json` artifact of the nightly security run |

The step command is the text recorded for the step (for an `anvil` step, the arguments after `anvil`; secrets already redacted).

## Which run a card comes from

Every card is taken from the newest run that **produced it**, not from the newest run. For the four step-based checks the newest 40 runs of the repository (newest first) are searched until each check is found. For `tests` and `security` the newest artifact of that name across all runs of the repository is used, however many newer runs there are. `run` in the report is the repository's newest run, whatever it did; each `CheckDetail.origin` is the run that produced that card. A nightly security run and a push therefore never blank each other's cards.

Results of finished runs are cached in memory (up to 256 runs for step checks, 64 parsed reports), so the front page, which asks for every repository, is cheap; a run still going is re-read each time.

## Step checks

Only finished steps (`started_at` and `finished_at` set) count. The log lines between the step's start and end are the input; ANSI colour codes are stripped.

* **lint, machete, audit** read **stdout only** (anvil's `--json` report is on stdout, its chatter on stderr). The pipeline must pass `--json`; without it the output is not recognised.
* **lint**: newline-delimited cargo JSON messages. At least one line with a `reason` must be seen, otherwise the output is "not recognised". Each `warning` or `error` message becomes a finding (`title` = message text, `severity` = level, `location` = `file:line:column` of the primary span). Headline: `clean`, `<w> warning(s)`, `<e> error(s)` or `<w> warning(s), <e> error(s)`.
* **machete**: one JSON document `{"findings": [{"package", "manifest", "unused": [...]}]}`; one finding per unused dependency (`title` = dependency, `location` = package). Headline `clean` or `<n> unused dependenc(y|ies)`.
* **audit**: one `cargo audit --format json` document: every entry of `vulnerabilities.list` plus every entry of every `warnings.*` list becomes a finding (`title` = advisory title, `id` = advisory id, `severity` = `informational` (for example `unmaintained`) else `cvss`, `date`, `location` = `<crate> <version>`, `extra` = `Solution: Upgrade to <patched versions>`). Headline `clean` or `<n> finding(s)`.
* **coverage**: the table of `cargo llvm-cov report`: columns 0, 7, 8, 9 (filename, lines, missed, line %). The `TOTAL` row gives the headline `<pct>% line coverage` (two decimals) and the metric (`<pct>` rounded, with `%`); each file with missed lines is a finding sorted by most missed lines (`title` = file, `severity` = its percentage, `location` = `<missed> of <lines> lines missed`). Without a `TOTAL` row the output is not recognised. The last coverage step of a job wins.
* **Not recognised** output falls back to the step's own result: a passed step has headline `passed` and no findings; a failed one has headline `failed (exit <code>)` (or `failed`) and the last ten non-empty log lines as findings.

`passed` of a step check is whether the step succeeded. At most 50 findings are kept per check.

## tests and security

Both read a `quench-test` report: a JSON with `summary` (`total`, `passed`, `failed`, `skipped`), `duration_ms` and `suites[].cases[]` (`name`, `status`, `message`, `location`). Headline `<passed> passed, <failed> failed, <skipped> skipped` plus the duration (`<n> ms`, `<s.s> s` or `<m>m <s>s`) when it is above zero; `metric` = `<passed>/<total>`; `passed` = no failed case; each failed case is a finding (`severity` `failed`, `location` = the case's location or its suite name, `extra` = its message). For `tests`, if several jobs of the run left a summary, the one with the largest total is used (the coverage job reruns the tests). For `security`, the check's `job_name` is `<stage/job>, <UTC time of the artifact>`.

Files come from the job's checkout; see [Pipelines](pipelines.md#artifacts) for the paths conveyor looks at and the artifact names (`summary.json`, `index.html`, `security-summary.json`, `security.html`). They are downloaded from warehouse (at most 16 MiB) and need `WAREHOUSE_URL`; without it `tests` and `security` never appear. The stored HTML page of a check is served by `GET /api/v1/repos/{id}/scan/{category}/report`, sandboxed so it cannot reach the session.

## Scanner findings

`GET /api/v1/repos/{id}/scan/findings` reads the artifact `security-findings.json` (declared by the nightly job as `artifacts = ["target/security-findings.json"]`; the file name is what is matched) of the newest run that kept one: a JSON array of objects with `severity`, `check`, `service`, `method`, `path`, `detail`. Severity is lower-cased; the order is most severe first (`critical`, `high`, `medium`, `low`, `info`; unknown levels last), then service, check, path, detail. The browser filters by level locally.

## Wire shapes

`CheckChip` (`kind`, `passed`, `findings` (count), `metric`), `CheckDetail`, `Finding`, `ScanReport`, `ScanFinding`, `ScanFindings`: see `components.schemas` of [openapi.yaml](openapi.yaml). The chip class in the browser: no result -> `chip-none` with label `-`; no findings -> `chip-clean`; findings but the step passed -> `chip-warning`; findings and failed -> `chip-danger`; the label is the metric if there is one, else the finding count.
