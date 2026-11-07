kind: Fix
---
**SEC-002 findings are Critical, as the rule's record says.** The gitleaks mapping reported `High` while `rules/sec.toml` has `default_severity = "critical"` (CireSnave 2026-11-07: "Raise the code to Critical"). The same scan under OPS-006 stays `High`, which is OPS-006's own record severity.
