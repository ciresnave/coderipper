# Cross-cutting material

Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

This page holds what does not belong to any single rule domain: where experienced engineers disagree and which side
CodeRipper leans toward, what each language's own tooling already gives you, how an LLM-assisted check should be
specified, what to build and what to hand to existing tools, the order in which to adopt checks, and how the
catalog record in this repository relates to the knowledge base's record shape.

## 1. Contested guidance

Good practice is not a single voice. When respected schools disagree, a rule should record both positions and ship a
default that a project can change, instead of presenting one school as settled. The register below states each dispute
and the position CodeRipper takes by default. Rule ids point to the catalog record where the position is applied.

| Topic | One side | The other side | CodeRipper's default |
| --- | --- | --- | --- |
| Function size | Keep functions tiny and extract until nothing is left to pull out | Deep modules with simple interfaces; excess splitting scatters logic and raises the reading burden | No length limit. Use cognitive complexity, nesting depth and locality. Report both sprawling functions and chains of trivial pass-throughs (MOD-008) |
| Duplication | One representation per piece of knowledge | A copy is cheaper than a premature abstraction | Merge duplicated knowledge; for code wait for a third occurrence; allow a recorded waiver when two copies only look alike (RDB-005) |
| Inverting dependencies | Wrap each outside dependency in an interface of your own | Do not abstract until variation or isolation is a real need | An abstraction needs a stated justification (ARC-008, EXT-007) |
| Services or one deployable | Independent deploys, scaling and team autonomy | Simplicity, atomic changes, lower running cost | Start as one deployable made of modules whose borders are enforced; carve out a service only when scaling, release cadence or ownership demands it |
| Exceptions or result values | Exceptions keep the success path uncluttered | Result values put failure in the type, where it can be combined | Follow the language idiom and stay consistent inside a codebase. The error rules are written to cover both |
| Inheritance or composition | Inheritance gives reuse and polymorphism | Inheritance couples subclasses tightly and breaks easily | Prefer composition; allow inheritance where an extension contract is written down (EXT-005) |
| Mocking style | Mock collaborators and assert on interactions | Use real collaborators and assert on resulting state | Fakes for boundaries you own; do not verify internal calls; judge the suite by mutation score, not by style (TST-004) |
| Comments | Needing a comment means the code failed to say it | Comments hold design intent that code cannot show | Welcome comments that explain why; reject ones that restate the code or have gone stale (RDB-008, RDB-010) |
| Leniency toward input | Accept anything plausible | Parse strictly; leniency hardens bad behavior into protocols and opens attack surface | Parse strictly where data enters. If leniency is needed, state it, document it and limit it (TYP-004) |
| Clarity or speed | Small objects and polymorphism by default | Data-oriented layout: the price of abstraction in hot loops is large | Clean by default; performance exceptions are documented and benchmarked (PRF-006) |
| Test-first | Writing tests first improves design | Tests written afterward or alongside are fine | Judge outcomes (changed-line coverage, mutation score), not the process |
| Coverage targets | A fixed line-coverage percentage as a gate | Coverage tracks fault detection weakly | Gate on changed-line coverage and mutation score; report the absolute figure without gating on it (TST-001, TST-002) |
| Early return | One exit per function | Guard clauses flatten nesting | Guard clauses, with scope-based cleanup (RAII, `defer`, `finally`) |
| Dependencies | Reuse instead of reinventing | Every dependency is a standing liability | Each dependency carries a recorded reason (SUP-007) |

The `contested` flag on a catalog record marks rules that sit inside one of these disputes. The rules each dispute
touches:

| Dispute | Rules affected |
| --- | --- |
| Function size | MOD-008 |
| Duplication | RDB-005 |
| Inverting dependencies | ARC-008, EXT-007 |
| Services or one deployable | the ARC domain (see ARC-011 on distributed monoliths) |
| Exceptions or result values | the ERR domain, written for both styles |
| Inheritance or composition | EXT-005 |
| Mocking style | TST-004 |
| Comments | RDB-008, RDB-010 |
| Leniency toward input | TYP-004 |
| Clarity or speed | PRF-006 |
| Test-first | none; judged by outcomes (see TST-001, TST-002) |
| Coverage targets | TST-001, TST-002 |
| Early return | none; a style default |
| Dependencies | SUP-007 |

## 2. Language and ecosystem profiles

This is a sample, not a census, and tooling status changes: confirm a tool is still maintained before relying on it.
The aim is to see what each ecosystem already gives you, so CodeRipper delegates that work and spends its effort on
what no tool does. The profile is arranged by purpose, then by language.

### Type and semantic facts

Facts that need real name and type resolution should come from the language's own front end, not from a re-parse.

- Rust: the compiler itself, `cargo metadata` for the package graph, and `syn` when only syntax is needed. Custom lints can
  be written with Dylint, and the rust-analyzer crates expose resolved names when `syn` is not enough. Rustdoc's JSON output describes the public surface but its format is unstable, so pin the toolchain.
- JVM languages: the Java compiler; ASM for reading compiled bytecode; Error Prone for extra compile-time checks.
- .NET: Roslyn gives full syntax and semantic models.
- TypeScript and JavaScript: the TypeScript compiler API, with tree-sitter when speed and breadth matter more than types.
- Python: the standard `ast` module or libcst for syntax; a type checker such as mypy or pyright for types.
- Go: `go/packages` plus the vet and staticcheck analyzers.
- C and C++: libclang and clang-tidy, driven by the project's compilation database.

### Boundaries and architecture

- Rust: the workspace dependency graph, cargo-modules for the module tree, Clippy's `disallowed_*` lints, and cargo-deny
  bans for crates. The compiler's own visibility lints also police what a module exposes.
- Java and Kotlin: ArchUnit expresses layer rules as tests; jMolecules adds architecture annotations; Sonargraph is a
  commercial option for architecture models.
- .NET: NetArchTest and ArchUnitNET do the same job as ArchUnit; NDepend is the commercial analyzer.
- TypeScript: dependency-cruiser, ESLint boundary plugins, Nx module-boundary rules, and madge for import graphs;
  knip finds unused exports.
- Python: import-linter for layer contracts, deptry for declared versus imported packages.
- Go: the compiler enforces `internal/` directories; depguard restricts imports further, and go-arch-lint checks a
  declared layering.
- C and C++: include-what-you-use for header hygiene; anything stronger is custom clang tooling.

### Tests and test strength

- Rust: cargo-llvm-cov for coverage, cargo-nextest as runner, cargo-mutants for mutation, proptest and cargo-fuzz for
  generated input; Miri, loom, shuttle or Kani for undefined behavior, interleavings and proofs.
- JVM: JaCoCo coverage, PIT mutation, jqwik properties.
- .NET: coverlet, Stryker.NET, FsCheck.
- TypeScript: c8 or Istanbul coverage, StrykerJS, fast-check.
- Python: coverage.py, mutmut, Hypothesis.
- Go: the built-in race detector and fuzzing support; mutation tooling exists but is less settled.
- C and C++: the compiler sanitizers, libFuzzer, and Mull for mutation.

### API compatibility

- Rust: cargo-semver-checks and cargo-public-api. JVM: japicmp. .NET: the public-API analyzers and ApiCompat.
- TypeScript: api-extractor. Python: griffe. Go: apidiff. C and C++: ABI compliance checkers.

### Where each ecosystem blinds the analysis

- Rust: code produced by macros, edges that exist only under some feature sets, calls through trait objects, and the lack of
  a stable compiler API.
- JVM and .NET: wiring done by dependency-injection containers, reflection, annotation processors and source generators.
- TypeScript: dynamic imports, types that vanish at runtime (validate at boundaries), and monkey-patching.
- Python: dynamic dispatch and optional typing; pair static analysis with runtime tracing.
- Go: interfaces satisfied implicitly hide who depends on whom.
- C and C++: the preprocessor, template instantiation, and results that change with the build configuration.

### Across languages

Tree-sitter parses most languages quickly; ast-grep and comby search and rewrite by structure; OpenRewrite covers JVM
refactoring. Semgrep and CodeQL run pattern and query rules (read the licence of any rule pack). SonarQube and
CodeScene report metrics and history. For policy as code, OPA, Kyverno, Conftest and Checkov are the common choices. OpenTelemetry
carries runtime telemetry, and SARIF carries analysis results between tools.

### If the project is Rust

The Rust toolchain already yields most of the exact and measured evidence a verifier wants: type checking, formatting,
a large lint suite, reproducible dependency resolution, advisory and licence policy through cargo-audit and cargo-deny,
API compatibility, coverage, mutation testing, fuzzing, and checkers for undefined behavior and concurrency. Clippy
sorts its lints into groups from must-fix correctness down to opinionated and experimental ones; the "restriction"
group is off unless you opt in lint by lint. What is left for CodeRipper is the layer above: stating architecture
intent, a fact graph analyzed per feature set, history analytics, LLM-assisted evaluation, and governance.

## 3. A worked rubric for an LLM-assisted check

An LLM-assisted check (evidence tier V4) is only acceptable when code assembles its inputs and code verifies its output.
The model never decides what to look at, and its claims are never taken on trust. This rubric, for the rule that asks
whether a module has one coherent responsibility (MOD-007), shows the shape. It is CodeRipper's own design built on the
same ideas.

```toml
rule = "MOD-007"
tier = "V4"
gating = "advisory"            # until measured precision reaches the target on a labelled set

[inputs]                       # gathered by code, not chosen by the model
public_surface = "signatures and doc comments of the module"
member_clusters = "which members use the same data (output of MOD-004)"
change_clusters = "files that changed together over the past year (output of EVO-003)"
declared_purpose = "owner and purpose from the architecture declaration"

[task]
steps = [
  "Name each distinct stakeholder or reason for change the module serves.",
  "Assign each member to a stakeholder, quoting its file and line span.",
  "Judge whether those stakeholders are one party or share a change rhythm.",
  "Compare with the declared purpose and note anything outside it.",
]
good_example = "a pricing module that only applies tax and discount rules, one stakeholder"
bad_example  = "a manager type that logs users in, stores them and mails them: three stakeholders"

[answer]
verdict = ["coherent", "mixed", "unclear"]
purpose_match = ["match", "partial", "mismatch"]
confidence = "0 to 1"
actors = "per stakeholder: a name, the member list, and the file and line spans that support it"
suggested_split = "optional: how the module could be divided along those stakeholders"

[post_checks]                  # deterministic, run on every answer
cited_spans_exist_and_name_the_symbols = true
listed_members_belong_to_the_module = true
disagreement_with_member_clusters = "lowers confidence, never hides the answer"
```

Why it is built this way:

- Inputs come from deterministic analyzers, so the model reasons over facts that were already verified.
- The answer is structured and every claim carries a citation that a script can check against the source.
- Disagreement between the model and a cheaper structural signal is information, so it lowers confidence rather
  than suppressing output.
- The check stays advisory until its precision is measured against a hand-labelled sample (the example target is 0.80 over
  one hundred modules). Findings that prove reliable are then distilled into cheaper deterministic rules.

Rubrics worth writing first, as the highest value for the lowest risk: whether a name matches behavior (RDB-009), whether
a comment or doc has drifted from its code (RDB-010, DOC-006), whether a test name matches its assertions (TST-008),
whether a change agrees with its decision record (ARC-010), whether an added layer of indirection is justified
(ARC-008), whether new code reuses what exists (AIH-003), and whether a dependency has its recorded reason (SUP-007).

## 4. Build or integrate

| Component | Build | Integrate | Reasoning |
| --- | :---: | :---: | --- |
| Language for architecture and quality intent; recovering it from old code | yes | | The differentiator; no existing tool states intent across languages |
| Fact graph and rule engine | yes | reuse query engines | The engine must not depend on any one language |
| Language front-ends | | yes | Compilers, language servers and tree-sitter already exist |
| History analytics (co-change, hotspots, ownership) | yes | borrow ideas | Results must join with the spec and the facts |
| LLM evaluation harness: rubrics, calibration, guardrails | yes | | The other differentiator |
| Governance: baselines, ratchets, waivers, trends | yes | | Must behave the same for every source of findings |
| Cross-repository graph and blast radius | yes | | Specific to an organization |
| Formatters, linters, type checkers | | yes | Mature, fast, editor-integrated |
| Security scanning, secrets, composition analysis, licences | | yes | Large rule corpora and databases that nobody should rebuild |
| Coverage tools, mutation testers, fuzzers, sanitizers, formal verifiers | | yes | Run them, then read in what they report |
| API and schema compatibility | | yes | Orchestrate |
| Deploy-time and runtime policy | | yes | OPA, Kyverno and IaC scanners |
| Benchmark harnesses | | yes | Layer budgets and gates over them |
| Result interchange and dashboards | emit SARIF | use code-scanning UIs | A standard format avoids lock-in |

CodeRipper's own part is the middle: it runs the tools, normalizes what they report, applies baselines and waivers, and
explains a finding. It does not compete with them.

## 5. Rollout path

1. **Inventory and baseline.** Classify repositories and the scope of code in each. Run everything report-only, snapshot
   the baseline, and recover draft architecture declarations for review.
2. **Deterministic gates.** Start with formatting, compiler warnings, banned dependencies, cycles, leaked secrets, lockfile hygiene, breaking API
   changes and dependencies with known advisories. This tier is quiet and valuable.
3. **Heuristic checks as ratchets.** Complexity, duplication, smells and test-hygiene checks block only on new or
   worsened findings. Tune thresholds against regression cases.
4. **Measured checks on the diff.** Changed-line coverage and mutation score on edited code; property tests and fuzzing
   on modules you mark; benchmarks on marked hot paths. Run the full versions nightly.
5. **History and LLM checks, advisory.** Hotspot and hidden-coupling reports; evidence-bound LLM comments. Record
   developer reactions as labels.
6. **Calibrate, then promote.** Track each rule's precision. Let findings that hold up become blocking, or rewrite them
   as cheap deterministic rules; demote any rule whose precision slips.
7. **Bring coding agents inside the loop.** Apply the same policy where agents write code, and give them each rule's
   rationale and examples.

## 6. The knowledge base's record and CodeRipper's catalog record

The knowledge base proposes that every rule be a record from which both the human guideline and the checker are
generated, so the two cannot drift. CodeRipper adopted that idea (design section 4, `src/catalog.rs`). The shape
differs in these ways.

- **Format.** The records are TOML tables (`[[rule]]` in `rules/<domain>.toml`), not YAML. The crate already parses
  TOML, and the available YAML crates are deprecated or young.
- **Strict loading.** An unknown field, a missing field, a blank statement or a repeated id rejects the file and names it.
- **No thresholds yet.** The knowledge base's record has a `thresholds` entry. The loader does not accept one; it will
  arrive with the first rule that needs a number, so for now a default figure is written in the statement.
- **Fields kept.** `id`, `title`, `domain`, `statement`, `rationale`, `evidence_tiers`, `evidence_strength`,
  `contested`, `applicability` (languages and scopes), `default_severity` (a severity from info to critical),
  `confidence_class`, `remediation`, `related` and `lifecycle`.
- **Fields added by CodeRipper.** `scope`, `network` (decides the run tier), `unit` (package or repository),
  `executes_project_code` (whether checking runs build scripts or tests), `aliases` (other ids a record answers to) and
  `kb_refs` (knowledge-base ids a record overlaps; empty for rules that already carry a knowledge-base id).
- **Renamed.** The knowledge base's `tiers` is `evidence_tiers` here, with the same V0 to V5 values. Its separate `evidence`
  field (classes of evidence such as resolved types or an architecture spec) is not stored as a field; it is
  expressed by the tier list and by each language module's coverage entry for the rule.
- **Fields not carried.** The knowledge base's `analyzers`, `exceptions`, `examples`, and the two generated prose
  fields meant for humans and for agents are not stored in the record. Analyzers belong to
  language modules (coverage is kept per rule and language), exceptions live in the project's allowlist, and the
  human and agent texts are produced from the statement and remediation.
- **Lifecycle.** The knowledge base uses experimental, advisory and blocking. All catalog records start as
  `experimental`; a project profile can raise or lower the level, and the exit code still follows `--deny`.
- **Ids.** New records use the knowledge base's `XXX-nnn` ids. The five original checks keep their kebab-case ids so
  existing allowlist entries keep working.
- **Words.** The text in each record is CodeRipper's own paraphrase, never the knowledge base's wording.

## Further reading

See [the reading list](reading-list.md) for the works behind these positions and [the lessons](lessons-and-corrections.md)
for the beliefs the catalog deliberately does not repeat.
