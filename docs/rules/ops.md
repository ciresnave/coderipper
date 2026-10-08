# Running software you can see into: configuration, signals and on-call readiness (OPS)

Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

## Why it matters

Clean source does not make a system maintainable if nobody can watch it, configure it safely or work out what
went wrong in production. Much of what makes up the real architecture sits in configuration and in the runtime
environment, where source-level analysis hardly reaches. Site-reliability and DevOps practice therefore treats
operability as a design property. The support is broad practitioner consensus, not controlled studies.

## How to apply

**Settings**
- Parse every setting into a typed value once, at startup, with documented defaults; refuse to start on a bad value.
- Offer a single access point, with no ad-hoc environment reads elsewhere, and keep secrets out of ordinary
  configuration and out of committed files.
- Delete feature flags once their rollout ends, and give each one a named owner and an end date while it lives.

**Seeing the system**
- Log through a structured facility with levels and correlation identifiers; keep secrets and personal data out
  of records. Libraries hand records to the application's logger and never print.
- Report the golden signals (latency, traffic, errors, saturation) for each endpoint and dependency, and bound
  the number of distinct label values.
- Carry the trace context over every outbound call so a request can be followed end to end.
- Distinguish a liveness probe from a readiness probe, and let a running artifact state its version and build.

**Being on call**
- Page on symptoms users feel, link alerts to service-level objectives, make them actionable, and keep a runbook
  for each page.

## Background and lineage

The Twelve-Factor App (settings held in the environment, logs treated as event streams, disposable processes,
parity between development and production); distributed tracing in the style of Google's Dapper (technical
report, 2010); the golden signals, the RED method and the USE method; OpenTelemetry (2019, the merger of
OpenTracing and OpenCensus) as the shared standard covering traces, metrics and logs, together with its semantic conventions;
flag lifecycle management; SLO-based alerting and runbooks.

## Measures and numbers

This domain defines no numeric threshold. The only quantity to manage is label cardinality in metrics, which
should stay small enough for the metrics backend to store cheaply.

## Cautions

- Several rules can only be confirmed by running the system (trace propagation, metrics, probes), so any
  automatic check is a presence check on instrumentation, not proof the signal is correct.
- Rules about alerts and objectives (OPS-009) depend on documents and tooling outside the repository; treat
  findings as prompts for a human review.
- Secrets in configuration are covered in depth by the security domain; OPS-006 only points there.

## Where it is checked

No rule in this domain is run by the current release; every row below is a catalog record only.

| Rule | Title | Class | Checked by |
| :--- | :--- | :--- | :--- |
| OPS-001 | One typed, validated configuration entry point | P | catalog record; no check yet (needs a parser) |
| OPS-002 | Config keys are documented, defaulted and used | P | catalog record; no check yet (needs a parser) |
| OPS-003 | Structured logging, no raw printing in libraries | P | catalog record; no check yet (needs a parser); planned: Clippy print and dbg lints, ESLint no-console |
| OPS-004 | Trace context follows outbound calls | P | catalog record; no check yet (needs a parser and runtime tests that assert propagation) |
| OPS-005 | Golden-signal metrics for endpoints and dependencies | P | catalog record; no check yet (needs a parser and a running service) |
| OPS-006 | No secrets in configuration files | N | catalog record; no check yet (delegates to the secrets rule SEC-002) |
| OPS-007 | Flags need an accountable owner and a sunset date | N | catalog record; no check yet (a registry check is static, staleness needs history) |
| OPS-008 | Liveness and readiness endpoints exist | P | catalog record; no check yet (needs a parser and probe tests) |
| OPS-009 | Alerts are tied to objectives and have runbooks | N | catalog record; no check yet (needs a human decision and external alert definitions) |
| OPS-010 | Build and version information is exposed | N | catalog record; no check yet (needs inspection of build scripts) |

## Further reading

- The Twelve-Factor App, Adam Wiggins
- Site Reliability Engineering, Beyer, Jones, Petoff and Murphy (Google)
- Dapper, a Large-Scale Distributed Systems Tracing Infrastructure, Sigelman et al. (Google, 2010)
- OpenTelemetry specification and semantic conventions
