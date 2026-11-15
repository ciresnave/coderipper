# Extending the `[architecture]` declaration: purity, purpose and owner, isolation, hot loops. A proposal for CireSnave's review

**Status: proposed, docs only (2026-11-12). Nothing is built, and no check will be built on this until CireSnave has reviewed it** (PM,
2026-11-12: write one proposal for the four rules that need new declaration fields, and put it in front of CireSnave). It extends
`2026-11-01-architecture-declaration-proposal.md`, whose six choices CireSnave accepted on 2026-11-01 (ruling 365), and keeps all of its
principles: normative not descriptive, exceptions visible and expiring, a stale declaration is a finding, no declaration is never "clean".
Evidence labels: **MEASURED** = read in this session at the ref named; **PROPOSED** = a design choice for review, not a claim about any
repository; **ILLUSTRATIVE** = invented to show the format.

## 1. What it is for

Four catalogued rules are blocked because the declaration has nowhere to say what they need to read. Two others look as if they were and
are not (section 6).

| Rule | What it asks | What is missing from the declaration today |
| :-- | :-- | :-- |
| EFX-001 Components declared pure avoid effectful APIs | components tagged pure do not touch file, network, environment, process, clock, randomness or thread APIs | no way to tag a component pure, and no list of what counts as an effect |
| DOC-003 Components carry a purpose and an owner | every declared component has a purpose statement and a named owner | a component has a `name`, `paths`, `role`, `volatility`, `owns_data` and `invariant_types`, and no purpose or owner |
| REL-011 Breakers and bulkheads where required | a dependency the specification says must be isolated is reached only through the declared breaker or bulkhead | no way to name a dependency that needs isolation, or the wrapper that isolates it |
| PRF-004 Hot loops allocate nothing | loops tagged hot allocate no memory per iteration | no way to tag a loop or function hot |

**MEASURED (the local primary checkouts of 12 repositories on 2026-11-12, `HEAD`; a checkout can lag `origin/main`):** none declares an
`[architecture]` table (8 have a `.coderipper.toml`, 4 have none; Fuel's tier file is separate and is not read). So today every A-class rule
reads "not run" everywhere, and these four would too. The design goal is therefore that **each addition is optional, small, and useful
on its own**, so a project can adopt one tag without writing a full declaration.

## 2. The four additions

### 2.1 Purity (EFX-001): `pure` on a component, and an effect list

```toml
[[architecture.component]]
name  = "pricing"
paths = ["crates/pricing"]
role  = "policy"
pure  = true            # PROPOSED: this component performs no effects; EFX-001 judges it

[architecture.effects]  # optional: the default list below is used when this table is absent
add    = ["metrics::"]  # more effect namespaces or names for this project
remove = ["std::time::Instant"]   # a family this project does not count as an effect, with the reason in a comment
```

`pure` is a boolean, absent meaning "not judged" (not "impure"). The **default effect list for Rust** (PROPOSED, the rule's own seven
families): file (`std::fs`, `tokio::fs`), network (`std::net`, `tokio::net`, `reqwest::`, `hyper::`), environment (`std::env`), process
(`std::process`, `tokio::process`), clock (`std::time::SystemTime`, `std::time::Instant`, `chrono::Utc::now`, `chrono::Local::now`),
randomness (`rand::`, `getrandom::`), threads (`std::thread`, `tokio::spawn`). A use is read the way ARC-004 reads names: resolved through
the module's own `use` declarations, so a renamed import is still seen. A deliberate exception is an item-level `waiver` (section 2.5),
with an owner and an end date, never a silent skip. **What it cannot see:** an effect behind a trait object the component is handed (that
is the point of a pure core: the effect lives in the adapter), macro-generated calls, a type alias in another crate.

### 2.2 Purpose and owner (DOC-003): two strings on a component

```toml
[[architecture.component]]
name    = "pricing"
paths   = ["crates/pricing"]
purpose = "Turns a basket and a rate card into a price; owns no I/O."   # PROPOSED
owner   = "billing lane"                                               # PROPOSED: a person, team or lane
```

DOC-003 reports a component whose `purpose` or `owner` is missing or blank. It is a finding of the rule, **not a parse error**, so a project
can adopt the declaration a step at a time. `owner` is free text that must be non-blank; a later cross-check against `CODEOWNERS` or the
portfolio's lane names is possible and is not part of this proposal.

### 2.3 Isolation (REL-011): a dependency and its wrapper

```toml
[[architecture.isolation]]            # PROPOSED
dependency = "payments_api::"         # a namespace (ends in ::) or a type or function name, as in [architecture.volatile]
wrapper    = ["CircuitBreaker", "Bulkhead"]   # the declared types or functions that isolate it
components = ["checkout"]             # the components that call it and must go through the wrapper
reason     = "the payments provider has caused two full outages"          # required
```

REL-011 reports a function in a listed component that names the dependency and does not also name a declared wrapper (the wrapper must
appear in the same function body or in the type of one of its parameters or its receiver). **Weak evidence by construction (low
confidence):** a call made through a helper in another module is seen only at the helper, and "uses the wrapper" is a name match, not
proof that the wrapper guards the call. That is why it is advisory and Low.

### 2.4 Hot loops (PRF-004): a marker in the source and an optional path list

A loop is not an item, so it cannot be named by a path. PROPOSED: a comment marker on the line directly above the loop,

```rust
// coderipper: hot
for row in rows { ... }
```

and, for a whole function, an optional list in a **separate** `[performance]` table (performance is not architecture):

```toml
[performance]
hot = ["crates/kernels/src/gemm.rs::matmul"]      # PROPOSED
```

PRF-004 reports an allocation inside a hot loop or function: `Vec::new`, `Vec::with_capacity`, `vec![]`, `String::new`, `String::from`,
`format!`, `.to_string()`, `.to_owned()`, `Box::new`, `Rc::new`, `Arc::new`, `.clone()` of a type that is not obviously `Copy`, and
`.collect()`. **What it cannot see:** an allocation inside a callee, a `clone` of a cheap type, allocator behaviour; low confidence, advisory.
The comment marker is read the way `SAFETY` comments and SEC-005's justification are read, so it adds no dependency to the analysed project.

### 2.5 Item-level waivers (shared)

The existing `[[architecture.waiver]]` waives an **edge**. EFX-001 and REL-011 need to waive an **item** (one function that must read the
clock). PROPOSED: the same table, with `item = "pricing::quote::now_for_audit"` in place of `edge`, and the same required `reason`,
`owner` and `until`; an expired item waiver is a finding, exactly as for an edge, and counts toward the ratchet when a project opts in.

## 3. What each rule reads and when it says "not run"

| Rule | Reads | Finding when | "Not run" when |
| :-- | :-- | :-- | :-- |
| EFX-001 | components with `pure = true`, `[architecture.effects]`, item waivers | a use of an effect name in a pure component (Medium, High confidence for the names it matches) | no component is tagged `pure` |
| DOC-003 | every component's `purpose` and `owner` | a component with either blank (Low, High confidence) | there is no `[architecture]` table |
| REL-011 | `[[architecture.isolation]]` | a function in a listed component names the dependency and no wrapper (Low, Low confidence) | no isolation entry |
| PRF-004 | `// coderipper: hot` markers, `[performance] hot` | an allocation shape in a hot loop or function (Low, Low confidence) | no marker and no `hot` entry |

Severities and confidences are the catalogue records' own (`rules/efx.toml`, `rules/doc.toml`, `rules/rel.toml`, `rules/prf.toml`). Every
one of these reports "not run" with what to add, never "clean", as the 2026-11-01 proposal's section 6 requires.

## 4. A worked example (ILLUSTRATIVE, not any repository)

```toml
[architecture]
style = "layered"

[[architecture.component]]
name       = "pricing"
paths      = ["crates/pricing"]
role       = "policy"
volatility = "stable"
pure       = true
purpose = "Prices a basket from a rate card."
owner = "billing lane"

[[architecture.component]]
name       = "checkout"
paths      = ["crates/checkout"]
role       = "adapter"
volatility = "volatile"
purpose = "Takes an order over HTTP and charges the card."
owner = "billing lane"

[[architecture.isolation]]
dependency = "payments_api::"
wrapper    = ["CircuitBreaker"]
components = ["checkout"]
reason     = "the provider has failed twice"

[[architecture.waiver]]
item   = "pricing::quote::audit_stamp"
reason = "stamps the quote with the wall clock for the audit trail; moves to the caller in #123"
owner  = "billing lane"
until  = "2027-03-01"
```

EFX-001 would read `pricing`, find the `SystemTime::now` in `audit_stamp`, and report it as waived until 2027-03-01. DOC-003 finds
both components complete. REL-011 reports any function in `checkout` that calls `payments_api::` without naming `CircuitBreaker`.

## 5. Choices that are CireSnave's (each with a recommendation)

1. **`pure` as a boolean or a closed set.** Recommend a boolean (`pure = true`), absent meaning "not judged". Alternative: a closed set
   such as `purity = "pure" | "io-boundary"`, which invites a third value later.
2. **The effect list.** Recommend a built-in default list that a project extends (`add`) or narrows (`remove`), so a first tag works with no
   list written. Alternative: the project writes the whole list (accurate, verbose, and it drifts).
3. **Purpose and owner: finding or parse error.** Recommend a DOC-003 finding, so adoption can be gradual. Alternative: reject a component
   with a blank field at load, which blocks every other rule until it is filled in.
4. **`owner` format.** Recommend free non-blank text now. Alternative: require a match against `CODEOWNERS` or a lane name list.
5. **How an isolation wrapper is named.** Recommend by type or function name, resolved through `use` as ARC-004 does. Alternative: by full
   path (exact, and breaks on a re-export).
6. **How a hot loop is tagged.** Recommend the `// coderipper: hot` comment plus the optional `[performance] hot` list. Alternative: an
   attribute (`#[coderipper::hot]`), which would put a dependency into every analysed project and does not tag a loop.
7. **Where the hot list lives.** Recommend a separate `[performance]` table. Alternative: under `[architecture]`, which mixes two concerns.
8. **Item-level waivers.** Recommend extending the existing waiver table with `item` (same reason, owner, end date, ratchet). Alternative:
   the existing allowlist, which has no end date.

## 6. What this proposal does not do, and what it does not need

- **Builds nothing.** No check is registered, no config key is read, no rule changes.
- **ERR-006 (domain errors free of transport details) and DOC-007 (diagrams drifted from the dependency graph) need nothing new.** ERR-006
  reads a `policy` component's error types against `[architecture.volatile]` names (the machinery ARC-004 already has); DOC-007 reads
  diagram edges against the declared graph. Both are scheduled and will be built on the schema as it is.
- **TYP-005 (money avoids binary floats)** is built today on a field-name heuristic. A component tag (`exact = true`) would let it judge
  a declared money module with more confidence; it is not proposed here, and would be a separate small addition if wanted.
- **Does not fix thresholds** (the `add` and `remove` lists and the allocation shapes are starting points a corpus reading will tune,
  reported with the rule, as for every other rule).
- **Does not decide how a declaration is first drafted** (the `coderipper architecture draft` idea in the 2026-11-01 proposal is still
  later and still never active on its own).
