kind: Fixed (additive): the auth-framework keyring.rs waiver is keyed to the commit gitleaks reports
---
**The two reviewed SEC-002 `private-key` waivers for `src/tokens/keyring.rs` (lines 67 and 72, commit ed51577a) are replaced by one: line 67, commit 55137e13b0e919945149a3ff53832735cf7724c1.** The 0.48.6 sweep on auth-framework#512 reported the old pair unmatched (each is itself a Medium) and the finding unwaived. A local gitleaks 8.30.1 scan of the #512 head (878d10dd, `origin/work/day-2026-11-26`; 429 commits, 89 findings in all) reports exactly one finding in that file: `private-key`, line 67, commit 55137e13; none at line 72. Same reviewer, evidence and expiry; replaced, not added. The embedded-list test and the on/off control use the new key, and line 72 and the old commit stay open as controls. Waivers are compiled in, so the fix reaches a sweep only after the next publish.

— lane: coderipper
