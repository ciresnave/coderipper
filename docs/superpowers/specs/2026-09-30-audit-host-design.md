# CodeRipper — design

**Status: APPROVED, repo created, nothing implemented yet.** Written 2026-09-30 by the portfolio PM, named
and approved by CireSnave the same day (`github.com/ciresnave/coderipper`, MIT OR Apache-2.0, Rust). History:
started as one tool (reachability/dead-code detection only). CireSnave asked whether that should instead be
one check among many in a general audit host, runnable "right next to `cargo clippy`," returning a brief
triaged summary — this revision restructures around that: reachability becomes the first *check* (a
plugin), not the whole tool. CLI is the primary interface (clippy-style); a server mode is also required,
for a free hosted instance on ThinkersJournal.com for open-source projects.

**Why host-plus-plugins, not a single-purpose tool**: this isn't speculative generalization — it matches
architecture to needs CireSnave already has stated as standing rules, each of which is its own cross-project
audit waiting to be automated (§6 sketches three of them to pressure-test the interface below, before
anything gets built against it). Building each as its own repo would mean four-plus repos each reinventing
CLI plumbing, report formatting, CI wiring, and an allowlist mechanism. One host with pluggable checks means
all of that is shared, and adding the next audit later is "write a check," not "stand up a project."

---

## 0. Motivation, not hypothetical

Tonight, while designing an unrelated feature, the Lightbulb lane found — by hand, via a whole-tree grep
prompted by an unrelated review — that `src/model_fuel/policies.rs`'s `BlockPrefixIndex`/`record`/`lookup`/
`splice_prefix` (a fully correct, well-tested prefix-matching system) had **zero callers anywhere in
Lightbulb's serving path**. The logic was right. Nothing called it. That's the exact defect class the first
check below exists to catch automatically, and it was found only because a review happened to go looking in
the right place. A periodic, systematic sweep would have surfaced it without needing the coincidence.

## 1. Two axes every check declares, and why they're independent

A naive design would give each check one "speed" label (fast/slow) and call it done. That collapses two
genuinely different properties, and this portfolio's own setup makes the collapse actively misleading:

- **Scope: `Project` or `Portfolio`.** Does the check need only the current repo's source, or does it need
  to read other portfolio repos too?
- **Network: `LocalOnly` or `NetworkRequired`.** Does the check ever leave the machine (a registry API, the
  GitHub API), or does it only ever read the local filesystem?

**These are independent, and the second one is the actual speed gate — not the first.** Every portfolio
repo already lives checked out locally under `C:\Projects` (established practice, not a new assumption).
That means a `Portfolio`-scope, `LocalOnly` check — reachability's cross-repo grep, for instance — is still
fast: it's reading files already on disk, same as `git grep` across sibling checkouts, which lanes already
do routinely. What's actually slow and rate-limit-sensitive is `NetworkRequired`: querying crates.io/npm/
PyPI for a dependency's latest version, or hitting the GitHub API for branch protection state. **A check's
run tier is decided by the network axis, not the scope axis.**

- **`fast` tier** (the clippy-adjacent one): every `LocalOnly` check, `Project`- or `Portfolio`-scoped alike.
  Meant to run on every PR, or on demand next to `cargo clippy`/`cargo test`.
- **`sweep` tier**: every `NetworkRequired` check, plus any `fast`-tier check a caller explicitly wants
  re-run as part of a full sweep. Meant for a periodic or PM-triggered run, not a per-PR gate — registry and
  GitHub API calls at portfolio scale need caching and rate-limit awareness that doesn't belong in a
  pre-merge hook.

One practical caveat for `fast`-tier `Portfolio`-scope checks specifically: reading every sibling repo's
source fresh on every invocation doesn't stay fast forever as the portfolio grows. The host should maintain
a cached per-project symbol index (rebuilt incrementally on that project's own commits, not on every other
project's every run) so a `Portfolio`-scope `fast` check reads the cache, not N fresh trees, on the common
path.

## 2. The shared finding schema

Every check, regardless of what it does, emits findings into one shape — this is what lets the host produce
one triaged "most concerning issues" summary across heterogeneous checks, instead of concatenating each
check's own report format and leaving the reader to reconcile them:

```
Finding {
    check_id:      string       // e.g. "reachability", "version-consistency"
    severity:      Info | Low | Medium | High | Critical
    confidence:    Low | Medium | High
                   // separate from severity on purpose: a symbol-grep miss is a plausible defect
                   // at High severity but only Medium confidence (name collisions, dynamic dispatch);
                   // a version mismatch is High severity AND High confidence (no ambiguity once read).
    project:       string       // which repo this finding is about
    location:      { file, line }?   // omitted for project-level findings (e.g. "no branch protection")
    summary:       string       // one line, what a triaged report shows
    detail:        string       // full explanation + evidence, shown in the full report
    positive_control: string?   // required for any check whose core claim is an ABSENCE
                                 // (see reachability, §3) — a finding making a "zero/none/missing"
                                 // claim without one is a bug in the check, not evidence
}
```

**Triage for the brief-response mode**: sort by `severity` descending, then `confidence` descending within
a severity tier; show everything at `High`/`Critical`, plus the top N (tunable, default maybe 5) of
`Medium` and below, with a count of what was omitted. Exact N and any smarter ranking (e.g. penalizing a
`check_id` that's dominating the list, so one noisy check doesn't crowd out a different check's one real
finding) is an implementation detail to tune against real output, not to over-design now.

## 3. First check: reachability (project- and portfolio-scope, both `LocalOnly`)

Kept from the original design, now expressed as one check with two sub-passes matching the two scope
levels:

**Sub-pass A, `Project` scope.** Does every function in this codebase get called from somewhere? The
Rust-specific wrinkle: `rustc`'s own `dead_code` lint deliberately exempts `pub` items (the compiler assumes
*some* downstream crate might use them). Mechanism: for the internal-reachability question, temporarily
compile with every `pub` downgraded to `pub(crate)` and let the existing lint do the real work — a known
trick, not new technology, just not packaged as a repeatable check here yet. Python (`vulture`) and
TypeScript (`ts-prune` or similar) already have adequate tools; this check just wires their output into the
shared `Finding` schema.

**Sub-pass B, `Portfolio` scope.** Does every project's declared public API surface actually get referenced
by another portfolio repo meant to consume it?

- **Extract, don't grep for extraction.** Get the real symbol set from the language's own tooling (rustdoc
  JSON for Rust, not text-scraping `pub fn` — that misses struct fields, trait impls, re-exports via
  `pub use`).
- **Search with the same aliasing awareness this portfolio has already been burned by.** Fuel's own
  `CLAUDE.md` records a naive `fuel_core::` search undercounting by ~30x because the real alias is `fuel::`
  — any reference search needs to know a crate's own re-export conventions, not just its defining name.
- **Every "zero hits" claim needs its `positive_control`, per §2's schema** — search for a symbol from the
  same crate already known to be used elsewhere, confirm the query finds it, before trusting a zero result.
  Not optional; a finding without one is the check's own bug.
- **A hit is a candidate, not a confirmation.** False positives (name collisions), false negatives (trait
  dispatch, macros, FFI boundaries like Fuel↔Baracuda). Findings report the referencing context, not just a
  count — this check surfaces candidates for review, it doesn't replace judgment. `confidence: Medium` on
  this check's findings by default, reflecting that.

## 4. Allowlist mechanism, generalized across checks

A finding isn't automatically a defect — some `pub` API is genuinely meant for external consumers this
portfolio doesn't control (crates.io publishes: Fuel, Baracuda, Synapse, Unpopped), or is deliberately built
ahead of its consumer ("'No consumer' is not a reason to skip building a capability — but it IS a reason to
sequence it behind things with consumers," Fuel's own working agreement). Flagging all of it would be noisy
enough that nobody trusts the output.

**Mechanism**: one allowlist file per project (e.g. `.coderipper.toml`), entries keyed by `check_id` + a
fingerprint of the specific finding, each carrying a **required reason string**. Generalized from the
reachability-specific design so every future check uses the same suppression path — no check invents its
own. Per this portfolio's own allowlist discipline (*"an allowlist entry must carry a detector for its own
cause dissolving," Fuel's working agreement*): an allowlisted reachability finding that later DOES get a
real caller should surface as its own (informational) finding — "this suppression may no longer apply" —
not silently keep suppressing forever.

**Added 2026-10-01, CireSnave's call: two kinds of suppression, both self-cleaning, one with an explicit
second trigger.** Up to now the discipline above ("surface as informational when the cause dissolves") was
stated as a norm but not given a concrete mechanism. Formalizing it:

- **The self-dissolving check applies to every entry, permanent or temporary, with no extra config.**
  Mechanism: on each run, the host compares each allowlist entry's fingerprint against this run's *raw*
  (pre-filter) findings for that check. An entry whose fingerprint matches nothing this run is stale — its
  cause has dissolved (CireSnave's framing: typically because "an implementation happening" changed the
  code enough that the original finding no longer reproduces). Surface it as its own `Informational`
  finding — "this suppression hasn't matched in N runs, consider removing it" — never silently kept
  forever. This is the mechanism the existing discipline above only asserted; it needed one.
- **A second, explicit trigger for temporary entries: suppress until a *different*, named finding clears.**
  CireSnave's case: a fix for one root-cause finding *should* also fix a secondary symptom elsewhere: rather
  than carry both as separate noise, suppress the symptom's finding with a declared dependency on the root
  cause's finding (same fingerprint shape: `check_id` + location/symbol). Each run, the host checks whether
  the *referenced* finding still reproduces; the moment it doesn't (presumed fixed), the dependent
  suppression expires the same way — surfaced informationally, not silently dropped, so a human confirms
  the symptom is actually gone too rather than assuming the dependency guess was right.

**Implemented 2026-10-02 (`docs/superpowers/plans/2026-10-02-allowlist-stale-entries.md`), narrower than
the above, by the PM's scoping call.** One axis, not two: the universal trigger only, and only in its
**stale-this-run** form. The host (not each check) applies the allowlist to every check's raw findings and
reports each entry that matched nothing *in this run* as an `Info` finding (check id `allowlist`; same
`Finding` type, no parallel channel). An entry is judged only if its check ran to completion in that run; an
entry naming an unregistered check is reported regardless. **Deliberately not built**: "hasn't matched in N
runs" (needs a run-history store that doesn't exist; revisit if one is built for another reason) and the
`until_fixed` link to a different finding. The "one axis or two" question is settled for now as one axis;
`until_fixed` is a documented future extension. **Known limit:** an entry's identity is (check, file, bare
symbol name), so two same-named methods in one file share it — one entry can hide a second finding and
will not go stale while either still matches. Fixing it means checks emitting a qualified `subject`.

## 5. Five more checks, sketched to pressure-test the interface above (not designed in full)

These exist here to check that §1's two axes and §2's schema actually fit something other than reachability
before either gets built against. None of these is ready to implement — they're validation, not scope.

**`version-consistency`** — *`Project` scope, `LocalOnly`, `fast` tier.* CireSnave's standing rule: every
crate/package within a project shares one version number, with a narrow named exception (a crate pinned to
match a *different* project's version, e.g. an emitter crate tracking the project it targets). Mechanism:
read every manifest in the project, compare versions, allow a declared exception list (itself a case of
§4's allowlist, scoped to this check) where the "correct" value is read from the referenced *other*
project's current manifest rather than compared for uniformity. Findings: `High` severity, `High`
confidence — no ambiguity once the versions are read. **Validates**: the axes hold even for a check that's
almost entirely `Project`-scope but has one narrow cross-project read baked into its exception path —
confirms scope is a property of the check's *typical* need, not an absolute boundary the host has to
enforce strictly.

**`dependency-staleness`** — *`Project` scope, `NetworkRequired`, `sweep` tier.* CireSnave's standing rule:
dependencies stay on their most recent versions. Mechanism: read each project's lockfile, query the
relevant registry (crates.io/npm/PyPI) for each dependency's latest version, diff. Findings: severity scales
with how far behind (`Low` for a patch behind, up to `Critical` if a known security advisory applies to the
pinned version) at `High` confidence. **Validates**: the network axis is genuinely independent of scope —
this check is `Project`-scoped, same as `version-consistency`, but lands in a completely different run tier
because of the registry calls, proving scope alone would have misclassified it.

**`ci-protection-presence`** — *`Project` scope, `NetworkRequired`, `sweep` tier.* CireSnave's standing rule:
every repo gets CI and enforced branch protection. Mechanism: `gh api` the project's branch protection
settings and — the portfolio's own hard-learned lesson — read `required_status_checks.enforcement_level`
and `contexts.length`, never just `.protected` (which "reads `true` on branches that enforce nothing").
Findings: `High` severity if protection is absent or enforces zero contexts. **Validates**: confirms a
GitHub-API-backed check fits the same `NetworkRequired`/`sweep` bucket as a registry-backed one without
needing a third axis.

**`unused-return-values`** — *`Project` scope, `LocalOnly`, `fast` tier (for Rust; see below).* Added
2026-10-01, CireSnave's call: of the two value-flow checks sketched here, this one ships first —
genuinely useful and not well-covered by existing tooling in any language this host is likely to target.
Rust's `#[must_use]` only flags a return value ignored at *one specific* call site, and only if the
function is explicitly annotated; nothing asks "does *any* caller, anywhere in the project, ever actually
use this function's return value at all?" — a stronger and more interesting signal than per-call-site
ignoring, closer in spirit to `reachability` but one layer past it (not "is this function called" but "is
its output ever consumed"). **Mechanism, Rust** (corrected 2026-10-01 after testing; see `docs/superpowers/plans/2026-10-01-unused-return-values.md`): in the throwaway worktree, tag every eligible function with `#[must_use = "CR:<id>"]` *and* `#[deprecated(note = "CR:<id>")]` (inline, so line numbers survive), rebuild with `--all-targets`, and join rustc's `unused_must_use` (call sites whose value is discarded) with its `deprecated` (every use) by `<id>`. `unused_must_use` alone cannot distinguish all-ignored from some-ignored, because it is silent at used call sites; the `deprecated` pairing supplies the denominator. Imports (`use`) and a function's own recursive calls are excluded from the counts. No `pub` -> `pub(crate)` rewrite is needed, so this check — unlike `reachability` — works on a package whose lib is consumed by its own bin or tests. Unit-returning, `Result`-returning, `async`, `extern`, and `-> &mut Self` functions are skipped. **Mechanism, other languages**:
real, separate detection work per language — no equivalent trick is assumed to exist elsewhere, and this
check should not block Rust support on having every language ready. **Validates**: confirms a check can
legitimately reuse another check's rewrite machinery rather than re-inventing it, and that "whole-project
call-site analysis" is a mechanism shape the host's two axes handle fine (still `Project`/`LocalOnly`/
`fast`, same as `reachability` — a build with instrumented diagnostics, not a network call).

**`unused-parameters`** — *`Project` scope, `LocalOnly`, `fast` tier.* Added 2026-10-01, alongside
`unused-return-values` but intentionally lower priority — CireSnave's correction to the PM's first-draft
reasoning, verbatim in spirit: *CodeRipper is not Rust-specific, so what Rust's own tooling already checks
isn't all that matters portfolio-wide.* **Per language, this check is one of two shapes, not one**: (a)
for a language whose own standard tooling already has a mature, default-on unused-parameter lint (Rust's
`unused_variables`, several Python linters), CodeRipper's real value-add is *surfacing that language's own
diagnostic through the unified `Finding` schema* for a portfolio-wide report, not re-detecting it — for
Rust specifically, this is cheap: shell out to `cargo build`/`clippy` and translate their diagnostics,
reusing none of `reachability`'s rewrite trick because none is needed. (b) for a language with no such
built-in coverage, this is real, language-specific detection work, not yet designed. **One Rust-specific
wrinkle worth tracking even though Rust's check is "free": the underscore-prefix convention (`_unused`) is
an intentional exemption from the warning, and the same blind-spot class `reachability` already exploits
(pub items exempted from `dead_code`) applies here too — a parameter someone prefixed with `_` to silence
the warning might be worth flagging as "accepted and silently ignored" rather than assumed deliberate.**
Not scoping that wrinkle in now; noting it so it isn't rediscovered cold later.

**Implemented 2026-10-02 (`docs/superpowers/plans/2026-10-02-unused-parameters.md`), Rust only, as the PM
scoped it.** Shape (a) of the above, with three corrections found by testing rustc's output rather than
assuming it is "free": (1) rustc words an unused *parameter* exactly like an unused *local* ("unused
variable: `x`"), so the diagnostic alone cannot say which it is; the check joins it to the exact positions
of function parameters found by `syn`. (2) rustc also reports the forced signature of every trait impl, trait
default bodies, closure parameters and macro-generated functions; all of those are out of scope. (3) An
explicit `#[allow(unused_variables)]` on an item is respected (a deliberate, local decision; record a reason in
the allowlist instead), but a crate-wide allow makes the per-run sentinel fail and the run errors rather than
reporting clean. The finding's `subject` is qualified (inline `mod {}` blocks, the impl's type, enclosing functions, then
`function::parameter`; the file is part of an entry's identity, so the file tree need not be), so unlike
the two earlier checks two same-named methods on different types in one file do not share an allowlist
entry. Known limit: generic arguments are not part of the type, so `impl W<u8>` and `impl W<u16>` share one. **Still not scoped:**
the underscore-prefix blind spot noted above, and shape (b) for languages without a built-in lint.

**Worth flagging, not solving here**: `provenance/license` (CireSnave's "I am not a plagiarist" rule —
trace every third-party file to its origin, verify licence/credit) doesn't cleanly fit one tier. A shallow
version — "every vendored file has a traceable origin comment and licence notice present" — is `Project`
scope, `LocalOnly`, `fast`. A thorough version — actually fetching the claimed upstream and diffing against
the vendored copy — is `NetworkRequired`, `sweep`. **This suggests a check can declare more than one depth
under the same `check_id`**, the host running the shallow pass at `fast` tier and the deep pass only at
`sweep` tier. Noted as a real interface requirement discovered by sketching, not assumed at the start —
exactly the kind of thing this pressure-testing exercise was for.

## 6. Output and integration

- One structured report per run (JSON — feeds a dashboard, a PR comment, or a portfolio-PM board item) plus
  the triaged human-readable summary from §2.
- Run modes: `--fast` (every `LocalOnly` check, single project or whole portfolio), `--sweep` (everything,
  `NetworkRequired` included), `--check <id>` (one check only, any tier — useful for testing a check in
  isolation or for CI to run just the checks relevant to what changed), `--project <name>` (scope to one
  project even for otherwise-portfolio checks, where that's meaningful).
- Every finding whose core claim is an absence carries its `positive_control` inline per §2 — a finding
  without one is a bug in the check, not evidence, and the host should refuse to emit it.

## 7. Decided, and what's still open

**Decided, 2026-09-30:**
- **Name: CodeRipper** (`github.com/ciresnave/coderipper`). Checked clean across crates.io, npm, PyPI,
  RubyGems, GitHub and Docker Hub before adoption — two earlier candidates ("Scrutin", "Scrutineer") were
  rejected specifically because they collided with existing tools in the *same domain* (code quality/review
  tooling) on PyPI, not just a bare namespace clash.
- **Repo created**, MIT OR Apache-2.0 (standard Rust dual-license convention), branch protection with
  required CI checks (per CireSnave's standing "every repo gets CI and enforced protection" rule).
- **Implementation language: Rust.**
- **Plugin loading: compiled-in checks for v1**, not dynamic loading — confirmed, not just recommended.
- **Interface: CLI-primary** (clippy-style, the common case), **plus a server mode** — required, not
  optional, because CodeRipper needs to run as a free hosted instance on ThinkersJournal.com for open-source
  projects. This means the `fast`/`sweep` tier split from §1 needs a THIRD consideration added at
  implementation-plan time: a server instance serving untrusted/arbitrary public repos is a different trust
  boundary than a CLI run against this portfolio's own local checkouts — sandboxing, resource limits and
  abuse prevention for the hosted mode aren't designed here yet and need their own pass before that mode
  ships, even though the CLI mode can ship without them.

**Still open:**
- **Where `sweep` tier runs and how often** for the portfolio's own use — PM-triggered, a scheduled cloud
  routine, or purely on-demand.
- **Exact triage ranking (§2)** — tune against real output once more than one check exists, not decided from
  first principles now.
- **ThinkersJournal.com hosting specifics** — not designed at all yet (deployment target, how a public
  OSS project submits itself for a scan, rate limits, abuse handling, whether results are public or
  submitter-only). Deliberately out of scope for the first implementation plan (CLI + reachability check);
  needs its own design pass once the core tool exists and the trust-boundary question above is answered.

---

Once CireSnave rules on §7, this is ready for **superpowers:brainstorming** proper on the actual
implementation (this document validates the shape against four checks' worth of pressure-testing, but it
is still a spec, not a fully brainstormed build plan) followed by an implementation plan, in whichever repo
gets created for it.
