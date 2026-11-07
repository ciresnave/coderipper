kind: Fix
---
**The lint-suppression ratchet counts only suppressions without a reason** (CireSnave 2026-11-07, ruling 2: "only unexplained"). It counted every `allow` and `ignore` attribute "reasons or not", so one well-reasoned allow failed a branch like ten bare ones. Now a branch that adds an `allow` or `ignore` with no reason is the Medium finding it was; one that adds a suppression with a `reason = ".."` (or `#[ignore = ".."]`) is an **Info** finding (`allow-explained-count`), which does not gate. The per-attribute "has no reason" finding is unchanged.
