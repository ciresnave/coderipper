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
| SUP-001 | Lockfile committed, CI resolves strictly from it | N | no check yet (catalog record only; planned: read lockfile names and CI install flags as text) |
| SUP-002 | Dependencies free of known advisories | N | no check yet (catalog record only; planned: osv-scanner, cargo-audit) |
| SUP-003 | Dependency licences satisfy policy | N | no check yet (catalog record only; planned: cargo-deny licenses (Rust); no check yet elsewhere) |
| SUP-004 | Dependencies only from approved sources | N | no check yet (catalog record only; planned: cargo-deny sources (Rust); manifest scans elsewhere) |
| SUP-005 | Limit on multiple versions of one dependency | N | no check yet (catalog record only; planned: cargo-deny bans, npm ls) |
| SUP-006 | Declared dependencies that are never used | P | no check yet (catalog record only; planned: cargo-machete, knip, deptry; needs import resolution) |
| SUP-007 | Justification record for each new dependency | M | no check yet (catalog record only; needs a human decision) |
| SUP-008 | CI workflows pinned and least-privileged | N | no check yet (catalog record only; planned: zizmor) |
| SUP-009 | Release produces and stores an SBOM | N | no check yet (catalog record only; planned: look for a generation step in release workflows) |
| SUP-010 | Signed artifacts with build provenance | N | no check yet (catalog record only; planned: look for signing and provenance steps (the level is a human choice)) |
| SUP-011 | Third-party code excluded from own analysis | N | no check yet (catalog record only; planned: compare vendored directories with exclusion config) |
| SUP-012 | New package names screened for squatting | N | no check yet (catalog record only; planned: registry existence, age and similarity lookups) |
| SUP-013 | Audited versions only in sensitive tiers | M | no check yet (catalog record only; audit records are human artifacts; cargo-vet is the model) |

The legacy checks do not overlap with this domain. Tool names in the last column come from CodeRipper's own triage; none is run
by the current release.

## Further reading

- Software Supply Chain Security guidance and the SLSA specification (OpenSSF).
- SPDX and CycloneDX specifications.
- cargo-vet documentation (Mozilla) and the RustSec advisory database.
- OpenSSF Scorecard documentation.
