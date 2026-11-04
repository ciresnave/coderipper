# CodeRipper directory structure

A map of the repository for newcomers. Paths are relative to the repo root.
CodeRipper is one engine with three faces: a Rust library, a `coderipper`
command (also a `cargo coderipper` subcommand), and — planned, not in this
crate — a hosted service.

## Root

| Path | What it is |
|---|---|
| `Cargo.toml` / `Cargo.lock` | Crate manifest and lockfile. Never edited by hand in a PR — see `changelog.d/`. |
| `README.md` | The user manual: install, run next to clippy, exit codes, waiving findings. |
| `ROADMAP.md` | Where the project is going. |
| `CHANGELOG.md` | Generated from `changelog.d/` fragments at release time. Never edited by hand. |
| `RELEASING.md` | How a release is cut. |
| `SECURITY.md` | How to report vulnerabilities. |
| `build.rs` | Build script (version stamping etc.). |
| `codecov.yml` | Coverage config. |
| `deny.toml` | `cargo-deny` config (licenses, advisories, bans). |
| `.coderipper.toml` | CodeRipper's config for running on itself. The project's own waivers live here. |
| `.gitignore` | Standard ignores. |
| `.github/` | `CODEOWNERS` + `workflows/` (CI, coverage, scheduled audits, release drift, portfolio). |
| `CONTRIBUTING.md` | How to add a check, write a fragment, and open a PR. |

## `src/` — the engine

| Path | What it is |
|---|---|
| `lib.rs` | Library entry point; the public API (`run_checks_in`, …). |
| `main.rs` | The `coderipper` binary; thin wrapper over the library. |
| `cli.rs` | Argument definitions, dispatch, report printing, the `cargo`-subcommand shim (over 1,800 lines; a `cli/` split is waived in `.coderipper.toml` until it is done). |
| `check.rs` | The check host: the trait every check implements, the runner, profiles (`fast` vs `sweep`). |
| `finding.rs` | The one `Finding` shape every check reports in: `Severity` × `Confidence` kept deliberately separate. |
| `catalog.rs` | The rule catalog: loads `rules/*.toml` into rule records. |
| `config.rs` | Parses `.coderipper.toml` (reachability allowlists, `[[allow]]` waivers). |
| `session.rs` + `session/` | A run session: scope, worktree, caches, reporting state. |
| `worktree.rs` | Checks out `HEAD` into a throwaway worktree; checks never touch your working tree. |
| `package.rs`, `cargo_json.rs` (+`cargo_json/`), `cargo_main.rs` | Cargo project model: packages, manifests, `cargo metadata` JSON. |
| `module/` | The module model (`rust.rs`, `external.rs`, `composite.rs`, `delegate/`, `protocol.rs`) and the language-neutral rules in `neutral/`, registered in `neutral/mod.rs`. |
| `checks/` | One directory per check family, each with the check plus its unit tests: `architecture/` (ARC rules: layers, cycles, modgraph, …), `reachability/`, `unused_parameters/`, `unused_return_values/`, `version_consistency/`, `dependency_freshness/`, `ci_protection_presence/`, `cli_basics/`, `cohesion/`, `size_coupling/`. |
| `tools/` | Installing and pinning the external tools some rules delegate to (`tools.lock` holds the pins); the delegating modules are in `module/delegate/`. |
| `github.rs` | GitHub API client (used by network-tier checks like CI protection presence). |
| `coverage.rs` | Coverage-gap reporting: a rule with no implementation is "not run here," never a failure. |
| `suppression.rs`, `allowlist.rs`, `qualify.rs` | Waiver/allowlist machinery (`[[allow]]` entries, public-API allowlists). |
| `redact.rs` | Secret redaction for output. |
| `result_cache.rs`, `build_cache/` | Caching of check results and build artifacts. |
| `rule_summary.rs` | Per-rule summary reporting. |
| `builtin.rs` | The `builtin_checks!` table: the one place a built-in check is registered. |

## `rules/` — the rule records

One TOML file per rule domain (21 today) (`arc.toml`, `mod.toml`, `sec.toml`, `doc.toml`, …).
Each file holds `[[rule]]` records: id, title, statement, rationale, severity,
confidence class, evidence tiers, applicability, remediation. See
`docs/FILE-FORMATS.md`. The human-readable guides for each domain live in
`docs/rules/<domain>.md`.

## `conformance/` — the rule proving ground

One directory per rule ID (`ARC-008/`, `SEC-002/`, …), each with `clean/` and
`defective/` subdirectories. Each holds a miniature fixture project in the
language(s) the rule covers, plus an `expect.toml` declaring exactly which
findings the check must produce. If a rule can't be demonstrated on a fixture,
it doesn't ship. See `docs/FILE-FORMATS.md`.

## `docs/`

| Path | What it is |
|---|---|
| `rules/` | Per-domain rule guides (`arc.md`, `mod.md`, …) + `README.md` (domain table), `verification-model.md`, `cross-cutting.md`, `lessons-and-corrections.md`, `reading-list.md`. |
| `lanes/` | Briefs for the remaining architecture and modularity rules, written so a new contributor can start cold. |
| `assets/` | Brand assets: `coderipper-logo-claw.png` (the 1600x1600 master) and a `.webp` export used by the README. |
| `hosted-service/` | Design notes for the planned hosted service (not part of this crate). |
| `portfolio-triage/`, `portfolio-ci-coverage.md` | Portfolio-level workflows. |
| `superpowers/` | Plans, specs and triage notes. |
| `PATENT-SWEEP.md` | Patent-sweep records and method. |
| `member-footprint-design.md` | Design note. |
| `CONSUMER-CHANGES-0.15-TO-0.17.md` | Migration guide for a breaking range. |

## `tests/` — integration tests

One file per check area (`reachability.rs`, `version_consistency.rs`, …) plus
`conformance.rs` (runs every fixture in `conformance/`), `golden/` (golden
output tests), `fixtures/` (shared fixture projects), `common/` (test helpers).
`readme.rs` tests that the README's examples stay accurate.

## Everything else

| Path | What it is |
|---|---|
| `examples/` | `custom_check.rs` (write your own check), `run_on_a_project.rs`, `fake_github.rs` (test double for the GitHub client). |
| `tools/` | `release-tag.sh`, `stamp-version.py` (release and version tooling). |
| `data/` | Vendored `patent-tiers.json` (+ `LICENSE`, `NOTICE`, `PIN`) used by the patent-safety checks. |
| `changelog.d/` | One fragment per change; stamped into `CHANGELOG.md` at release. Never edit the changelog by hand. |
| `patent-sweep.d/` | Patent-sweep fragments, stamped into `docs/PATENT-SWEEP.md` like changelog fragments. |

## The shape of a check (where things plug in)

```
rules/<domain>.toml        rule record (the contract: what the rule claims)
docs/rules/<domain>.md     human guide for the domain
src/checks/<family>/       the implementation (+ unit tests)
src/checks/mod.rs          the module line for the family
src/builtin.rs             one builtin_checks! row: the registration
conformance/<RULE-ID>/     clean/ + defective/ fixtures proving it
tests/<area>.rs            integration tests
changelog.d/<topic>.md     the changelog fragment
```

A new check touches all of these. See `CONTRIBUTING.md`.
