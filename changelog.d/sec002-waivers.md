kind: Feature: SEC-002 reviewed waivers (CodeRipper-side, visible)
---
**SEC-002 can now waive a verified false positive from outside the repository.** A repository cannot silence its own secret scan, so a false positive in
history that no commit can fix (ThinkersJournal/secretant: gitleaks `generic-api-key` reading a cipher name in changelog prose as a key, in two CHANGELOG commits)
kept SEC-002 red. `waivers/secrets.toml` in this repository, shipped inside the binary, now lists the exact finding (repo, commit, file, rule, line), the
reviewer (`pm`), the date, the evidence and an expiry (at most 365 days). A waived finding is reported as **Info**, not dropped, and does not gate. An expired
entry returns the finding as **Medium** ("waiver expired"); an entry whose commit is in the scanned history but which names no finding any more is **Medium**
too ("names a finding that no longer exists"). Matching is exact: another line, commit, file, rule or repository is not waived, and a repository's own
`.gitleaksignore` still has no effect. Two entries added (secretant, the two commits). Rule ids and aliases untouched. **Also corrects the 0.47.12 documentation** (RDB-002 inner allows and `unsafe(no_mangle)`, REL-002 computed delays, the unparseable-only-file note of five Rust rules): see the "Corrections 2026-11-17" paragraph at the top of the 0.47.12 entry; the behaviour fixes are in 0.47.14 (#363). Measured, not shipped: an anchored
algorithm-name allowlist on `generic-api-key` (3/0, 5/5, 15/15 findings on three repositories) would make those two entries stale; see
`docs/secret-waivers-design.md`. Evidence: secretant's full history scanned with the old and the new binary, three Critical before, two Info plus the unrelated
lane-branch hit after.
