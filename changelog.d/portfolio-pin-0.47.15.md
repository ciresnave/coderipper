kind: Fix: the portfolio workflow installs CodeRipper 0.47.15
---
**The reusable portfolio workflow now pins `CODERIPPER_VERSION` to 0.47.15 (was 0.47.13).** 0.47.15 is published (tag `v0.47.15`, crates.io), and it carries the reviewed SEC-002 waivers for commy-sdk-rust history that 0.47.13 does not have, so a repository calling this workflow at a commit with the old pin installed a binary that could not apply them. No code change; `tests/portfolio_version.rs` is the guard (the pin must equal the newest published version).
