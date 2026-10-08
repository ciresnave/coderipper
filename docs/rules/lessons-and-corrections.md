# Lessons and corrections

Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

The catalog was assembled by checking an earlier draft, and a critique of that draft, against the literature and against
practice. Several common beliefs did not survive. They are recorded here as lessons, because a checker that encodes a
popular misreading will confidently report the wrong thing. This page is background, not part of the rules.

## Conceptual corrections

**Not every principle is a mechanical constraint.** Some claims depend on intent or on history, and no static check
settles them. The catalog therefore tags each rule with the kinds of evidence that can support it (deterministic facts,
structural heuristics, measurement, history, model judgment, human attestation) instead of pretending that everything
is verifiable the same way.

**Single responsibility is about who asks for change.** The principle concerns one stakeholder, one reason for a
module to change. It is not "a class does one thing", and a name containing "and" is a weak hint at best. The
responsibility rules combine several signals (how members cluster by shared data, how files change together, what the
module claims to be for) and treat any one of them as a low-confidence clue.

**Open-closed and dependency inversion are not object-oriented only.** They can be met with traits, algebraic data
types, plain functions and modules. The catalog states them as a rule about volatility (what changes often should not be
depended on by what changes rarely), offers a menu of mechanisms, and separates sets of variants that must stay closed
from those meant to be extended.

**SOLID has five letters.** An earlier treatment covered three. Substitutability and interface segregation are now
covered, and substitutability is checked through contract tests that every implementation must pass.

**Fail-fast is a boundary policy, not "crash at once".** The useful idea is to reject bad input where it enters and to
refuse to continue in a state the program cannot reason about, while still containing and reporting failures where a
caller can act on them. Crashing everywhere is not the rule.

**Asynchronous messaging does not by itself stop cascading failure.** Queues bring failure modes of their own: a growing
backlog, poison messages, repeated delivery and retries that line up in time. Resilience rules therefore ask
for explicit limits, dead-letter handling and idempotent consumers.

**"Add retries" is dangerous advice on its own.** Retrying is safe only when the operation is idempotent, the delays
back off with randomness, and the total retry effort is capped. Otherwise retries amplify an outage.

**Thresholds are defaults, not laws.** Figures such as a cognitive-complexity ceiling, an inheritance depth limit or a
cyclomatic cap depend on context and track code size. The catalog distinguishes absolute limits, limits relative to the
surrounding code, and regression limits that only forbid getting worse.

**Microservices are not simply single responsibility at larger scale.** The comparison with the Unix philosophy is an
analogy. It overlooks the distributed monolith, where services are separate to deploy but tightly bound to change.

**"Most of the cost is maintenance" has weak provenance.** The catalog keeps only the directional claim that
maintenance matters, with no percentage.

**Unverified attributions and tool claims were dropped.** One principle was credited to a specific author without
confirmation, and one platform was said to split classes using a cohesion metric without confirmation. The idea was kept
and the attribution removed; cohesion metrics are a low-confidence signal because the evidence for them is thin.

**Clean Architecture is a style, not the target.** Teams choose among layered, hexagonal, modular and other shapes. The
catalog works from a declared architecture, which the project states (or which is recovered from the code and then
reviewed), and checks code against that declaration.

**Whole areas were missing or thin.** Each of these became its own domain: testing, correctness, concurrency, evolving APIs, supply chain, speed,
running in production, and mining history.

**Tools listed per principle are not a method.** A list of tools says nothing about what claim each one supports. The
catalog follows a chain from claim, to evidence, to analyzer, to finding.

## What the critique contributed

Taken over as written: the claims-and-evidence model; the declared architecture as an input; attention to what a
dependency means and to implementation types leaking across boundaries; fitness functions as the governance idea; metrics
as signals with regression limits; correctness; test quality, with mutation testing; safe concurrency and resource handling;
security, supply-chain, API-compatibility and operability domains; version history as evidence; baselines, ratchets and
waivers; refusing a single composite score; making invalid states unrepresentable; separating pure logic from effects;
configuration correctness; separate treatment of generated and vendored code; and using the existing Rust tooling as the
base of the verifier.

Taken over with changes:

- The critique's table of how automatable each topic is became the evidence-tier model. A rule can carry several tiers,
  and history analysis and LLM judgment lift topics once rated "hard" to advisory grade.
- Its fourteen-category taxonomy informed the list of domains, but the catalog stays organized by problem, with
  verifiability recorded per rule.
- Its points on retries and on fail-fast went into the reliability and error-handling domains.
- Architecture metrics (instability, abstractness, distance from the main sequence) are included, with the warning that
  abstractness depends on the language.

Not adopted:

1. **Organizing around what can be verified.** The catalog is meant to guide people who write code as well as tools, so it
   is thematic by problem; verifiability is an attribute.
2. **Treating model-based evaluation as unavailable.** With guardrails, it turns several "low" items into usable
   advisory checks and provides a path for discovering new deterministic rules.
3. **Building scanners for security, supply chain and operations.** Those domains specify policy and wiring only; build
   effort stays on stating architecture intent, modeling evidence, judging with models, and governing results.
4. **The self-scoring table** of the earlier draft, which carried little actionable content.

The critique itself left gaps that the catalog fills: recovering architecture for legacy code; guardrails for model
evaluation, including prompt injection from the code being reviewed; recording contested guidance; performance as a
domain; hygiene for AI-assisted development; confidence on edges and analysis per build configuration; calibrating rules
and testing the rules themselves; and guideline text that coding agents can act on.

## Additions from the independent review

A model-judgment tier with nine guardrails and a "distill to deterministic" ratchet; model-produced facts feeding
deterministic rules; architecture recovered from code and history; edges that record where they came from and how
sure they are; analysis per build configuration; layered analysis keyed for caching; the dependency rule driven by
volatility, plus the stable-dependencies metric; the expression problem as the way to think about open-closed; deep
modules to balance excessive extraction; the register of
contested guidance ([cross-cutting.md](cross-cutting.md)); evidence-strength tags; a rule lifecycle with measured
precision; tests for the rules; performance and AI-assisted development as domains; a single record from which human
guidelines, agent instructions and checker behavior come; and SARIF for exchanging results.
