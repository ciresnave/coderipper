kind: fix
---
ARC-002 no longer reads `pub(crate)`, `pub(super)` or `pub(in path)` as a reference to the module the path names: a visibility restriction is not a dependency, so it no longer closes a false cycle (reported from auditant, `pub(crate) fn check_hash`).
