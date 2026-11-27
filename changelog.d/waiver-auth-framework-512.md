kind: Fixed (additive): two reviewed SEC-002 waivers for auth-framework#512
---
Two `[[waiver]]` entries (`private-key`, `src/tokens/keyring.rs` lines 67 and 72, commit ed51577a): `pkcs8_pem` is a PEM encoder, so the label is armour text and no key material is committed. Reviewer pm, one-year expiry, CireSnave board 510 A route. A squash-commit pair follows once #512 merges.

— lane: coderipper
