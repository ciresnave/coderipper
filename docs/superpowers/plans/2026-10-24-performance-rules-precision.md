# Performance rules: the precision sample (30 per candidate rule)

> **Status: MEASUREMENT, for PM review. Nothing here is built.** The PM's task of 2026-10-23: *"the precision sample to 30 per candidate
> rule"*, the gate of `2026-10-23-performance-rules.md` section 7. **Result: no rule clears the gate (at least 80% true positives on at
> least 30 sampled findings). The allocation-in-loop family is 0 of 42.** The two rules that look usable are small populations that need a
> larger portfolio before they can be sampled to 30.

## 1. Method

**Detector.** The throwaway `syn` visitor of the design document, **refined to the scoping that document's section 4 promises**, so the
sample measures what would ship and not the raw heuristics:

- the allocation rules (`format!`, `.clone()`, `.to_string()`, `.to_owned()`/`.to_vec()`, `Vec::new`/`vec![]`/`String::from`) are silent in an
  error path: the closure of `map_err`, `ok_or_else`, `unwrap_or_else`, `or_else`, `map_or_else`; the arguments of `Err(..)`; `fn main`;
  an `fmt` implementation;
- `contains` is reported only when the receiver is a plain identifier the function declares as a `Vec` or a slice (a parameter, or a `let` with
  a `Vec` type or initialiser): a `HashSet::contains` is not;
- `Vec::new()` then push is reported only when the next loop runs over a range or a collection, pushes once per turn as a direct statement,
  and has no `continue`, `break` or `return`.

**Controls (run before the sample).** `control/src/lib.rs` seeds one instance of each of the 19 pattern kinds and the refined detector finds
all 19 (the original control, kept). `control/src/refined.rs` seeds four cases that must **not** fire (a `format!` inside `map_err`; a
`HashSet::contains`; a push behind a `continue`; a `format!` in `fn main`) and two that must (a `Vec::contains`; one push per turn): it reports exactly
those two. Both files are in `perf-prototype/`, with the detector, the sampler and the table generator, as `.txt` so they are not built or
scanned as code.

**Population.** Every non-test `.rs` file that `git ls-files` lists, outside `tests/`, `benches/`, `examples/` and `build.rs`, at **fuel
`5a00e3a`** (636 files), **baracuda `8e162f7`** (496) and **Unpopped `21e6279`** (36): depth-1 clones of the default branch on 2026-10-23.
The refined counts: `loop:format!` 1752, `loop:clone` 474, `loop:alloc-new` 385, `loop:to_string` 127, `loop:iter-linear-search` 37,
`loop:to_owned/to_vec` 28, `loop:contains(vec)` 23, `vec-new-then-push-loop` 9, `nested-loop-same-collection` 3,
`collect-then-iterate` 2, `unbuffered-io` 2. (`Vec<Box<T>>`, `LinkedList`, `Rc<RefCell<T>>`, a regex compiled in a loop, an N+1 call and padded
`repr(C)` have **no** finding in these repositories; they cannot be sampled.)

**Order and stopping rule.** For each rule the findings are sorted by `(repo, file, line)` and shuffled with `random.Random(2026)`
(`show.py`); they are judged in that order. **Stop at 30 judged, or at 7 false positives, whichever comes first**: with 7 false positives
the 80% bar can no longer be reached within 30. A rule with fewer than 30 findings is read whole (a census), and cannot satisfy the "30
sampled" clause at all. Where a rule stopped early, the rows judged *beyond* the stop (the `to_owned/to_vec` rows 11 to 20) are kept in the table.

**Rubric.** A finding is **true** only if the change it suggests is a **strict improvement**: it removes the work at no cost in clarity
(or speed) to the reader. It is **false** if the allocation or the copy is needed (a by-value API, an owned field, a copy out of a lock's scope,
an error value), if the loop is the algorithm (a cross product, clique building, an overlap predicate), if the collection is bounded by a
small fixed domain (backends, devices, axes, kernel names) so that a set would not help, or if the benefit cannot be shown from the code
(counted false: a lower bound). A finding that is true but sits on cold code (a build script) is still true; where the code is cold the
table says so.

**Who judged.** One reader, this lane, with every verdict and its reason in the tables below so that anyone can dispute a row. The
repositories are one author's HPC and tooling code. **This is not an independent labelling.**

## 2. Result

| rule | findings | judged | true | precision | stopped | disposition |
|---|---|---|---|---|---|---|
| `loop:format!` | 1752 | 10 | 0 | 0% | 7th false at row 7; 10 kept | opt-in, hot paths only |
| `loop:clone` | 474 | 12 | 0 | 0% | 7th false at row 7; 12 kept | opt-in, hot paths only |
| `loop:alloc-new` | 385 | 10 | 0 | 0% | 7th false at row 7; 10 kept | opt-in, hot paths only |
| `loop:to_string` | 127 | 10 | 0 | 0% | 7th false at row 7; 10 kept | opt-in, hot paths only |
| `loop:to_owned/to_vec` | 28 | 20 | 6 | 30% | 7th false at row 11; all 20 kept | narrow it (below), opt-in |
| `loop:contains(vec)` | 23 | 12 | 0 | 0% | 7th false at row 7; 12 kept; the bar is unreachable even if the other 11 were true (11 of 23) | drop |
| `loop:iter-linear-search` | 37 | 10 | 1 | 10% | 7th false at row 8; all 10 kept | narrow it (below), opt-in |
| `vec-new-then-push-loop` | 9 | 9 | 7 | 78% | census | opt-in; fix two causes; sample more |
| `nested-loop-same-collection` | 3 | 3 | 1 | 33% | census | opt-in |
| `collect-then-iterate` | 2 | 2 | 0 | 0% | census | exclude a type change |
| `unbuffered-io` | 2 | 2 | 2 | 100% | census | default-on candidate under the exact-syntax clause; the PM decides |

**The allocation-in-loop family (`format!`, `clone`, `alloc-new`, `to_string`) is 0 of 42.** With zero true in 42 the 95% upper bound on
its precision is about 7% (3/42). No amount of rule tuning on the syntax shown here changes that: in these repositories an allocation
inside a loop is almost always the by-value API of the code around it, a name that is the lookup key, or generated text that is the
output. The only remaining route is the one the design names: report it only in a path declared hot with `[perf] hot_paths`, and measure the
gate on a sample drawn **from declared hot paths** (none is declared anywhere yet).

**What each result says about the rule.**

- **`loop:to_owned/to_vec`, 6 of 20.** The six true ones share one shape: the copy is **only read afterwards** (`let dims = x.to_vec();` then `&dims`, a
  comparison, a `format!`), or is built only to be compared (`Object::Unicode(key.to_owned())` inside `find`). A rule that fires only when the
  binding is never moved, mutated or stored needs a use-analysis the syntax-only detector does not do; it is a candidate for a second prototype,
  not a rule today.
- **`loop:iter-linear-search`, 1 of 10, and `loop:contains(vec)`, 0 of 12.** The failures are **size-blind**: a linear search over a handful of
  backends or axes is the right code. The one true case is an **equality-keyed lookup in a collection that grows with the input** (a ledger searched
  by claim, once per record). A narrower rule (`.iter().any(|e| e.key == x)` or `.find(|e| e.key == x)` on a collection built in the same function from
  input-sized data) is plausible and unmeasured. The predicate forms (`.all`, `.position(|r| r.matches(..))`) are not lookups and should not fire.
- **`vec-new-then-push-loop`, 7 of 9 (census).** The two false ones are the two causes the design already guessed: other pushes in nested loops (length
  not evident) and a **deliberate** `Vec::new()` with a comment explaining that `with_capacity(len)` would trust an untrusted length. Excluding those
  two shapes would make this census 7 of 7, which is circular (the exclusions were read off the census) and says nothing about a new sample: **it needs
  30 findings from other repositories** before it can be default-on.
- **`collect-then-iterate`, 0 of 2.** Both collect into a `HashSet` to deduplicate and then back into a Vec: the change of type is the point. A rule
  that ignores a collect into a different collection type has **no finding left in these repositories**; it is exact on its syntax with nothing to
  sample, which is the policy's second clause.
- **`unbuffered-io`, 2 of 2 (census).** Both are `writeln!` per item into an unbuffered `File`, in build scripts (small and cold). Exact on its
  syntax, but two findings are not a sample. It meets the policy's clause for an exact rule with almost nothing to sample; whether two cold findings
  justify a default-on rule is the PM's call.
- **`nested-loop-same-collection`, 1 of 3.** The false two are cross products that are the intent (a backend-pair cost table, a dtype x dtype
  table); the true one is a scan of every node for each node.

**The earlier 12-finding sample, restated.** The design document's section 6 judged twelve findings from the **unrefined** detector and reported
4 true. Under this document's stricter rubric, row 8 (a `slice.contains` over a small `OpKind` list) is false, so it is **3 of 12**. The
conclusions of the design document stand; its figure is 25%, not 33%.

## 3. What follows for the design document

1. **PRF-016 (allocation in a loop) and PRF-018 stay opt-in and hot-path-scoped**, and this sample is the evidence that the scoping is not optional.
2. **PRF-012 (linear search) is rewritten** from "`contains` / `.iter().any` in a loop" to "an equality-keyed lookup in a collection built from
   input-sized data", and loses the predicate forms.
3. **PRF-015 (collect, then iterate)** gains an exclusion: a `collect` into a different collection type.
4. **PRF-014 (`with_capacity`)** gains two exclusions: a nested-loop push, and a `Vec::new()` beside a comment that says capacity is deliberately not
   preallocated.
5. **No rule is default-on on this evidence.** `unbuffered-io`, `collect-then-iterate` (after the exclusion) and `repr(C)` padding are the only
   candidates, by the exact-syntax clause and not by a sample.
6. **To reach 30 for the small populations the portfolio needs more Rust:** the three repositories hold 9, 3, 2 and 2 findings of the rules that
   behaved. The next sample should add the repositories the PM has not yet had measured (mlmf, synapse, lsp-bridge, auth-framework, lightbulb).

## 4. The verdicts, one row each

Each table lists the findings in the judged order (rank = position in the seeded shuffle of that rule's findings). `site` is
`repo path:line` at the refs of section 1.

#### `loop:format!` (1752 findings; 10 judged, 0 true)

| # | site | verdict | why |
|---|---|---|---|
| 1 | fuel `fuel-transformers/src/models/lazy_dinov2.rs:449` | false | a weight-name string is the lookup key, one per tensor at model load |
| 2 | fuel `fuel-transformers/src/models/lazy_llava.rs:545` | false | a weight-name string is the lookup key, one per tensor at model load |
| 3 | fuel `fuel-transformers/src/models/lazy_tiny_vit.rs:682` | false | a weight-name string is the lookup key, one per tensor at model load |
| 4 | fuel `fuel-transformers/src/models/lazy_debertav2.rs:630` | false | a weight-name string is the lookup key, one per tensor at model load |
| 5 | baracuda `crates/baracuda-cuda-emit/src/cuda.rs:5370` | false | generated CUDA source text is the output |
| 6 | fuel `fuel-transformers/src/models/lazy_llava.rs:690` | false | a weight-name string is the lookup key, one per tensor at model load |
| 7 | fuel `fuel-transformers/src/models/lazy_qwen3_moe.rs:731` | false | a weight-name string is the lookup key, one per tensor at model load |
| 8 | fuel `fuel-transformers/src/models/lazy_eva2.rs:429` | false | a weight-name string is the lookup key, one per tensor at model load |
| 9 | fuel `fuel-transformers/src/models/lazy_trocr.rs:423` | false | a weight-name string is the lookup key, one per tensor at model load |
| 10 | baracuda `crates/baracuda-cuda-emit/src/cuda.rs:4670` | false | generated CUDA source text is the output |

#### `loop:clone` (474 findings; 12 judged, 0 true)

| # | site | verdict | why |
|---|---|---|---|
| 1 | fuel `fuel-nn/src/optim.rs:556` | false | an owned Vec the callee takes by value |
| 2 | fuel `fuel-graph/src/lib.rs:9992` | false | a Shape by value to the node builder |
| 3 | fuel `fuel-graph/src/lib.rs:11494` | false | a Shape by value to the node builder |
| 4 | fuel `fuel-graph/src/scan.rs:429` | false | copied out of a lock or borrow scope |
| 5 | fuel `fuel-transformers/src/models/lazy_modernbert.rs:204` | false | an Arc-backed tensor handle: a cheap clone |
| 6 | fuel `fuel-graph/src/scan.rs:180` | false | copied out of a lock or borrow scope |
| 7 | baracuda `crates/baracuda-forge/src/builder.rs:479` | false | moved into the job list |
| 8 | fuel `fuel-tensor/src/pipelined_bridge.rs:1498` | false | used again after the move |
| 9 | fuel `fuel-graph/src/lib.rs:10002` | false | a Shape by value to the node builder |
| 10 | fuel `fuel-transformers/src/models/lazy_efficientvit.rs:459` | false | an Arc-backed tensor handle: a cheap clone |
| 11 | fuel `fuel-model-llama/src/lib.rs:2529` | false | a Shape by value per layer |
| 12 | fuel `fuel-graph/src/lib.rs:9679` | false | a Shape by value to the node builder |

#### `loop:alloc-new` (385 findings; 10 judged, 0 true)

| # | site | verdict | why |
|---|---|---|---|
| 1 | fuel `fuel-graph/src/registry/causal_conv1d.rs:216` | false | a `vec![..]` argument the node builder takes by value |
| 2 | fuel `fuel-graph/src/lib.rs:11500` | false | a `vec![..]` argument the node builder takes by value |
| 3 | fuel `fuel-graph/src/lib.rs:11443` | false | a `vec![..]` argument the node builder takes by value |
| 4 | fuel `fuel-graph/src/lib.rs:9993` | false | a `vec![..]` argument the node builder takes by value |
| 5 | fuel `fuel-graph/src/lib.rs:9274` | false | a `vec![..]` argument the node builder takes by value |
| 6 | fuel `fuel-graph/src/lib.rs:9161` | false | a `vec![..]` argument the node builder takes by value |
| 7 | fuel `fuel-graph/src/scan.rs:207` | false | a per-step substitution map, rebuilt each step by design (a comment says so) |
| 8 | fuel `fuel-graph/src/lib.rs:9698` | false | a `vec![..]` argument the node builder takes by value |
| 9 | fuel `fuel-graph/src/lib.rs:11002` | false | an owned input list, extended on the next lines |
| 10 | fuel `fuel-graph/src/lib.rs:10050` | false | a `vec![..]` argument the node builder takes by value |

#### `loop:to_string` (127 findings; 10 judged, 0 true)

| # | site | verdict | why |
|---|---|---|---|
| 1 | fuel `fuel-dispatch/src/fkc/verify/seed_vulkan_ledger.rs:324` | false | an owned String field or element that has to own its text |
| 2 | fuel `fuel-dispatch/src/fkc/verify/seed_vulkan_ledger.rs:286` | false | an owned String field or element that has to own its text |
| 3 | baracuda `crates/baracuda-cuda-emit/src/cuda.rs:2338` | false | an owned String field or element that has to own its text |
| 4 | fuel `fuel-transformers/src/models/lazy_gemma2.rs:580` | false | an owned String field or element that has to own its text |
| 5 | fuel `fuel-dispatch/src/fkc/shape_constraint.rs:109` | false | an error value that owns its text (an error path) |
| 6 | baracuda `crates/baracuda-cuda-emit/src/cuda.rs:873` | false | an owned String field or element that has to own its text |
| 7 | fuel `fuel-dispatch/src/fkc/verify/seed_cpu_ledger.rs:784` | false | an owned String field or element that has to own its text |
| 8 | fuel `fuel-dispatch/src/fkc/verify/seed_vulkan_ledger.rs:282` | false | an owned String field or element that has to own its text |
| 9 | fuel `fuel-inference/src/tool_call.rs:312` | false | an owned String field or element that has to own its text |
| 10 | fuel `fuel-inference/src/tool_call.rs:341` | false | an owned String field or element that has to own its text |

#### `loop:to_owned/to_vec` (28 findings; 20 judged, 6 true)

| # | site | verdict | why |
|---|---|---|---|
| 1 | fuel `fuel-graph/src/lib.rs:10352` | false | owned copy out of a lock guard's scope |
| 2 | fuel `fuel-formats/src/pickle.rs:732` | true | allocates `Object::Unicode(key.to_owned())` per element only to compare; a `matches!` needs no copy |
| 3 | fuel `fuel-dispatch/src/fkc/verify/seed_vulkan_ledger.rs:257` | false | an owned `dtypes_vec` stored in the ledger record |
| 4 | fuel `fuel-dispatch/src/fkc/verify/seed_cpu_ledger.rs:989` | false | an owned `dtypes_vec` stored in the ledger record |
| 5 | fuel `fuel-transformers/src/models/lazy_mimi_quantization.rs:231` | true | `dims` is only extended into another Vec: `extend_from_slice` on the borrowed dims needs no copy |
| 6 | fuel `fuel-dispatch/src/fkc/verify/seed_vulkan_ledger.rs:178` | false | an owned `dtypes_vec` stored in the ledger record |
| 7 | fuel `fuel-ir/src/dlpack/validate.rs:1039` | true | `strides` is only borrowed by the call: a slice of `b.strides` needs no copy |
| 8 | fuel `fuel-tensor-tools/src/main.rs:385` | true | `dims` is only borrowed by `summarize`: the view's shape slice needs no copy |
| 9 | fuel `fuel-transformers/src/models/lazy_rwkv_tokenizer.rs:60` | false | stored in the table |
| 10 | fuel `fuel-dispatch/src/fkc/verify/seed_cuda_ledger.rs:470` | false | an owned `dtypes_vec` stored in the ledger record |
| 11 | fuel `fuel-graph/src/lib.rs:10347` | false | owned and mutated afterwards |
| 12 | fuel `fuel-dispatch/src/fkc/verify/seed_cpu_ledger.rs:862` | false | an owned `dtypes_vec` stored in the ledger record |
| 13 | fuel `fuel-graph/src/lib.rs:10331` | false | owned and mutated on the next line |
| 14 | fuel `fuel-dispatch/src/fkc/verify/seed_cpu_ledger.rs:750` | false | an owned Vec stored in the record |
| 15 | fuel `fuel-tensor/src/lazy.rs:8145` | true | `dims` is only read and printed: the borrowed slice needs no copy |
| 16 | fuel `fuel-graph/src/opt.rs:1831` | false | borrow of `g` may conflict with later mutation; not shown to be avoidable |
| 17 | fuel `fuel-graph/src/opt.rs:1848` | false | borrow of `g` may conflict with later mutation; not shown to be avoidable |
| 18 | fuel `fuel-graph/src/lib.rs:10812` | false | copy of a temporary Shape's dims; not shown to be avoidable |
| 19 | fuel `fuel-inference/src/ngram_draft.rs:74` | false | an owned Vec returned |
| 20 | fuel `fuel-tensor/src/lazy_latent_cache.rs:159` | true | `got` is only compared and printed: the borrowed slice needs no copy |

#### `loop:contains(vec)` (23 findings; 12 judged, 0 true)

| # | site | verdict | why |
|---|---|---|---|
| 1 | fuel `fuel-dispatch/src/dispatch.rs:216` | false | a Vec bounded by a small fixed domain (backends, devices, axes, kernel names): a set would not help |
| 2 | fuel `fuel-dispatch/src/plan.rs:181` | false | a Vec bounded by a small fixed domain (backends, devices, axes, kernel names): a set would not help |
| 3 | fuel `fuel-dispatch/src/fkc/shape_constraint.rs:449` | false | a Vec bounded by a small fixed domain (backends, devices, axes, kernel names): a set would not help |
| 4 | fuel `fuel-dispatch/src/driver.rs:453` | false | a Vec bounded by a small fixed domain (backends, devices, axes, kernel names): a set would not help |
| 5 | fuel `fuel-tensor/src/pipelined_bridge.rs:832` | false | a Vec bounded by a small fixed domain (backends, devices, axes, kernel names): a set would not help |
| 6 | fuel `fuel-tensor/src/pipelined_bridge.rs:1980` | false | a Vec bounded by a small fixed domain (backends, devices, axes, kernel names): a set would not help |
| 7 | Unpopped `crates/unpopped/src/shape.rs:138` | false | a Vec bounded by a small fixed domain (backends, devices, axes, kernel names): a set would not help |
| 8 | fuel `fuel-dispatch/src/fkc/verify/harness.rs:329` | false | a Vec bounded by a small fixed domain (backends, devices, axes, kernel names): a set would not help |
| 9 | Unpopped `crates/unpopped-vocab/src/structure_key.rs:827` | false | a Vec bounded by a small fixed domain (backends, devices, axes, kernel names): a set would not help |
| 10 | Unpopped `crates/unpopped-vocab/src/structure_key.rs:824` | false | a Vec bounded by a small fixed domain (backends, devices, axes, kernel names): a set would not help |
| 11 | fuel `fuel-metal-backend/src/storage.rs:323` | false | a Vec bounded by a small fixed domain (backends, devices, axes, kernel names): a set would not help |
| 12 | fuel `fuel-graph/src/run.rs:526` | false | a Vec bounded by a small fixed domain (backends, devices, axes, kernel names): a set would not help |

#### `loop:iter-linear-search` (37 findings; 10 judged, 1 true)

| # | site | verdict | why |
|---|---|---|---|
| 1 | fuel `fuel-tensor/src/judge/mod.rs:2317` | false | a predicate over every cluster member (clique building), not a lookup |
| 2 | fuel `fuel-dispatch/src/jit_ingest.rs:1714` | true | a keyed lookup (`claim`) in a growing ledger, repeated per record: quadratic, a map by claim fixes it |
| 3 | fuel `fuel-transformers/src/models/lazy_yolov3.rs:928` | false | non-maximum suppression: an overlap predicate over kept boxes, inherent to the algorithm |
| 4 | baracuda `crates/baracuda-forge/src/dependency.rs:85` | false | a few include paths |
| 5 | fuel `fuel-dispatch/src/fkc/contract_audit_flip.rs:266` | false | a short fixed list of op names |
| 6 | fuel `fuel-graph/src/opt.rs:320` | false | a predicate over rules (`matches`), not an equality lookup |
| 7 | fuel `fuel-graph/src/opt.rs:2321` | false | readers of one node: a handful |
| 8 | fuel `fuel-dispatch/src/ranker/alternative_set.rs:314` | false | a few backends |
| 9 | fuel `fuel-correctness-fixtures/src/capture.rs:372` | false | a predicate over every cluster member (clique building), not a lookup |
| 10 | fuel `fuel-dispatch/src/optimize.rs:504` | false | a predicate over a few backends |

#### `vec-new-then-push-loop` (9 findings; 9 judged, 7 true)

| # | site | verdict | why |
|---|---|---|---|
| 1 | fuel `fuel-transformers/src/models/lazy_wuerstchen.rs:1322` | true | pushes once per turn over `0..n_bottleneck`: `with_capacity(n_bottleneck)` |
| 2 | fuel `fuel-transformers/src/models/lazy_convnext.rs:863` | true | pushes once per turn over `0..depths[si]` |
| 3 | fuel `fuel-transformers/src/models/lazy_bert.rs:714` | true | one push per element of `unique` (checked: the `?` leaves early only on error) |
| 4 | fuel `fuel-transformers/src/models/lazy_wuerstchen.rs:1748` | true | pushes once per turn over `0..n_res` |
| 5 | baracuda `crates/baracuda-cutensor-sys/src/lib.rs:900` | false | other pushes sit in nested loops and conditions: the length is not evident |
| 6 | fuel `fuel-transformers/src/models/lazy_phi.rs:595` | true | one push per element of `unique` |
| 7 | fuel `fuel-transformers/src/models/lazy_gemma2.rs:583` | true | one push per element of `unique` |
| 8 | fuel `fuel-transformers/src/models/lazy_llama2c.rs:409` | true | one push per element of `unique` |
| 9 | fuel `fuel-formats/src/gguf.rs:370` | false | the code deliberately avoids `with_capacity(len)` because `len` is untrusted (a comment says so) |

#### `nested-loop-same-collection` (3 findings; 3 judged, 1 true)

| # | site | verdict | why |
|---|---|---|---|
| 1 | fuel `fuel-tensor/src/scheduling.rs:526` | false | a cost table over backend pairs: the cross product is the intent |
| 2 | fuel `fuel-graph/src/opt.rs:2340` | true | for every node, a scan of every node for readers of its target: quadratic, a consumer index fixes it |
| 3 | baracuda `crates/baracuda-cuda-emit/src/cuda.rs:7498` | false | a dtype x dtype cast table: the cross product is the intent |

#### `collect-then-iterate` (2 findings; 2 judged, 0 true)

| # | site | verdict | why |
|---|---|---|---|
| 1 | fuel `fuel-dispatch/src/topology.rs:461` | false | collects into a `HashSet` to deduplicate, then back into a Vec: the type change is the point |
| 2 | fuel `fuel-dispatch/src/kernel.rs:1603` | false | collects into a `HashSet` to deduplicate, then back into a Vec: the type change is the point |

#### `unbuffered-io` (2 findings; 2 judged, 2 true)

| # | site | verdict | why |
|---|---|---|---|
| 1 | baracuda `crates/baracuda-forge/src/builder.rs:1104` | true | `writeln!` per kernel path into an unbuffered `File` (a build script: small and cold) |
| 2 | baracuda `crates/baracuda-forge/src/builder.rs:1046` | true | `writeln!` per object path into an unbuffered `File` (a response file: small and cold) |
