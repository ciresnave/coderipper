kind: Fix
---
**ERR-001 text corrected (claims audit):** the rule and its catalog entry said the `if let Ok(..)` form covers "read, write and flush methods". The list is `read_to_string`, `read_to_end`, `write_all` and `flush`; a bare `read` or `write` (a lock has them too) is not read. The behaviour is unchanged and now pinned by a test. Opt-in rule, so no run changes.
