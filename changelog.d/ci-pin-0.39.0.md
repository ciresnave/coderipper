kind: Fix
---
**The `ci.yml` caller of the reusable portfolio workflow is pinned to the commit that installs coderipper 0.39.0** (`portfolio.yml@556c7c587b4c...`, from the 0.20.1 commit `84cb133`), the second step of the two-step pin after the workflow itself moved to 0.39.0 (#253), as for 0.20.1 (#235, #236). No source change.
