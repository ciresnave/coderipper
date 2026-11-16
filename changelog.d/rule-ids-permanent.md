kind: Changed
---
**Rule ids and aliases are permanent (CireSnave 2026-11-16).** The old rule names (`reachability`, `version-consistency`, ...) and the merged-duplicate ids keep working for good: the "accepted for this release only" warning now says they are never removed, `tools/migrate-rule-ids.py` is optional, and CONTRIBUTING states the rule (never rename a rule, never reuse an id, never remove an alias). `tests/catalog.rs` pins all 23 aliases that ever shipped and fails when one stops resolving (checked by removing `TYP-003`) or when a new alias is not added to the pin.

— lane: coderipper
