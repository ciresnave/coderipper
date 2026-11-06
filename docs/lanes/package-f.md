# Package F: Rust concurrency (CON-002 guard held across a suspension point, CON-003 blocking calls in async code, CON-004 + REL-003 unbounded queues)

Read [README](README.md) first, then the rows of these rules in [`docs/rules/REMAINING.md`](../rules/REMAINING.md). All are Rust, source only (`syn`), per package. Unlike the ARC/MOD packages they are
not architecture checks: they are **neutral-module checks**, so the loop in the README applies except step 2 (register in `src/module/neutral/mod.rs` the way `lock_order.rs` and `sec004.rs` are; read how
`conformance/CON-006` and `conformance/SEC-004` are laid out) and the profile (the `classic` column of `builtin_checks!` in `src/builtin.rs` is the model; read how the neutral module checks are kept out of `classic`).

**CON-002** (`rules/concurrency.toml`): a guard (`MutexGuard`, `RwLock*Guard`, `RefCell` `Ref`/`RefMut`, or a name bound from `.lock()`, `.read()`, `.write()`, `.borrow()`, `.borrow_mut()`) still in scope at an
`.await` inside an `async fn` or `async` block. Copy the guard naming and visiting of `lock_order.rs` (CON-006; patent idea 19 swept the neighbouring mechanism: read that row first). Syntactic only: a guard moved
into `drop(g)` or in a block that ends before the await is not held; a guard from a `tokio::sync` lock is meant to be held across awaits and is not reported. Clippy has `await_holding_lock` and
`await_holding_refcell_ref`, so measure what yours adds over clippy; if it adds nothing on the corpus, say so in the PR rather than ship a duplicate. Ship at Medium until the sample is read.

**CON-003**: a call that blocks the thread inside an async fn: `std::thread::sleep`, `std::fs::*`, blocking `std::net` connects, `reqwest::blocking`, `.recv()` on a `std::sync::mpsc` receiver. A call inside
`spawn_blocking`, `block_in_place` or a closure handed to one is fine. Copy the walk of `efx003.rs`. Keep the list of blocking paths short and named in the finding; a path you are unsure about is left out, not guessed.

**CON-004 and REL-003** (one detection, claim both ids: see how `src/module/delegate/osv.rs` claims two): the unbounded constructors, `unbounded_channel()`, std `mpsc::channel()` (unbounded by default),
`crossbeam::unbounded()`, and a `spawn` inside a loop over an input of unknown size, only when the text beside the constructor shows no capacity and no stated overflow policy. A comment or doc line that states
the overflow policy next to the constructor silences it (that is what the rules ask for). Info, low confidence, opt-in until measured.

Measure on the corpus (README step 4), committed trees. Expect CON-002 to be the quiet one and CON-004 the noisy one; read every CON-002 hit.
Patent read first for each: lock-held-across-await detection (clippy lints are prior art; expected hits are lint tools), blocking-in-async detection.

Done when: the checks are registered, fixtures (`defective` and `clean` per rule), corpus numbers in the PR, patent rows, fragments, READY. Order: CON-002 (highest value), then CON-004/REL-003, then CON-003.
