# Contributing to CodeRipper

Thanks for looking under the hood. CodeRipper audits projects in any language for the
integration defects compilers and linters don't report — dead public code,
discarded return values, unread parameters, version drift, missing CI
protection — as pluggable checks under one host, all reporting in one
`Finding` shape. It started with Rust, and it is built to audit interwoven
multi-language projects.

## Ground rules

- **Review goes through `.github/CODEOWNERS`** (one owner for the whole repository today). Say in the PR which check family you touched.
- **A lane never edits `CHANGELOG.md`, `Cargo.toml`, `Cargo.lock`, or the
  README version.** Write a `changelog.d/` fragment instead (see below) —
  those files are what parallel PRs collide on.
- **Severity and confidence are separate.** A finding can be `High` severity
  at `Medium` confidence. Don't inflate confidence to make a finding look
  worse, or severity to make it look surer.
- **A coverage gap never fails a run.** If your check can't run somewhere
  (unsupported language, missing tool), report "not run here." That is
  CodeRipper's incomplete coverage, not a defect in the checked code.
- **No finding is waived silently.** Every `[[allow]]` entry needs a reason,
  an owner, and the condition under which it can be removed.

## Adding a check (the full loop)

A check is six coordinated pieces. All six, or it doesn't ship:

1. **Rule record** — `rules/<domain>.toml`: the `[[rule]]` entry (id,
   statement, rationale, severity, confidence class, applicability,
   remediation). This is the contract; write it first.
2. **Domain guide** — `docs/rules/<domain>.md`: the human-readable rationale
   for the rule, in the domain's voice.
3. **Implementation** — `src/checks/<family>/`: the check plus its unit
   tests. Report only in the `Finding` shape (`src/finding.rs`).
4. **Registration** — one row in the `builtin_checks!` table in `src/builtin.rs`, plus the
   module line in `src/checks/mod.rs` (or `src/module/neutral/mod.rs` for a language-neutral rule).
   Read `docs/lanes/README.md` first: it is the loop, step by step.
5. **Conformance fixtures** — `conformance/<RULE-ID>/{clean,defective}/`:
   miniature projects proving the rule fires exactly when it should
   (`expect.toml` declares the findings; clean must be silent).
6. **Changelog fragment** — `changelog.d/<topic>.md`: `kind: …`, `---`,
   entry body.

New rules start at `lifecycle = "experimental"` and report advisory until
they've earned trust on real code.

## Waiving a finding (in your own project)

In your project's `.coderipper.toml`:

```toml
[[allow]]
check = "RULE-ID"
file = "src/thing.rs"
reason = "Why this is acceptable, who owns it, and when it can go away."
```

Waive the finding, never the file. It's fine to write "a real candidate,
not a false alarm" and defer the fix — but then the entry must name what
the fix is.

## Testing

```sh
cargo test                    # unit + integration
cargo test --test conformance # every rule fixture, clean and defective
cargo coderipper --deny medium   # the project on itself; keep it clean
```

`tests/readme.rs` keeps the README's examples honest — if you change CLI
behavior, update the README in the same PR.

## Pull requests

- One `changelog.d/<topic>.md` fragment per change (see
  `docs/FILE-FORMATS.md` §5).
- Update `docs/` alongside behavior changes; update the README's CLI section
  if flags change.
- Keep the project's own `.coderipper.toml` clean: if your change introduces
  a finding on CodeRipper itself, fix it or waive it with reason + owner in
  the same PR.
- New checks: all six pieces above, in one PR if reviewable, split if not
  (CodeRipper's own history: the `cli.rs` split was deliberately deferred to
  keep its introducing change reviewable — say so in the waiver).

## Reporting issues

Open an issue with: the check id, a minimal reproducing project (or a
conformance-style fixture), the finding you got, and the finding you
expected. "It flagged my code" without a repro is hard to act on; the
fixture format in `conformance/` is the ideal bug report.

## Security

See `SECURITY.md`. Don't open a public issue for a vulnerability in
CodeRipper itself.
