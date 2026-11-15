# Package I: extensibility, exact amounts and async hygiene (EXT-002, TYP-005, EXT-001, CON-005, CON-008 + REL-001, TST-003)

Claimed by the coderipper-c lane (PM go, 2026-11-06). Read [README](README.md) first, then the rows of these rules in [`docs/rules/REMAINING.md`](../rules/REMAINING.md). All are Rust, source only (`syn`), per
package; **neutral-module checks** like Package F: one file per rule (or detection) in `src/module/neutral/`, registered in `mod.rs` (the `RULES` row, `READS_ONLY`, `PROVEN`, and
`opt_in_rule_ids` until measured), a `rules/*.toml` `checked_today`, `docs/rules/*.md`, fixtures, a `changelog.d/` fragment and a `patent-sweep.d/` fragment. `concurrency.rs` is the pattern.

Not in this package (claimed elsewhere): F (CON-002, CON-003, CON-004, REL-003; merged), G (ERR-001, TYP-006), the main lane (EXT-003, SEC-005 in #250; Package H), B (API, COR, ERR-004).

| Rule | Detection | Order |
| :--- | :--- | :--- |
| EXT-002 | `downcast_ref`/`downcast_mut`/`downcast`/`is::<T>`/`type_id`/`TypeId::of`, a downcast to an error type excepted | 1 (`ext002.rs`) |
| TYP-005 | a field, parameter or binding named like an amount of money (`price`, `cost`, `balance`, `fee`, `salary`, `payment`, `amount`, `money`) with a float type, a rate/ratio name excepted | 2 |
| EXT-001 | a wildcard arm in a `match` over an enum of the same crate (enum identity by path) | 3 |
| CON-005 | a `spawn` whose handle is dropped with no comment beside it | 4 |
| CON-008 + REL-001 | one detection: a network or database client built or awaited with no timeout in sight | 5 |
| TST-003 | a `#[test]` with no assertion and no call into the crate | 6 |

Each rule ships opt-in until a corpus run (README step 4) shows few false positives; the PR carries the numbers. Patent read first for each (type tests and lint rules are old: expect lint-tool hits).
