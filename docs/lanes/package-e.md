# Package E: the remaining rules, documentation only

Read [README](README.md) first. No code: a table of every catalog rule that has no check yet, so the next planning pass starts from facts.

Deliver `docs/rules/REMAINING.md`: one row per rule in `rules/*.toml` that has no `BuiltinCheck` row (`BuiltinCheck::ALL` ids against catalog ids, read from the committed tree), with columns
id, title, family, input needed (source only / history / network / a model / a runtime), the closest existing check to copy, a size guess (S/M/L), whether it is advisory-only,
and patent-sweep risk (low/medium/high, with the idea number if already swept). Group by what a lane could do without a model; rules needing a model or a runtime go last.
End with the recommended order for the next three packages. A `changelog.d/` fragment (`kind: Docs`) is the only other file.

Done when: the table is generated from the catalog (a short script, or by hand with the count checked), its row count equals the catalog count minus `BuiltinCheck::ALL.len()`, READY.
