kind: Fix
---
**`version-and-changelog` understands the changelog.d fragment scheme.** A branch that adds exactly one `changelog.d/<topic>.md` fragment (not the README, not one the base already had) and leaves the version alone is no longer reported as "still has version X, the version of the merge-base": the gate stamps the number at merge, and the run says so in a note. No fragment and no bump still fires, and so do two or more fragments.
