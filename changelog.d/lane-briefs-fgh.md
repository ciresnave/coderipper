kind: Docs
---
**Lane briefs for packages F (concurrency), G (panics, swallowed errors, unsafe) and H (repository files), and TYP-003 is now an alias of MOD-010.** The two records were one rule (a declared invariant type has private fields and a validating constructor); the catalog keeps one record, `MOD-010`, which answers to `TYP-003` (`aliases`), so allowlist entries and `coderipper check TYP-003` keep working. The catalog has 231 rules; `docs/rules/REMAINING.md` has 183 rows.
