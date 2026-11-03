# changelog.d: CHANGELOG fragments

A lane never edits `CHANGELOG.md`, `Cargo.toml`, `Cargo.lock` or the README version: those are what parallel PRs collide on.
Write one fragment per change instead, `changelog.d/<rule-or-topic>.md`:

    kind: Breaking: a new default-on rule
    ---
    **ARC-005 is built: ...** (what, why, how to opt out, evidence)

- First line `kind: ...` is the text after `proposed,` in the heading (`Breaking: a new default-on rule`, `Fix`, `Docs`, ...).
  When several fragments are stamped together, a `Breaking` kind wins.
- A line `---`, then the entry body in the CHANGELOG's own style (no heading).
- `python tools/stamp-version.py <version>` (see `docs/lanes/README.md`) turns the fragments into one heading
  `## <version> - <date> (proposed, <kind>)`, sets the version everywhere and deletes the fragments.
