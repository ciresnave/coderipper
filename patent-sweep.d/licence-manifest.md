<!-- addendum -->
### {{VERSION}} ({{LINE}} line) - {{DATE}}

Five rules are added, read from `src/module/neutral/licence_manifest.rs`: `SUP-022` to `SUP-026` (checks over a tracked `LICENCE-MANIFEST.toml`). Swept 2026-11-21 by the coderipper lane (Claude Sonnet 5.5), same method and limits as above.

**Idea {{IDEA1}} (`SUP-022..026`).** Read a tracked manifest that declares the origin, copyright holder and licence of copied files; report a manifest that does not parse, a required notice or licence text that is not tracked, a file that says it is derived from something but is covered by no entry, an entry that matches no tracked file, and a declared licence that the origin licence forbids (a static table). Published prior art: the FSFE REUSE specification and `reuse lint`, and the licence-tools reference validator of this portfolio.

| # | Idea | Query ({{DATE}}) |
|---|---|---|
| Q{{Q1}} | {{IDEA1}} | patent detect missing copyright notice license attribution third-party source code files manifest compliance check repository (restricted to patents.google.com) |
| Q{{Q2}} | {{IDEA1}} | patent open source license compatibility determine incompatible license between declared component license and origin license (restricted to patents.google.com) |
| Q{{Q3}} | {{IDEA1}} | REUSE specification software licensing compliance tool lint missing copyright license information files (no domain restriction) |

**Positive controls:** Q{{Q3}} returned `reuse lint` documentation, so the engine reaches the published form of the idea; Q{{Q1}} and Q{{Q2}} returned patents in the neighbourhood (below), so the engine reaches it.

**Read (independent claim 1 in full, from the public Google Patents page):**
- **US9436463B2** (checking open source usage): claim 1 compares a characteristic (a hash or keyword sequence) of source code entities to a repository of an open source library, assigns a probability that an entity contains library code, and infers a second entity does too. We take no hash, keep no repository of library code and compute no probability; our entries are written by hand. Clear as read.
- **US8875301B2** (software licence incompatibility determination): claim 1 takes input naming the licences of the components and usage information (industry, internal use, third party, resale) and compares them with an overall licence, then displays a recommendation. `SUP-026` takes no usage information, compares an entry's declared licence with its origin licence by table, and recommends nothing. Clear as read, **with a re-read trigger: any use of how the product is to be used or resold in the compatibility judgement.**

**Seen by title and snippet only, no verdict:** US7797245B2 and US8010538B2 (scanning content for copyright notice strings and comparing them with known notices; the engine's own summary says they flag files lacking notices), US20150309790A1, US8307351B2, US9152414B2.

**Result: no hit reads on `SUP-022..026` as read.** **Watch:** read the claims of US7797245B2 and US8010538B2 before a check reports a file that carries no notice at all, with no manifest involved; `SUP-023` today starts from a manifest entry, never from a scan.

**Limits:** three queries, one search engine, no USPTO, Espacenet or WIPO full-text search, no CPC search, no citation chasing, and two claims read. A due-diligence read, not legal advice.
