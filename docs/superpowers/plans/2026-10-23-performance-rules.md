# Performance rules (`PRF`) — design

> **Status: DESIGN, for PM review. Nothing here is built.** Task from the PM, 2026-10-23, carrying CireSnave's words: *"Get the CodeRipper lane
> building that performance rules design document"*; he also wants the ruleset expanded beyond Clean Code and Clean Architecture. This document
> extends the catalogue records PRF-003 to PRF-006 (`rules/prf.toml`, `docs/rules/prf.md`); it does not replace them. No rule below ships until
> it passes the precision gate in section 4.

## 1. The statically checkable rules

Every rule is a reading of the source text with a parser (`syn` for Rust). **None can know types**, so each rule's precision note says what it
guesses. "Loop" means the body of `for`, `while` or `loop`, and a closure written inside one; a test (`#[test]`, `#[cfg(test)]`), `tests/`,
`benches/`, `examples/` and `build.rs` are not read. Severity is Low unless stated; none is blocking (section 4).

| id (proposed) | rule | what it reads | precision note | positive control |
|---|---|---|---|---|
| PRF-009 | **Regex compiled in a loop** | `Regex::new` / `RegexSet::new` inside a loop | Near-exact: the pattern is almost always loop-invariant. Not exact when the pattern is built from the loop variable. | `for x in xs { let re = Regex::new("a+")...}` finds it; the same call hoisted above the loop does not |
| PRF-010 | **Database or network call in a loop** (N+1) | a call named `execute`, `query`, `query_one`, `query_as`, `fetch_*`, `query_row`, `send`, `reqwest::get`, `ureq::get` in a loop | Name-based, so `send` on a channel matches. Needs a type-aware pass (or a configured list of client types) before it ships default-on. | a loop calling `db.execute(..)` finds it; a channel `send` in a loop is the known false positive |
| PRF-011 | **Unbuffered file or socket I/O** | a function that opens `File`, `OpenOptions` or `TcpStream`, writes or reads in small calls (`write!`, `writeln!`, `write_all`, `read_line`, `read_exact`) inside a loop, and names no `BufReader`, `BufWriter` or `LineWriter` | The wrapper may be created in a caller or hold the handle in a field; it misses a handle passed in. Reports a pattern that is often cold (build scripts). | `File::create` + `writeln!` in a `for`: found; the same with `BufWriter::new(f)`: not found |
| PRF-012 | **Linear search in a loop** (O(n²)) | `.contains(x)` on a non-literal receiver, or `.iter().position/find/any/all(..)`, inside a loop | The receiver's type is unknown: `HashSet::contains` is O(1) and matches. Only a configured set of known collection names, or a type pass, makes it precise. | `for x in items { if other.contains(x) {} }` with `other: &Vec<_>`: found |
| PRF-013 | **Nested loops over the same collection** | a `for` whose iterated path equals an enclosing `for`'s (after stripping `&`, `.iter()`, `.iter_mut()`, `.into_iter()`) | Cross-product loops are often the intent (a dtype x dtype table). Never flag a range. Cannot see that the inner loop could be a map lookup. | `for x in &v { for y in &v {..} }`: found; `for i in 0..n { for j in 0..n }`: not found |
| PRF-014 | **`Vec::new()` then push in the next loop** | `let mut v = Vec::new();` and, within the next three statements, a `for` whose body calls `v.push(` | The loop's length is often unknown (`break`, `continue`, `filter`), where `with_capacity` helps little. Fires only when the iterated expression is a range, a slice, a `.len()`-bearing iterator or a collection. | the control from the prototype (below) finds it |
| PRF-015 | **Collect, then iterate** | `.collect::<..>().iter()` / `.into_iter()` as one chain | Exact on the chain; the collect is sometimes needed for ownership or a second pass. | `(0..3).collect::<Vec<_>>().iter()`: found |
| PRF-016 | **Allocation in a loop** (`format!`, `.clone()`, `.to_string()`, `.to_owned()`, `Vec::new`, `vec![]`, `String::from`, `Box::new`) | each of these inside a loop | **Low precision (section 6: 0 of 7 sampled were worth changing).** Most are error paths, outputs that must be built, or required copies. Ships only under `[perf] hot_paths` (section 4). | each form in a loop is found; outside a loop it is not |
| PRF-017 | **String concatenation in a loop** | `x += &..`, `x += "..."`, `x += format!(..)` in a loop | The target's type is unknown; `+=` on a `String` is amortised O(1) via `push_str` anyway, so the real defect is `s = s + &t` (quadratic copy). Check the `s = s + ..` form first. | `out += &x[..]` in a loop: found (a weak finding by itself) |
| PRF-018 | **Pointer-chasing containers** | `Vec<Box<T>>` (not `Box<dyn Trait>`), `LinkedList`, `Rc<RefCell<T>>` as a type anywhere | Exact on the type, but whether it is on a hot path is not static: only under `[perf] hot_paths`. | the three types in a struct: found |
| PRF-019 | **`repr(C)` padding waste** | a `#[repr(C)]` struct of primitive fields only whose C layout is at least 25% larger than its packed size | Exact for primitive fields; skipped when any field is not a primitive. FFI structs must keep the C layout, so the finding says "reorder if you own the layout". | `u8, u64, u8` (24 bytes vs 16): found |

PRF-009 to PRF-019 are proposals; the PM or CireSnave may renumber. PRF-003 stays the umbrella record: PRF-009 and PRF-012 to PRF-017 are the
checks it was written for (its record says "pattern scan needs a parser; Clippy perf lints are the model"). PRF-004 (hot loops allocate nothing)
is PRF-016 under a declared hot path. PRF-005 (growth bounds) is a separate rule, not touched here. **Clippy already has** `regex_creation_in_loops`,
`manual_memcpy`, `unnecessary_to_owned`, `redundant_clone`, `iter_nth`, `needless_collect` and `format_collect`; where one exists CodeRipper
should require it to be enabled (the lint-suppression rule already finds an `#[allow]` without a reason) and add only what Clippy lacks: scoping by
`hot_paths`, the portfolio count, the benchmark-evidence waiver, and the database and I/O shapes.

## 2. What is NOT statically checkable, and must never become a rule

Each of these depends on the hardware, the compiler's output or the data, not on the source text:

- **Branch predictability.** Whether a branch is predictable depends on the input data.
- **Auto-vectorisation.** Whether the compiler vectorises a loop depends on the compiler version, target features and flags; the source gives no answer. (A rule may *require a benchmark* beside a claim; it may not claim to know.)
- **Cache-line behaviour, false sharing, prefetching, NUMA, memory bandwidth.** They depend on access patterns at run time and the machine.
- **Whether code is hot.** Only a profile knows. This is why every loop rule is scoped (section 4) and why a finding says "in a loop", never "slow".
- **Whether an allocation matters.** A `format!` once per request is fine; once per element of a 100-million-element batch is not. The source has no sizes.
- **Inlining, monomorphisation cost, code-size effects.**

A rule for any of these would be guessing, and a wrong "fix" costs more than the finding saves. They are measured by benchmarks and profiles, which is PRF-001 and PRF-002 (`X`, runtime telemetry).

## 3. "Never optimise without profiling", as a check (PRF-020)

**Rule:** performance-motivated code in a crate with no benchmark is a finding, *"performance claim without a benchmark"*. Claims are read from the AST, not from prose:

- `#[inline(always)]`;
- `get_unchecked`, `get_unchecked_mut`, `unwrap_unchecked`, `from_utf8_unchecked` (and `unsafe` blocks that contain only these);
- a `#[repr(...)]` whose neighbouring comment says speed, and `unsafe` whose adjacent non-doc comment contains "for speed", "for performance" or "perf" (a narrow marker; see the measurement below).

**"Has a benchmark"** means the crate (the nearest `Cargo.toml` with a `[package]`) has a `benches/` directory, a `[[bench]]` table, or `criterion`, `divan` or `#[bench]` in its manifest or sources; a declared `[perf] benchmarks = [".."]` entry links a claim to a benchmark elsewhere in the workspace (the PRF-006 marker, with a link that must resolve). **Precision note:** a benchmark can exist outside the crate (a workspace-level bench crate), or in a form the rule does not know (fuel measures at run time through its own judge and `profile_oracle`; that is evidence, but not a `cargo bench`); the finding therefore says *"no benchmark found in this crate"* and the waiver is the declared link. **Positive control:** an `#[inline(always)]` fn in a crate with no bench: found; the same fn after a `benches/` file is added: not found.

This rule is the enforceable core of PRF-006 and of the repository's own "default is clean code with documented, benchmarked exceptions" stance (`docs/rules/prf.md`). It does not say the code is slow or fast.

## 4. Scoping, so it is not noise

1. **Rules fire only where speed is plausible.** The loop rules (PRF-009 to PRF-017) fire inside a loop, as defined above. PRF-016 and PRF-018 (the two with the worst measured precision) fire **only** in a module or function declared hot in the configuration: `[perf] hot_paths = ["fuel-cpu-kernels", "src/kernels/"]` (crate names or path prefixes), and a function carrying a marker attribute or doc line `#[coderipper::hot]`. Outside a declared hot path they are silent.
2. **Profile.** The rules are in the `perf` group. *Default-on policy (section 7)*: the exact rules (PRF-009, PRF-015, PRF-019, PRF-020) are default-on in `extended`; the name-based and type-blind ones (PRF-010, PRF-011, PRF-012, PRF-013, PRF-014, PRF-016, PRF-017, PRF-018) are **opt-in until they pass the precision gate**. The `portfolio` profile picks up a perf rule only when it is default-on, and only after a [NOTICE].
3. **Severity** is Low (Info for PRF-016 and PRF-018). Nothing here ever blocks (default-on never means blocking; a project makes a rule required for itself with `blocking = [..]`).
4. **A waiver needs evidence.** An `[[allow]]` entry for a perf rule must carry a `reason` that links a benchmark or profile (a path in the repository, or a URL); `reason = "fast enough"` would be rejected by the allowlist loader for these ids (a **proposed** check, not built).
5. **No finding in a cold path.** Error paths (`.map_err(|e| ..)`, `Err(..)` arms, `panic!`/`unreachable!` arguments), `main`, argument parsing and `Drop` are skipped for the allocation rules, because one of the sampled allocation findings was on an error path (`.map_err(|e| ..)`) and the rest were required copies.

## 5. How it avoids contradicting the Clean Code rules

The catalogue's stance (`docs/rules/prf.md`): *clean code first; a departure for speed carries a benchmark link*. The perf rules follow it in three ways.

- **A perf finding never asks for less clear code without evidence.** Hoisting a `Regex`, adding `with_capacity`, wrapping a file in `BufWriter`, replacing `Vec::contains` with a set, and removing a `collect` are as clear as what they replace, or clearer. Those are the only fixes a default-on rule suggests.
- **Clarity-costing changes are not suggested; they are *policed*.** `#[inline(always)]`, `unsafe` for speed and manual layout are exactly what PRF-020 demands a benchmark for. That is the clean-code rule ("no speculative complexity") enforced, not contradicted.
- **Where the two could pull apart, the clean-code rule wins by default.** A table:

| perf rule | clean-code rule it could fight | resolution |
|---|---|---|
| PRF-016 (clone / format! in a loop) | ownership clarity, small functions, no premature optimisation | only in declared hot paths; elsewhere silent |
| PRF-014 (`with_capacity`) | none, but a wrong capacity hides intent | fires only when the length is evident from the iterated expression |
| PRF-018 (`Vec<Box<T>>`, `Rc<RefCell<T>>`) | trait objects and shared ownership are legitimate design (`Box<dyn Trait>` is excluded) | hot paths only; a design decision is waived with a link to why |
| PRF-019 (`repr(C)` padding) | an FFI layout is dictated by the other side | the message says to reorder only a layout we own; waiver = "FFI layout" |
| PRF-020 (claims need a benchmark) | none: it enforces "no unmeasured complexity" | n/a |

## 6. Measurement on the portfolio (the precision numbers that decide what ships)

**Method.** A throwaway `syn` visitor (not shipped code; kept in the author's scratch directory) implemented the heuristics in section 1 and the claim reader of section 3, and read every non-test `.rs` file that `git ls-files '*.rs'` lists, outside `tests/`, `benches/`, `examples/` and `build.rs`. Refs: **fuel `5a00e3a`** (636 files), **baracuda `8e162f7`** (496), **Unpopped `21e6279`** (36), each a depth-1 clone of the default branch on 2026-10-23, read-only. **The prototype is a heuristic reading, not the shipped rule; a shipped rule would add the scoping of section 4, which removes some of these counts.**

**Positive control.** A 57-line fixture that seeds one instance of each pattern (`control/src/lib.rs`) was run first. The first run missed two patterns (unbuffered I/O, whose loop test never matched, and `Vec::<T>::new()`); both were fixed and the fixture then found all 19 pattern kinds it seeds (the 16 pattern kinds of section 1 and the 3 claim readers). A zero below therefore means "the fixed prototype saw none of that pattern".

**Finding counts** (prototype heuristics, before section 4's scoping):

| pattern | fuel | baracuda | Unpopped |
|---|---|---|---|
| `loop:format!` | 1841 | 159 | 26 |
| `loop:clone` | 462 | 12 | 20 |
| `loop:alloc-new` | 394 | 7 | 10 |
| `loop:to_string` | 144 | 8 | 7 |
| `loop:contains` (type unknown) | 85 | 3 | 7 |
| `vec-new-then-push-loop` | 51 | 11 | 3 |
| `loop:iter-linear-search` | 32 | 3 | 2 |
| `loop:to_owned/to_vec` | 28 | 0 | 0 |
| `collect-then-iterate` | 2 | 0 | 0 |
| `nested-loop-same-collection` | 2 | 1 | 0 |
| `unbuffered-io` | 0 | 2 | 0 |
| Regex in a loop, DB/network call in a loop, `+=` string concat, `Vec<Box<T>>`, `LinkedList`, `Rc<RefCell<T>>`, `repr(C)` padding | 0 | 0 | 0 |
| `claim:inline-always` | 82 | 0 | 0 |
| `claim:unchecked` | 5 | 0 | 0 |
| `claim:comment` (keywords) | 95 | 31 | 12 |

The prototype saw **none** of the exact structural defects (regex-in-loop, N+1, pointer-chasing types, padded `repr(C)`) in these three repositories, which is a result of its own: those rules add no noise to ship, and a seeded fixture is their only test until a project has one. All 87 claims by `inline(always)` and `*_unchecked` (82 + 5, all in fuel) are in crates with no `benches/`, no `[[bench]]` and no `criterion` or `divan` dependency (the PRF-020 shape), in 5 fuel crates (`fuel-cpu-backend`, `fuel-cpu-kernels`, `fuel-examples`, `fuel-metal-kernels`, `fuel-quantized`); fuel measures at run time instead, which is why the rule asks for a *link*.

**Sampled findings, with a verdict each.** Twelve findings were drawn with a fixed seed (42), stratified by pattern (not cherry-picked), and each was read in context. **True positive** = the pattern is present and avoidable at no cost to clarity; **false positive** = it is needed, on an error path, or is the intent.

| # | repo / pattern / site | verdict | why |
|---|---|---|---|
| 1 | fuel `loop:format!` `fuel-transformers/.../lazy_quantized_qwen3_moe.rs:256` | **false** | inside `.map_err(|e| ..)`: runs only on the error path |
| 2 | fuel `loop:format!` `fuel-lazy-examples/src/bin/gguf-inspect.rs:23` | **false** | the output string of an inspector tool is the product; one per value is needed |
| 3 | fuel `loop:clone` `fuel-dispatch/src/fkc/lower.rs:764` | **false** | the copy is mutated next (`extend_from_slice`); required |
| 4 | fuel `loop:clone` `fuel-tensor/src/judge/mod.rs:819` | **false** | cloned before a move, per the adjacent comment; required |
| 5 | fuel `loop:alloc-new` `fuel-graph/src/lib.rs:9933` | **false** | `vec![up_id]` because `push_node` takes a `Vec`; the API, not the caller, allocates |
| 6 | fuel `vec-new-then-push` `fuel-ir/.../header_check.rs:162` | **false** | the loop `continue`s over lines that do not match; the length is unknown |
| 7 | fuel `vec-new-then-push` `fuel-inference/.../tiered_storage.rs:267` | **false** | the loop `break`s when enough is freed; the length is unknown |
| 8 | fuel `loop:contains` `fuel-dispatch/.../seed_cuda_ledger.rs:463` | **true** | `only: Option<&[OpKind]>` is a slice: a linear search per table entry (small slice, cold code) |
| 9 | fuel `loop:iter-linear-search` `fuel-dispatch/src/jit_ingest.rs:1714` | **true** | `.iter().any(..)` over a growing ledger inside a loop over probes and records: quadratic in the ledger |
| 10 | baracuda `nested-loop-same-collection` `baracuda-cuda-emit/src/cuda.rs:7498` | **false** | the dtype x dtype cast table: a cross product is the intent |
| 11 | baracuda `unbuffered-io` `baracuda-forge/src/builder.rs:1046` | **true** | `writeln!` per object path into an unbuffered `File` (a build-time response file; small and cold) |
| 12 | baracuda `loop:to_string` `baracuda-types-derive/src/lib.rs:94` | **true** | `Ident` compares with `&str` directly, so `to_string()` is avoidable (a proc macro: cold) |

**Per pattern:** `loop:format!` 0 of 2, `loop:clone` 0 of 2, `loop:alloc-new` 0 of 1, `vec-new-then-push` 0 of 2, `nested-loop-same-collection` 0 of 1; `loop:contains` 1 of 1, `loop:iter-linear-search` 1 of 1, `unbuffered-io` 1 of 1, `loop:to_string` 1 of 1. **Overall 4 of 12 (33%).**

Eight claims were also read (seed 7): the four `claim:comment` hits were **all false** (a "hot path" in a doc comment about a correctness proof, the word "optimization" in a type name `OptimizationContext`, a benchmark library's comment "baracuda faster than PyTorch", a driver doc); the two `inline(always)` and two `get_unchecked` hits were **all true** perf-motivated departures (SIMD kernels and a copy loop). So the **comment-keyword reading is dropped** from PRF-020 (it keeps only a `// for speed` marker immediately beside `unsafe` or `inline(always)`), and the AST-based claims stay.

**What the numbers say, and what they do not.**
- The **allocation-in-loop family (PRF-016 and the `Vec::new` shape) is 0 of 7 in this sample.** It is noise unless scoped by `hot_paths`, which is why it is opt-in. This is the most useful result.
- The **search and I/O shapes (PRF-011, PRF-012) were 3 of 3 true**, but all three sat on cold code. They are real patterns whose value depends on where they are, which scoping must supply.
- **The sample is far too small to decide a bar.** Twelve findings, one to two per pattern, cannot establish 80%. The proposed gate (section 7) needs at least 30 per rule; these figures only rank the order to measure in.
- Three of the three repositories are one author's HPC and tooling code; a web-service portfolio would produce different counts (N+1 and regex-in-loop would matter).

## 7. Plan, rule by rule (design only; no build is scheduled)

**Precision gate (proposed bar).** A rule joins the *default* when, on at least 30 findings sampled with a recorded seed across at least three portfolio repositories, at least **80% are true positives** by the definition above (and the sample is committed beside the rule). Below the bar the rule ships **opt-in**. A rule with a zero count on the portfolio (nothing to sample) ships default-on only if it is *exact on its syntax* (PRF-009, PRF-015, PRF-019) and is proven by a seeded fixture both ways.

| order | rule | ships | needs first |
|---|---|---|---|
| 1 | PRF-020 performance claim without a benchmark | default-on (AST claims only) | `[perf] benchmarks` link syntax; the waiver-evidence check; fixture pair |
| 2 | PRF-009 regex in a loop, PRF-015 collect-then-iterate, PRF-019 `repr(C)` padding | default-on | fixture pair each; require the matching Clippy lints to be enabled |
| 3 | PRF-011 unbuffered I/O, PRF-012 linear search in a loop | opt-in until 30-finding sample passes | scoping to `hot_paths`; a collection-type allowlist for PRF-012 |
| 4 | PRF-013 nested loops, PRF-014 `with_capacity` | opt-in | evident-length test for PRF-014; range exclusion for PRF-013 |
| 5 | PRF-016 allocation in a loop, PRF-018 pointer-chasing | opt-in, `hot_paths` only | `[perf] hot_paths`; the cold-path skips of section 4 |
| 6 | PRF-010 N+1 | opt-in | a configured list of database and HTTP client types |
| 7 | PRF-017 string concat | opt-in | the `s = s + ..` form first |

Releases: each is a **Breaking** change for consumers only when it is default-on (a new finding for an existing project; pre-1.0 that is the second number), so steps 1 and 2 go together in one release and the opt-in rules ride later patch releases. Network checks stay in the sweep tier; none of these uses the network.

## 8. Open points for the PM

1. **Numbering and umbrella.** Keep PRF-003 as the umbrella and add PRF-009+, or split PRF-003 into the rules above?
2. **Marker syntax.** `[perf] hot_paths` and `#[coderipper::hot]` (an attribute needs a proc-macro or `cfg_attr`-free marker comment): prefer a comment marker `// coderipper: hot` so no dependency is added.
3. **Where the portfolio runs the opt-in rules:** the weekly sweep, with the counts as evidence, never a pull-request job.
4. **Measuring beyond the three repositories** to reach 30 per rule is a follow-up: it needs the prototype turned into the real rule so the sample is of what ships.
