# CodeRipper rule guides

Informed by the owner's knowledge base v2 and written in CodeRipper's own words; rule IDs and ideas carry over, its text does not.

## Domains

| Code | Guide | Summary | Claim in brief | Outlook for automation | Tiers |
| :--- | :--- | :--- | :--- | :--- | :--- |
| GOV | [gov.md](gov.md) | Governance and enforcement: baselines, waivers, rule lifecycle, scopes, reporting | Automated, versioned, reviewable enforcement; per-dimension reporting | feature of the tool itself | V0 V5 |
| MOD | [mod.md](mod.md) | Decomposition and information hiding | Cut modules where change pressures differ; conceal volatile decisions | strong for structure; whether a cut is right needs history and semantic review | V0 V1 V3 V4 |
| ARC | [arc.md](arc.md) | Dependencies and architectural boundaries | Dependencies run toward the stable; boundaries stay intact | strong once the architecture is declared | V0 V1 V3 V4 |
| EXT | [ext.md](ext.md) | Extensibility and substitutability | Open up only where variation is proven; implementations keep their promises | moderate; substitutability needs contracts that can be run | V1 V2 V3 |
| RDB | [rdb.md](rdb.md) | Simplicity, readability and naming | Reduce the load on whoever reads the code | strong for style, moderate for cognitive load, semantic review for meaning | V0 V1 V4 |
| TYP | [typ.md](typ.md) | Domain modelling and type-driven design | Types should rule out invalid states; parse input at the edge | strong in statically typed languages | V0 V1 V4 |
| EFX | [efx.md](efx.md) | State, purity and effect boundaries | Push effects and hidden state to the outer layer | strong where API deny-lists exist | V0 V1 V2 |
| ERR | [err.md](err.md) | Error handling and failure semantics | Each failure has an owner, a kind and context | moderate to strong | V0 V1 V2 |
| COR | [cor.md](cor.md) | Correctness, contracts and specifications | Behaviour is stated precisely instead of only sampled | moderate; strong when specs execute | V2 V4 V5 |
| TST | [tst.md](tst.md) | Test quality | A test suite should catch faults, not only touch lines | strong (mutation testing, diff coverage) | V1 V2 |
| CON | [con.md](con.md) | Concurrency and resource safety | Concurrency and resource lifetimes are deliberate, bounded, owned | moderate; varies by language | V0 V1 V2 |
| REL | [rel.md](rel.md) | Reliability and resilience | Expect parts to fail; every retry or queue has a price | moderate | V1 V2 V4 |
| SEC | [sec.md](sec.md) | Security | Untrusted input crosses named boundaries; privileges stay minimal | ingest scanners; semantic review for logic flaws | V1 V2 V4 V5 |
| SUP | [sup.md](sup.md) | Dependencies and supply chain | Using someone else's code is a trust decision | very strong; integrate existing tools | V0 |
| API | [api.md](api.md) | API design and evolution safety | A public surface is a promise to its users | very strong where tools exist | V0 V2 |
| PRF | [prf.md](prf.md) | Performance and efficiency | Speed is a requirement; measure on both sides of a change | strong given benchmark infrastructure | V2 |
| OPS | [ops.md](ops.md) | Operability, observability and configuration | Running systems can be diagnosed and configured safely | moderate | V0 V1 V2 |
| DOC | [doc.md](doc.md) | Documentation, intent and decision records | Write intent down so it can be tested | moderate; semantic review for drift | V0 V4 V5 |
| EVO | [evo.md](evo.md) | Change history and evolutionary health | Version history reveals the real structure | strong signals, advisory in force | V3 |
| WSP | [wsp.md](wsp.md) | Multi-codebase and workspace governance | Quality also spans repository boundaries | moderate | V0 V2 V3 |
| AIH | [aih.md](aih.md) | AI-assisted development hygiene | Generated code changes how software fails | early stage | V0 V2 V4 |

## Purpose and organisation

These pages serve two outputs: coding guidelines that developers apply, each with its reasoning and examples plus a note on how it is verified, and a verification toolchain that audits codebases against those rules using both exact and model-assisted analysis. Pages group rules by the problem they address, since guideline authors work that way. Verifiability, supporting evidence and trustworthiness of the outcome are recorded per rule rather than driving the layout.

## Conventions

- **Rule ids** look like `ARC-004`: a domain prefix and a number. Treat a rule as a default assertion that each project tunes, not as a command.
- **Tier tags** V0 to V5 are explained in [verification-model.md](verification-model.md).
- **Evidence strength** tags: E means support from several empirical studies; C means agreement among practitioners that lacks solid empirical support; F means folklore or openly contested.
- **Thresholds** are starting defaults, never universal laws.
- **Vocabulary.** Rules sometimes use object-oriented words for convenience. Read "interface" as any trait, protocol, abstract base, or function type; read "class" as a type together with its implementations or its module. Examples come from several mainstream languages, including Rust, Java, C#, TypeScript, Python, Go and C++.
- **Tool statements** reflect what was known about the tool landscape in late 2026. Before depending on a tool, confirm its current version, whether it is maintained, and its licence, especially for third-party rule packs.
- **Statistics.** Widely repeated figures, such as the share of lifetime cost spent on maintenance, have weak provenance. Take the direction they point in, not the number.

## Scope

- **Core design and code quality** (MOD, ARC, EXT, RDB, TYP, EFX, ERR, COR, TST, CON, EVO): treated in depth, since that is where a verifier contributes most.
- **Integrated quality attributes** (REL, SEC, SUP, API, PRF, OPS): say what to demand and how to take in output from the leading existing tools.
- **Cross-cutting** (DOC, WSP, AIH, GOV): capturing intent, governing several repositories, hygiene for the era of generated code, and enforcement mechanics.
- **Out of scope:** user-interface design and accessibility, validity of machine-learning models, organisational process and team topology (except where Conway's-law-driven boundaries shape code), and legal compliance regimes. They come up only when they limit how code can be structured.

## Background pages

- [verification-model.md](verification-model.md): evidence classes, verification tiers, findings, intent as input, model-assisted checks and their guardrails.
- [cross-cutting.md](cross-cutting.md): themes that span several domains.
- [lessons-and-corrections.md](lessons-and-corrections.md): ideas that were corrected or are contested.
- [reading-list.md](reading-list.md): the works the guides recommend.

## From a rule record to its guide page

1. Rule records live in `rules/<code>.toml`, one file per domain, with lowercase file names.
2. A record has an `id` such as `ARC-004`; its three-letter prefix is the domain code.
3. The guide for that domain is `docs/rules/<code>.md`, in this directory.
4. The page explains the problem, the recommended practice and the cautions for the whole domain.
5. Its "Where it is checked" table has one row per rule id: title, checkability class, and what, if anything, checks it today.
6. A record's `evidence_tiers` use the V0 to V5 scale explained in the verification model page.
7. `default_severity` and `confidence_class` map to the severity and confidence axes described there.
8. `related` lists rule ids that belong with this one; look them up in their own domain files.
9. Older records with kebab-case ids (for example `reachability`) name a check that runs today; their `kb_refs` point to the numbered rules.
10. Every number on the pages is a default to configure per project, not a law.
