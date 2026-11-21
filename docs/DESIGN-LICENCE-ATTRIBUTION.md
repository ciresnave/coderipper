# Licence and attribution check: design (phase 1)

> **Status, 2026-11-19: sections 1-5 are the survey and design; section 6 records what is built (branch `licence-attribution`, not yet merged).** Task: PM `tolkn1-9j4-nz` (CireSnave, 2026-11-19), with pm-platform's notice that
> CodeRipper and licence-tools share ONE schema. This file grows into the design note; its first section is the survey the PM asked for before any code.

## 1. Survey: what exists, read versus only seen

Read means the page was fetched and read by a lane subagent on 2026-11-19; "SAW" means a search or index snippet, or a page not fetched.

### REUSE (FSFE) specification 3.3 and reuse-tool 6.3.0: READ

| need | REUSE gives | gap |
|---|---|---|
| per-file licence and copyright | comment headers (`SPDX-License-Identifier`, `SPDX-FileCopyrightText`), `<file>.license` sidecars, `REUSE.toml` `[[annotations]]` (`path` with `*`/`**` globs, `precedence` = closest/aggregate/override); DEP5 deprecated, mutually exclusive with REUSE.toml | none |
| line ranges | **No line ranges.** In-file snippets only, delimited by comment tags `SPDX-SnippetBegin`/`SPDX-SnippetEnd` with `SPDX-SnippetCopyrightText`; commentable files only; `REUSE.toml` `path` is file-level | a range cannot be declared outside the file |
| origin, "derived from", upstream URL | **None.** Other keys are allowed in REUSE.toml ("their semantics are not defined"); `SPDX-FileComment`/`SPDX-FileContributor` are suggested examples | origin is ours to define |
| required companion files (NOTICE, licence text) | **None** except: licence texts must be `LICENSES/<SPDX id>.txt`; `LICENSES/` holds nothing else; unused ones are errors | no "this file requires that file" |
| checks | `reuse lint`: files without copyright or licence info, missing/unused/bad/deprecated licence texts, invalid expressions | **no compatibility check** |

### SPDX 2.3.1 (file, snippet, relationships, other licensing): READ (the `/v2.3/` URLs are redirect stubs to 2.3.1)

* File: `LicenseInfoInFile`, `LicenseConcluded`, `FileCopyrightText`, `FileNotice`, `FileContributor`, `FileAttributionText`, `FileComment`.
* Snippet: `SnippetByteRange` (required) and `SnippetLineRange` (optional, `a:b`, starts at 1; if the two disagree the byte range wins); snippet licence and copyright fields exist. **This is the standard's line-range construct.**
* Relationships: `COPY_OF` (exact copy), `DESCENDANT_OF`, `FILE_MODIFIED`, `VARIANT_OF`, `PATCH_FOR`, `DISTRIBUTION_ARTIFACT` ("distributing A requires that B also be distributed": the nearest thing to "must ship with"). **No "ported from"**: use `DESCENDANT_OF`/`FILE_MODIFIED`, or `OTHER` with a comment.
* No field for a required companion notice file. Serialisations shown on the pages read: tag-value and RDF/XML (JSON and YAML: SAW only).

### Scanners and tools

| tool | reads | finds | line ranges | companion-file check | compatibility |
|---|---|---|---|---|---|
| ScanCode (READ: docs home, licence-detection page) | source files, archives, manifests | licence text and copyright statements, with start/end line per detection; **no "derived from/ported/vendored/Origin:" wording** | **yes, per detection** | no (it reports what it finds) | no |
| ORT (READ: analyzer, scanner, evaluator, evaluator-rules, curations, package-configurations, ort.yml) | manifests plus scanner results (ScanCode and others) | what its scanner finds; `is_modified` is a manual curation flag | **yes**: `license_finding_curations` take `path`, `start_lines`, `line_count`, `reason` (a precedent for line-range exceptions); path excludes are file/glob only | no built-in; evaluator rules are custom Kotlin (`projectSourceHasFile` exists) | no built-in matrix; classifications plus custom rules |
| cargo-about (READ: README, book config, clarify) | Cargo graph, licence files in crate sources | licence text only | no (`clarify` substring start/end, not lines) | partly: `clarify` pins a file by sha-256 | an `accepted` SPDX list, not compatibility |
| license-finder (READ: README) | dependency manifests | package-reported licence names | no | no | `permitted`/`restricted` lists |

SAW only, not read: SPDX 3.0, ClearlyDefined, ScanCode "add a rule" and CLI pages, ORT licence-classifications, the REUSE FAQ and JSON schema, `reuse.software/specifications/` (the page `reuse.software/spec/` loaded as a bare title, so that 3.3 is the newest REUSE version is **not** confirmed). The licence of ScanCode itself was not on the pages read.

## 2. What the survey says (for the design)

1. **No tool detects undeclared origin wording** ("Origin:", "derived", "ported", "vendored"), and none checks that a source's required notice files are present. Those two checks (the task's phase 1) are ours, whatever the manifest format.
2. **REUSE fits the per-file part, and extends cleanly:** a valid `REUSE.toml` may carry extra keys, so a manifest that is a *superset* of REUSE.toml keeps `reuse lint` working unchanged. It lacks origin, required companion files, and line ranges.
3. **Line ranges have two precedents** (SPDX `SnippetLineRange` `a:b` and ORT's `start_lines` + `line_count`); REUSE has none outside the file. A separate table (for example `[[snippet]]`) would be ignored by `reuse lint`, whereas an `[[annotations]]` entry with a range key would be read by it as a whole-file entry: a hazard to design around.
4. **Compatibility is nobody's built-in feature**; a table of our own (phase 2) is unavoidable.

## 3. Schema: comparison with licence-tools' draft 1 (2026-11-19, `lane/manifest-schema` @299b237)

licence-tools (read in full: `docs/MANIFEST-SCHEMA.md`) chose what the survey pointed to: `LICENCE-MANIFEST.toml`, a REUSE.toml superset,
`[[annotations]]` per file or glob (REUSE keys `path`, `precedence`, `SPDX-FileCopyrightText`, `SPDX-License-Identifier`; ours `origin-url`, `origin-ref`,
`origin-licence`, `relation` = an SPDX relationship name, `requires` = tracked files that must ship), and a **separate `[[snippet]]` table** with `lines = "a:b"`
for ranges. **CodeRipper adopts this as the one schema.** Where it agrees, nothing more to say. Divergences and gaps, each for the two lanes to settle:

1. **Obligation by licence, not only by `requires`.** The positive control (a missing RIKEN MIT notice) is missed if the entry lists no `requires`:
   the author who forgot the notice also forgot to list it. The check should derive the obligation from `origin-licence`: for any licence that
   requires attribution (MIT, BSD-*, ISC, Apache-2.0, ...; not CC0-1.0, Unlicense, 0BSD) the entry must carry **`origin-copyright`** (the holder line at
   the source) and the source's copyright notice must appear in a tracked file (the entry's `requires`, a NOTICE, or this file's own header), not only the generic
   `LICENSES/MIT.txt` text, which names no holder. Proposed new key: `origin-copyright`.
2. **Line range inclusivity.** The draft says "1-based, inclusive". SPDX 2.3.1 says 1-based but does not state inclusive for lines (and the byte range wins on conflict).
   Define it in the schema, since neither standard does.
3. **Entry coverage and precedence** must be stated for LM-003 ("no entry covers it"): follow REUSE (`closest` default, last matching table wins, `aggregate`/`override`), and say whether a `[[snippet]]` also counts as covering its file for the marker check.
4. **Check ids.** `LM-001..004` are the licence-tools validator's. The CodeRipper rules take permanent ids in CodeRipper's own families (candidates: `SUP-020..`, next to SUP-014..019 of the 2026-10-24 provenance plan); a crosswalk LM-00x to SUP-0xx belongs in the design.
5. **Strict unknown keys** (a typo is an error) is right for the validator; CodeRipper must apply it too, or the two tools will disagree about the same file.
6. **`relation`** is an SPDX name but the survey found no "ported from"; allow `DESCENDANT_OF`, `FILE_MODIFIED`, `COPY_OF`, `VARIANT_OF`, and `OTHER` plus `relation-comment`.

Not yet done: the Baracuda run (67 files, ATTRIBUTION.md and NOTICE files), the compatibility table (phase 2), the limit statement in final form, the `[ASK]`s for CireSnave.

## 4. Settled with licence-tools (2026-11-19, `lane/manifest-schema` @13f5c5c)

All six gaps of section 3 were adopted. Two additions to mirror in the check: (a) a credit is not a notice (the holder must sit on a line with
`copyright`, `(c)` or the copyright sign, so prose such as "RIKEN-RCCS" in an ATTRIBUTION.md does not satisfy the RIKEN control); (b) a `[[snippet]]` does not
cover its file for the undeclared-origin check, only marker lines inside its range. Notice search: `requires`, then `NOTICE*`/`ATTRIBUTION*`/`COPYING*`/`LICENSE*`/`THIRD*`
in the covered directories and their ancestors (not `LICENSES/`), then the covered files' first 60 lines.

**Rule ids (assigned here, permanent):** `SUP-018`, `SUP-019`, `SUP-020` stay reserved for the 2026-10-24 provenance plan and `SUP-021` is taken (shipped), so the crosswalk is
LM-001 = **SUP-022** (manifest does not parse or breaks the schema), LM-002 = **SUP-023** (missing notice or licence text), LM-003 = **SUP-024** (undeclared origin marker),
LM-004 = **SUP-025** (stale entry). LM-005 = **SUP-026** (incompatible licence: the entry's SPDX-License-Identifier contains a licence its origin-licence forbids; unknown ids are info only; allocated 2026-11-19 at licence-tools' ask, table in licence-tools `tools/licence_compat.py` @lane/compat-table 7f62d41). SUP-026 had 0 references on origin/main and this branch (control: SUP-021 has 12). licence-tools' proposed SUP-020..023 would have collided with SUP-021.

— lane: coderipper

## 5. Baracuda control run (2026-11-19, Baracuda `origin/main` @1440128, schema `lane/manifest-schema` @9011b5d)

Read by hand (git ls-tree / git show / git grep on the ref, 2506 tracked files); the check itself is not written yet, so this is the predicted result, not a tool run.

- **Positive control (LM-002, RIKEN MIT notice): CONFIRMED missing.** `crates/baracuda-ozimmu-sys` ports `RIKEN-RCCS/accelerator_for_ozIMMU` (MIT; ATTRIBUTION.md lines 32-33, `split.hpp:38`, `split.cu:330`, `gemm.cu:255`). Tracked lines naming RIKEN: 11, none carries `copyright`, `(c)` or the copyright sign (0 of them across the whole tree); its `LICENSE-MIT` line 3 is `Copyright (c) 2026 Eric Evans`. So rule 2 (holder on a copyright line) is unmet, and prose "RIKEN-RCCS" in ATTRIBUTION.md correctly does not satisfy it. Same query on a present thing: `LICENSE-MIT` does match `copyright` (lines 3, 12, 18).
- **LM-003 in this crate under rule 7:** code markers (findings): `ozimmu.hpp:12` ("vendored"), `split.hpp:38` ("ported from"), `src/lib.rs:30` ("vendored"); ATTRIBUTION.md is prose (info). No `LICENCE-MANIFEST.toml` or `REUSE.toml` exists in Baracuda, so all three are undeclared until an entry covers `crates/baracuda-ozimmu-sys/**`.
- Not yet done: the full 67-file set, phase-2 compat table, `[ASK]`s for CireSnave, docs PR on a later day.

Rule 7 (LM-003 scope, pm-platform ruling) is adopted for SUP-024: licence-text files are never scanned; markers in `.md/.markdown/.rst/.txt/.tsv` are info and never change the exit code; other files are findings.

— lane: coderipper

## 6. Built (SUP-022..SUP-026), 2026-11-19

Implementation: `src/module/neutral/licence_manifest.rs` (the five rules, one analysis), `licence_compat.rs` (the table, ported from licence-tools `tools/licence_compat.py`, first @7f62d41, since 2026-11-21 from `lane/gate-migration`), fixtures `conformance/SUP-02{2..6}/{defective,clean}`. Default on in `extended` and `portfolio`; not applicable when `LICENCE-MANIFEST.toml` is not tracked. Findings are Low, except **SUP-026, which is Medium from 2026-11-21** (see the end of this section); the info lines of the schema (prose markers of rule 7, unchecked licences of LM-005) are run notes, which never change the verdict.

**Command:** `coderipper check SUP-023 --project <repo-root>` (any of SUP-022..SUP-026; it reads `HEAD` of that git repository, so commit the manifest first).

**Reference validator:** the licence-tools validator (`lane/manifest-schema` @27dc254, `lane/compat-table` @0dfd055) agrees on all ten conformance fixtures and on the Baracuda export (same 110 files for LM-003, same single LM-002). Known differences: a wrong-typed value (`path = 3`) is an LM-001 finding here and a Python exception there; a REUSE list for `SPDX-FileCopyrightText` is accepted by both; the findings carry the line of the entry header or the marker; one LM-002 finding per missing `requires` file (the validator's code, not the schema's wording "one per entry").

**SUP-026 after CireSnave's rulings (2026-11-21, relayed by pm-platform and licence-tools; mirrored at `6220b00`):** *"Any incompatibility blocks. Anything else is informational."* A conflict is therefore **Medium** (the other four stay Low; an id the table lacks stays an info note). Table changes mirrored from licence-tools `tools/licence_compat.py` on `lane/gate-migration`: Apache-2.0 into GPL-2.0-or-later is denied (as GPL-2.0-only and LGPL-2.1-only already were); MPL-2.0 into GPL-3.0 and LGPL-3.0 is allowed; LGPL-2.1-only into GPL-3.0 and AGPL-3.0 is allowed; a covered file or a `requires` file that carries the MPL-2.0 Exhibit B notice ("Incompatible With Secondary Licenses", case and whitespace ignored) limits that code to MPL-2.0, and the finding names the marked file; BlueOak-1.0.0 moves to the no-holder-notice list. Evidence for each pair: licence-tools `docs/LICENCE-COMPAT-EVIDENCE.md`. The table is data from a reading of licence texts, not legal advice.

**What the check proves, and what it does not (the limit statement).** It proves only what a manifest DECLARES: that each declared origin has its notice and licence text tracked, that no marker of origin sits in a file the manifest does not cover, that no entry is stale, and that the declared outbound licence is one the declared origin licence allows. It cannot see copied code that carries neither an origin marker nor an entry, and it does not judge whether a licence id is the true licence of the origin. A similarity helper (comparing files with known third-party sources) could do more and is a separate, later tool.

**Open for CireSnave (sent to pm-platform as one [ASK], 2026-11-21):** whether the check stays advisory-default (default on in `extended` and `portfolio`, never gating the build by its own severity) or becomes gating; recommendation: advisory-default until the Baracuda manifest exists, then revisit.

— lane: coderipper
