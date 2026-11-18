kind: Feature: SEC-002 reviewed waivers (CodeRipper-side, visible)
---
**SEC-002 can now waive a verified false positive from outside the repository.** A repository cannot silence its own secret scan, so a false positive in
history that no commit can fix (ThinkersJournal/secretant: gitleaks `generic-api-key` reading a cipher name in changelog prose as a key, in two CHANGELOG commits)
kept SEC-002 red. `waivers/secrets.toml` in this repository, shipped inside the binary, now lists the exact finding (repo, commit, file, rule, line), the
reviewer (`pm`), the date, the evidence and an expiry (at most 365 days). A waived finding is reported as **Info**, not dropped, and does not gate. An expired
entry returns the finding as **Medium** ("waiver expired"); an entry whose commit is in the scanned history but which names no finding any more is **Medium**
too ("names a finding that no longer exists"). Matching is exact: another line, commit, file, rule or repository is not waived, and a repository's own
`.gitleaksignore` still has no effect. Two entries added (secretant, the two commits). Rule ids and aliases untouched. **Cargo.lock refreshed** (lock-only, two commits; SUP-015 reported 22 then 3 locked-behind crates): `zip` 9.4.1 to 9.7.0 (a minor bump of a direct dependency; it brings `nix` 0.32.0 on Unix targets, `constant_time_eq` and `zeroize_derive`), `clap` 4.6.7 to 4.6.8, `flate2` 1.1.10 to 1.1.11, `ureq` 3.5.1 to 3.5.2, `syn` 3.0.9 to 3.0.10 (plus `syn` 2.0.119 as a new transitive dependency), the `regex` trio and 15 more patch releases; nothing held back by a pin; upstream release notes were not read, the full suite and `cargo deny check` pass on the result. **Also corrects the 0.47.12 documentation** (RDB-002 inner allows and `unsafe(no_mangle)`, REL-002 computed delays, the unparseable-only-file note of five Rust rules): see the "Corrections 2026-11-17" paragraph at the top of the 0.47.12 entry; the behaviour fixes are in 0.47.14 (#363). Measured, not shipped: an anchored
algorithm-name allowlist on `generic-api-key` (3/0, 5/5, 15/15 findings on three repositories) would make those two entries stale; see
`docs/secret-waivers-design.md`. Evidence: secretant's full history scanned with the old and the new binary, three Critical before, two Info plus the unrelated
lane-branch hit after.
