kind: Fix
---
**The reusable portfolio workflow installs coderipper 0.39.0** (`CODERIPPER_VERSION` in `.github/workflows/portfolio.yml`, from 0.20.1), the newest published release (crates.io, tagged `v0.39.0`). The 0.20.1 install printed the two MOD-002 "not registered" Info lines because it lagged 19 minor versions behind the rules the repository's own configuration names; 0.39.0 registers them. The caller pin in `ci.yml` (`portfolio.yml@<commit> # 0.20.1`) moves to the commit of this change in a second, fragment-only change once this one is on `main`, as for 0.20.1.
