kind: Fix
---
**The portfolio workflow installs coderipper 0.43.77.** It was pinned at 0.43.49, so a caller could not use the changelog.d fragment rule of `version-and-changelog` (#313), a root `PATENT-SWEEP.md` (#327) or `[architecture] none = [..]` (#322) however it repinned the workflow. Callers pick the new binary up by moving their workflow SHA to this change.
