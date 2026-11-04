kind: Fix
---
**lint-suppression says that a comment beside the attribute is not a reason.** The rule is unchanged (only `reason = ".."` and `#[ignore = ".."]` count, as documented), but a reader who had written `#[allow(dead_code)] // why` was told only "has no reason". The finding's detail now names the attribute form and says a comment is not one. Found on the first third-party sweep (docs/RIG-0E.md), where the reviewer first took three such findings for false positives; the PM ruled them correct.
