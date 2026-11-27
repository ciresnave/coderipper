kind: Additive: the portfolio workflow installs 0.48.9
---
**The portfolio workflow installs 0.48.9.** `CODERIPPER_VERSION` in `portfolio.yml` goes from 0.48.6 to 0.48.9 (published 2026-11-27; crates.io `max_version` read 0.48.9). 0.48.9 carries the one reviewed SEC-002 waiver for auth-framework (`private-key`, `src/tokens/keyring.rs` line 67, commit 55137e13; #382, f806c77), which 0.48.6 keyed to the wrong commit; auth-framework#512 waits on it. Workflow only; no rule, option or exit code changes. `tests/portfolio_version.rs` is the guard. The own `ci.yml` pin moves to this change's merge commit in a follow-up.

— lane: coderipper
