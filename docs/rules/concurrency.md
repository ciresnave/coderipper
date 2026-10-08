# Owning what runs and what is held: the CON rules

Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

## Why it matters

Concurrency bugs depend on timing, so ordinary tests rarely reproduce them, yet their consequences are
severe. The evidence that such bugs are common and costly is strong; the evidence for any one specific
discipline is weaker and rests on practice. The lasting lesson is that resource lifetimes are an
architectural matter. Who owns a file, socket, connection, task, block of memory or GPU buffer, who is
responsible for releasing it, and what limits how much of it can exist are design decisions, not details
to leave to whoever writes the code.

## How to apply it

**Shared state and locks**
- Share less mutable state: prefer handing ownership across, passing messages and using immutable data.
- Where locks are unavoidable, keep critical sections small, document the order in which locks are taken,
  do not call code you do not control while holding one, and never keep a lock across an await or a
  blocking call.
- Start atomics from the strongest memory ordering and justify each weaker one in a comment.
- Never paper over a race with a sleep.

**Tasks and async code**
- Keep blocking work out of async code by moving it to a pool made for blocking operations.
- Give every task an owner that waits for it, passes on its errors and can cancel it. A detached task is
  an exception and needs a stated reason (structured concurrency).
- Cancellation is cooperative. An async function that holds partial state when it can be stopped should
  say what a cancellation at each suspension point leaves behind.

**Limits and lifetimes**
- Bound queues, parallelism, retries, memory use and waiting time, with a policy for backpressure and for
  overflow.
- Release resources with scope-based mechanisms (RAII, defer, try-with-resources, using, context
  managers) so error paths release too. Size pools deliberately and finish outstanding work at shutdown.

## Background and lineage

The field grew from Dijkstra's semaphores and Hoare's monitors in the 1960s and 70s. Two message-based
models followed: Hoare's communicating sequential processes and Hewitt's actors, the latter carried into
practice by Erlang. Java's memory model (2004) and Goetz's book made shared-memory rules explicit for
mainstream developers. Go popularised goroutines with channels, and async/await spread through
mainstream languages in the 2010s. Structured concurrency, in which tasks have owners and bounded
lifetimes, was articulated by Sustrik (2016) and Smith (2018) and appears in Trio, Kotlin and Java's task
scopes. Rust's ownership rules together with the `Send` and `Sync` traits give freedom from data races in
safe code. On the verification side, the tools are race detectors, thread sanitizers, testing across
permuted schedules (loom, shuttle), simulation runs with deterministic scheduling, and Jepsen for distributed systems.

## Measures and numbers

The source gives no numeric thresholds for this domain. The quantities to choose and record are queue
capacities, pool sizes, timeouts and retry limits; none has a universal default.

## Cautions

- For Rust, loom and shuttle probe concurrency algorithms across schedules, while Miri examines `unsafe`
  code.
- Race detectors, loom, shuttle and Miri document their limits: they cover only the schedules and
  platforms they explore. A clean pass is evidence, not proof.
- Over-application: lock-free designs where a plain mutex is enough, and actor-everything architectures
  that bring the failure modes of distributed systems inside a single process.
- Several rules (CON-009, CON-010) end in human or model judgement; automation can only surface candidates.

## Where it is checked

None of these rules has a CodeRipper check today. All are catalog records only, so the description and
guidance are available but no finding is produced.

| Rule | Title | Class | Checked by |
| :--- | :--- | :--- | :--- |
| CON-001 | No data races | P | catalog record; no check yet (needs a parser; the proof is the compiler or a race detector run) |
| CON-002 | No lock held over a suspension point | P | catalog record; no check yet (needs a parser; Clippy has lints for this) |
| CON-003 | No thread-blocking calls in async code | P | catalog record; no check yet (needs a parser and a list of blocking APIs; stall detection is runtime) |
| CON-004 | Bounded queues, channels and spawns | P | catalog record; no check yet (needs a parser; the overflow policy lives in the specification) |
| CON-005 | No unsupervised detached tasks | P | catalog record; no check yet (needs a parser) |
| CON-006 | No lock-order cycles | X | catalog record; no check yet (needs execution: deadlock detectors and schedule permutation) |
| CON-007 | Deterministic release of resources | P | catalog record; no check yet (needs a parser; leak counters run in tests) |
| CON-008 | Timeouts on externally dependent awaits | P | catalog record; no check yet (needs a parser; exceptions are declared in the specification) |
| CON-009 | Cancellation safety is documented | L | catalog record; no check yet (needs a human decision; a model can only propose candidates) |
| CON-010 | Weak atomic orderings are justified | P | catalog record; no check yet (comment presence is mechanical, comment quality needs judgement) |
| CON-011 | Shared state has a declared owner | A | catalog record; no check yet (needs a declared architecture) |

## Further reading

- *Java Concurrency in Practice* by Brian Goetz and others
- "Communicating Sequential Processes" by C. A. R. Hoare
- Nathaniel J. Smith, "Notes on structured concurrency, or: Go statement considered harmful" (2018)
- Martin Sustrik, "Structured Concurrency" (2016)
- Documentation of loom, shuttle, Miri and ThreadSanitizer
