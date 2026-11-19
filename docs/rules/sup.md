# Trusting what you import: the dependency rules

Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

## Why it matters

Most of what ships in a product was written by someone else, and each imported package brings imports of its own. Taking on a
dependency therefore means deciding how far to trust it, who keeps it maintained, what licence applies and how much attack surface it
adds. Incidents have made this concrete: the left-pad removal (2016), the event-stream takeover (2018), Log4Shell (2021), the xz
backdoor (2024) and dependency-confusion campaigns. The evidence behind these rules is strong. The practical conclusion is to pin what
you build with, take inventory of it, scan it continuously, allow it only from approved places, and add to it only with a reason.

## How to apply

### Know exactly what you build with

- Applications commit their lockfile, and CI installs in the package manager's strict mode (locked, frozen, clean install) so a stale
  or edited lockfile fails the build instead of quietly resolving something new.
- Reproducible builds let anyone rebuild an artifact and compare it with the one published.
- Keep vendored third-party code out of what your own analysis covers; the team's code is judged separately.

### Keep the list short and justified

- For each new dependency, write down why it is needed, what the alternatives were, how well it is maintained, how large its
  transitive tree is, and whether it pulls in procedural macros, build scripts or any unsafe code.
- Remove declared dependencies that nothing uses; they carry cost and risk for no benefit.
- Check that a new package exists, has some history and is the one intended. Typosquatting is old; slopsquatting is newer, where an
  attacker registers a name that a coding assistant tends to invent.

### Scan and constrain continuously

- Check against advisory databases (RustSec, OSV, NVD) and keep a licence policy.
- Allow only listed sources; release builds contain no ad-hoc git or local-path dependencies.
- Cap how many versions of one package may coexist.
- Update on a defined schedule. Update bots such as Dependabot or Renovate help only when tests stand between a proposed bump and
  the merge.

### Prove where artifacts came from

- Sign releases (Sigstore is the common route) and generate an SBOM for each one in SPDX or CycloneDX form.
- Pin the actions CI uses to exact commits, and give CI tokens only the permissions each job needs.
- Choose a target SLSA level, and use OpenSSF Scorecard as an outside yardstick.
- In the most sensitive tiers, accept only versions a person has audited, sharing audit records in the style of cargo-vet or crev.

## Background and lineage

Lockfiles and reproducible resolution came first; vulnerability databases followed. SBOM formats spread after a 2021 US presidential
order on software supply chains. The toolbox is completed by automated update bots, OpenSSF Scorecard, signing through Sigstore, graded SLSA provenance
levels, and audit sharing as in Mozilla's cargo-vet or crev.

## Measures and numbers

The material sets no fixed numeric thresholds. Two values are left to each project: the duplicate-version budget (SUP-005) and the
target SLSA level (SUP-010). Update cadence is likewise something to define, not a prescribed figure.

## Cautions

Rewriting everything in-house to avoid dependencies only moves the maintenance and security burden onto the team. An update bot that
merges without tests is the opposite failure. Dependency reuse versus minimalism is a contested point: one school says reuse rather
than reinvent, the other treats every dependency as a long-term liability. The default here is to allow dependencies that come with
a recorded rationale, which is why SUP-007 is marked contested. Several rules are cheap to detect but ask for human judgement about
the response (SUP-007, SUP-013), and the SLSA target is a human choice. Vendored code (SUP-011) is a scoping matter shared with the
governance domain. Procedural macros and build scripts both execute whatever they contain while compiling, so every dependency review should list them.

## Where it is checked

| Rule | Title | Class | Checked by |
| :--- | :--- | :--- | :--- |
| SUP-001 | Lockfile committed, CI resolves strictly from it | N | built: native; Cargo binaries and private npm packages have a committed lockfile and GitHub Actions workflows install only in strict mode (`--locked`, `npm ci`, `--frozen-lockfile`). Other ecosystems and CI systems are not checked |
| SUP-002 | Dependencies free of known advisories | N | built: delegated to osv-scanner against the OSV database on the day of the run; needs the tool and the network |
| SUP-003 | Dependency licences satisfy policy | N | no check yet (catalog record only; planned: cargo-deny licenses (Rust); no check yet elsewhere) |
| SUP-004 | Dependencies only from approved sources | N | built: native; Cargo and npm dependencies that are not a published version (path outside the repository, git) and `[patch]`/`[replace]` paths, plus `include!`, `#[path]` and `CARGO_MANIFEST_DIR` joins that leave the crate. The registry allowlist is not judged |
| SUP-005 | Limit on multiple versions of one dependency | N | check `SUP-005` (Rust, Info): a package that `Cargo.lock` resolves to more than three versions is listed with them; the limit is chosen from the corpus and is a constant |
| SUP-006 | Declared dependencies that are never used | P | built in part (Rust, from `Cargo.toml` and the source text, name-based like `cargo-machete`: a declared dependency whose name is a token in none of the package's `.rs` files and that no feature forwards to; a `cargo-machete` / `cargo-udeps` ignore list, a `-sys` crate, an effect-only crate and a package that includes `OUT_DIR` code are not reported; Low severity and confidence; default on in `extended` and `portfolio`, advisory; a library name that differs from the package name and a derive-only crate are not seen) |
| SUP-007 | Justification record for each new dependency | M | no check yet (catalog record only; needs a human decision) |
| SUP-008 | CI workflows pinned and least-privileged | N | built: delegated to zizmor (actions and images not pinned to a hash or digest, broad token permissions) on workflows and action definitions; a missing tool is a reported gap |
| SUP-009 | Release produces and stores an SBOM | N | check `SUP-009` (Info): a GitHub Actions workflow that publishes a release and names no SBOM tool; releases made elsewhere and SBOMs made in called workflows are not seen |
| SUP-010 | Signed artifacts with build provenance | N | check `SUP-010` (Medium, Low confidence; opt-in, `check SUP-010`): a GitHub Actions workflow that publishes a release and names no signing or provenance tool (cosign, sigstore, slsa, provenance, attest, gpg, minisign); releases made elsewhere and signing done in called workflows are not seen; the published release is not verified and the assurance level is a human choice |
| SUP-011 | Third-party code excluded from own analysis | N | built: native; vendored directories (`vendor`, `third_party`, `node_modules`, ...) are named in the codecov, codacy, sonar-project, eslintignore or deepsource configuration; whether the name is an exclusion is not judged |
| SUP-012 | New package names screened for squatting | N | no check yet (catalog record only; planned: registry existence, age and similarity lookups) |
| SUP-013 | Audited versions only in sensitive tiers | M | no check yet (catalog record only; audit records are human artifacts; cargo-vet is the model) |
| SUP-021 | Build scripts stay within what a build may do | P | built in part, opt-in (Rust, `syn`: a `build.rs` beside a `Cargo.toml` that reaches the network (a socket type or an HTTP client crate), writes files in a script that never names `OUT_DIR`, prints no rerun directive (no `rerun` in its text and no call to `prost_build`, `tonic_build`, `cc::` or `pkg_config`), or uses `unsafe`; up to four Low findings per script, each once; proc-macro crates, the record's second half, are not read) |
| SUP-022 | The licence manifest parses and keeps to its schema | P | built in part (LM-001 of the licence-tools validator; reads `LICENCE-MANIFEST.toml`, advisory Low, not applicable without it): Read from the tracked LICENCE-MANIFEST.toml at the repository root (not applicable when it is not tracked): a TOML syntax error; a version other than 1; an entry with an unknown key, without path, SPDX-License-Identifier or SPDX-FileCopyrightText (a snippet also lines), a key of the wrong type, a lines value that is not a:b with 1-based numbers and a <= b, a relation outside COPY_OF, DESCENDANT_OF, FILE_MODIFIED, VARIANT_OF, OTHER, OTHER without relation-comment, relation-comment without relation, a precedence outside closest, aggregate, override, an origin-* key without origin-url and origin-licence, an origin-licence that asks for attribution without origin-copyright. |
| SUP-023 | Declared origins carry their notice and licence text | P | built in part (LM-002 of the licence-tools validator; reads `LICENCE-MANIFEST.toml`, advisory Low, not applicable without it): For each manifest entry with origin-licence (outside CC0-1.0, Unlicense, 0BSD, MIT-0): each requires file is tracked; a tracked text names origin-copyright (case and whitespace insensitive) on a line that also carries copyright, (c) or the copyright sign, searched in requires, in NOTICE, ATTRIBUTION, COPYING, LICENSE and THIRD files in the directory of a covered file or any ancestor (never under LICENSES/), and in the first 60 lines of each covered file; the licence text is LICENSES/<id>.txt or a requires file. One finding per missing required file, else one for the holder, else one for the licence text. |
| SUP-024 | Origin markers are declared in the manifest | P | built in part (LM-003 of the licence-tools validator; reads `LICENCE-MANIFEST.toml`, advisory Low, not applicable without it): In each tracked file other than licence texts (LICENSE*, COPYING*, anything under LICENSES/) and the manifest, a line among the first 60 with Origin:, derived from, ported from or vendored (any case) that no [[annotations]] path covers and no [[snippet]] lines range (1-based, inclusive) of that file contains. A marker in a .md, .markdown, .rst, .txt or .tsv file is an info note, never a finding. |
| SUP-025 | Manifest entries describe files that exist | P | built in part (LM-004 of the licence-tools validator; reads `LICENCE-MANIFEST.toml`, advisory Low, not applicable without it): Each [[annotations]] and [[snippet]] path is matched as a REUSE glob (* inside a directory, ** across directories) against the tracked files; an entry matching none is reported. |
| SUP-026 | Declared origins allow the licence they are carried under | P | built in part (LM-005 of the licence-tools validator; reads `LICENCE-MANIFEST.toml`, advisory Low, not applicable without it): For each entry with origin-licence and an SPDX-License-Identifier string, every licence id of the expression (WITH exceptions dropped; A OR B offers both) is looked up in a table ported from licence-tools tools/licence_compat.py (permissive, no-attribution and copyleft origins; Apache-2.0 into GPL-2.0-only and LGPL-2.1-only denied). An id the table does not know is an info note, never a finding. The table is advisory data, not legal advice. |

The legacy checks do not overlap with this domain. Tool names in the last column come from CodeRipper's own triage; none is run
by the current release.

## Further reading

- Software Supply Chain Security guidance and the SLSA specification (OpenSSF).
- SPDX and CycloneDX specifications.
- cargo-vet documentation (Mozilla) and the RustSec advisory database.
- OpenSSF Scorecard documentation.
