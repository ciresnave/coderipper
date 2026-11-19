# Licence and attribution check: design (phase 1)

> **Status: SURVEY ONLY, 2026-11-19. Nothing here is built.** Task: PM `tolkn1-9j4-nz` (CireSnave, 2026-11-19), with pm-platform's notice that
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

— lane: coderipper
