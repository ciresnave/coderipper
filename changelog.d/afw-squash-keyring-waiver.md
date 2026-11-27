kind: Fixed (additive): one reviewed SEC-002 waiver for auth-framework's squash commit d9b3937c
---
**One `[[waiver]]` entry: `private-key`, `src/tokens/keyring.rs` line 67, auth-framework commit d9b3937c7c454a3e92935924327977472a73c322 (the squash merge of #512).** The 55137e13 entry covers the branch history; once #512 was squash-merged the same line is reported at the squash commit, which that entry cannot match. `pkcs8_pem` is a PEM encoder, so the label is armour text and no key material is committed. A local gitleaks 8.30.1 scan of d9b3937c (89 findings in the history) reports exactly one finding in that commit: this one; none at line 72. Same reviewer (pm), evidence route and one-year expiry as the 55137e13 entry, which stays (a sweep of a branch still names it). The embedded-list test and the on/off control cover it; line 72 at the squash commit stays open. Waivers are compiled in: it reaches a sweep after the next publish.

— lane: coderipper
