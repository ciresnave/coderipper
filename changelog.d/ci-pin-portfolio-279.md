kind: Fix
---
**CI: the caller pins the portfolio workflow at the commit that exempts "declares no `[architecture]` table" notes (#279).** The pin was the 0.39.0 commit, so main's Portfolio job stayed red on the ARC-001..007 notes after #279 fixed the workflow. Only `ci.yml` changes; no code.
