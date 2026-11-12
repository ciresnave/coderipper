<!-- addendum -->
## Addendum: the {{LINE}} line ({{DATE}}, {{VERSION}})

Version line covered by this addendum: **{{LINE}}** ({{VERSION}}). One rule is added, read from `src/module/neutral/architecture_ratchet.rs`: `architecture-ratchet` (once a repository's architecture declaration says `ratchet = true`, the number of waivers in it may only fall). Swept {{DATE}} by the coderipper-c lane (Claude Sonnet 5.5), same method and limits as above.

**Idea {{IDEA1}} (`architecture-ratchet`).** Read the repository's configuration file at `HEAD` and at the merge-base of `HEAD` with the default branch, count the waiver entries of its architecture declaration in each, and report when the opted-in count at `HEAD` is higher. Comparing two counts of a declared list across two commits is the whole mechanism; the waivers' contents are not compared and no code is analysed.

| # | Idea | Query ({{DATE}}) |
|---|---|---|
| Q{{Q1}} | {{IDEA1}} | patent code quality gate count of suppressed warnings or waivers must not increase compared to baseline commit merge base ratchet static analysis exceptions |
| Q{{Q2}} | {{IDEA1}} | patent.google.com software architecture conformance checking declared allowed violations exceptions list limited number reduced over time compare configuration file between commits fail build if exceptions increase |

**Positive controls:** Q{{Q1}} returned SonarQube ratcheting-gate threads, TIOBE's managed violation suppressions and a baseline-ratchet recipe (tools and forum threads, not patents), so the engine reaches the neighbourhood. Q{{Q2}} returned US10031748 (tools for the structure of a large software system), US8677316B2 (Enforcement of architectural design during software development) and Structure101 and Lattix documentation, so it reaches the neighbourhood too. **A patent that fails a check because the number of declared architecture exceptions rose between two commits was not found; that absence is a statement about these two queries only** (one engine, no USPTO or Espacenet search). Baseline-and-ratchet gates are prior art for the practice itself, not patents.

**Read:** the claims of **US8677316B2** (Enforcement of architectural design during software development, Microsoft, filed 2010-05-12; Google Patents shows it Expired - Fee Related) through `tools/patent-claims.py`, all three independent claims (1, 8, 14). Each requires receiving rules extracted from a design intent that define the permissibility of communications between components, merging them with a codebase that has a static and a dynamic structure, applying them by inspecting the static structure and simulating the dynamic structure, and indicating whether the codebase complies. **Not read in full:** US10031748 and every other result named above, by title and snippet only.

**Result: no hit reads on `architecture-ratchet`.** The rule extracts no rules from a design intent, inspects no static structure, simulates no dynamic structure and judges no code; it counts waiver entries in two versions of one file. **Watch:** re-read US8677316B2 and US10031748 before the ratchet compares the declared rules against the code, or follows which communication a waiver permits.

**Limits:** two queries, one search engine, no USPTO, Espacenet or WIPO full-text search, no citation chasing, snippets not claims (except the one patent read), and the status shown is Google Patents', not the official record. A due-diligence read, not legal advice.
