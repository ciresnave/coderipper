# CodeRipper file formats

The machine-readable files CodeRipper itself reads and writes. All are TOML
unless noted. Field lists below describe the format as implemented; when in
doubt the source (`src/`) governs.

## 1. Rule records — `rules/<domain>.toml`

One file per rule domain (`arc.toml`, `mod.toml`, `sec.toml`, …). Each holds
one or more `[[rule]]` records. A rule record is the contract a check
implements: what the rule claims, how sure CodeRipper is, and how to fix a
violation.

```toml
[[rule]]
id = "ARC-001"
title = "Dependencies follow the declared component graph"
domain = "ARC"
statement = "Every dependency edge between components appears in the project's declared list of allowed edges."
rationale = "An architecture only holds if its intended structure is checked against what the code really does; otherwise it erodes unnoticed."
evidence_tiers = ["V0"]
evidence_strength = "C"          # E | C | F, CodeRipper's judgment of the evidence behind the rule
contested = false                # true where the guidance itself is disputed
scope = "project"                # "project" or "portfolio" (as written in rules/*.toml today)
network = "local"                # "local" or "network" (decides the run tier)
unit = "repository"              # "package" or "repository": what one run judges
executes_project_code = false
default_severity = "low"         # info | low | medium | high | critical
confidence_class = "high"        # high | medium | low
kb_refs = []                     # knowledge-base rule ids that overlap; cited by id only
# aliases = []                   # optional: other names the rule answers to
# checked_today = "..."          # optional: what the check judges today, when narrower than the statement
remediation = "Remove the offending dependency, or, if the design has really changed, update the declared graph in the same change."
related = ["ARC-002", "ARC-003"]
lifecycle = "experimental"       # experimental | advisory | blocking

[rule.applicability]
languages = ["any"]              # any | rust | typescript | python
scopes = ["first_party"]         # first_party | third_party
```

`domain` is one of the three-letter domains the catalog knows (`AIH`, `API`, `ARC`, `CON`, `COR`, `DOC`, `EFX`, `ERR`, `EVO`, `EXT`,
`GOV`, `MOD`, `OPS`, `PRF`, `RDB`, `REL`, `SEC`, `SUP`, `TST`, `TYP`, `WSP`); any other is refused when the catalog is read. Records
refuse unknown keys (`deny_unknown_fields`), so a misspelt field is an error, not a silent default. Read a record through the
accessors on `Rule` (`src/catalog.rs`).

Notes:

- `default_severity` and `confidence_class` are CodeRipper's own judgment,
  made when the record was written; a project can override them, and they are
  open to correction (see `docs/rules/README.md`).
- `lifecycle`: `experimental` is newly written and expected to be noisy, `advisory` is reported but not gating, `blocking` gates.
- The human-readable companion for each domain is `docs/rules/<domain>.md`.

## 2. Project config — `.coderipper.toml`

Lives at the repository root of the project being checked. Three jobs:
declare public API surface, waive specific findings, tune checks.

```toml
# Declare entry points the reachability check cannot see callers for.
[reachability]
public_api = [
  { file = "src/lib.rs", symbol = "run_checks_in",
    reason = "The library's public entry point; its callers are other programs, which a project-scope check cannot see." },
]

# Waive one finding, with a reason and an owner. Nothing is waived silently.
[[allow]]
check = "SEC-002"
file = "src/redact.rs"
reason = "history only: commit f8c78dbe holds a doc comment naming the private-key BEGIN/END markers (no key body). Owner: coderipper lane. Remove when history is rewritten or the commit ages out."

[[allow]]
check = "MOD-002"
file = "src/allowlist.rs"
symbol = "allowlist,architecture,config"
reason = "a real cycle, not a false alarm: … Owner: coderipper lane."
```

`[[allow]]` takes `check`, `file` and `reason` (all required), and optionally `symbol` (waive only the finding about that symbol) and
`lines` (`"12"` or `"10-20"`, inclusive). Unknown keys are refused. A waived finding is still printed, with its reason. The file holds
more tables than the two shown (`[architecture]`, `[rdb008]`, `[[cli_waiver]]`, ...); the README documents each where it is used.

Conventions (from CodeRipper's own config):

- Only `reason` is required by the parser. By this project's convention it also names an owner and the condition under which the
  entry can be removed; a waiver without a removal condition is a smell.
- Waive the finding, never the file: entries name `check` + `file` (+ `symbol`
  where it helps). Nothing is excluded by path.
- A waiver may honestly say "a real candidate, not a false alarm" and defer
  the fix — but then it names what the fix is.

## 3. Conformance fixtures — `conformance/<RULE-ID>/{clean,defective}/`

Each rule proves itself on a miniature project with a seeded defect and a clean twin, in the language(s) the rule covers (the
runner is `src/conformance.rs`; the integration test is `tests/conformance.rs`). Layout, shown for a Rust rule; other languages use
their own project shape, and a fixture may be a single file (`conformance/SEC-002/defective/settings.cfg`):

```
conformance/ARC-008/
  clean/
    Cargo.toml      # a tiny crate project
    src/            # …with no violation
    expect.toml     # expects nothing (a comment is enough)
  defective/
    Cargo.toml
    src/            # …with exactly the violation
    expect.toml     # expected findings, spelled out
```

`expect.toml`:

```toml
needs_network = false   # optional: the rule needs the network; unproven unless the run allows it
origin = "https://github.com/acme/widgets"   # optional: gives the fixture's repository this `origin` remote

[[expect]]
rule = "ARC-008"
file = "src/lib.rs"
# line = 3              # optional
# many = true           # optional: one or more findings here, not exactly one
```

The runner copies each fixture into a throwaway git repository (checks read `HEAD`). Every expectation must be met by exactly one
finding (or one or more with `many = true`), nothing else may be reported in the `defective` project, and nothing at all in the
`clean` one. A claimed rule without a complete pair is not covered, and a defect with no clean twin proves nothing. **Fixtures are
code that runs** (a build script executes with your privileges): run conformance only over fixtures you wrote or trust.

## 4. Findings: the `Finding` shape (JSON)

Every check reports in one shape (`src/finding.rs`). Severity and confidence are deliberately separate axes: *how bad* is
independent of *how sure*.

```jsonc
{
  "check_id": "reachability",
  "severity": "low",        // info | low | medium | high | critical
  "confidence": "high",     // low | medium | high
  "project": "my-crate",    // the project, by directory name
  "location": { "file": "src/lib.rs", "line": 12 },   // null for a project-level finding; line may be null
  "subject": "my_fn",       // the symbol, when the finding is about one (with check_id and file, its fingerprint)
  "summary": "...",         // one line: what is wrong
  "detail": "...",          // why it matters and how to resolve it
  "member": "my-crate",     // only in a --workspace run (the cargo package); omitted otherwise
  "positive_control": "...",// required when the claim is an absence ("zero callers"); null otherwise
  "claim": "absence"        // omitted for a presence
}
```

Severity: `info` = worth knowing, nothing to fix; `low` = small cost; `medium` = will cost someone time; `high` = likely to cause a
failure; `critical` = is a failure. Confidence `low` means "a plausible reading; expect false positives", `high` "read directly from
the project, no ambiguity". The `--message-format json` stream wraps each finding in a line with a `"reason"` (`coderipper-finding`,
`coderipper-waived`, ...); the README's "Machine-readable output" documents that envelope.

The CLI prints human text by default; `--message-format json` is the stable machine interface. Exit codes: `0` every check ran and
no finding is at or above `--deny`; `1` a finding at or above `--deny` that no `[[allow]]` waives; `2` usage error; `3` the audit
could not be completed (a check could not run; this outranks 1). A coverage gap, a rule CodeRipper has not implemented or whose tool
is missing, is "not run here" and never fails a run.

## 5. Changelog fragments — `changelog.d/<topic>.md`

A lane never edits `CHANGELOG.md`, `Cargo.toml`, `Cargo.lock`, or the README
version — those are what parallel PRs collide on. One fragment per change:

```
kind: Breaking: a new default-on rule
---
**ARC-005 is built: ...** (what, why, how to opt out, evidence)
```

- First line `kind: …` is the release-kind stamp (`Breaking: …`, `Fix`,
  `Docs`, …). When fragments are stamped together, a `Breaking` kind wins.
- Then a `---` line, then the entry body in the CHANGELOG's own style (no
  heading).
- `python tools/stamp-version.py <version>` turns the fragments into one
  `## <version> - <date> (proposed, <kind>)` heading, sets the version
  everywhere, and deletes the fragments.

## 6. Vendored data — `data/patent-tiers.json`

Machine-readable patent-tier data used by the patent-safety checks, vendored
with its `LICENSE`, `NOTICE`, and `PIN` files. Treat as read-only data:
regenerate from upstream, don't hand-edit.

## 7. Check registration: `src/builtin.rs`

A built-in check is registered with one row in the `builtin_checks!` table in `src/builtin.rs` (and a module line in
`src/checks/mod.rs` for a check family, or in `src/module/neutral/mod.rs` for a language-neutral rule module). Tests derive the
registered set from that table. Adding a check = rule record + implementation + registration + fixtures + docs + changelog fragment;
see `CONTRIBUTING.md` and `docs/lanes/README.md`.
