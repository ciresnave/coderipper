kind: Additive: the portfolio workflow installs 0.48.6
---
**The portfolio workflow installs 0.48.6.** `CODERIPPER_VERSION` in `portfolio.yml` goes from 0.48.1 to 0.48.6 (published 2026-11-27; crates.io `max_version` read 0.48.6). 0.48.6 carries the reviewed SEC-002 waivers for the keyring.rs history that auth-framework#512 waits on, which 0.48.1 cannot apply. Workflow only; no rule, option or exit code changes. `tests/portfolio_version.rs` is the guard. The own `ci.yml` pin moves to this change's merge commit in a follow-up (a SHA exists only after the merge).

— lane: coderipper
