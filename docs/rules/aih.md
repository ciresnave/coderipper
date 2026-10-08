# Keeping AI-assisted changes trustworthy

Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

## Why it matters

Coding assistants and agents shift which mistakes appear in code. Common ones are code that looks
right and is not, invented APIs and package names, fresh copies where reuse was available,
abstractions nobody requested, departures from the local style, tests that simply assert whatever
the generated code returns, and changes too large to review properly. The countermeasures are
inexpensive and mostly already covered by rules elsewhere in the catalog. There is also a
consequence for this project itself: the guidance it publishes will be read by coding agents, so
rules should be written in a form a machine can act on.

## How to apply

Close the loop for the agent:

- Run the same rules, at the same pinned version, in the agent's own loop, so a failure reaches
  the author while the code is being written rather than in CI later.
- Supply intent: an architecture description, the rule catalog, decision records, a glossary and
  a few model examples for each pattern. A good habit: study three similar existing
  implementations, then write the new one.

Check what the agent claims exists:

- Each API a change refers to must compile. Each new package must be real, be what it says it
  is, and pass the supply-chain vetting rules.

Check the tests:

- Write tests from the specification instead of from the code the agent produced, and measure
  how well they catch faults with a mutation score.

Control the change and its review:

- Keep diffs small and on task, and flag unrelated refactors.
- Scale human review to risk. Changes touching security, authorization or data migration always
  get a person's review.

Keep the paper trail:

- Record provenance with a trailer on AI-assisted commits, to support analysis and not to assign
  blame.
- Set a policy on copyright and licensing of generated code, agreed with the organization's
  counsel.

## Background and lineage

Research here is recent. Early studies found security-prone suggestions (Pearce and colleagues,
2021 and 2022), and industry analyses have reported growing duplication and rework. These remain
hypotheses for now.

## Measures and numbers

The one metric this domain relies on is the mutation score of the changed code, used to judge
whether generated tests would notice a fault. No fixed threshold is set here.

## Cautions

- Checks that compare a change to the task description, or judge whether code matches local
  style or duplicates something existing, need a model's judgment and give uncertain results.
- Diff statistics can show size and mixed reformatting deterministically; whether an edit is
  related to the task cannot be settled that way.
- Registry lookups for AIH-002 need network access, and the build step runs project code.

## Where it is checked

| Rule | Title | Class | Checked by |
| :--- | :--- | :--- | :--- |
| AIH-001 | One policy for humans, agents and CI | N | catalog record; no check yet (needs a shared policy bundle to compare against) |
| AIH-002 | Referenced APIs and new packages really exist | P | catalog record; no check yet (planned: build plus registry lookup) |
| AIH-003 | New helpers do not repeat existing utilities | L | catalog record; no check yet (needs a model to judge duplication) |
| AIH-004 | A change stays within the task it was given | N | catalog record; no check yet (diff statistics are mechanical, matching the task needs a model) |
| AIH-005 | Tests check the specification, not the code's own output | X | catalog record; no check yet (needs mutation testing, which runs project code) |
| AIH-006 | Generated code follows the local conventions | L | catalog record; no check yet (needs a model and mined conventions) |
| AIH-007 | AI-assisted commits are marked in their trailers | N | catalog record; no check yet (planned: a commit-message check) |
| AIH-008 | No abstractions or options nobody asked for | L | catalog record; no check yet (needs a model to judge speculation) |

## Further reading

- "Asleep at the Keyboard? Assessing the Security of GitHub Copilot's Code Contributions",
  Hammond Pearce and co-authors (2021).
- Mutation testing literature and tools, for judging test strength.
