kind: Fix
---
**One reviewed SEC-002 waiver for vulkane (additive).** `waivers/secrets.toml` waives gitleaks finding `vault-service-token` at `ThinkersJournal/vulkane` commit 9ac77dcb, `vulkane/src/safe/physical.rs:680`: a one-letter closure variable reading the Vulkan field `storageBuffer16BitAccess`, not a credential. CireSnave board 510, verbatim "510: A". Reviewer pm, expires 2027-11-26; renew by 2027-11-01 with new `reviewed` and `evidence`. Reported as Info (waived), not hidden.

— lane: coderipper
