kind: Fix
---
**RDB-008 commented-out code no longer reads layout notation as code.** A run of whole-line comments now counts as commented-out code only when at
least one line is strongly code-shaped (ends in `;`, `{` or `}`, or opens with a statement keyword). Lines that merely end in `(`, `,` or `)` are how
dispatch tables, layout algebra and shape comments read too, so a run made only of them is documentation. The check stays opt-in
(`[rdb008] commented_out_code = true`). Measured on NVIDIA/cutlass at v4.8.1 (7978 files at HEAD, opt-in on): 49 files before, 29 after; of 7 dropped files
sampled, 6 were notation or documentation and 1 (`python/cutlass_library/generator.py`, tuple-literal rows) was real commented-out code that is now missed.
