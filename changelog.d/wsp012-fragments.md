kind: Fix
---
**WSP-012 reads one or more added changelog fragments as the version bump.** It read exactly one, and reported "still has version" for a pull request with two or more, against `changelog.d/README.md` (several fragments are stamped together under one version) and the one-PR-per-group rule. No fragment and no bump still fires; a fragment the base already had does not count. Found by coderipper-c on #370 and decided by this lane: the rule was wrong, not the README.
