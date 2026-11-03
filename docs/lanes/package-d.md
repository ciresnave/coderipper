# Package D: MOD-006 divergent change, ARC-009 architectural drift (ARC-011 if feasible)

Read [README](README.md) first. **Read the patent sweep first**: these use change history, the most patent-dense area; stop at a live-claim hit and send `[HOLD]`.

**MOD-006**: a module changed in commits that touch otherwise unrelated sets of files (clusters of co-changed files that never co-change with each other).
Input: `git log --name-only` over a bounded window (Repository unit, committed history, no network). Report modules with two or more disjoint change
clusters, Info, low confidence. Window and thresholds are `[mod006]` settings; measure false positives on the corpus (squash-merge history makes commits coarse).

**ARC-009**: compare architecture metrics now (cycles, instability margins, violations, public-surface size) with the same at a base ref (the previous release
tag `v<version>`, see `tools/tag-release.sh`); report growth beyond a budget. Needs a checkout of the base ref: reuse the throwaway-checkout machinery
the reachability check uses. Repository unit.

**ARC-011** (only if the first two are merged and time allows): signs of a distributed monolith, i.e. components that always change together (MOD-006's clusters) and call each other in both directions.

**Skip ARC-010 and MOD-007**: they need a language model and are not in this package.

Done when: MOD-006 and ARC-009 registered, fixtures, corpus numbers, patent rows, fragments, READY.
