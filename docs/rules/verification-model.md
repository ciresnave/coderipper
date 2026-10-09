# How CodeRipper decides what to report

Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

This page explains how CodeRipper reasons about checking code: what a finding contains, which sources of evidence can back it, how far each source deserves trust, how a team's intent enters the picture, and where language models help and where they must be fenced in.

## What a finding is

A finding is a structured record rather than a line of log text. It identifies the rule and restates the claim, names the tier that produced it, gives the location, lists one or more pieces of evidence (their kind, detail and producing tool), states how serious it is and how likely it is true, records its baseline state (new, baselined or waived), proposes a remediation, and carries any waiver attached to it.

Severity means the impact if the claim turns out to be false; confidence means the measured precision of that rule and tier. These are separate axes, and gates use both. Results are emitted as SARIF so existing code-scanning views and editors can show them with no bespoke integration.

The full path of a claim runs: claim, required evidence, analyzer, finding, remediation, an optional waiver that expires, and finally a trend line.

## Why claims, and why tiers

Tidy code and tidy architecture are not things a tool can simply read from source text. They are collections of claims. For instance: business logic knows nothing about how requests arrive; a reader can follow one function without jumping elsewhere; supporting a new payment provider will not disturb order handling. Each claim can be tested more or less strongly, using different material. So a verifier has five duties:

1. Make claims explicit, through written policy and a declared architecture.
2. Collect whatever material bears on those claims, from all available sources.
3. Weigh each claim and attach a calibrated degree of confidence.
4. Present findings together with how serious they are, what supports them and how to fix them.
5. Manage change over time with baselines, ratchets and waivers.

Two shortcuts are refused. The first holds that every principle can be reduced to a mechanical constraint. It cannot: single responsibility, the quality of an abstraction and "reasonable" complexity all hinge on purpose and on how the code evolves. The second holds that whatever is not mechanical is barely automatable. That undersells what becomes possible when version history, executable specifications and model-based reading join the toolbox. Consider responsibility: combine dependency clusters, which files change together, and a model-written summary compared against the stated purpose, and you get a usable advisory signal. Uncertainty calls for calibration and graded trust, not abstention.

## The six kinds of evidence

- **E1, code facts.** Output of parsers, compilers, type checkers and language servers: declared items, access levels, signatures, resolved types, call locations, unsafe regions. Cheap to get.
- **E2, graph facts.** E1 joined with build metadata: who depends on whom, who calls whom, cycles, layers, the exported surface. Cheap to moderate.
- **E3, execution.** What happens when the program runs: test results, coverage, mutation outcomes, fuzzing, sanitizer reports, benchmarks, traces, production telemetry. Moderate to expensive.
- **E4, history.** Data from version control, reviews, pipelines and incident records: how often files change, which change together, who owns them, where defects cluster, which tests are flaky. Moderate.
- **E5, intent.** Artifacts that people write: the architecture specification, decision records, contracts, threat models, requirements, documentation. Costs human effort.
- **E6, semantic judgment.** A language model reading E1 to E5 and producing role labels, doc-versus-code drift reports or rubric scores. Moderate to expensive, and not repeatable.

## The six tiers of trust

Tiers describe how a conclusion was reached and therefore how much to trust it and how to gate on it.

- **V0, exact.** The answer follows logically from E1 and E2 given the declared policy. Repeatable, with almost no false alarms provided the facts are correct. Gate: block.
- **V1, heuristic-structural.** Metrics, smell detectors and pattern matchers. Repeatable, but with known misses and false alarms. Gate: block only violations that are new or have worsened (a ratchet); merely warn about absolute levels.
- **V2, empirical.** Requires running something: tests, mutation, fuzzing, benchmarks, sanitizers. Mostly repeatable but subject to flakiness, noise and cost. Gate: block on criteria for the changed code; schedule costly runs overnight.
- **V3, historical.** Statistics drawn from E4. Probabilistic, showing correlation rather than cause. Gate: advisory, mainly to prioritise.
- **V4, semantic.** A model judges against a rubric or against declared intent. Varies from run to run and needs calibration. Gate: advisory at first; blocking only once calibrated, and never as the sole basis for a security decision.
- **V5, attested.** A person makes a decision and it is stored as an artifact. Needed for waivers, threat models and architectural changes.

One rule may draw on several tiers. Responsibility checks use cohesion clusters (V1), co-change patterns (V3) and a model's reading of purpose (V4); when independent tiers agree, confidence rises. A high tier is not a high priority: an exact style check can matter less than a soft advisory on authorization.

## Intent as input

CodeRipper checks the architecture a team picked; it does not impose a favourite. Layered, hexagonal, onion, vertical slices, a modular monolith, plugins, pipelines and event-driven structures can all be stated as components, each with a role and an expected rate of change, plus the dependency edges that are permitted. The declared style label is informational; every check derives from the declared components and edges, not from the label. Extra declarations may name types that must not show up in a component's public signatures, flag components that must stay free of filesystem, network, clock, randomness and environment access, bound the size of a public surface, and record exceptions, each with a reason, an owner and an end date.

A companion specification holds quality defaults. As example values: a warning at cognitive complexity 15 and an error at 30, a regression allowance of about a quarter on touched functions, a floor of 0.80 for coverage of changed lines, and a warning when the mutation score on changed code falls under 0.70. A third file, the security specification, sits alongside the architecture and quality ones.

The quality specification also assigns rule sets to analysis scopes. For example, generated code still runs the dependency-boundary and security families but skips readability and test-quality ones, while test code skips performance rules and one error-handling rule. It allows path-specific overrides too, which must state why, for example a higher complexity ceiling for a table-driven parser.

Most older codebases have no declared architecture. The verifier therefore drafts one: it partitions the dependency graph into clusters (dependency-structure-matrix partitioning or community detection), asks a model to name and summarise each cluster using its public interface and docs, infers how volatile each cluster is from change history rather than guessing, and writes a draft marked with confidence levels. A person edits and signs it (V5), after which it binds. Without such a starting point, a demand to declare the architecture simply blocks adoption.

## Where language models help, and the fences around them

Models earn their place where static analysis runs out:

- labelling roles and responsibilities (entity, transfer object, adapter, use case, effectful or pure), then caching and reviewing those labels so exact rules can consume them;
- assessing a change or module against recorded intent, such as a design decision;
- spotting contradictions between a name and its behaviour, documentation and code, a test title and its assertions, an error message and its cause, configuration documentation and actual use;
- recognising duplicated logic written differently, or an existing helper that makes new code unnecessary;
- learning the local habit for doing something, then flagging departures;
- explaining and sorting other findings, and drafting patches;
- suggesting exact rules based on repeated valid findings.

Their weaknesses are equally real. Answers shift with phrasing, ordering and how context is assembled. They anchor on labels: a comment asserting thread safety pushes the verdict. They can cite lines that are not there. They struggle with whole-program claims and cannot dependably prove that no route exists between two places. They are slow and costly, and they change when the underlying model does. Most important, everything under review counts as untrusted input, since comments, docs, fixtures and dependency readmes can carry planted instructions.

### Guardrails

1. Output tied to evidence: each finding cites files and line ranges, and plain code checks that the quoted text exists before anyone sees it.
2. Rubrics and schemas: every semantic rule defines its criteria with good and bad examples and returns a fixed-shape verdict.
3. For gating checks, plain code gathers the facts, the graph neighbourhood and the diff, rather than letting the model go looking; free-roaming agent exploration is reserved for advisory checks.
4. Calibration: each rule has a labelled sample set, measured precision and recall, and a reported confidence. A rule advances through experimental, advisory and blocking only past a precision bar, and steps back if quality drifts. Models and prompts are pinned, and the sample set is rerun after any change.
5. Control of variance: cool sampling, multiple samples or agreement among runs before a blocking verdict, and a second pass whose job is to knock the finding down, ideally run on a different model from the first.
6. Lopsided powers: a model may raise or explain a finding. It may not dismiss an exact one, approve a waiver, or alone justify a security gate.
7. Treat repository text as data to quote, never to obey. The reviewing process holds no capability to alter the repository or send data out. Embedded instructions are disregarded and may be reported.
8. Convert to exactness over time: once a kind of model-found problem keeps turning out genuine, write it as a pattern rule (an ast-grep or Semgrep-style pattern, or a custom lint), leaving the model with discovery and sorting. The ratchet thus runs from semantic to heuristic to exact.
9. Data handling: code leaving the organisation's boundary follows its policy; confidential code goes to local or private models.

Analysis follows the natural nesting of code, from workspace down to function. A summary of each unit is stored under a hash of its content. Every claim is checked at the level where it applies, using the exported interfaces and stored summaries of the units concerned instead of all their source. Cost then grows with how much changed, not with how big the repository is.

## How the verifier is built

Policy and intent files sit on top (the architecture, quality and security specifications, decision records and contracts). Below them is an evidence model, a typed graph of facts fed by the five evidence producers. A rule engine asks questions of that graph and emits findings carrying severity, confidence and proof. After that comes governance: baselines, ratchets, waivers and trends. Finally results flow to SARIF consumers, CI gates, editors, agent loops and dashboards.

- The engine does not care where a fact originated. "The core may not depend on infrastructure" is a single rule regardless of language. Rules query a graph whose nodes include workspaces, packages, modules, items, types, functions, call sites, dependencies, commits, tests and findings, and whose edges include containment, import, call, implementation, exposure, co-change, coverage and ownership. Datalog-style and graph-query engines fit, for example Trustfall (behind cargo-semver-checks), CodeQL and Joern.
- Every edge records how it was learned and how sure we are: resolved by a compiler, inferred by heuristic or model, or declared in a spec. Reflection, dependency-injection containers, macros and generated code obscure edges; an unresolved edge reduces confidence rather than disappearing.
- Accurate semantics need front ends of compiler quality, through language servers or compiler interfaces. Tree-sitter is the wide-coverage fallback for syntax-only checks.
- Existing compilers, linters, security scanners and mutation, coverage and supply-chain tools are producers of evidence to ingest, not things to rewrite; the build-versus-integrate reasoning is in [cross-cutting.md](cross-cutting.md).
- One policy should apply in the editor, at commit time, in CI, and in the loop of coding agents, so feedback appears where writing happens (see AIH-001 for the agent case).
- Prefer incremental work: store facts under content hashes, analyse only the diff for pull requests, run the full pass overnight.
- Build configuration alters the graph (Rust features and `cfg`, the C preprocessor, variants), so analyse every declared combination, not just the default.

## Measures and thresholds: using them honestly

- Thresholds come in three kinds. Absolute ones set fixed levels (a cognitive-complexity warning near 15 and an error near 30). Contextual ones set different levels for parsers, state machines or generated code and record the reason. Regression ones forbid touched code from becoming noticeably worse; they are the most useful and the least argued over.
- Adjust for size. In many datasets cyclomatic complexity tracks line count closely (Herraiz and Hassan), so raw totals say little beyond size. Favour percentile outliers inside the same codebase and per-change differences.
- Know the research. Cognitive complexity shows a moderate, validated link to how hard code is to understand (Munoz Baron, Wyrich and Wagner, 2020). Coverage is a weak indicator of test strength after size is accounted for (Inozemtseva and Holmes, 2014), whereas mutation score is a closer stand-in for fault detection (Just and colleagues, 2014). The LCOM family of cohesion metrics disagrees internally and rests on little evidence. Change history forecasts defects fairly well (Nagappan and Ball, 2005), so use it to rank work, not to block it.
- Goodhart's law bites. Combine measures that can be inflated in opposing ways: complexity alongside test strength, coverage alongside mutation score. Do not feed individual-level numbers into performance appraisal.
- A metric is a clue, not a judgment. Crossing a threshold raises a question; a waiver that states a reason can settle it.

All figures on these pages are starting defaults that each project should set for itself.
