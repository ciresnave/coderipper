# Learning from version history: churn, co-change and ownership (EVO)

Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

## Why it matters

Version-control history records how a system really behaves when people change it, which structure-only analysis
cannot see. A shared schema can bind two services although neither imports the other. A small-looking class may be
edited for five unrelated reasons. One person may be the only one who understands a critical module. History turns
static linting into a measure of architectural health and is the best way to choose which of thousands of
structural findings to fix first. Unusually for this catalog, there is research support: studies of change
coupling, churn and ownership (Zimmermann et al., Nagappan and Ball, Bird et al.) found they predict defects and
how changes propagate, and Tornhill's hotspot analyses have been validated in practice.

## How to apply

**Prepare the evidence**
- Leave out bulk reformatting commits, generated files, bot-authored commits and large renames, and cope with
  squash merges and rename detection. Use the full history; shallow clones give wrong answers.

**Find where to work**
- Rank hotspots by combining change frequency, complexity and recent defects, and refactor there first.
- Look for defect clustering: components with far more fix commits than their share.

**Find structure the code hides**
- Pairs of files that change together often but lack a declared dependency, or that straddle a declared boundary,
  expose links the import graph misses, such as shared schemas, wire protocols and duplicated logic.
- A module whose files change in separate groups of commits probably has several responsibilities.

**Watch the people and the trend**
- A critical component dominated by one person is a knowledge risk.
- Watch how violations, waivers and the age of waivers evolve for each component, and how change sizes and
  per-function complexity move over time.
- Rules about components untouched for a long time may be out of date, or the code may be deliberately frozen;
  find out which.

**Interpret with care**
- Results are statistical, not causal: use them to rank work and raise questions, never to assign blame or gate merges.

## Background and lineage

Lehman's evolution laws; change-coupling mining (Gall et al.; Zimmermann et al.); relative churn as a defect
predictor; ownership and quality; hotspots and temporal coupling; truck-factor estimation; the SZZ family of
bug-introducing-change analyses.

## Measures and numbers

The hotspot score is churn times complexity times recent defect density. Co-change mining uses support and
confidence thresholds, and this domain sets no default for them yet.

## Cautions

Over-application is the main danger: treating correlation as cause, using per-person metrics for performance
management, and feeding noisy histories (squashes, reformatting) into confident conclusions. Ownership findings
in particular are advisory.

## Where it is checked

No rule in this domain is run by the current release; every row below is a catalog record only.

| Rule | Title | Class | Checked by |
| :--- | :--- | :--- | :--- |
| EVO-001 | Hotspot ranking from churn, complexity and defects | H | catalog record; no check yet (needs history analysis); planned: CodeScene-style analysis |
| EVO-002 | Hidden coupling found through co-change | H | catalog record; no check yet (needs history analysis) |
| EVO-003 | Divergent change inside a module | H | catalog record; no check yet (needs history analysis) |
| EVO-004 | Sole-owner risk in critical components | H | catalog record; no check yet (needs history analysis; advisory only) |
| EVO-005 | Erosion trend of violations and waivers | T | catalog record; no check yet (reporting over the catalog's own findings) |
| EVO-006 | Stale rules and specs for untouched components | H | catalog record; no check yet (needs history analysis and a human decision) |
| EVO-007 | Defect clustering by component | H | catalog record; no check yet (needs history analysis; an issue-tracker link is optional) |
| EVO-008 | Change-size distribution | H | catalog record; no check yet (needs pull-request metadata from the forge) |
| EVO-009 | Per-function complexity trend | H | catalog record; no check yet (needs history analysis and per-commit metrics) |

## Further reading

- Your Code as a Crime Scene, Adam Tornhill
- The Influence of Organizational Structure on Software Quality: An Empirical Case Study, Nagappan, Murphy and Basili (ICSE 2008)
- Mining Version Histories to Guide Software Changes, Zimmermann et al.
- Don't Touch My Code! Examining the Effects of Ownership on Software Quality, Bird et al.
- Use of Relative Code Churn Measures to Predict System Defect Density, Nagappan and Ball
- Programs, Life Cycles and Laws of Software Evolution, Meir Lehman
