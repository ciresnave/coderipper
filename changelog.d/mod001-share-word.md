kind: Fix
---
**MOD-001 no longer fails a run with an invalid finding when a large public surface has a single gap.** A share under 1% (1 of 142 undocumented items, found on rig-tools) was written `(0%)`, and the bare word `0` made `Finding::validate` take the summary for an absence claim without a positive control, so the finding was dropped with an error. It now reads `under 1%`. Found by the first sweep of a third-party workspace (docs/RIG-0E.md).
