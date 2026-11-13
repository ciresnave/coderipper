kind: Fix
---
**`patent-sweep-record` reads a root `PATENT-SWEEP.md` when `docs/PATENT-SWEEP.md` is not tracked** (redactant#16): a repository that keeps its documents at the top is judged on that file (same version-line, heading and unread-hit checks; the finding names the file read). `docs/` wins when both exist; neither is still the finding that asks for `docs/PATENT-SWEEP.md`.
