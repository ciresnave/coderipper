# Writing down intent: decision records, docs that stay true, and a shared vocabulary (DOC)

Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

## Why it matters

A program shows what it does but not why, what it must never do, or where its edges lie. Intent that was never
recorded cannot be checked, and any tool that verifies architecture needs that intent as input. Prose also
drifts away from the code it describes without anyone noticing, and a verifier can catch some of that drift.
Practitioner consensus backs this, with growing support for decision records and docs-as-code in large
organisations.

## How to apply

**Capture decisions**
- Write a decision record for each significant choice: context, the decision, consequences, status, and the
  record it supersedes. Point it at whichever rules and fitness checks enforce it.

**Describe the structure**
- Keep an architecture overview in the repository: system-context, container and component views written as code,
  regenerated from, or compared with, the real dependency graph. State each component's purpose and owner.

**Document modules and APIs**
- For a module, say what it is for, what invariants it keeps and what it deliberately does not do. Give public
  API docs examples, and have continuous integration run them.

**Keep it alive**
- Store documentation beside the code and review it in the same change. Record user-visible changes in a
  changelog, keep a glossary so terms mean one thing, and check that links still resolve.

## Background and lineage

Knuth's literate programming; Parnas and Clements on presenting a rational design process; Nygard's
architecture decision records; arc42 and the C4 model; docs-as-code; the Diataxis framework (four kinds of page:
tutorial, how-to guide, reference and explanation); doctests and rustdoc tests; OpenAPI and similar
machine-readable contracts; the Keep a Changelog convention.

## Measures and numbers

No numeric threshold is defined here. The measurable facts are presence ones: documented public items, examples
that run, decision records that parse, changelog entries per change, links that resolve.

## Cautions

- Deciding whether prose still matches behaviour (DOC-006) needs judgement; the mechanical part is cross-checking
  names and signatures.
- Whether a change is "architecturally significant" is approximated by graph differences; a missing record is a
  prompt for the author, not proof of neglect.
- Link checking of external addresses needs network access and can fail for reasons unrelated to the project.

## Where it is checked

No rule in this domain is run by the current release; every row below is a catalog record only.

| Rule | Title | Class | Checked by |
| :--- | :--- | :--- | :--- |
| DOC-001 | Public items are documented | P | catalog record; no check yet; planned: rustc for Rust, typedoc for TypeScript, ruff for Python |
| DOC-002 | Documentation examples build and run | X | catalog record; no check yet (needs running project code) |
| DOC-003 | Components carry a purpose and an owner in the spec | A | catalog record; no check yet (needs a declared architecture) |
| DOC-004 | Significant architectural changes come with a decision record | P | catalog record; no check yet; planned: a native check diffing dependency edges and looking for a record in the change |
| DOC-005 | Decision-record statuses and links are consistent | N | catalog record; no check yet; planned: a native parser of record front matter for status and supersession links |
| DOC-006 | Docs and comments match current behaviour | L | catalog record; no check yet (needs a language-model judgement) |
| DOC-007 | Diagrams that have drifted from the dependency graph | A | catalog record; no check yet (needs a declared architecture) |
| DOC-008 | Changes users can see get changelog entries | N | catalog record; no check yet; planned: a native check that the changelog is touched when non-test source changes |
| DOC-009 | A glossary exists and terms are used consistently | N | catalog record; no check yet; planned: a native text search for glossary presence and term use (consistency in general needs judgement) |
| DOC-010 | Documentation links resolve | N | catalog record; no check yet; planned: lychee (external links need network) |

## Further reading

- Literate Programming, Donald Knuth
- A Rational Design Process: How and Why to Fake It, David Parnas and Paul Clements
- Documenting Architecture Decisions, Michael Nygard
- arc42 and the C4 model (Simon Brown)
- Diataxis, Daniele Procida
- Keep a Changelog
