kind: Additive: the portfolio workflow installs 0.48.12
---
**The portfolio workflow installs 0.48.12.** `CODERIPPER_VERSION` in `portfolio.yml` goes from 0.48.9 to 0.48.12 (published 2026-11-27; crates.io `max_version` read 0.48.12; tag `v0.48.12` is 219803b and carries the reviewed auth-framework waiver for the squash commit d9b3937c, `src/tokens/keyring.rs` line 67, #385). Auth-framework's main sweep needs it. Workflow only; no rule, option or exit code changes. `tests/portfolio_version.rs` is the guard. The own `ci.yml` pin moves to this change's merge commit in a follow-up.

— lane: coderipper
