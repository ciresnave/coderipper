# The `[architecture]` declaration: a proposal for CireSnave's review

**Status: proposed, docs only (2026-11-01). Nothing is built, and no A-class check will be built on this until CireSnave has reviewed
it** (PM, 2026-11-01: *CireSnave ruled "lane proposes, I review"*). It answers design note
`2026-11-01-arc-mod-checks-design.md` section 4, ASK 4. Evidence labels: **MEASURED** = read in this session at the ref named;
**ILLUSTRATIVE** = invented to show the format, not a claim about any repository.

## 1. What it is for

The A-class rules (ARC-001, 002, 003, 006, 007, and the declared halves of ARC-004, ARC-012 and MOD-010) ask whether the code follows an
architecture. A tool cannot read the intended architecture from the code (the code is what is being judged), so the project writes it
down as data and the tool compares the real graph with it. The declared style (layered, hexagonal, slices, modular monolith) is a label
only; **every check derives from the components and edges stated**, so no style is imposed.

Principles, each from something already decided in this portfolio:

1. **Normative, not descriptive.** Fuel's own tier file says it plainly: *"THIS FILE IS NORMATIVE, NOT DESCRIPTIVE. DO NOT 'FIX' IT TO
   MATCH THE GRAPH."* A declaration that merely transcribes the graph passes forever. The declaration states where things *should*
   sit; the disagreement is the finding.
2. **Exceptions are visible and expire.** A deliberate violation is a per-edge `waiver` with a reason, an owner and an end date, printed
   in every run. There is no path-wide exclusion and no silent skip.
3. **A declaration that has gone stale is itself a finding.** (MEASURED, fuel `origin/main` 3a41200c: its tier file names `fuel-core`, a
   dissolved crate, and leaves 4 members unplaced.)
4. **No declaration is not a failure and not "clean".** See section 6.

## 2. Where it lives

**Proposal:** a `[architecture]` table in the repository's own `.coderipper.toml`, read by the existing loader, so unknown keys are
an error that names the file, and `--no-config` and the system/user files behave as for every other rule. A large declaration may move to a
file named by `[architecture] file = "architecture.toml"` (same schema). Reason for the choice: one loader, one set of rules about
which files are read; the cost is that a very large declaration makes `.coderipper.toml` long, which `file =` answers.

## 3. The schema

```toml
[architecture]
style = "layered"   # informational only: layered | hexagonal | slices | modular-monolith | pipeline | plugin | free text
# Components: every workspace member must be in exactly one (a member in none is reported as unplaced; a component naming nothing is stale).
[[architecture.component]]
name       = "domain"
paths      = ["crates/domain"]       # workspace-relative crate directories, or module paths inside a crate: "src/billing/**"
role       = "policy"                # policy | adapter | infrastructure | composition | test-support
volatility = "stable"                # stable | volatile   (what changes least / most; used by ARC-003 and as context for ARC-005)
owns_data  = ["orders"]              # ARC-007: tables/collections/files only this component may name
invariant_types = ["Money"]          # MOD-010: types whose fields must stay private

# Direction. EITHER an ordered list of layers (a component may depend on its own layer or any layer BELOW it) ...
[[architecture.layer]]
name       = "foundation"
components = ["domain", "kernels"]
[[architecture.layer]]
name       = "adapters"
components = ["http", "storage"]
# ... AND/OR explicit edges for what the layers do not say:
[[architecture.allow]]
from = "http"
to   = "storage"
reason = "the http adapter reads the storage adapter's health probe"   # required: an allowed cross-edge says why

# Volatile details (ARC-003/004): names whose use inside a policy component is a violation.
[architecture.volatile]
names      = ["BackendId", "DeviceLocation"]        # enums/types: any path or variant naming them
namespaces = ["sqlx::", "reqwest::", "axum::"]      # crates/paths infrastructure code uses
feature_cfg_in_policy = true                        # count #[cfg(feature = "...")] lines in policy components (inventory)

[architecture.composition]                          # ARC-012: where concrete wiring may happen
roots = ["src/main.rs", "crates/app/src/wire.rs"]

[architecture.api_budget]                           # ARC-006: pub items per component
domain = 40

[[architecture.waiver]]                             # a deliberate violation: edge or item, with owner and end date
edge   = ["domain", "storage"]
reason = "migration in progress, tracked in #123"
owner  = "billing lane"
until  = "2027-03-01"                               # an expired waiver is a finding, not silence
```

Rules for reading it: unknown keys are an error; a name used but not declared is an error; `layer` order is bottom to top and the
bottom layer is listed first; a component in no layer may only use `allow` edges (it is judged, not skipped).

**Addendum, 2026-11-12 (built):** a repository that genuinely has none of an optional input says so with `none = ["volatile", "api_budget", 
"owns_data", "composition"]` (any subset) in `[architecture]`, and the rule that needs it runs with 0 sites by declaration instead of 
"not run"; listing an input that is also declared is an error. See `docs/rules/arc.md`.

## 4. Three worked examples

### 4.1 Fuel: layers (MEASURED source: `fuel-ir/tests/crate_dependency_tiers.txt` at fuel `origin/main` 3a41200c; the crate names are
from `cargo metadata` of the same tree)

Fuel already states its dependency order as a tier per crate, "a crate may depend only on crates at its own tier or BELOW". That is
the `layer` form with the tier numbers as the order. The declaration below is the same information, regrouped by layer name (shortened;
a real file lists every crate):

```toml
[architecture]
style = "layered"

[[architecture.layer]]
name = "ir"                  # tier 10
components = ["fuel-ir", "fuel-kv-pool"]
[[architecture.layer]]
name = "kernels"             # tier 20
components = ["fuel-conv", "fuel-cpu-kernels", "fuel-vulkan-kernels", "fuel-metal-kernels", "fuel-kernels-source", "fuel-kernel-seam-types", "fuel-kernel-seam-announce"]
[[architecture.layer]]
name = "contracts"           # tier 30
components = ["fuel-backend-contract", "fuel-kernel-seam", "fuel-correctness-fixtures", "fuel-formats"]
[[architecture.layer]]
name = "foundation"          # tier 40
components = ["fuel-graph", "fuel-quantized", "fuel-kiss-ref-backend", "fuel-loaders"]
[[architecture.layer]]
name = "backends"            # tiers 50-60
components = ["fuel-cpu-backend", "fuel-vulkan-backend", "fuel-cuda-backend", "fuel-metal-backend"]
[[architecture.layer]]
name = "runtime"             # tiers 70-80
components = ["fuel-hardware", "fuel-memory", "fuel-dispatch"]
# ... vendor backends (90), tensor (100), models (110), the facade `fuel` (120),
# orchestration (130), consumers (140), as in the tier file

# `fuel` deliberately sits ABOVE the libraries although (when the tier file was written) it sat below them in the real graph; such an
# edge is the violation the tier file exists to show, and it is a waiver with a reason. The Fuel lane reports 1 such edge today
# (`fuel-onnx` -> `fuel`, allowlisted in Fuel's own file):
[[architecture.waiver]]
edge   = ["fuel-onnx", "fuel"]
reason = "ROADMAP item 12: the facade inversion (as Fuel's own allowlist states it)"
owner  = "fuel lane"
until  = "2027-01-31"

# What the crate graph cannot see (MEASURED by the Fuel lane, row 136, fuel#574): backend knowledge inside generic crates
# (375 cfg(feature = <backend>) lines and 770 BackendId/DeviceLocation uses in 11 generic crates; fuel-dispatch 233 of 594).
[architecture.volatile]
names = ["BackendId", "DeviceLocation"]
feature_cfg_in_policy = true
# the generic crates are the `policy` components; the backends are `adapter`s:
#   [[architecture.component]] name = "fuel-dispatch"  paths = ["fuel-dispatch"]  role = "policy"  volatility = "stable"
```

What this gives for Fuel: ARC-001 compares 166 real edges with the layer order; ARC-003 counts `BackendId`/`DeviceLocation` uses and
backend `cfg(feature)` lines in the policy crates and ranks them (the Fuel lane's counts are the first thing to compare with); the
declaration's **staleness** check reports the tier file's `fuel-core` and the unplaced members.

### 4.2 A small single crate (ILLUSTRATIVE, not any repository)

```toml
[architecture]
style = "modular-monolith"

[[architecture.component]]
name = "core"
paths = ["src/plan.rs", "src/classify.rs", "src/ledger.rs"]
role = "policy"
volatility = "stable"

[[architecture.component]]
name = "adapters"
paths = ["src/registry.rs", "src/ci.rs", "src/cargo.rs"]
role = "adapter"
volatility = "volatile"

[[architecture.component]]
name = "shell"
paths = ["src/main.rs", "src/cli.rs", "src/run.rs"]
role = "composition"

[[architecture.layer]]
name = "bottom"
components = ["core"]
[[architecture.layer]]
name = "middle"
components = ["adapters"]
[[architecture.layer]]
name = "top"
components = ["shell"]

[architecture.composition]
roots = ["src/main.rs"]
```

Within one crate the compiler allows every module-to-module use, so this is where the declaration earns its keep: ARC-001 and ARC-003
read the module graph (the same graph MOD-002 builds), so `core` naming `adapters` is a finding even though cargo sees nothing.

### 4.3 The smallest useful declaration (ILLUSTRATIVE): a library and its command-line tool

```toml
[architecture]
style = "layered"
[[architecture.layer]]
name = "lib"
components = ["mylib"]
[[architecture.layer]]
name = "bin"
components = ["mylib-cli"]
```

Nine lines. Components default to one per workspace member named by `components`, so a crate-granular declaration needs no
`[[architecture.component]]` blocks; they are needed only for role, volatility, tags, or to split a crate into module groups.

## 5. What each rule reads from it

| Rule | Reads | Finding when |
| :-- | :-- | :-- |
| ARC-001 dependencies follow the component graph | components, layers, `allow`, `waiver` | a real edge (crate graph, or module graph in a single crate) goes up a layer or is not allowed; an expired or unused waiver |
| ARC-002 no cycles between components | components | a strongly connected set spans two or more components (cycles inside one component are Info) |
| ARC-003 stable policy avoids volatile detail | `role`, `volatility`, `[architecture.volatile]` | a `policy`/`stable` component names a volatile type or namespace, or carries backend `cfg(feature)` lines; counts ranked per component |
| ARC-004 infrastructure types in policy signatures | `volatile.names`, `volatile.namespaces`, `role` | such a name appears in a `pub` signature of a policy component (syntactic; a type alias hides it) |
| ARC-005 depend toward stability | components (no tags needed) | an edge to a component less stable by the margin; works with no declaration at crate and module level |
| ARC-006 API within budget | `api_budget` | a component's `pub` item count exceeds its budget, or it re-exports a third-party type |
| ARC-007 data owned by one component | `owns_data` | a second component names an owned table/collection |
| ARC-012 construction only in composition roots | `composition.roots` | a type from a non-root component is constructed outside a root |
| MOD-010 invariant types keep fields private | `invariant_types` | such a type has a `pub` field |
| MOD-001/003/005 | `role = "test-support"` only | not used otherwise |

Every one of these is exact for the facts it sees (V0) and says what it cannot see (macros, glob imports, type aliases).

## 6. What a project without a declaration gets

1. The **P-class rules run unchanged**: MOD-001 to 005, 008 to 010, ARC-005 (crate graph, and top-level modules in a single crate).
2. Each A-class rule is **reported as not run** in the coverage line, naming the missing `[architecture]`, so a missing declaration is a
   visible gap and never "clean".
3. One Info note per repository states how many members and top-level modules there are and that a declaration would let N rules run.
4. **Later, not part of this proposal:** `coderipper architecture draft` would partition the real graph and write a proposed file with
   confidence marks for a person to edit and adopt. It would never be active on its own.

## 7. Choices that are CireSnave's (each with a recommendation)

1. **Where the declaration lives.** Recommend: `[architecture]` in `.coderipper.toml`, with `file =` for a long one. Alternative: a
   separate `architecture.toml` always.
2. **Layers vs explicit edges.** Recommend both: `layer` (Fuel already thinks in tiers) for the common case, `allow` + `reason` for
   exceptions. Alternative: explicit edges only (verbose for 45 crates).
3. **Component granularity.** Recommend: crate-granular by default, module groups on request (`paths` with `**`). Alternative: always
   path-based.
4. **`volatility` as a closed set (`stable`/`volatile`) or free tags.** Recommend closed: the rules need two values and free tags invite
   drift.
5. **Waiver expiry.** Recommend: `until` required, an expired waiver is a finding. Alternative: allow a waiver with no end date for
   permanent seams (it would show as such in every run).
6. **Ratchet.** Recommend: the count of waivers may only fall (Fuel's rule for its allowlist) once a project opts in with
   `ratchet = true`. Alternative: no ratchet.

## 8. What this proposal does not do

Builds nothing. Fixes no thresholds (those belong to the P-class rules, which have none that depend on this file). Does not decide how a
declaration is first drafted. The Fuel numbers above are the Fuel lane's, relayed by the PM and read here from fuel `origin/main`, not
re-measured.
