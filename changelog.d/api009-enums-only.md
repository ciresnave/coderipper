kind: Fix
---
**API-009 reads enums only** (CireSnave 2026-11-07, ruling 3: "yes"). It also flagged every `pub struct` whose named fields are all `pub`, but `#[non_exhaustive]` on a data struct stops a downstream crate building it with a struct expression, so that is usually the design, not a defect; the cure the rule asked for broke the callers it meant to protect. A `pub enum` with variants and no `#[non_exhaustive]` and no doc saying the set is closed is still reported. The rule stays opt-in (`coderipper check API-009`).
