kind: Fix
---
**The one rule-count pin #262 left in `tests/neutral_module.rs` (`proofs.len() == 28`) reads the registry**, with a check that no rule is registered twice. It passed only while the neutral module had exactly 28 rules, so the first rule PR after #262 failed `every_registered_claim_is_earned_by_its_fixtures`. No source change.
