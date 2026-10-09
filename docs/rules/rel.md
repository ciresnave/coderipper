# Surviving partial failure: the REL rules

Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

## Why it matters

Once a system spans processes and networks, some parts fail while others carry on. Advice that sounds
protective can backfire: retries pile extra load on a dependency that is already struggling, a queue
relocates a failure instead of preventing it, and an unbounded buffer turns delay into memory exhaustion.
The evidence is practice-based, drawn from many incident analyses. The lesson is that resilience means
choosing trade-offs openly, building them in, and then testing them. The claim that asynchronous messaging
on its own stops cascading failure is wrong as a general statement.

## How to apply it

**Calls leaving the process**
- Put a timeout on every external call and carry the remaining deadline from hop to hop.
- Retry only what is safe to repeat, with exponential backoff and jitter, a capped attempt count and an
  overall budget. Concentrate retrying in one layer so attempts do not multiply, and sort errors into retryable and
  permanent.
- Isolate shared dependencies with circuit breakers and bulkheads, and protect capacity with load shedding
  and backpressure.

**Queues and messages**
- Bound every queue. Expect growing backlog, poison messages, duplicate delivery, lost ordering,
  saturation and retry storms, and answer them with a dead-letter policy, handlers that tolerate repeats and
  deduplication keys.

**Running and stopping**
- Distinguish liveness from readiness, and drain in-flight work when shutting down.
- Measure durations with monotonic clocks, and give persistent components a defined durability protocol.

**Planning for failure**
- Take each component that relies on outside services and list its failure modes, their effects, how they
  are detected and how they are handled (a light failure-mode analysis).
- Design and test degraded modes: say what the service offers while a dependency is down, such as stale
  cache data or fewer features.
- Exercise all of this with fault injection and chaos experiments, including recovery after a hard kill.

## Background and lineage

Fault-tolerant design traces back to Tandem's systems and Jim Gray's analysis of them (1985), and to
Erlang/OTP with its supervision trees. Nygard's *Release It!* (2007) gathered the stability patterns:
timeouts, circuit breakers, bulkheads and a steady state. Netflix's Hystrix put breakers in a library; it
has since entered maintenance, and Resilience4j and service-mesh policies have taken over. Chaos engineering
grew out of Netflix in the 2010s. Google's SRE book contributed service-level objectives, error budgets
and guidance on overload and cascading failure. For transactions across services, the saga idea dates to
1987, with idempotency keys and the outbox pattern as supporting techniques. Backoff with jitter came from
AWS engineers, "The Tail at Scale" addressed latency variance, and deterministic simulation testing is a
newer way to explore failures.

## Measures and numbers

The source gives no fixed thresholds for this domain. The tunable quantities are the timeout, the retry
attempt cap and budget, queue capacity, and the error budget implied by a service-level objective; each
should be chosen per dependency and recorded, not copied from a default.

## Cautions

- Over-application: retries in every layer (retry storms), circuit breakers that nobody tuned, and
  resilience machinery wrapped around in-process calls where returning an error is the right answer.
- REL-003 is the reliability view of the same bounded-buffer concern as CON-004.
- REL-012 judges service-level objectives from live telemetry, so it needs network access to a data source.

## Where it is checked

No rule in this domain has a CodeRipper check yet; they are catalog records. Some name outside tools the
triage considered (Semgrep-style patterns for REL-001; OPA, Kyverno, Conftest or Checkov for REL-005;
Toxiproxy for REL-008).

| Rule | Title | Class | Checked by |
| :--- | :--- | :--- | :--- |
| REL-001 | Outbound calls have deadlines | P | catalog record; no check yet (needs a parser; Semgrep patterns are a candidate) |
| REL-002 | Retries follow a safe policy | P | catalog record; no check yet (needs a parser; judging idempotency needs a model) |
| REL-003 | Bounded channels with overflow policy | P | catalog record; no check yet (needs a parser; duplicates CON-004) |
| REL-004 | Idempotent message handlers | L | catalog record; no check yet (needs a human decision or a model, plus replay tests) |
| REL-005 | Probes and resource limits in manifests | N | catalog record; no check yet (needs YAML and IaC parsing; policy engines cover it) |
| REL-006 | Graceful shutdown | P | catalog record; no check yet (needs a parser; the shutdown test runs code) |
| REL-007 | Failure model per component | N | catalog record; no check yet (presence is checkable; sign-off is a human decision) |
| REL-008 | Fault-injection tests for declared failures | X | catalog record; no check yet (needs execution of tests; Toxiproxy-style tooling) |
| REL-009 | Monotonic clocks for durations | P | catalog record; no check yet (needs a parser) |
| REL-010 | Crash-recovery tests for stateful components | X | catalog record; no check yet (needs execution of a crash harness) |
| REL-011 | Breakers and bulkheads where required | A | catalog record; no check yet (needs a declared architecture) |
| REL-012 | SLO conformance checked at runtime | X | catalog record; no check yet (needs execution and live telemetry) |

## Further reading

- *Release It!* by Michael Nygard
- *Site Reliability Engineering* (Google)
- "Principles of Chaos Engineering"
- "Exponential Backoff and Jitter" (AWS Architecture Blog) and the Amazon Builders' Library
- "The Tail at Scale" by Jeffrey Dean and Luiz Andre Barroso
- "Sagas" by Hector Garcia-Molina and Kenneth Salem
- "Why Do Computers Stop and What Can Be Done About It?" by Jim Gray
