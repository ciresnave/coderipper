# Speed by design: budgets and measurement

Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

## Why it matters

Performance is a requirement, and architecture settles most of it: the choice of algorithms, data layout, allocation habits, the
shape of I/O and the concurrency model are all costly to change late. Yet optimising too early wastes effort and harms clarity.
The way through combines stated budgets with measurement: say what each critical path must achieve, and keep watching for regressions. The
evidence is strong for measurement-driven work and weak, or folklore, for broad claims that a coding style is fast or slow.

## How to apply

### Decide what matters

- Declare budgets for paths tagged as critical: latency percentiles, throughput, memory, start-up time, and utilisation targets for
  code bound to an accelerator.
- Report tail latency alongside means, because averages hide what users feel.

### Measure before changing anything

- Profile first, and deal with algorithmic complexity and choice of data structure before micro-tuning.
- Make benchmarks part of every CI run, with noise controlled. Prefer deterministic instruction counts (callgrind-style) where they fit; otherwise
  compare statistically on dedicated hardware.
- Inspect query plans for the queries that matter.

### Spend effort only where it pays

- Eliminate allocation, copying, virtual calls and contended locks only in places where measurement shows they cost something, and
  record each speed-motivated departure from ordinary structure with a link to its benchmark.
- Watch for accidental quadratic behaviour and per-row query patterns.
- Bound growth: caches evict, and collections that take outside input have limits.

## Background and lineage

Knuth (1974) and Michael Jackson's rules set the stance on optimisation, and Pike's rules (1989) restated it. The tooling lineage
runs from gprof through modern profilers and flame graphs, with allocation profilers for memory. Thompson's mechanical sympathy (2011)
and Acton's data-oriented design (2014) explain why layout matters; Dean and Barroso's study of tail latency (2013) explains why
percentiles do. Compilers and databases benchmark continuously, and callgrind-based instruction counting gives noise-free CI runs.
Query-plan analysis plays the same role for databases.

## Measures and numbers

The material gives no fixed figures. The numbers are the project's own: percentile targets such as p95 and p99, throughput, memory
and start-up limits per tagged path, and the noise threshold that separates a real regression from jitter.

## Cautions

Style versus speed is contested: one camp defaults to polymorphism and small objects, the data-oriented camp says the price of
abstraction on hot paths is large. The default is clean code with documented, benchmarked exceptions, which is why PRF-006 is marked
contested. Do not optimise code nobody measured, do not gate merges on noisy wall-clock timings, and do not give up clarity for a
gain that no benchmark confirms. Most of these rules depend on tagging the paths that matter (a declaration the project must make)
and on running benchmarks, tests or telemetry, so a static scan can offer only partial help.

## Where it is checked

| Rule | Title | Class | Checked by |
| :--- | :--- | :--- | :--- |
| PRF-001 | Hot paths have budgets enforced by benchmarks | X | no check yet (catalog record only; needs benchmarks to run and a budget in a spec) |
| PRF-002 | Statistically detected benchmark regressions | X | no check yet (catalog record only; needs benchmark comparison tooling) |
| PRF-003 | Algorithmic red flags in code | P | no check yet (catalog record only; pattern scan needs a parser; Clippy perf lints are the model) |
| PRF-004 | Hot loops tagged for speed allocate nothing | P | no check yet (catalog record only; needs a parser and tagged paths; allocation counters confirm) |
| PRF-005 | Caches and collections lacking growth bounds | P | no check yet (catalog record only; needs a parser; pairs with CON-004) |
| PRF-006 | Performance exceptions documented with benchmark link | P | no check yet (catalog record only; marker presence is mechanical, linkage quality needs an LLM) |
| PRF-007 | Critical query plans avoid full table scans | X | no check yet (catalog record only; needs a live database) |
| PRF-008 | Tail latency tracked beside averages | X | no check yet (catalog record only; needs runtime telemetry) |

The legacy checks do not overlap with this domain.

## Further reading

- "Structured Programming with go to Statements", Donald Knuth (1974).
- Michael Jackson's rules of optimization.
- Rob Pike's rules of programming (1989).
- "The Tail at Scale", Jeffrey Dean and Luiz Barroso (2013).
- "Mechanical Sympathy", Martin Thompson (2011).
- Data-oriented design, Mike Acton (2014).
- Flame graphs, Brendan Gregg.
