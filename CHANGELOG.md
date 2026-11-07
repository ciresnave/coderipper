# Changelog

All notable changes to CodeRipper. Versions follow the portfolio rule: **every pushed change changes the version;
a breaking change changes the major version, and before 1.0 the major is the second number** (0.n.x). **0.6.3 was published
to crates.io on 2026-10-12** (the PM read `yanked=false` back); crates.io also lists 0.6.9, 0.6.11 and 0.6.14, none
yanked (read from the registry API on 2026-10-14), though their entries below still say "proposed"; 0.3.0 - 0.6.2 were not published, and none of these is
recorded here as tagged on GitHub: the earlier ones are the versions of `main` at each merge.

## 0.43.3 - 2026-11-06 (proposed, Fix)

**`zip` 9.4.0 to 9.4.1 in `Cargo.lock`** (a patch release of the archive reader the tool installer uses for the zip-packaged tools: gitleaks, lychee, zizmor, checkov on Windows). Found by the installed 0.39.0 portfolio sweep (`dependency-freshness`, Medium: "zip is locked at 9.4.0 and 9.4.1 is available"). No source change; the tool-install tests (`tools::` archive and download tests) pass on the new version.

## 0.43.2 - 2026-11-06 (proposed, Fix)

**The tests no longer pin the number of rules, so a new rule no longer conflicts with every other rule PR.** `tests/neutral_module.rs` read a hard-coded list and count of the neutral module's rules (22, 25, 26, 29...), `tests/coverage_cli.rs` a count of the covered neutral rules and of the claimed-but-unproven delegated ones, and two unit tests the number of rules registered. They now read the registry (`NeutralModule::describe`, `delegated_rule_ids`), with a positive control that the registry holds a known rule, and a test that a rule id is registered exactly once. A registered rule without fixtures still fails the proof test. No source change.

## 0.43.1 - 2026-11-06 (proposed, Fix)

**The reusable portfolio workflow installs coderipper 0.39.0** (`CODERIPPER_VERSION` in `.github/workflows/portfolio.yml`, from 0.20.1), the newest published release (crates.io, tagged `v0.39.0`). The 0.20.1 install printed the two MOD-002 "not registered" Info lines because it lagged 19 minor versions behind the rules the repository's own configuration names; 0.39.0 registers them. The caller pin in `ci.yml` (`portfolio.yml@<commit> # 0.20.1`) moves to the commit of this change in a second, fragment-only change once this one is on `main`, as for 0.20.1.

## 0.43.0 - 2026-11-06 (proposed, Breaking: a new default-on rule (API-004) and two opt-in rules)

**API-004, API-009 and ERR-004 are built** (the public-surface package, `src/module/neutral/surface.rs`, one `syn` walk over first-party Rust outside tests and binary roots; the neutral module's rules 20 to 22). An item counts as public when its visibility is a bare `pub` and every inline module above it is `pub`; the methods read are the `pub` methods of inherent `impl` blocks of types that are not private in the same file. **API-004** reports a `#[deprecated]` attribute with no `since` or no `note` (Low severity, Medium confidence). **API-009** reports a `pub enum` with variants, or a `pub struct` whose named fields are all `pub`, that has no `#[non_exhaustive]` and no doc comment calling it closed (Low, Medium). **ERR-004** reports a `pub fn` returning `Result` whose error is `String`, `&str`, a boxed trait object, `anyhow::Error` or `eyre::Report`, or `anyhow::Result` (Medium, Medium). Each has fixtures under `conformance/` and a `checked_today` record that says what is not seen (items reached through `pub use` or a module in its own file, macros, whether a note names a replacement, a type alias that hides the error).
**Measured on the corpus** (eight committed trees at 2026-11-06, after the review fixes: coderipper, auth-framework, Unpopped, PersistAnt, configurant, httpant, releaseant, synapse): API-004 reported 0 sites; `#[deprecated]` attributes exist in three of the trees and the ones read carry both fields, so the zero is the rule's reading and not a missed file (positive control: the unit tests and the fixture report a bare attribute), and API-004 is on by default. API-009 reported 1,619 sites (auth-framework 1,074) and ERR-004 reported 128 (in all eight repositories, 24 in coderipper itself, mostly `anyhow::Result` in internal helpers that are `pub` inside a private module tree), so **API-009 and ERR-004 are opt-in** (`coderipper check API-009`), as EFX-003 is.

## 0.42.0 - 2026-11-06 (proposed, Breaking: new default-on rule (EXT-003))

**SEC-005 (unsafe inventory, opt-in) and EXT-003 (trait impl stubs, default-on in `extended` and `portfolio`) are built** as neutral-module checks over Rust packages (`syn`, from `HEAD`), each with a seeded-defect fixture and a clean twin. **EXT-003** (Medium): a method of a non-test trait impl that calls `todo!`/`unimplemented!`, or is a short straight-line body that only says "not supported" or "not implemented". On the nine corpus repos: 2 findings, 1 clear stub (`BlockQ8_1::to_float`, `unimplemented!`) and 1 arguable (`AuthMethodEnum::refresh_token`, a refusal beside a `supports_refresh() -> false` flag); a first version also reported two methods that merely name the phrase in one arm or write a message, which is why branching bodies and `Display`/`Debug` impls are no longer read for the phrase. **SEC-005** (Info, opt-in: `coderipper check SEC-005`): per package, `unsafe` blocks, `unsafe fn`s and `unsafe impl` items are counted; a finding names the blocks with no `SAFETY` comment directly above, or tells a package with no `unsafe` to forbid `unsafe_code`. On the corpus: 73 findings in 8 of the 9 repos, 57 of them the forbid prompt and 16 packages with undocumented sites (88 of 104 blocks in `fuel-cpu-backend`, 168 of 226 in `fuel-cuda-backend`), which is why it is opt-in. COR-006 is not claimed. One reader, one pass.

## 0.41.1 - 2026-11-06 (proposed, Docs)

**Lane briefs for packages F (concurrency), G (panics, swallowed errors, unsafe) and H (repository files), and TYP-003 is now an alias of MOD-010.** The two records were one rule (a declared invariant type has private fields and a validating constructor); the catalog keeps one record, `MOD-010`, which answers to `TYP-003` (`aliases`), so allowlist entries and `coderipper check TYP-003` keep working. The catalog has 231 rules; `docs/rules/REMAINING.md` has 183 rows.

## 0.41.0 - 2026-11-05 (proposed, Breaking: a new default-on rule (CON-002) and three opt-in rules)

**CON-002, CON-003, CON-004 and REL-003 are built** (Package F, Rust concurrency; `src/module/neutral/concurrency.rs`, one `syn` walk over first-party Rust outside tests, the neutral module's rules 20 to 23). **CON-002** reports a std-style guard (`.lock()`, `.read()`, `.write()`, `.borrow()`, `.borrow_mut()`) that is held when an `.await` is evaluated in an `async fn`, `async` block or `async` closure, at the acquisition (a `let`-bound guard to its block's end or `drop`, a temporary to its statement's end; an awaited acquisition, an async lock, is not one). **CON-003** reports a call that resolves through the file's `use` items to `std::thread::sleep`, a `std::fs` function, `TcpStream::connect`, `reqwest::blocking`, `Command::new(..).output()/.status()` or `.block_on(..)` inside async code (not inside a non-async closure, which is what `spawn_blocking` takes). **CON-004** and **REL-003** (one reading, two ids) report a channel created with no capacity (`std::sync::mpsc::channel`, `unbounded_channel`, `futures` `unbounded`, `crossbeam_channel::unbounded`, `flume::unbounded`, `async_channel::unbounded`). All four are Medium severity and Medium confidence, with fixtures under `conformance/` and `checked_today` records that say what is not seen (helpers, macros, receiver types, byte-bounded queues).
**Measured on the corpus** (eight committed trees at 2026-11-05: coderipper, auth-framework, Unpopped, PersistAnt, configurant, httpant, releaseant, synapse): CON-002 reported 0 sites in all eight, so there is no sample to read and the rule is on by default at Medium, tested only by its unit tests and fixtures; CON-003 reported 35 (27 in PersistAnt, 5 in synapse, 3 in auth-framework; `std::thread::sleep` and `std::fs` calls in async functions, true to the rule's text, many of them start-up reads); CON-004 and REL-003 reported 15 each (a `std::sync::mpsc::channel` in thread code in five repositories, one tokio `unbounded_channel` each in PersistAnt and synapse). Because those two would put new Medium findings in five of eight repositories, **CON-003, CON-004 and REL-003 are opt-in** (`coderipper check CON-003`), as EFX-003 is; CON-002 runs in the profiles that run the neutral module. coderipper's own one site (a channel bounded by a byte budget in `src/module/external.rs`) is reported when the opt-in rules are asked for by id; the sweeps do not run them, so it carries no waiver.

## 0.40.0 - 2026-11-05 (proposed, Breaking: two delegated rules claimed (OPS-006, REL-005))

**OPS-006 (no secrets in configuration files) and REL-005 (probes and resource limits in manifests) are claimed by the delegates that already run, with no new detection.** Both mapping questions were answered by running the pinned tools on a fixture written for the purpose: the pinned gitleaks 8.30.1 reports a made-up password and a Stripe-shaped key in `config/app.yaml` (SEC-002 rules `generic-api-key` and `stripe-access-token`), and the pinned checkov 3.3.30 reports `CKV_K8S_8`, `_9`, `_11` and `_13` (liveness probe, readiness probe, CPU limits, memory limits) for a Deployment with none, and none of the four for a twin that sets them. **OPS-006** is the gitleaks scan of SEC-002, narrowed to configuration files (YAML, TOML, INI, properties, XML, JSON other than lockfiles, dotenv and docker-compose files) and reported under its own id (High; a secret in a configuration file is reported under SEC-002 and OPS-006 both). **REL-005** is checkov asked for those four checks over the tracked infrastructure definitions, Medium, one finding per failed check at the resource; the requests checks (`CKV_K8S_10`, `_12`) are not the rule's. Each has a seeded-defect fixture and a clean twin, proven against the real tools (`coderipper conformance --module delegated`: OPS-006 proven, REL-005 proven). Both need their tool installed (a gap otherwise); the Rust-language coverage count of claimed-but-unproven rules goes from 11 to 13 while no tool is installed. One reader, one pass.

## 0.39.0 - 2026-11-05 (proposed, Breaking: a new default-on rule)

**ARC-004 is built: infrastructure types kept out of policy signatures.** For each policy component of an `[architecture]` declaration (`role = "policy"`, or no role and `volatility = "stable"`), the rule reads the public signatures of its library with `syn` (a `pub` function, a `pub` method of a public type, a public trait's items, `pub` fields, enum variant fields, public aliases, constants and statics) and reports the types they name that resolve, through the module's `use` paths, to a declared infrastructure namespace or name (`[architecture.volatile]`, or the new `[arc004] patterns`) or to a crate of a volatile, adapter or infrastructure component. A public alias of such a type is looked through. One finding per signature at Medium severity and medium confidence, subject `<component>:<item>`; a deliberate exception is an allowlist entry with a reason. A repository that declares no policy component or nothing as infrastructure gets "not run", never clean. Default on in `extended` and `portfolio`; a repository without the declaration is unaffected. Syntactic: glob imports, macro-generated items and aliases in other crates are not seen. Fixtures: `conformance/ARC-004`.

## 0.38.0 - 2026-11-05 (proposed, Breaking: a new default-on rule)

**ARC-011 is built: signs of a distributed monolith, as far as one crate shows them.** For each workspace crate, a pair of top-level modules that **each name the other** (the edges `MOD-002` reports) **and** were changed together is reported (Info, low confidence, default on in `extended` and `portfolio`, never gating): at least 3 commits touched a file of both (`[arc011] min_commits`) and they are at least 60% of the commits that touched either (`min_share_percent`), over the newest 500 non-merge commits of the crate (`window`; a commit touching more than 25 files is set aside, `max_files`). Two modules that cannot be understood apart and are never edited apart are one module split in two; the finding names both edges with file and line and the newest commits, and says to merge them or to decide a direction and remove the back reference. A service graph across repositories, shared schemas and call fan-out are not read, and only direct mutual references between two top-level modules are. Measured on eight of our repositories (the committed trees of coderipper 3f5eb81, auth-framework 881c696d, Unpopped dcc3174, PersistAnt 32ccc43, configurant dd7a331, httpant 466d8ad, releaseant d773059, synapse 40ec84f; 25 pairs of modules that refer to each other were compared): 0 findings at the defaults, 0 at 2 commits and 50%, and at 2 commits and 30% 1 in Unpopped (`kernel` and `launch`, 7 of 17 commits, plausible), 1 in configurant (`error` and `schema`, 6 of 16 commits, probably not a defect: an error module and the modules that return it refer to each other by design) and 1 in synapse (`sender_auth` and `types`, 3 of 9 commits, probably not a defect: a shared `types` module is a hub). Of the three, one is plausible and two look like modules that refer to each other by design, so the thresholds stay where they are and the rule stays Info. A false-positive rate is not claimed. Squash-merge history makes commits coarse and renames are not followed. Allowlist the subject `<crate>:<a>,<b>` with a reason. Conformance fixtures: `ARC-011/defective` and `clean`. The conformance runner now also clears the worktree before it copies the fixture itself after a `.history` step, so a fixture file that keeps the size and timestamp of the last step's is not committed as unchanged.

## 0.37.4 - 2026-11-05 (proposed, Docs)

**`docs/rules/REMAINING.md`: the catalog rules that have no check yet.** One row for each of the 184 catalog rules that no check of any kind covers (232 catalog rules, 20 `BuiltinCheck` rows, 19 in-process module checks, 9 delegated-tool rules, 48 built by some route), grouped by what a lane can do without a model, with the input each needs, the closest check to copy, a size guess, an advisory flag and a patent-sweep risk. It also records why the package-e brief's count (catalog minus `BuiltinCheck::ALL`, 212) was replaced by the accurate one, and recommends the next three packages. Documentation only; no behaviour changes.

## 0.37.3 - 2026-11-05 (proposed, Fix)

**Dependencies updated to their newest versions** (`cargo update`; `Cargo.lock` only, no manifest requirement changed): cc 1.10.0 to 1.11.0, crc32fast 1.5.3 to 1.5.4, js-sys 0.3.106 to 0.3.107, log 0.4.34 to 0.4.35, toml 1.1.8 to 1.2.0, toml_writer 1.1.3 to 1.2.0, ureq 3.5.0 to 3.5.1, ureq-proto 0.6.5 to 0.7.1, wasm-bindgen 0.2.129 to 0.2.130 (with wasm-bindgen-macro, wasm-bindgen-macro-support and wasm-bindgen-shared), winnow 1.0.4 to 1.0.5 and zip 9.1.3 to 9.4.0. These are the 14 packages the repository's own `dependency-freshness` rule reported as behind on 2026-11-05; none was held back.

## 0.37.2 - 2026-11-05 (proposed, Fix)

**MOD-004 sets aside hub fields.** A field that more than half of a large type's (eight or more) clustered methods use joined every method into one cluster, so big dispatcher types could never be reported (Fuel's `VulkanBackend`). It is now set aside before grouping and named in the finding. Over every package of the nine corpus repos (result cache off): 8 findings before, 13 after, 7 new and 2 dropped; the 7 are read in `docs/rules/mod.md` (3 read as mixed jobs, 4 arguable). A single-field type (Fuel's `Tensor`) cannot be split by field and stays unreported; that limit is now documented. Still Info, low confidence, advisory.

## 0.37.1 - 2026-11-05 (proposed, Fix)

**Registering a check no longer edits the same lines as every other new check.** The rows of `builtin_checks!` after the six legacy ones are sorted by id (a test enforces it), the check types re-exported from `checks` and the `mod`/`pub use` pairs in `checks/architecture` are one per line in name order, and the README's Rust rule rows are in id order (a test checks that too, and that each built-in check has a row). Two changes that add different rules now touch different lines instead of conflicting at the end of each list. The set of checks and their results are unchanged; the non-legacy checks now run in sorted order, so under `extended` findings of different rules can appear in a different order in the report (measured on this repository at 22bb19e: the same 14 findings, the same exit code 3, and the sorted output is byte-identical; only the position of two MOD-002 findings moved). The `classic` output is unchanged. Because the rows are now inserted in order, the implicit discriminants of the public `#[non_exhaustive]` enum `BuiltinCheck` move when a row is added; `Cargo.toml` allows cargo-semver-checks' `enum_no_repr_variant_discriminant_changed` lint for it, and `BuiltinCheck::ALL` is the order to rely on.

## 0.37.0 - 2026-11-04 (proposed, Breaking: a new default-on rule)

**MOD-009 (long reach-through chains) is built.** From the package's own structs, it finds the fields and the one-argument-free accessors that lead from one struct to a different one, and reports a method-call or field-access chain whose consecutive links each follow such a hop once there are at least 4 of them, e.g. `r.b.c.d.e`. Builders and iterator pipelines are not chains. Advisory: Info, low confidence, never gating; default on in `extended` and `portfolio`, not `classic`. Set `[mod009] min_hops = 3` in the repository's own `.coderipper.toml` to see more (at least 2); the note prints how many chains there are at each depth. Allowlist the chain text with a reason. Matching is by name, so a configuration tree read on purpose reads as a chain. Measured 2026-11-04 on committed trees with the depth histogram the check prints (every package of each repository): coderipper 3f5eb81: 1 chain of 3 hops; auth-framework 881c696d: 193 of 2 hops and 37 of 3; synapse 40ec84f: 56 of 2 and 5 of 3; configurant dd7a331: 15 of 2; PersistAnt 32ccc43: 2 of 2; releaseant d773059: 2 of 2; Unpopped dcc3174 and httpant 466d8ad: 0 of 2 or more. The deepest chain anywhere is 3 hops, so the default of 4 reports 0 findings on every repository, and a seeded chain of 4 is reported (`conformance/MOD-009/defective`). All 43 chains of 3 hops were read: 37 are `parent.config.<section>` or `builder.config.<section>` in auth-framework's sub-builders (a builder that writes one section of a configuration it holds), 5 are `config.smtp.password` and `config.imap.password` in synapse (a configuration tree read on purpose) and 1 is `concrete.location.start_point` in coderipper's parser of a tool's JSON report; none is a traversal that one would ask to be hidden behind a method, which is why the default is 4 and `[mod009] min_hops = 3` is the way to see them. Patent read first: no hit reads on the rule (`docs/PATENT-SWEEP.md`).

## 0.36.0 - 2026-11-04 (proposed, Breaking: a new default-on rule)

**ARC-009 is built: architectural drift since the last release.** For each workspace crate, the module cycles (the count `MOD-002` reports) and the public surface (the sum `ARC-006` budgets) are read at `HEAD` and at the base, the newest `v<version>` tag that is not `HEAD` or `[arc009] base`, from the git object store with no checkout. **Low** when the cycles grew; **Info** when the surface grew by more than 25% and at least 10 items (`[arc009] surface_growth_percent`, `surface_min_items`). A repository with no release tag and no `base` gets "not run" in the coverage, never clean. Measured on eight of our repositories: 1 finding (httpant-server, a real new cycle). Instability margins and violations of declared layers over time are not compared yet. Allowlist `<crate>:cycles` or `<crate>:surface` with a reason.

## 0.35.1 - 2026-11-04 (proposed, Fix)

**reachability: an exported item with no in-repo caller is Info, and use in a doc example counts as use in tests.** On the fresh 0.33.6 Rig run (rig-core, 38 judged findings: 0 false, 0 true, 38 arguable) 24 of 28 Medium findings were undocumented `pub` constants (provider model ids) that downstream crates import. A `pub` item reachable through `pub` modules is now Info whether or not it is documented (new "exported" class; documented items keep their own wording and stay the roots of the "what it reaches is live" rescue, undocumented ones are not roots). A name that appears only in a fenced block of a `///` or `//!` comment is graded like one named only in tests, and the summary now says "only used in this crate's tests or documentation examples" instead of "zero callers". A `pub` item in a private module, and a dead private item, stay Medium. Matching is still by bare name. Recall is unmeasured: no true positive has been seen on this corpus.

## 0.35.0 - 2026-11-04 (proposed, Breaking: a new default-on rule)

**MOD-006 is built: divergent change, from the committed history.** For each source file at `HEAD`, `git log` over the newest 500 non-merge commits (`[mod006] window`) is read, the commits that touched the file are joined when they touched a source file in common, and the file is reported (Info, low confidence, default on in `extended` and `portfolio`, never gating) when two or more groups of at least 3 commits (`min_commits`) result: it is edited together with sets of other files that never overlap. A commit touching more than 25 files (`max_files`), a file that over a fifth of the commits touch, a document edited with the file and a registry (`lib.rs`, `mod.rs`) are set aside and counted in a note. Measured on eight of our repositories: 1 finding at the defaults, 8 with `min_commits = 2`, 75 with `min_commits = 1`. Squash-merge history makes commits coarse; renames are not followed. Conformance fixtures can now carry a `.history` directory of numbered steps, each committed in order before the fixture. To accept a finding, allowlist the file path with a reason.

## 0.34.0 - 2026-11-04 (proposed, Breaking: a new default-on rule)

**ARC-005 (depend toward stability) is built.** For each component of the dependency graph that `ARC-002` already reads (workspace crates by `path`, and the module groups of a crate declared in `[architecture]`; with no declaration, each workspace member), it computes the instability `I = Ce / (Ca + Ce)` and reports an edge `A -> B` when `I(B) - I(A)` reaches the margin, ranked by the margin. Advisory: Info, low confidence, never gating. The default margin is 0.3; set `[arc005] min_margin = 0.5` in the repository's own `.coderipper.toml` to report less (a value outside `(0, 1]` is an error). An edge to a component declared `volatility = "volatile"` and an edge inside a cycle (`ARC-002`'s) are not reported. Default on in `extended` and `portfolio`, not `classic`. Allowlist the subject `a->b` with a reason. Instability counts neighbours, not change, so a frozen dependency can be flagged: the finding says so. Measured 2026-11-04 on committed trees (coderipper 3f5eb81, auth-framework 881c696d and releaseant d773059: one crate, 0 edges, so nothing to judge; Unpopped dcc3174: 4 components, 5 edges; PersistAnt 32ccc43: 9, 18; configurant dd7a331: 6, 7; httpant 466d8ad: 6, 9; synapse 40ec84f: 9, 17; Fuel 627c2bb7c: 45, 166): 0 findings at the default margin 0.3 in every repository, and 0 at a margin of 0.05 in the five small multi-crate ones (Fuel and the single-crate repositories were not re-run at 0.05). The same binary reports 1 finding on the seeded fixture `conformance/ARC-005/defective`. A false-positive rate is not claimed, because there was nothing to read: the crate-level reading is quiet on a layered workspace, and the module-level reading needs declared module groups. Patent read first: no hit reads on the rule (`docs/PATENT-SWEEP.md`).

## 0.33.9 - 2026-11-04 (proposed, Fix)

**ci.yml calls portfolio.yml at the commit that installs 0.20.1** (was `ee9c395`, 0.9.24), so this repository's own portfolio job runs the newest published release rather than one from before many fixes.

## 0.33.8 - 2026-11-04 (proposed, Fix)

**portfolio.yml installs the newest published release.** `CODERIPPER_VERSION` was 0.10.0 while crates.io has 0.20.1, so the scheduled release-drift check was red and every fix since was unreachable from callers. Pinned to 0.20.1. Callers pin `portfolio.yml` by commit SHA, so `ci.yml` is repinned to the merge commit of this change in a follow-up (a SHA on main exists only after the merge). The 0.21 to 0.33 line is not published; a publish batch repins again.

## 0.33.7 - 2026-11-04 (proposed, Docs)

**docs/RIG-0E.md: the first precision sample of a real third-party workspace (0xPlaygrounds/rig at 44f8d5b8).** 329 findings judged (up to 30 per rule); `reachability` is the weak rule (23 of 30 false), and the defects it found in CodeRipper are scheduled as separate fixes. Read-only; nothing was posted upstream.

## 0.33.6 - 2026-11-04 (proposed, Fix)

**reachability no longer reports everything behind a documented library entry point.** A documented `pub` item of a library (the class E8 grades Info as library API) is the interface: callers outside the repository reach whatever it reaches. After the `pub` narrowing nothing inside the crate constructs it, so rustc called it dead and every private helper, constant and handler only it reaches, and the advice "remove or revive those first", followed in a cascade. Those items are now live: the identifiers of each such entry point are roots of the reachability walk, the entry points themselves are still reported at Info, and the note on the run says how many items this set aside. An undocumented `pub` item is not declared interface and keeps its cascade (a test covers both, and a dead private function beside a documented entry point). Measured on 0xPlaygrounds/rig at 44f8d5b8, package `plugins/rig-steel`: 70 findings (56 Medium, 14 Info) before, 2 (both Info, `SteelPlugin` and `RUN_STEEL`) after. Found by the first third-party sweep (docs/RIG-0E.md, where 23 of 30 sampled reachability findings were false); the cascade is the mechanism behind every case I traced (a Bevy plugin's `build`, a tool's `call`), not a missed `add_systems` registration. A documented entry point that another documented entry point reaches (a constant or method its plugin's code uses) is no longer reported either: "zero callers" was false of it, and only the outermost entry points stay at Info. Matching stays by bare name, so a documented entry point whose name another dead item shares can hide that item.

## 0.33.5 - 2026-11-04 (proposed, Fix)

**lint-suppression says that a comment beside the attribute is not a reason.** The rule is unchanged (only `reason = ".."` and `#[ignore = ".."]` count, as documented), but a reader who had written `#[allow(dead_code)] // why` was told only "has no reason". The finding's detail now names the attribute form and says a comment is not one. Found on the first third-party sweep (docs/RIG-0E.md), where the reviewer first took three such findings for false positives; the PM ruled them correct.

## 0.33.4 - 2026-11-04 (proposed, Fix)

**MOD-001 no longer fails a run with an invalid finding when a large public surface has a single gap.** A share under 1% (1 of 142 undocumented items, found on rig-tools) was written `(0%)`, and the bare word `0` made `Finding::validate` take the summary for an absence claim without a positive control, so the finding was dropped with an error. It now reads `under 1%`. Found by the first sweep of a third-party workspace (docs/RIG-0E.md).

## 0.33.3 - 2026-11-03 (proposed, Docs)

**CONTRIBUTING.md, docs/DIRECTORY.md, docs/FILE-FORMATS.md and the logo; the README no longer says CodeRipper audits only Rust.** The README, the
`lib.rs` crate docs and the `--help` summary now say what is true: CodeRipper audits a project in any language it holds, started with Rust, and
reports a rule that does not cover a project's language as *not run here*. The three new documents were checked field by field against
`src/finding.rs`, `src/catalog.rs`, `src/allowlist.rs` and `src/conformance.rs`. The logo master is `docs/assets/coderipper-logo-claw.png`
(1600x1600) with a `.webp` export that the README shows. The `Cargo.toml` description still says "a Rust project" (a lane may not edit it): the PM
is asked to change it when the version is stamped.

## 0.33.2 - 2026-11-03 (proposed, Fix)

**RDB-008 commented-out code no longer reads layout notation as code.** A run of whole-line comments now counts as commented-out code only when at
least one line is strongly code-shaped (ends in `;`, `{` or `}`, or opens with a statement keyword). Lines that merely end in `(`, `,` or `)` are how
dispatch tables, layout algebra and shape comments read too, so a run made only of them is documentation. The check stays opt-in
(`[rdb008] commented_out_code = true`). Measured on NVIDIA/cutlass at v4.8.1 (7978 files at HEAD, opt-in on): 49 files before, 29 after; of 7 dropped files
sampled, 6 were notation or documentation and 1 (`python/cutlass_library/generator.py`, tuple-literal rows) was real commented-out code that is now missed.

## 0.33.1 - 2026-11-03 (proposed, Fix)

**Parallel-lane preparation, no behaviour change.** `src/builtin.rs` is now one table (`builtin_checks!`): a new check is one row, and the registration order, the proven set and the `classic` membership are derived from it, so five test files no longer hard-code counts (`catalog`, `coverage`, `coverage_cli`, `conformance`, `external_module` now derive them from `BuiltinCheck::ALL`). `NOT_IN_CLASSIC` is gone; `BuiltinCheck::in_classic` replaces it, and a unit test pins the `classic` set to the six legacy checks. `tools/stamp-version.py` stamps a version onto fragments in `changelog.d/` and `patent-sweep.d/`, so lanes stop colliding on `CHANGELOG.md`, `Cargo.toml` and `docs/PATENT-SWEEP.md`. One-page briefs for the next rule packages are in `docs/lanes/`.

## 0.33.0 - 2026-11-03 (proposed, Breaking: a new default-on rule)

**ARC-008 is built: indirection that earns nothing, advisory** (PM task 2026-11-01, ARC and MOD families; design note 3.10: shapes only). A new Rust rule on the package, default on in `extended` and `portfolio`, not in `classic`; Info at low confidence only, never gating.

**Two shapes, and the check judges neither.** (1) **A trait with one implementation and no test double**: a trait of the package with at least one method, implemented exactly once in the non-test code, never implemented in test code (`#[cfg(test)]` items and test files by name), the implementing type not named like a double (`Mock`, `Fake`, `Stub`, `Dummy`, `Spy`, `InMemory`, `Noop`, `Null`, `Test`). Not reported: a `pub` trait of a library (consumers may implement it: the `ARC-006` public-surface reading), a supertrait of another trait, a blanket-implemented trait, a trait with no method. (2) **A chain of forwarding types**: a struct with one field (through `Arc`, `Rc`, `Box` or a reference) whose every method taking `self`, at least two, is one call to that field with the same arguments; when the type of its field is such a struct too, the two are a chain, reported once at the first type. Not done: the design's chains of near-identical data mappers and long call paths before any I/O, which need data flow.

**Measured, and said plainly.** On seven of our repositories on 2026-11-03 the rule finds **nothing**: 0 in all seven. With the public exemption switched off, for the measurement only, the shape finds 9 traits: coderipper's `Cargo` (implemented by `RealCargo`) and `Github` (`GhCli`), five in auth-framework (`AuthorizationStorage`, `ConfigWatcher`, `AuthRouterExt`, `ExtractProfile`, `FederationOrchestrator`) and two in synapse (`ConnectivityConfigExt`, `StateStore`); every one is a `pub` trait of a library, a port or an extension point, which is why public traits are exempt. No chain of forwarding types exists in any of the seven. So there is no corpus rate and no true positive found on real code; the shapes are shown by the unit tests and the conformance fixtures, which is all that can be said.

**Scope, said plainly.** Syntactic and by name: two traits or types of one name share their facts, an implementation made by a macro is not seen, and a test double written in `tests/` (an integration test) is not read, so a non-public trait whose only double is there would be reported (the text says allowlist it with the reason). Text only: no change, patch or list of options is offered (the `MOD-003` patent watch).

**Checked.** 12 unit tests (one implementation and no double is a shape; a second implementation, a double in a `cfg(test)` module and a double by name each end it, with the plain form as control; a blanket impl, a marker trait, a supertrait and public API are not reported; an implementation in a test file is a double; a Service-Repo-Adapter chain is reported once at its first type; one forwarding type alone, a type with a method of its own and a type with one method are no chain; methods written in another file than the struct still count; findings are Info at low confidence and name the shape; an unparsable file is an error). Conformance fixtures `conformance/ARC-008/{defective,clean}`.

**Patent sweep for the 0.33 line** (`docs/PATENT-SWEEP.md`): idea 41, two queries (Q88, Q89), one patent read in full (hit 75). **It is active and was in no earlier sweep, and it bears on `ARC-001`'s layers more than on this rule**: US12190107B2 (Huawei, adjusted expiration 2041-05-15 per its own Google page) claims analysing the files of modules for module call information against a user's layering rule and call rule and displaying violations in a layered architecture presentation interface with coloured or formatted arrows. The element chart is in row 75: the layered presentation interface and the arrow display are missing from `ARC-001`, `ARC-001b` and `ARC-002` in each independent claim. A watch forbids adding a layered diagram, coloured arrows or per-pair call counts in such a view.

## 0.32.0 - 2026-11-03 (proposed, Breaking: it changes what gates by default)

**`reachability` grades documented library API and test-only items as Info (E8, option B, approved by the PM 2026-11-03).** Every finding of the legacy `reachability` check was Medium, and Medium gates under `--deny medium`. Two classes of what it finds are not a defect of the code, and are now **Info**; everything else stays **Medium**. This is a change to the default gating of an existing check, so it is Breaking (the second number).

**Library API.** The item is written `pub`, is reachable through `pub` modules (or a `pub use`) from the library root, and carries documentation (`///`, `#[doc = ..]`, `#[doc(..)]` other than `hidden`, or the `//!` in a module's own file): the public surface `ARC-006` and `MOD-001` read. Its callers are outside this repository and removing it is a breaking change. An undocumented `pub` item, or a documented one in a module no `pub` path leads to, stays Medium. **Test-only.** The item's name appears in the crate's own test code (a `#[cfg(test)]` item, or a file named `tests.rs`, `test.rs`, `*_test.rs`, `*_tests.rs`) which the library build does not compile, and the library build finds no live caller (a caller that is itself dead does not count, so the name may also appear in library code nothing reaches). Matching is by bare name, so a dead item that shares its name with something a test calls is Info too (still reported, one grade lower; the text says so). `[reachability] public_api` declarations and `[[allow]]` entries work as before.

**Not done (classes of the evidence file `C:\Projects\.lane-state\e8-reachability-evidence.md`).** A type reached only through a field or a return type, a name collision between callers, and a chain of dead roots (classes a, d, f) are not handled; neither is the `unreachable_pub` lint.

**Measured.** On the committed trees of five of our repositories on 2026-11-03: 777 findings, every one Medium before this change; now **536 are Info as documented library API** (releaseant 2, PersistAnt 1, synapse 52, auth-framework 481), **58 are Info as test-only** (synapse 6, auth-framework 52) and **183 stay Medium** (synapse 33, auth-framework 150; configurant has none), a 76% reduction of what gates under --deny medium. 10 library-API rows and 14 test-only rows were read: the library-API rows are documented pub items (the declaration and the line above were read); the test-only rows include private functions called only by another dead function and by tests, which is what showed that the first wording of the detail (the name appears nowhere in the library) was wrong for dead chains; it was corrected to no live caller. Whether some of the 58 are name collisions was not measured (matching is by bare name, said in the finding).

**Checked.** 4 unit tests in `reachability/classify/tests.rs` (a documented public item is library API and the same item not written `pub`, or on another line or file, is plain; a name in test code is test-only and library API wins over it; the API sites are the documented items reachable through `pub` modules and not an undocumented one nor one in a private module, and a package without a library has none; test identifiers come from `#[cfg(test)]` items and test files by name and not from the library) and 4 end-to-end tests in `tests/reachability.rs` through `rustc` (documented API is Info and an undocumented one and a private dead one stay Medium; a documented item in a private module stays Medium; an item named in `#[cfg(test)]` code with no live caller is Info and a dead one named nowhere stays Medium; a test file by name makes the same item test-only). **Mutation check, one per rule:** with the documented-and-public condition removed (every `pub` item treated as API) the unit test and two end-to-end tests fail; with the test identifiers ignored the unit test and two end-to-end tests fail. Both mutations were applied by `sed`, run, and reverted.

**Patent sweep for the 0.32 line** (`docs/PATENT-SWEEP.md`): idea 40, two queries (Q86, Q87), three patents read (hits 73 and 74 and a re-seen row). **One is active and was missing from the 0.9 sweep**: US10521203B2 (Fujitsu, anticipated expiry 2038-03-01), which finds unused symbols by compiling a modified copy of a source file and reading what the compiler reports. It concerns the pipeline `reachability` has had since 0.9, not E8; the element-by-element chart of its three independent claims is in rows 73 (element (a) is missing from our pipeline in each), the PM was told and decided to ship and record. A watch stops work if a continuation claim lacks (a).

## 0.31.0 - 2026-11-03 (proposed, Breaking: a new default-on rule)

**MOD-008 is built: pass-through wrappers, advisory** (PM task 2026-11-01, ARC and MOD families; design note 3.6, which said "measure first"). A new Rust rule on the package, default on in `extended` and `portfolio`, not in `classic`; Info at low confidence only, never gating.

**What it reports.** A free function or an inherent method whose whole body is one call that forwards all its parameters, in order, unchanged: `g(a, b)`, `self.g(a)` (an alias for another method of the type), `g(&self.x, a)`; the call may be inside `Ok(..)`, `Some(..)`, `?`, `.await` or `.into()`. At least one parameter besides `self` must be forwarded. One **Info** finding per wrapper naming the pair and the reasons a layer may be worth keeping.

**What it does not report, learned from the corpus.** The first definition also counted `self.field.method(args)` and constructors. On seven of our repositories it flagged **228** items, 198 in one. A random sample of 24 of those 198 showed what they were: documented methods of a facade type forwarding to its managers (`AuthFramework::register_email` to `self.mfa_manager.email.register_email`), accessors hiding a field (`add_rule` calling `self.rules.push`), and newtype or enum-variant constructors (`Self(roles)`, `Self::DeviceFlow(error)`). Those hide something (the field), or build a value, so they are not what the design's "adds interface without hiding anything" means; they were removed from the shape, one class at a time. The 52 left after the field calls were excluded were read line by line: 15 were constructors (then excluded too), and of the remaining 37 coderipper's own 2 were removed from the code, leaving 35 that are aliases and convenience layers (`google_client_id` calling `client_id`, `constant_time_eq` calling `secure_string_compare`, `builder(..)` calling `Builder::new(..)`, `seconds(s)` calling `Duration::from_secs(s)`, `check_username` calling `validate_username`), which do match the definition. Whether such a layer earns its place is the reader's call, which is why the finding is Info at low confidence and says so.

**Measured.** After the shape was narrowed: 0 findings in four repositories (coderipper, PersistAnt, configurant, httpant) and 35 in three (auth-framework 30, synapse 4, releaseant 1). The two wrappers coderipper had (`acquire_for_session` calling `acquire`, `shown_path` calling `first_line`) were removed in this change, not waived: the one caller of the first now calls `acquire` and keeps its comment, and the three callers of the second call `first_line`, whose doc already says it is safe to print.

**Scope, said plainly.** Syntactic and per file: the callee is named, not resolved; a macro body is not read; generics, `where` clauses and `async`-ness of the callee are not compared; a parameter that is a pattern makes it not a pass-through. Text only: no change, patch or list of options is offered (the `MOD-003` patent watch).

**Checked.** 10 unit tests (all arguments forwarded in order is a wrapper; an alias method and a free call with a field are wrappers and a call on a field is not; `Ok`, `Some`, `?`, `.await`, `.into()` and `return` are looked through; a changed, reordered, added or dropped argument is not a pass-through, with the plain form as control; a function with nothing to forward is delegation; trait impl methods and the crate-root facade are exempt and the same function elsewhere is not; test items are not read and functions are counted; one Info finding per wrapper; an unparsable file is an error; a tuple-struct or variant constructor is not a wrapper). Conformance fixtures `conformance/MOD-008/{defective,clean}`.

**Patent sweep for the 0.31 line** (`docs/PATENT-SWEEP.md`): idea 39, two queries (Q84, Q85), two patents read in full (hits 71 and 72). Nothing reads on the rule; one is active (US20140165035A1, Microsoft, to an estimated 2033), about expanding and reducing source code around a refactoring operation, which this rule does not do.

## 0.30.0 - 2026-11-03 (proposed, Breaking: a new default-on rule)

**MOD-001 is built, in part: a small, documented public surface, advisory** (PM task 2026-11-01, ARC and MOD families; the public-surface half of the design note's section 3.9). A new Rust rule on the package, default on in `extended` and `portfolio`, not in `classic`; Info and Low only, never gating.

**What it reports.** One finding per library package that has undocumented public items, on the public surface `ARC-006` reads (declarations reachable through `pub` modules and `pub use` leaves, plus the public inherent functions of public types): the items that carry no documentation (no `///`, no `#[doc = ..]` or `#[doc(..)]` other than `hidden`, and, for `pub mod x;`, no `//!` in `x.rs`), with **both counts** (the design's census distinction: the public *items* and the *members* of the public types), the share, and the first places. A `#[doc(hidden)]` item is in neither list. **Info** for any gap, **Low** from 50% of at least 20 documentable items, a chosen figure that no repository of ours is near. The size of the surface is stated, not judged (a declared ceiling is `ARC-006`'s job).

**Measured, and read.** On committed trees on 2026-11-03: coderipper 0 of 233 public items undocumented (the first run flagged 22 `pub mod` declarations, which are documented by the `//!` inside their own files, as `rustc`'s `missing_docs` accepts; the check now reads the module file, and nothing in coderipper was changed to make it pass), PersistAnt 0 of 97, releaseant 1 of 200, configurant 26 of 226 (11%), auth-framework 380 of 4,762 (8%), synapse 237 of 1,152 (21%). Sixteen of the findings in auth-framework and configurant were read in the source: every one is a public item with no `///` above it (a plain `//` comment is not documentation), so the findings are true gaps. No false-positive rate is claimed beyond that.

**Not done here, and said so.** The part of the design that folds the reachability check's precision (E8: types reached through fields, the public API of a library root as a root, test-only callers, name collisions, non-function items, dead-root chains) is a change to the `reachability` check and is a separate piece of work; so are the delegated `unreachable_pub` and `missing_docs` rustc lints, which need a build (this rule reads the source and sees every item's doc attribute at once). A public item that nothing uses stays `reachability`'s finding.

**Scope, said plainly.** Syntactic: an item is public when every module above it is `pub` or it is re-exported; macro-generated items, doc comments made by a macro, `#[path]` modules and `cfg`-gated items are not resolved; trait items and enum variants are not asked for documentation; a binary-only package offers no library API and is noted. The finding names counts and places and offers no change (the `MOD-003` patent watch).

**Checked.** 9 unit tests in `public_surface/tests.rs` (documented and undocumented told apart; `#[doc(hidden)]` and non-public items left out, with a control; public inherent functions of public types counted and private types not; an item in a private module not part of the surface; a documented surface has no finding and a gap among ten is Info; most of a large surface is Low and a small one never; both counts and the first places stated; a binary-only package; a file module documented by its own `//!`). `api_budget.rs` gained the documentation facts; its 13 tests still pass. Conformance fixtures `conformance/MOD-001/{defective,clean}`.

**Patent sweep for the 0.30 line** (`docs/PATENT-SWEEP.md`): idea 38, two queries (Q82, Q83), one application read in full (hit 70, abandoned). Nothing found reads on the rule; the nearest, US20140123110A1, scores quality from compiling and testing, and US8656365B2 (row 55) starts from the API's documentation as input.

## 0.29.0 - 2026-11-03 (proposed, Breaking: a new default-on rule and a new opt-in setting)

**MOD-010 is built: invariant-bearing types keep fields private, advisory** (PM task 2026-11-01, ARC and MOD families; on the `invariant_types` key of a component, which the 0.21 declaration already parses). A new Rust rule on the repository, default on in `extended` and `portfolio`, not in `classic`; Low and Info only, never gating.

**Declared half.** A type named in the `invariant_types` of a component, found by name in the `src/` of that component's crates (test items skipped): **Low** when it has a `pub` field; **Info** when it derives `Default` or `Deserialize` (without `#[serde(try_from = ..)]` or `from`) while it has a validating-looking constructor, because the derive builds a value without the check; **Info** when the name is a struct in none of the component's crates (stale declaration, with a positive control). **Not measured:** no repository of ours declares `invariant_types` yet, so there is no corpus rate for this half; it fires only for types a project names, which is why it is on by default.

**Inferred half, opt-in** (`[mod010] inferred = true` in the repository's own `.coderipper.toml`; a system or user file does not turn it on; an unknown key under `[mod010]` is an error that names the file). **Info** at low confidence for a `pub` struct with a validating-looking constructor (an inherent `new`, `try_new`, `parse` or `from_*` that takes an argument, returns `Result<Self, _>` or `Option<Self>` and rejects directly: `Err`, `None`, `bail!` or `ensure!`, `None` counting only for an `Option` and `?` not counting) and a `pub` field. The first definition counted `?`, took zero-argument constructors and counted `None` as a field value: 22 findings on three repositories, of which the configuration loaders (`from_env`), state builders (`new()` calling `Foo::new()?`) and `None` initial values were plainly not guards; those were removed one by one. **What is left, read constructor by constructor: 14 findings; 6 guard an invariant (a SIWE message, a SPIFFE ID, a permission string, an operand descriptor, a structure key, an accumulator pair), 4 are wire-format parsers whose invariant is debatable (the TACACS+ header and three reply bodies), and 4 are plain data records (a passkey method whose only `Err` is for a disabled feature, a provider profile parsed from a token, two benchmark records).** That is 4 to 8 false positives of 14, over what a default-on rule may have (CireSnave, 2026-10-25), so this half is off unless asked for, as `RDB-008`'s commented-out-code half is.

**Scope, said plainly.** Syntactic and matched by name; a derive made by a macro, a manual `impl Default` or `Deserialize`, a builder and an `unsafe` constructor are not seen. The finding names the fields and nothing else: no change, patch or list of options is offered (the `MOD-003` patent watch, rows 64 and 65 of `docs/PATENT-SWEEP.md`). `ComponentDecl`'s `invariant_types` was the last key of the declaration no rule read, so its `#[expect(dead_code)]` is gone.

**Checked.** 13 unit tests (fields, derives and constructors read; a wrapping constructor, `?` alone, a missing argument and `None` as a field value are not validating, each with a positive control; serde `try_from`; test items skipped; a declared type with a `pub` field is Low and with private fields clean; `Default`/`Deserialize` derives are Info; a stale name is Info with a control; the inferred half needs a public type, a public field and a checking constructor and is off unless asked for; a declared type is not also reported by the inferred half; the setting is read from the config text and a broken file is off; one end-to-end run on a real git workspace). Conformance fixtures `conformance/MOD-010/{defective,clean}`.

**Patent sweep for the 0.29 line** (`docs/PATENT-SWEEP.md`): idea 37, two queries (Q80, Q81), one patent read in full (hit 69, expired). Nothing found reads on the rule; the two queries reached only adjacent material.

## 0.28.0 - 2026-11-03 (proposed, Breaking: a new default-on rule)

**MOD-002 is built: no cycles between modules, advisory** (PM task 2026-11-01, ARC and MOD families; the same graph and set-finding as `ARC-002`, at the grain of a module). A new Rust rule on the package, default on in `extended` and `portfolio`, not in `classic`; Low and Info only, never gating.

**What it reports.** In the module graph of a package (`modgraph.rs`: the tracked `.rs` files under `src/` reached from `lib.rs`/`main.rs` through `mod`, test items skipped), the strongly connected sets of two or more modules. An edge is a path written in an item (or a `use`) in one module that names an item inside another.

**The design question (2 of the design note), answered as it proposed:** a child naming its parent (`use super::X`, `self::`) is idiomatic and would make nearly every module a cycle with its parent, so it is not an edge; a module reaching back to one that contains it counts only when written from the crate root (`crate::a::X`); the crate root takes no part (`crate::Error` or `crate::Result` in `lib.rs` is the crate's façade); the `use` that builds a `pub use` re-export is the interface, not a dependency. **Low** when a set spans two or more top-level modules, **Info** when it is inside one. One finding per set (subject: the module paths sorted and comma-joined, file: that of the first module) with the number of modules, the edges (file and line each) and the edges whose removal dissolves it. No dependency levels, tree, metric or ranking is computed (the `ARC-002` patent watch).

**Measured, and read.** On seven of our repositories on 2026-11-02: 0 findings in four (Unpopped, PersistAnt, httpant, releaseant) and 7 sets in three (coderipper 2, auth-framework 3, configurant 2). Sixteen edges were read in the source: all six of coderipper's own, the four of configurant's two sets, those of `providers`/`tokens` and of `storage::core`/`dashmap_memory`, and two of the 12-module set in auth-framework. Every one is a real `use` or path, so the findings are true cycles; whether a cycle is *wrong* (a trait and its in-crate implementation are a cycle by construction) is a judgement left to the reader at Low and Info. Not every edge of the 12-module set was opened. No false-positive rate is claimed.

**CodeRipper's own two sets are waived with reasons, not fixed here** (`.coderipper.toml`, the same form as the MOD-005 entries): `allowlist`, `architecture` and `config` (the loaders of the first two read the file through `config::read_file`, and `config` holds the types they own) and `session` and `worktree` (one concept in two files). Each entry names an owner and the condition that removes it. A first attempt at fixing the first (moving `FILE_NAME` to a neutral module) did not remove the cycle and was reverted.

**Checked.** 9 unit tests (a loop across top-level modules is Low with file and line; a chain is none with its edge as control; `super` is no edge; an absolute reach back is an Info cycle inside one top-level module; the crate root takes no part; a `pub use` is no edge; test items skipped; a three-module loop names every dissolving edge; a real package with and without the loop). Conformance fixtures `conformance/MOD-002/{defective,clean}`.

**Patent sweep for the 0.28 line** (`docs/PATENT-SWEEP.md`): idea 36, one query (Q79), two patents read in full (hits 67 and 68). **One is a Chinese application** (CN115113858A, Ping An Bank): Google labels it pending and lists a rejection after publication on 2026-05-08, with no counterpart outside China (WIPO PATENTSCOPE and Espacenet UNVERIFIED); its claim 1 (acquire files, screen by format, build a dependency list, build a directed graph, detect loops) matches the general pipeline in the abstract, as it does for `ARC-001`/`ARC-002`. Recorded with the differences; the PM decided to ship and record, and asked for a status re-check at each minor release.

## 0.27.0 - 2026-11-02 (proposed, Breaking: a new default-on rule)

**MOD-003 is built: no reaching into a module's internals, advisory** (PM task 2026-11-01, ARC and MOD families; the first P-class check after the A-class ones, on the module graph of 0.25). A new Rust rule on the package, default on in `extended` and `portfolio`, not in `classic`; Low only, never gating.

**What it reports.** In the module graph of a package (the tracked `.rs` files under `src/` reached from `lib.rs`/`main.rs` through `mod`; test items skipped), a path written in an item, or a `use`, that names an item inside a module at least one level below a sibling's root (`crate::a::b::X` written outside `a`), **when a module on the way re-exports that item** (`pub use`, `pub(crate) use` or `pub(super) use`; by name, renamed, or by a glob of the module that holds it). The owner offers the item at its entry point and the caller went past it. One **Low** finding per file with the count, the first line and the shorter path the owner offers. The module graph now also keeps the line of every path and every re-export (`modgraph.rs`), shared with the ARC rules.

**The design question (3 of the design note), answered as it proposed:** report a reach-through only where a re-export was bypassed; a deep path with no re-export in the way is not reported, because the owner never offered a shorter one and the rule cannot say the caller went around anything. A path inside the caller's own subtree and the `use` that builds the re-export are never reported.

**Measured, and what that does and does not say.** On the checked-out trees of seven of our repositories on 2026-11-02 (the Rust packages among those tried; a repository with no Rust package at its root was not run), **0 findings in six** (coderipper itself, Unpopped, PersistAnt, configurant, httpant, releaseant) and **33 files in auth-framework**. For the 33, the module pairs behind the findings were read in that repository's source (`utils` re-exports `validation::*`, `authentication` re-exports `credentials::*`, `storage` re-exports `core::AuthStorage`, `config` re-exports `ConfigBuilder as LayeredConfigBuilder`, `server::core` globs its children, `server` re-exports `ClientRegistrationRequest`): in each, the offered path exists, so the finding is a true bypass of a re-export. Not every one of the 33 files was opened, only every distinct module pair behind the sampled ones. Whether bypassing is *wrong* in a crate whose modules are `pub mod` is a judgement the rule leaves to the reader (Low). No false-positive rate is claimed beyond that.

**Scope, said plainly.** Syntactic: a re-export made through a macro or an alias is not seen; a path inside a macro call's tokens is not read; a glob re-export is taken to offer every item of the module it globs (true only of its public items); a file `syn` cannot parse is an error, never a smaller count. One package at a time.

**Checked.** 9 unit tests (a deep path with a re-export is a reach-through; the entry-point path and a deep path with no re-export are not; a `use` and a renamed re-export; a glob re-export; the re-export's own `use` and paths inside the subtree are not; test items skipped; a path into another crate; one finding per file with count, line and shorter path; no root file). Conformance fixtures `conformance/MOD-003/{defective,clean}`.

**Patent sweep for the 0.27 line** (`docs/PATENT-SWEEP.md`): idea 35, two queries (Q77, Q78), three patents read (hits 64 to 66; every independent claim and every dependent claim of the two active ones read in full). **Two are active** (US10534604B1 and its continuation US10853060B2, Architecture Technology Corp, to an estimated 2038): they claim finding code violations from a syntax tree and generating, displaying and applying refactoring options. `MOD-003` reports the shorter path in its text and nothing more; a watch records that no `--fix`, option list with a selection step, automatic source rewrite or learning from selected options may be added without reading the claims again and asking the PM.

## 0.26.1 - 2026-11-03 (proposed)

**`portfolio.yml` no longer walks `$HOME` on the machine that runs it.** The two "Disk use ... (log only)" steps ran `du -sm "$RUNNER_TEMP/coderipper-build" "$HOME/.cargo" "$HOME/.cache/coderipper-bin"` on every runner. On a self-hosted runner the machine user's cargo home is huge: Humboldt run 41550578993 (job `portfolio`, runner `-humboldt`) sat 28 minutes in that `du` and held the runner, blocking Humboldt #165; the same unbounded-`du` class as an earlier hang. Now `df -h /` runs everywhere, and the directory sizes are measured only on a GitHub-hosted Linux runner (`if: runner.os == 'Linux' && runner.environment == 'github-hosted'`), each `du` bounded by `timeout 60`. Nothing reads these numbers, so the diagnosis the log gave on a hosted runner is kept and a self-hosted runner loses only a number nobody used.

**Checked.** A test in `tests/portfolio_workflow.rs` reads the workflow text: every `du` and `find` in a step must be bounded by `timeout 60`, and one that names `$HOME`, `${HOME}` or `~/` must sit in a step limited to a GitHub-hosted runner; its positive control is that the scan finds exactly the two bounded steps. The workflow's `CODERIPPER_VERSION` is unchanged (it names a published release).

## 0.26.0 - 2026-11-02 (proposed, Breaking: a new default-on rule)

**ARC-002 is built: no cycles between components, advisory** (PM task 2026-11-01, ARC and MOD families; on the component graph `ARC-001` reads, now with module groups). A new Rust rule on the repository, default on in `extended` and `portfolio`, not in `classic`; Low only, never gating.

**What it reports.** The strongly connected sets (Tarjan) of two or more components in the graph of edges between components: workspace crates (normal and build dependencies by `path`) and module groups of a crate (the module graph of 0.25), each edge mapped to the components that hold its ends. One **Low** finding per set, subject the component names sorted and joined by commas, with the number of components it spans, the edges between them and the edges whose removal dissolves it: every single edge that does when one is enough, otherwise a depth-first set in name order that dissolves it but is not necessarily the smallest. **High** confidence when only crate edges are in the set, **Medium** when a module group is. Cargo refuses a cycle between crates, so a cycle here is between components of more than one crate, or between module groups of one crate.

**No declaration is not clean.** No `[architecture]` table, or one that declares no component and no layer: reported as **not run** with what to add.

**Scope, said plainly.** An `allow` edge or a waiver of `ARC-001` does not excuse a cycle. Cycles *inside* one component are not reported, which departs from the design sketch (Info): in Rust a parent module and its child routinely name each other, so the finding would be noise on almost every module. The module graph is a lower bound (a cycle through a macro call's tokens, an alias or a glob re-export is not seen); the edges named are real as far as they go. No false-positive rate is claimed: the rule was not run on a corpus for this PR.

**Checked.** 9 unit tests (a loop found and a chain not, with the same nodes as the control; two loops are two sets; every single dissolving edge named; when none is enough the depth-first set really dissolves the cycle; two components that reach each other through their crates are one finding with the right subject, confidence and edges; a one-way dependency is none, with the edge count as control; a module group lowers the confidence; the not-run outcomes; two end-to-end runs on a real git repository, a cycle between module groups and its clean twin). Conformance fixtures `conformance/ARC-002/{defective,clean}`.

**Patent sweep for the 0.26 line** (`docs/PATENT-SWEEP.md`): idea 34, two queries (Q75, Q76), three patents read (hits 61 to 63). **One of them is active**: US7904892B2 (Northrop Grumman, to an estimated 2030) identifies strong components and the dependency arc whose removal best reduces a metric; its independent claims also require a displayed tree view of dependency levels and the metric, which `ARC-002` has not. A watch records that no tree view, level display or metric-ranked list of arcs may be added without reading the claims again.

## 0.25.0 - 2026-11-02 (proposed, Breaking: an existing default-on rule now judges module groups)

**ARC-001 judges module groups inside a crate** (PM task 2026-11-01, ARC and MOD families; the single-crate example of the declaration proposal, 4.2). A component whose `paths` name files, directories or patterns inside a crate was noted and not judged; it is now judged from the module graph of the crate, with the same layers, `allow` edges and waivers as crate edges. A repository that declares module groups gets findings it did not get before, which is why this is Breaking.

**What it reads.** The tracked `.rs` files under `src/` at `HEAD` (files of test code by name and `cfg(test)` items skipped), reached from `src/lib.rs` (or `src/main.rs` when there is no library) through `mod x;` declarations and inline modules. A **reference** is a path written in an item that starts with `crate`, `self` or `super`, or with the name of a child module of the module it is written in, or a `use` of one (groups, renames and globs by their prefix); it resolves to the file that holds the longest module path it names, and an edge goes from the file holding the reference to that file when the two differ (the `mod x;` declaration itself is containment, not dependency). A file is placed in the component whose `paths` entry names it (an exact file, a directory, or a pattern with `*` and `**`; the longest entry wins), and the edges between components are judged like crate edges; each group is also a member for `allow`, waivers and findings. A group that matches no reachable file is an **Info** stale-declaration finding with a positive control (how many files the same read placed elsewhere); a note per crate gives its file count, edge count and files in no group.

**Scope, said plainly.** A lower bound: a path inside a macro call's tokens, a path through a `use` alias or a glob re-export and an item named only by its type are not followed; `#[path]` modules are not read; with both `lib.rs` and `main.rs`, only the library's tree is read and the binary's is noted; a file declared by no `mod` is counted in a note. A file `syn` cannot parse is an error, never a smaller graph. A multi-crate workspace whose declaration places only the inside of one crate still judges edges into that crate as before. **No false-positive rate is claimed**: the rule was not run on a corpus for this PR.

**Checked.** 12 new unit tests in `modgraph/tests.rs` (paths through `crate`, `self`, `super` and child modules; external paths and a path past the root name nothing, with a positive control; edges from referring file to referred file; use groups, globs and `super`; an inline module belongs to its file; test items skipped and unreached or missing files noted; a library with a binary; an unparsable file is an error; `paths` entries as files, directories and patterns; the longest entry places a file; two end-to-end runs on a real git repository, one defective and one clean twin) and 3 in `tests.rs` (module groups judged against the layers, an `allow` edge covering one, a stale group with its control).

**Patent sweep for the 0.25 line** (`docs/PATENT-SWEEP.md`): idea 33, two queries (Q73, Q74), one patent newly read (hit 60) and rows 42 and 43 re-read as the watch required. No hit reads on the module-group reading as read; the BEA/Oracle family (expired) is the same area as before.

## 0.24.0 - 2026-11-02 (proposed, Breaking: a new default-on rule)

**ARC-007 is built: data owned by one component, advisory** (PM task 2026-11-01, ARC and MOD families; the fourth A-class check, on the `owns_data` key of a component the 0.21 declaration already parses). A new Rust rule on the repository, default on in `extended` and `portfolio`, not in `classic`; Low and Info only, never gating.

**What it reads.** For each name some component declares in `owns_data`, every *other* component's crates at `HEAD`: the string literals of the `.rs` files under `src/` (test items skipped; literals inside macro calls such as `sqlx::query!` and `format!` are read) that are exactly the name or are shaped like SQL (a word among select, insert, update, delete, join, into, table, alter, truncate, drop, create) and hold the name as a whole word, ignoring case; and the `.sql` files under the crate directory (lines outside `--` comments that name it). One **Low** finding at **low confidence** per (name, other component) with the count and the files, subject `<component>:<name>`; an **Info** finding for a name two components both declare.

**No declaration is not clean.** No `[architecture]` table, or no component with `owns_data`: reported as **not run** with what to add.

**Scope, said plainly.** A text match, not a query analysis: a log label equal to the name, or an SQL-shaped message that holds it, is counted; a name built at run time, read from configuration or mapped by an ORM is not seen; non-Rust crates are not read; `tests/`, `benches/`, `examples/` and `build.rs` are not read. A file `syn` cannot parse is an error, never a smaller count. Crate granularity; module groups are noted, not judged. **No false-positive rate is claimed**: the rule was not run on a corpus for this PR.

**Checked.** 10 unit tests (a literal that is the name or SQL holding it; prose, a longer identifier and a part of a word not, with a positive control; literals read from macro calls with test items skipped; raw and escaped strings by value; an unparsable file is an error; `.sql` lines outside comments; one finding per name and other component with counts and files; nothing found is nothing; two owners is Info; the not-run outcomes). Conformance fixtures `conformance/ARC-007/{defective,clean}` (a billing crate that selects from the `orders` table another component owns, and the twin that reads its own table).

**Patent sweep for the 0.24 line** (`docs/PATENT-SWEEP.md`): idea 32, two queries (Q71, Q72), three patents read (hits 57 to 59). No hit reads on `ARC-007` as read; the IBM application that extracts table names from code strings (expired) is the nearest, and two watches are recorded.

## 0.23.1 - 2026-11-02 (proposed, a new opt-in setting)

**RDB-008 second half is built, opt-in: runs of commented-out code** (the named row left from 0.17; PM ruling 2026-11-02: opt-in). Off unless the repository's own `.coderipper.toml` sets `[rdb008] commented_out_code = true` (a system or user file does not turn it on; an unknown key under `[rdb008]` is an error that names the file). When on, every tracked C-like, Python and Ruby file is read, not only those that name a to-do, and a run of three or more consecutive whole-line comments that are at least three quarters code-shaped (a line ending in `;`, `{` or `}`, or opening with a statement keyword; doc comments `///` and `//!` are not read) is one Low finding per file at low confidence, with the count of runs and the first line. The unowned-TODO half is unchanged.

**Why opt-in, with the measurement.** The heuristic was run over the cutlass tree (the corpus the PM named) and the first run of each flagged file was read: 49 files, 82 runs; **18 were commented-out code and 31 were documentation** (layout notation such as `E<0,1> := ((_0,_1,_0,...),_0,_0,...)`, pseudo-code that explains an optimized loop, concept sketches, tables of tensor shapes), so about **63% false positives**, far above what a default-on rule may have (CireSnave, 2026-10-25: default on only when the false positives are few). Four cases that could be read either way were counted as false positives. Only the first run of each file was read, so this is a rate of files, not of runs. Tightening the line test does not separate the pseudo-code from real code. Revisit default-on when the heuristic can tell the two apart (ROADMAP row).

**Checked.** 10 unit tests in `rdb008.rs` (runs found, and prose, quoted code and doc comments not; Python; TOML not judged; off without the key, on with it, false the same as absent; the key read from the config text and a broken file is off).

**Patent sweep:** `docs/PATENT-SWEEP.md` addendum for this change (idea 31, one query, one hit read).

## 0.23.0 - 2026-11-02 (proposed, Breaking: a new default-on rule)

**ARC-006 is built: component API within budget, advisory** (PM task 2026-11-01, ARC and MOD families; the third A-class check, on the `[architecture.api_budget]` key the 0.21 declaration already parses). A new Rust rule on the repository, default on in `extended` and `portfolio`, not in `classic`; Low and Info only, never gating.

**What it counts.** For each component with a budget, in the library (`src/lib.rs`) of each of its crates, from the tracked `.rs` files under `src/` at `HEAD`, test items skipped: the public **items** (a `pub` function, struct, enum, union, trait, type alias, constant, static, module, `#[macro_export]` macro or `pub use` leaf, counted only when every module above it is `pub`; `pub(crate)` and `pub(super)` are not public) and the **members** of the public types (pub fields, variants, trait items, inherent `pub` associated items, wherever in the crate they are written, so a type declared in a private module and re-exported still has its members counted). The finding states both counts (CireSnave's census rule: sites and distinct items differ), and their sum is compared with the budget. **Low** when over budget; **Info** for a `pub use` whose root is a library the component does not own (subject `<component>:third-party`) and for a budget entry naming no declared component.

**No declaration is not clean.** No `[architecture]` table, or no `[architecture.api_budget]` entry: reported as **not run** with what to add.

**Scope, said plainly.** Syntactic: macro-generated items and `#[path]` modules are not seen, a `cfg`-gated item counts as enabled, two types of one name share their members, a glob re-export counts once, and a third-party type in a public *signature* is not seen (that is API-008). A binary-only crate counts zero and says so; a file `syn` cannot parse is an error, never a smaller count. Crate granularity; module groups are noted, not judged. **No false-positive rate is claimed**: the rule was not run on a corpus for this PR.

**Checked.** 13 unit tests (items by kind, members, private modules hiding items unless re-exported, no double count of a re-exported declaration, test items skipped, third-party roots including `::crate` paths and group/rename leaves, globs, binary crate and missing module file noted, an unparsable file is an error, over budget versus at budget, the Info findings, not-run outcomes). Conformance fixtures `conformance/ARC-006/{defective,clean}` (five public items against a budget of four with a re-exported library type, and a twin inside the budget).

## 0.22.0 - 2026-11-02 (proposed, Breaking: a new default-on rule)

**ARC-003 is built: stable policy avoids volatile detail, advisory** (PM task 2026-11-01, ARC and MOD families; the second A-class check, on the `[architecture]` declaration of 0.21). A new Rust rule on the repository, default on in `extended` and `portfolio`, not in `classic`; Low and Info only, never gating.

**What it counts.** For each component tagged `volatility = "stable"` (with `role = "policy"` or no role; an adapter, infrastructure, composition or test-support component is not judged), in the tracked `.rs` files under `src/` of each of its crates at `HEAD`, test items skipped: uses of the `[architecture.volatile] names` (an identifier token equal to one), uses of the `namespaces` (`sqlx::` is the tokens `sqlx` and `::`; a longer identifier such as `not_sqlx` does not match, and a name in a string or comment is not a token), and, when `feature_cfg_in_policy = true`, the `cfg` and `cfg_attr` attributes that test a Cargo feature, per feature. One finding per component with the counts, the three most affected files and the features; the components are ranked in a note (the Fuel lane's counts, fuel#574, are what the ranking is first compared with). **Low** when a name or namespace is used, **Info** when only feature attributes are found; medium confidence.

**No declaration is not clean.** No `[architecture]` table, no `[architecture.volatile]` names, namespaces or `feature_cfg_in_policy`, or no component tagged stable: reported as **not run** with what to add (`Check::not_configured`, as ARC-001).

**Scope, said plainly.** Syntactic: an identifier equal to a declared name is counted whether or not it is that type, a type alias, a re-export or a macro's output hides a use, and a `use` and each later use of the imported name are separate places. `cfg!(feature = ..)` calls are not counted, only attributes; `build.rs`, `tests/`, `benches/` and `examples/` are not read. Crate granularity: a component whose `paths` point inside a crate is noted, not judged. A file `syn` cannot parse is an error, never a smaller count. **No false-positive rate is claimed**: the code was not read on a corpus for this rule; the finding says it is a count to inspect. A deliberate use is allowlisted with the component as subject and a reason. The `VolatileDecl` keys now carry no `dead_code` expectation; `quote` is a direct dependency (already in the tree through `syn`).

**Checked.** 10 unit tests (a name counted wherever it is an identifier, not in a string or comment with the same declaration finding the real thing, a namespace prefix and not a longer identifier, feature attributes per feature and only when asked, test items skipped, an inline module counted once per item, an unparsable file is an error, Low versus Info and the ranking, only stable policy components judged, the not-run outcomes). Conformance fixtures `conformance/ARC-003/{defective,clean}` (a stable `core` crate that declares and uses `BackendId`, and the twin that takes a trait it owns, with the same declaration) earn the claim.

**Patent sweep for the 0.22 line** (`docs/PATENT-SWEEP.md`): idea 29, three queries (Q64 to Q66), three patents read (hits 51 to 53). No hit reads on `ARC-003` as read; the Lattix application (expired) overlaps in the abstract with the dependency-rule area of the 0.21 sweep; three watches are recorded.

## 0.21.0 - 2026-11-02 (proposed, Breaking: a new default-on rule and a new configuration table)

**ARC-001 is built: dependencies follow the declared component graph, advisory** (PM task 2026-11-01, ARC and MOD families; the first A-class check, on the `[architecture]` format CireSnave accepted: `docs/superpowers/specs/2026-11-01-architecture-declaration-proposal.md`, all six recommendations). A new Rust rule on the repository (`cargo metadata --no-deps`, which runs no build script), default on in `extended` and `portfolio`, not in `classic`; Low and Info only, never gating.

**The declaration** is an `[architecture]` table in the repository's own `.coderipper.toml` (a repository's file may set it): `[[architecture.layer]]` bottom first (a crate may depend on its own layer or any layer below), `[[architecture.allow]]` edges (a `reason` is required), `[[architecture.waiver]]` (`edge`, `reason`, `owner` and `until`, all required) and `[[architecture.component]]` (`paths` that point at a crate directory make the component stand for that crate). The keys the later A-class rules read (`volatile`, `composition`, `api_budget`, `owns_data`, `invariant_types`, `ratchet`) already parse, so a declaration is written once, and are not judged yet. An unknown key, a name used but not declared, a crate in two layers, an `allow` with no reason and a waiver with no real end date are errors that name the file. `file = "..."` (a declaration in another file) is refused with an error that says it is not read yet.

**What it reports.** Low: a crate-graph edge that goes up a layer, or is not allowed, with both layers named. Info, about the declaration itself: a declared component that names no crate (an absence claim, with the crate list as its positive control), a crate the declaration does not place, a waiver for an edge that is not a violation. An **unexpired waiver is printed as a note on every run** with its reason, owner and end date; **an expired one no longer covers its edge**, and the finding says it ended.

**No declaration is not clean.** A repository with no `[architecture]` table, or one with no layers, is reported as **not run** in the coverage with a note that says what to add. This is a new outcome for a built-in rule (`Check::not_configured`, host reason code `not_configured`), counted as not run and never as clean or as a failure.

**Scope, said plainly.** Crate granularity only: a component whose `paths` point inside a crate (module groups) is noted and not judged yet (the module graph of a single crate is the next step); dev-dependencies are not judged (the note counts them); a dependency by version with no `path` resolves to the registry copy and is not an edge; an edge to or from a crate the declaration does not place is not judged (counted in the note). No false-positive rate is claimed: the rule is exact for the edges it sees, and what it judges is the declaration the project wrote.

**Checked.** 21 unit tests (the declaration's validation, layer direction, `allow`, a waiver unexpired and expired, a stale waiver, an unplaced crate, a stale component, a component over a crate directory, module paths, the metadata reader, the not-run outcome). Conformance fixtures `conformance/ARC-001/{defective,clean}` (two crates in two layers, the foundation crate depending on the top one, and the same declaration with the dependency reversed) earn the claim.

**Patent sweep for the 0.21 line** (`docs/PATENT-SWEEP.md`): idea 28, four queries (Q60 to Q63), nine patents read claim by claim (hits 42 to 50). **This is the closest sweep to the idea itself:** rules of allowed and forbidden dependencies between software modules, checked and reported, are claimed in a BEA/Oracle family (US7076764B2, US7434202B2; expired 2024 and 2025 by Google Patents' estimate) and a Bosch application (ceased); each granted claim set requires elements `ARC-001` lacks (per-module specifications combined by precedence). The two patents shown as active need a generated build (US7640533B1) and a computed design quality index (US20110231828A1). None reads on `ARC-001` as read; four watches are recorded.

## 0.20.1 - 2026-11-02 (proposed)

**CI: `portfolio.yml` has a job timeout, a caller can change it, default 30 minutes** (PM request 2026-11-02: a slow portfolio job held both Humboldt runners for 40 minutes; GitHub's default job timeout is 360). New input `timeout-minutes` (number, default 30) and `timeout-minutes: ${{ inputs.timeout-minutes }}` on the `portfolio` job. A repository whose sweep needs longer passes `with: timeout-minutes: <n>`. A test (`tests/portfolio_workflow.rs`) pins the input, its type and default, and that the job uses it; removing the job line makes it fail. **Callers pick this up only by moving their workflow SHA** to a commit that has this change; nothing changes in any caller until the PM announces a repin. No change to the CodeRipper binary or `CODERIPPER_VERSION`.

## 0.20.0 - 2026-11-02 (proposed, Breaking: a new default-on rule)

**MOD-004 is built: types that split into separate usage clusters, advisory and low confidence** (PM task 2026-11-01, ARC and MOD families; design `docs/superpowers/specs/2026-11-01-arc-mod-checks-design.md` section 3.5; stacked on 0.19.0 / #210). A new Rust-module rule (`syn`, read at `HEAD`), default on in `extended` and `portfolio`, not in `classic`; Info only, never gating. For each struct it groups the methods of its inherent `impl` blocks by the `self.field`s they use, directly or through the sibling methods they call (macro arguments such as `format!("{}", self.a)` are read as tokens), and reports a type whose methods fall into two or more clusters of two or more methods each, among four or more methods outside the thin ones. Accessors, setters, derived values (at most two statements, two fields and no sibling call) and builder steps (`self` by value) are members of no cluster, so a struct with a getter per field does not split into as many clusters as it has fields. The finding names each cluster's methods and fields, says it is weak evidence, and says what to do.

**The figures are chosen by reading, not by percentile, and the finding says so.** On the nine-repo corpus (origin/main on 2026-11-01: 3,706 structs, 720 with any cluster) the first definition, which excluded only one-statement accessors, reported 19 types; reading their clusters, about eight were builders (`QuickStartBuilder`, `ProviderProfile`, `AuthToken`), data records with a helper per field (`SessionData`, the two Dinov2 configs), a config object (`AuthConfig`) or an encode/decode pair (`Tokenizer`). Treating thin methods and builder steps as accessors left **6**: auth-framework `TuiApp`, `DashboardManager`, `AdvancedJarmManager`, `FapiManager`; fuel `ResidencyFile` and `Tokenizer`. By their clusters, 5 of the 6 read as mixed jobs (for example `AdvancedJarmManager`: response creation and JWE decryption over five fields, delivery over the HTTP client) and `Tokenizer` does not (encode and decode over inverse tables). This is a reading of cluster shape, not of the code, so **no false-positive rate is claimed.**

**Recall is low and that is the honest limit.** Fuel's large types (`Tensor` with 252 methods, `VulkanBackend` with 183) are not reported: their methods reach one another through shared state or helpers, so they form one cluster, and a field reached by destructuring or passed on through a helper is not seen. Of 3,706 structs, 6 are reported. The rule finds a type that has clearly separate halves; it does not find every type that does too much (MOD-005 finds the large ones).

**Checked.** Unit tests (19) cover clusters over disjoint fields, a bridging method, a call to a sibling, accessors, a derived value, a builder step, macro arguments (a field and a call), trait-impl methods, test items, a name two structs share, an unparseable file, and a type named `None` still validating. Mutation: 8 mutations that compile and could ship (a call no longer carries the callee's fields; test items read; trait-impl methods as members; thin up to three statements; builder steps as members; macro arguments unread; two clusters not enough; a cluster needing three methods), each applied once with its anchor asserted: 8 of 8 caught. Conformance fixtures `conformance/MOD-004/{defective,clean}` (a type with a log/level half and a cache/hits half, and the same type with one method that uses both) earn the claim.

**Patent sweep for the 0.20 line** (`docs/PATENT-SWEEP.md`): idea 27, three queries (Q57 to Q59), four patents surfaced and the three on point read claim by claim (US11036527B2, US20190317760A1, US9158503B2); none reads on `MOD-004` as read. The nearest, US11036527B2 (IBM, expired), splits a class at runtime and replaces call sites; `MOD-004` is a static report. One watch is recorded (re-read it before any change that generates the split). Due-diligence read, not legal advice.

**Not seen, said in every finding:** a field reached by destructuring (`let Self { a, .. } = self`) or passed on through a helper taking `&self.a`; items a macro generates; a macro that renames `self`; a name two structs in a crate share (noted); trait-impl methods; enums. Test items are not read.

## 0.19.0 - 2026-11-01 (proposed, Breaking: a new default-on rule)

**MOD-005 is built: size and coupling outliers, advisory** (PM task 2026-11-01 for the ARC and MOD families, CireSnave: *"Those in its ARC and MOD families are very important to me."*; design `docs/superpowers/specs/2026-11-01-arc-mod-checks-design.md` section 3.1). A new Rust-module rule (`syn`, read at `HEAD`), default on in `extended` and `portfolio`, not in `classic`; reported and never gating (Info and Low). It builds the shared fact layer (per-file code lines, the other modules a file names, the files naming it, per-type fields and inherent methods) the later P-class rules read.

**Findings.** Info: a file of 1,035 or more code lines; or one that names 14 or more other modules (fan-in alone is not reported: a small stable module that many files depend on is depend-toward-stability working, and CodeRipper's own `finding.rs`, named by 44 files, showed it); or a type with 30 or more members (fields plus inherent methods) and at least one method (a record of fields only is skipped). Low: a file of 2,234 or more code lines that also names 7 or more other modules or is named by 6 or more other files. Each finding states the value, the threshold, where the threshold came from, and what to do.

**Thresholds are percentiles of the pooled corpus, MEASURED by this check's own metrics** on a snapshot of each repo's `origin/main` on 2026-11-01 (Fuel 3a41200c, auth-framework 24e45ca7, Unpopped 019050a, KISS d4fa3d1 (conformance crate), PersistAnt 7590b33, configurant dd7a331, httpant 466d8ad, releaseant c924154, coderipper ee8c579): 1,263 production files and 3,706 structs. Code lines p95 1,035, p99 2,234; fan-out p95 7, p99 14; fan-in p95 6 (beside size only); members p99 30. Pooled, not per project: Fuel's own p99 is 4,734, so a project-relative rule would call a 4,000-line file ordinary in the project that most needs the finding. Whole-file line counts in the design note (p95 1,927) include inline test modules and are superseded by these.

**What it reports on the corpus (117; 132 before the record rule and the fan-in change):** 8 Low (Fuel 6, Unpopped 2), 56 Info size, 14 Info coupling, 39 Info type. By repo (Low / Info size / Info coupling / types): Fuel 6/28/4/20, auth-framework 0/22/5/13, Unpopped 2/2/2/3, KISS 0/0/0/1, PersistAnt 0/2/0/0, configurant 0/0/0/2, httpant 0/1/1/0, releaseant 0/0/1/0, coderipper 0/1/1/0.

**On this repository** (its CI runs `--deny info` on itself): `src/cli.rs` (size: 1,439 code lines, `run_and_report` alone about 437) and `src/module/neutral/mod.rs` (names 21 modules). Each is a per-finding `[[allow]]` in `.coderipper.toml` with its reason, owner and what ends it; no path is excluded. `cli.rs` is a real candidate for a split and the entry says so; `neutral/mod.rs` is the rule registry, whose fan-out is the number of rules by construction. A third finding, `finding.rs` named by 44 files, was not waived: it showed that fan-in alone is not a defect, and the rule was changed.

**Checked, and what that does and does not show.** (1) The counts were reproduced by a second, independent implementation of the thresholds over the same dump: every repo agrees (it validates the threshold logic, not the metrics). (2) Code lines: 24 files (all 8 Low and 16 seeded-random Info-size) recounted by a separate brace-matching counter that skips each `#[cfg(test)]`/`#[test]` item: 24 agree (one within 9 of 1,158 lines). (3) Fan-in: two Low files recounted by grep: `unpopped/src/plan.rs` 10 = 10 once comments and test items are excluded; `fuel-tensor/src/lazy.rs` 9 against 10 reported (the one difference is not explained). (4) **Whether findings are worth acting on was judged from file and type shape, NOT by reading the code**, so no false-positive rate is claimed. By shape: of 41 flagged types, 2 are records of 31 fields with no method (now skipped) and 2 are builders; the large Fuel and auth-framework managers (`Tensor` 252 methods, `VulkanBackend` 183, `NodeHandle` 194) look like real candidates. Of the 8 Low files, 5 look like real candidates (`fuel-dispatch` `dispatch.rs` 6,513 lines and `pipelined.rs`, `fuel-cuda-backend` `storage.rs`, `fuel-tensor` `lazy.rs`, `fuel-vulkan-backend` `lib.rs` 12,851) and 3 look cohesive by structure (`unpopped` `oracle.rs`, a deliberately independent reference interpreter; `plan.rs`, the central plan builder; `fuel-quantized` `k_quants.rs`, exactly at the threshold). The Info tiers are cues to inspect, which is why none gates. (5) Mutation check: 7 mutations that compile and could ship (a threshold `>=` to `>`; `||` to `&&` for coupled; records reported again; test items measured; ancestor edges kept as fan-out; comment lines counted), each applied once with its anchor asserted (an earlier run failed to apply one after `cargo fmt` moved its text, and was redone): 6 of 6 applicable caught by the unit tests; a seventh (the file itself counted in its own fan-in) is an equivalent mutant (its own module is already removed from its fan-out), so the dead filter was deleted. (6) Conformance fixtures `conformance/MOD-005/{defective,clean}`: a 1,100-line file and a 31-member type against a 1,000-line file and a 29-member type; the rule is proven (`the_rust_module_earns_six_of_its_eight_claims`).

**Not seen, said in every finding:** items a macro generates; glob-import targets; `#[path]` modules; a type name two structs in a crate share (counted in a note, methods attributed to neither). A module naming its own ancestor is containment and is not fan-out (design ASK 2, the PM's ruling pending). Growth over time needs history and is not measured. A `src/bin` file is measured for size only. `tests/`, `benches/`, `examples/` and `build.rs` are outside `src/`.

**Patent sweep for the 0.19 line** (`docs/PATENT-SWEEP.md`, CireSnave's standing instruction of 2026-10-25; `patent-sweep-record` fails the release without it, and it failed this PR's self-check until written): idea 26, three queries (Q54 to Q56), four patents read claim by claim (US10261883B2, US11892935B2, US11474927B1, US8458663B2); none reads on `MOD-005` as read. Two watches are recorded (a roll-up of file metrics over a graph; dynamic coupling coverage). Due-diligence read, not legal advice.

**One hazard met, not fixed here:** the result cache is keyed by version, check id and committed tree, so while the version does not change, an edit to a cacheable check serves the earlier result for an unchanged tree (it hid a bug fix in this change's corpus runs until `CODERIPPER_RESULT_CACHE=off`). A release changes the version; a working copy does not.

## 0.18.6 - 2026-11-01 (proposed)

**Docs only: the `[architecture]` declaration, proposed for CireSnave's review** (`docs/superpowers/specs/2026-11-01-architecture-declaration-proposal.md`; PM notice 2026-11-01: *"lane proposes, I review"*). The schema, three worked examples (Fuel's layers from its own tier file, a small single crate, the smallest useful declaration), what each ARC/MOD rule reads from it, what a project without one gets, and six choices with recommendations. No A-class check is built on it until he has reviewed it. No code changed.

## 0.18.5 - 2026-11-01 (proposed)

**Docs only: design note for the ARC and MOD checks** (`docs/superpowers/specs/2026-11-01-arc-mod-checks-design.md`; PM task 2026-11-01, relaying CireSnave). Per rule: class, measurement, data source, threshold method, the `[architecture]` declaration format for A-class rules, a build order, five open choices marked ASK, a corpus size table (9 repos, measured at each `origin/main`). No code changed; no rule's catalogue row changed.

## 0.18.4 - 2026-11-01 (proposed)

**Docs only: ROADMAP row E8, `reachability` precision on a large library workspace** (the Fuel lane's finding on 497 Medium rows: test-only callers, dead-root chains, non-fn items; PM 2026-11-01: scheduled for the first week after FULL SPEED, not built now). No code changed.

## 0.18.3 - 2026-11-01 (proposed)

**Docs only: the two patents flagged for a read before release were read in full** (PM task 2026-10-31). `docs/PATENT-SWEEP.md` rows 33 (US11288376, IBM, active) and 34 (US10929277, Citrix, expired) and an addendum: every claim of both, verbatim, with `tools/patent-claims.py` from Google Patents (machine-converted text, not the official record). As read, neither reaches `SEC-004`: US11288376 needs a value-based second rule and the combination of both rules' results (SEC-004a has only the call-name rule); US10929277 needs a list of strings rendered through a user interface. Not a legal opinion. No code changed.

## 0.18.2 - 2026-10-31 (proposed)

**Brief mode: `--top N` prints the N most severe findings and says what it left out** (ROADMAP E3, PM task 2026-10-31). Findings are ranked by severity, then confidence, then report order; human and `short` output end with `coderipper: N more finding(s) not shown (2 high, 1 medium)`; JSON prints N `coderipper-finding` lines and adds `findings_omitted` to the `coderipper-summary` line, whose `findings` stays the full count. Without the flag nothing changes, and the flag changes no exit code, no waived-finding line and no coverage output.

## 0.18.1 - 2026-10-31 (proposed)

**SEC-004 part c is built: a weak algorithm in use, Info** (ROADMAP R3, family 1, plan section 8 step 3; stacked on 0.18.0). A `use` of the `md5`, `sha1`, `sha1_smol`, `des`, `rc4` or `ecb` crate or of a type named `Md5`, `Sha1`, `Des`, `Rc4` or `Ecb` is one finding per `use` item, and a fully qualified call into one of those crates with no `use` of it in the file is one finding per call; test items are skipped. It is an inventory, not a defect: protocols mandate MD5 and SHA-1 in places, so the finding asks for a waiver that names the protocol. Reported and never gating. Not built: the loader check that rejects a weak-algorithm waiver without a reason; algorithms reached through a wrapper or a non-Rust language.

## 0.18.0 - 2026-10-31 (proposed, Breaking: a new default-on rule)

**SEC-004 parts a and b are built: a literal key and TLS verification switched off** (ROADMAP R3, family 1, PM task 2026-10-31; plan `2026-10-24-security-crypto-rules.md` section 8 step 2). A new language-neutral-module rule, default on in `portfolio` and `extended`, reported and never gating. Every tracked Rust file outside vendored, fixture and development directories is parsed with `syn`, and test items are skipped. **4a (High):** `from_secret`, `new_from_slice`, `Key::from_slice`, `hmac::Key::new` or `UnboundKey::new` given a non-empty string or byte literal, or a local bound to one in the same function. **4b (High):** `danger_accept_invalid_certs` or `danger_accept_invalid_hostnames` with anything but the literal `false`, and rustls `.dangerous()`; a configuration pass-through and `.dangerous()` (which installs any custom verifier, strict or not) are Medium confidence and say to check. One finding per site. Exact on API names, which is why it is default-on under the ruling of 2026-10-25 (item 4: exact syntax, few findings, a seeded fixture both ways). Not judged: weak algorithms (4c), AEAD without associated data (4d), non-constant-time comparison (4e), keys from `concat!`, a `const` or a helper, a custom verifier, other languages. **Measured:** the rule finds the shape of the family's incident in a portfolio project; findings in portfolio projects go to their lanes privately, and no site, file or line is named here until the fixes ship. A portfolio-wide false-finding count was not complete when this entry was written. Patent sweep addendum: idea 25, no hit read, two USPTO patents on hard-coded secrets (US10929277, US11288376) flagged for a read and not cleared.

## 0.17.7 - 2026-10-31 (proposed)

**Docs only: the performance design (`docs/superpowers/plans/2026-10-23-performance-rules.md`) now states the 2026-10-25 rulings** (ROADMAP R1 and D2; PM task of 2026-10-31). Default-on: PRF-020, PRF-009, PRF-019, PRF-011, PRF-015. Opt-in with positions: PRF-014, PRF-013, PRF-010, PRF-017, PRF-012; PRF-016 and PRF-018 only under `[perf] hot_paths`. The written bar is 70% of a 30-finding sample, not 80%; the marker is the comment `// coderipper: hot`; the first 4-of-12 sample is marked superseded. No code changed.

## 0.17.6 - 2026-10-31 (proposed)

**EFX-003 is built: environment variables are read in one configuration module, and it is OPT-IN** (M13; lane pick; PM ruling 2026-10-31: the first measurement's false-finding rate was "many", so it is not default-on). The rule runs when named (`coderipper check EFX-003`), and in no profile on its own; it is claimed and proven by its fixtures like the other neutral rules. A new `opt_in_rule_ids()` in the neutral module lists such rules, and the profiles leave them out unless the id is named. Tracked source files in Rust, Python, JavaScript and TypeScript, Go, Java, Kotlin, Scala, Ruby, C# and PHP; a repository whose **named** reads are in three or more files gets one Low finding naming them (the three is **chosen, not measured**, and does not separate the noise). Not counted: forwarding the whole environment (`env::vars()`, `os.environ.copy()`, `dict(os.environ, ..)`), platform variables (`PATH`, `HOME`, `USERNAME`, `LOCALAPPDATA`, `XDG_*`, `CI`, `CARGO_MANIFEST_DIR`, ...), a read inside a string literal or a Python docstring, development tooling (`tools`, `scripts`, `probe`, `spikes`, `xtask`, `docs`, `e2e`, `examples`, `.github`, `*.config.*`, `build.rs`) and test code. Not Breaking: no profile runs it, so no existing run gains a finding. Patent sweep addendum: idea 24 (US11269626B2, read, outside as read with a stated limit). **Measured** (2026-10-31, this build, `check EFX-003` over the 66 primary checkouts under C:\Projects, working-tree HEAD; all hit lines read, my judgment, unclear counted as a false finding): **before the exclusions** 21 findings (12 true, 4 false, 5 unclear: 19% false, 43% with unclear); **after** 16 flagged checkouts (auth-framework is 3 clones): 12 true, 0 clearly false, 4 unclear (OverMind, baracuda's `-sys` loaders, coderipper's own allowlist forwarding, consent) = **25% with unclear counted** (12.5% if OverMind and consent are read as true); of the 66 checkouts, 49 clean, 16 flagged, 1 could not run (pm-assist has no commit). Repos flagged by threshold after the exclusions: 2 files 19, 3 files 16, 4 files 15, 5 files 12. That is above the 10% bar for default-on, hence opt-in.

## 0.17.5 - 2026-10-31 (proposed)

Docs only: `docs/CONSUMER-CHANGES-0.15-TO-0.17.md`, what 0.15.1 to 0.17.4 changed for a consumer of the published 0.15.0 (rules, exit codes, output, migration), for a publish decision. No change to the binary; the number is a proposal, the PM allocates at gate.

## 0.17.4 - 2026-10-31 (proposed)

Docs only: ROADMAP row F6 is parked as likely stale (seen on 0.9.6, not on 0.10.0). No change to the binary; the number is a proposal, the PM allocates at gate.

## 0.17.3 - 2026-10-31 (proposed)

**A failed link now says what the linker could not do** (roadmap F6, step 1). The cause of a broken build carried the first note of rustc's `linking with `cc` failed` error, which is the whole `"cc" ...` command line, so the 400-character cut hid the linker's own words. A link failure now carries the note that names the missing library or symbol (`cannot find -l...`, `undefined reference`, `LNK1...`), and no diagnostic carries a command-line note when another exists. Message text only; not Breaking. Test: `a_failed_link_carries_the_linkers_words_not_the_command_line`. The number is a proposal, the PM allocates at gate.

## 0.17.2 - 2026-10-31 (proposed)

Docs only: ROADMAP row F6 (a member that cannot link fails the whole sweep, from the cpufish finding). No change to the binary; the number is a proposal, the PM allocates at gate.

## 0.17.1 - 2026-10-30 (proposed)

**RDB-008 false positives fixed** (the two the PM asked about after the 0.17.0 measurement; 5 of 74 first-hit lines were wrong, ~7%). A comment marker inside a single-line string literal (`"`, and `'` or a backtick where the language has them; a Rust lifetime and a char literal are handled) is no longer a comment, and `TODO [name]` counts as an owner like `TODO(name)`. Only removes findings, so not Breaking. A string that spans lines is still not tracked. Tests: two new unit tests. The number is a proposal, the PM allocates at gate.

## 0.17.0 - 2026-10-30 (proposed) - BREAKING

**RDB-008 first part is built: a `TODO` or `FIXME` in a source comment names an owner or a ticket** (M13; lane pick, no config needed). A new language-neutral rule, default on in `portfolio` and `extended`, experimental in the catalog and so reported, never gating (**Breaking** by the default-on rule: a repository with a bare to-do gains a finding). Tracked source files by extension (C-like `//` and `/* */`, hash and dash comments) outside the usual fixture and vendored directories; a to-do is owned by `TODO(name)`, `@name`, an issue reference (`#123`, `PROJ-123`) or a link on its line. Low, one finding per file. Not judged: the rule's second half (commented-out code), an owner on another line, prose files. A comment marker inside a string literal is read as a comment (known false positive). **Measured** (PM ruling 2026-10-30) on the 66 primary checkouts under C:\Projects (working-tree HEAD, not origin/main): 56 with no finding; 10 with findings, 76 files and 117 bare comments in all (cutlass 72, fuel 26, wasm-sandbox 6, lightbulb 4, vulkane 4, auth-framework 1 plus two copies, fam 1, auto-discovery 1). Of 16 hit lines read, 14 were real and 2 were a marker inside a string literal (both vulkane); the other ~100 were not read. Patent sweep addendum: idea 23, no hit, not a clearance (US11269626B2 flagged for a later read).

## 0.16.1 - 2026-10-30 (proposed)

Docs only: ROADMAP row E1 (cached symbol index) is marked deferred until E2 exists, with the reason (the C7 patent design rule). No change to the binary; the number is a proposal, the PM allocates at gate.

## 0.16.0 - 2026-10-30 (proposed)

**Breaking: `reachability` and `unused-return-values` findings carry a qualified `subject`** (roadmap E5), as `unused-parameters` already did: the item's inline modules, impl type and enclosing functions, then its name (`S::new`, `m::S::method`). Two same-named methods in one file no longer share an allowlist entry. A top-level item keeps its bare name. **To migrate:** an `[[allow]]` `symbol` or a `[reachability] public_api` `symbol` naming a method, field or variant must now be the qualified name; an entry that no longer matches is reported as stale, so it cannot go quiet. A line with two same-named candidates, or a file `syn` cannot parse, keeps the bare name. Tests: `qualify` unit tests and one for the unused-return-values tags. The number is a proposal, the PM allocates at gate.

## 0.15.9 - 2026-10-30 (proposed)

**The dotless `/etc/coderipper/coderipper.toml` is read** (roadmap F4, rest), before `/etc/coderipper/.coderipper.toml`, so a system file there may be spelled either way (both are read; the dotted one is the stronger). Unix only; the other places keep the one dotted name. Not Breaking: a machine without the new file is unchanged. Test: the search-order unit test. The number is a proposal, the PM allocates at gate.

## 0.15.8 - 2026-10-30 (proposed)

Docs only: ROADMAP row F1 is marked built (0.15.3 to 0.15.6 all merged). No change to the binary; the number is a proposal, the PM allocates at gate.

## 0.15.7 - 2026-10-30 (proposed)

**`max_runtime_minutes` is enforced** (roadmap F4, first part). When a `[limits]` file or `CODERIPPER_MAX_RUNTIME_MINUTES` sets the ceiling, a watchdog thread started after the configuration is read ends the process at that many minutes with `coderipper: stopped: the run passed max_runtime_minutes = N (from SOURCE); part of the audit did not happen` and exit 3 (a check could not run), so a run that hit the ceiling can never read as clean. Before this the ceiling was only reported. Child processes already spawned (cargo, a tool) are not killed by the watchdog and may outlive it. Not Breaking: without the ceiling nothing changes, and the 30-minute value in the docs is a suggestion, not a default. Test: a unit test that the watchdog fires after its delay; the end-to-end stop at real minutes was NOT run. 0.15.6 is #189's proposed number; the PM allocates at gate.

## 0.15.6 - 2026-10-30 (proposed)

**`--message-format short`** (roadmap F1, last slice): one line per finding, `file:line: severity: summary [check id]`, with the place first so an editor can jump to it (a finding with no place leads with its package). Stdout carries nothing else (no waivers, coverage or summary); errors and notes stay on stderr. Also a value for the `message-format` configuration key. `human` and `json` are unchanged.

## 0.15.5 - 2026-10-30 (proposed)

**`cargo coderipper --sweep` and `--check <id>` as flags** (roadmap F1, third slice): the cargo subcommand takes the modes as flags, as cargo spells its own. `--sweep` runs the sweep, `--check <id>` (or `--check=<id>`) runs exactly one check; both together, or `--check` with no id, is a usage error (exit 2); nothing after `--` is read as a flag. The explicit subcommands (`sweep`, `check <id>`) work as before, and the `coderipper` binary is unchanged.

## 0.15.4 - 2026-10-30 (proposed)

**`-p/--package <name>`** (roadmap F1, second slice): as `cargo -p`, checks only the workspace member with that package name (repeatable; implies `--workspace`, conflicts with it). The checks that judge the repository as a whole do not run (a note says so), a name that is no member is an error that lists the members, and `[[skip_member]]` still applies to a named member. Library: `CheckContext::packages(..)`, a new field on a `#[non_exhaustive]` struct.

## 0.15.3 - 2026-10-30 (proposed)

**`--manifest-path <Cargo.toml>`** (roadmap F1, first slice): names the project to check as `cargo --manifest-path` does, in place of `--project` (the directory that holds the file). It conflicts with `--project` and `--scope`, and a path whose file name is not `Cargo.toml` is a usage error (exit 2). `-p/--package`, `--sweep`, `--check <id>` as flags and the `short` message format remain open.

## 0.15.2 - 2026-10-30 (proposed)

**`--min-coverage <pct>`: an opt-in floor on COVERAGE** (roadmap M3; PM task 2026-10-30, option A: the spec's section 6 and Q7 wording; a floor on the share of rules passed would duplicate `--deny`). With it, the run exits 1 when the covered percentage (covered / rules in the catalog) of any language it reports is below `pct`, names each such language on stderr (`coderipper: coverage below the floor: ...`), and turns the coverage report on. A language found in the repository that no module checks is measured by the language-neutral rules alone (**measured** with this build on a Rust crate with one `.ts` file: rust 11.8%, typescript 8.6%); a run that reported no language at all fails the floor explicitly, never silently. It never lowers a worse code (3 over 1 over 0). `pct` outside 0 to 100 is a usage error (exit 2). **Not Breaking:** without the flag nothing changes, so no existing caller's exit code moves; the new flag only adds a way to exit 1. The ROADMAP row now says coverage, so it cannot be read as the rules-passed floor again. Tests: `tests/coverage_cli.rs` (floor 0 passes, 100 fails naming rust, a floor of 10 fails the TypeScript package (the percentages move with the tools installed, 5.9% and 8.6% were both seen, so the test only asserts that it is named), a finding keeps exit 1, bad values exit 2) and unit tests of the floor decision. Run on Windows only; CI covers Linux and macOS.

## 0.15.1 - 2026-10-30 (proposed)

**`restored_files_get_a_new_mtime_and_files_nothing_touched_keep_theirs` no longer races the filesystem clock** (PM task 2026-10-30; it failed once on ubuntu-latest in CI run 41019953681 at the "stamped AFTER the build ended" assertion: Linux stamps mtimes from a coarse clock that can lag `SystemTime::now()`). The test now pins every file to an explicit past time, refreshes git's index, and compares the restored file with the rewrite's own stamp. Test-only: no change to the binary or the library. Run 5 times on Windows only; Linux is checked by CI, not by me. The 50 ms sleep that 0.15.0 carried is superseded.

## 0.15.0 - 2026-10-30 (proposed) - BREAKING

**`security-support-statement` (a released project's `SECURITY.md` promises no support period and claims no support for an old line) is built** (PM task 2026-10-30), and this repository gets a `SECURITY.md` whose "Supported versions" section is the PM's text word for word (`C:\Projects\.lane-state\support-text.md`; diffed identical). A new language-neutral rule, default on in `portfolio` and `extended`, experimental in the catalog and so reported, never gating (**Breaking** by the default-on rule: a repository with such a file gains a finding). For a repository whose root manifest declares a publishable version (the `patent-sweep-record` test) and that tracks a `SECURITY.md` (root, `.github/` or `docs/`) with a section headed "supported versions": Medium for a table row marked with a check mark for a line older than the manifest's, or a second marked row; Medium for a support period, a number (digits, or one to twelve, eighteen or twenty-four as words) followed by month(s) or year(s). Not judged: a repository with no `SECURITY.md` or no such section, a promise worded without a number, whether the statement is true. Patent sweep addendum: idea 22, no hit. **Measured** on the 49 primary checkouts under C:\Projects (their working trees, not `origin/main`; `tracks a SECURITY.md` counted from `origin/main`, 20 of 48 fetchable): 14 reached a verdict, 13 clean and 1 with a finding (role-system: a template table marking `0.1.x` as the supported line while the manifest is at 1.3.0); the other 35 were not judged (no `SECURITY.md`, no such section, or not releasable). Not read: why each of the six repositories that track a file but were not judged was skipped.

## 0.14.1 - 2026-10-30 (proposed)

**`portfolio.yml`: a full disk and a tool that would not install are named, and the disk is made roomier first** (PM task 2026-10-30; synapse#290, run 41007729285, which called the workflow at 30e012b: three workspace members failed to build with "failed to write", the gitleaks download failed with `No space left on device`, SEC-002 never ran, and the job ended `Process completed with exit code 3` with the cause one line in thousands). On a GitHub-hosted Linux runner, before the checkout: the toolchains the image ships and this job never uses (`/usr/share/dotnet`, the Android SDK, GHC, CodeQL, Boost, Swift) and the Docker image cache are removed, the space freed is logged, and the job fails with `disk: only N GB free ...` if `MIN_FREE_GB` (8, **chosen, not measured**) is not met; the same floor is checked again before the sweep, after the install and cache restores. The sweep step now keeps the secrets scan's exit code apart, turns each `check error: ... failed to run` line (the first ten) and each tool-install failure (`tool_install_failed`, `tool_missing`, `checksum_mismatch`, `download_failed`, `consent_not_given`, ...) into an `::error::` annotation naming the cause, and fails the job when one was printed even if the exit code was 0. Before this the job already failed on that run (exit 3), but the guard looked only for `note: ... not run:` lines, and a failed install is a `check error:` line. Tests `tests/portfolio_workflow.rs` pin the steps and their order. **Callers pick this up only by moving their workflow SHA.** No change to the CodeRipper binary or `CODERIPPER_VERSION`. **Positive controls, run on `ubuntu-latest` from scratch branches of this repository that called this workflow as `uses: ./...` (the branches are deleted):** (1) a seeded fake secret: the first seed (`api_key = "a50aa64a..."`, entropy 3.418) was NOT caught, run 41011084402 green, because it is below gitleaks' generic-rule entropy floor of 3.5, so that control was invalid and says nothing about the workflow; a re-seed of entropy 4.9 was caught, two `generic-api-key` findings at `scratch-seeded-secret.txt:1` and `:2`, run 41011655133 red; (2) a 1 MB tmpfs mounted over `$RUNNER_TEMP/tools` before the sweep, so the gitleaks download hits a real `No space left on device`: run 41011088468 red, with annotations `SEC-002 failed to run (tool_missing): tool_install_failed: gitleaks 8.30.1: No space left on device ...` and `a tool the secrets scan needs was not installed: ...`; (3) `MIN_FREE_GB` set to 9999: run 41011093690 red in the first steps with `disk: only 113 GB free on / after removing the unused toolchains; the job needs at least 9999 GB ...`. **Measured on that image:** the cleanup freed 27 GB (about 86 GB free before, 113 GB after). The failing synapse run has no `df` numbers (it ran the older workflow), so how much its sweep used is NOT known; the "Disk use after the sweep" step will show it on the next run, and if a big workspace uses most of 86 GB the cleanup is margin, not a cure.

## 0.14.0 - 2026-10-30 (proposed) - BREAKING

**`OPS-012` now judges the declared-feature conventions** (PM task 2026-10-29, ROADMAP M16). A convention is judged only for a feature the binary itself offers, from its source and manifest (`syn`): (1) `--yes`, `--quiet` or `--verbose` matched in an argument loop, or a clap field with `long` and no `short`, while the short form (`-y`, `-q`, `-v`) is named nowhere in the file; (2) a file that calls `stdin()` and opens files, in a package where nothing treats `"-"` as standard input and `clio` is not a dependency (Low confidence); (3) a manifest naming a colour crate that reads no `NO_COLOR` by default (`owo-colors` without `supports-colors`, `yansi`, `nu-ansi-term`; read from their sources: `colored`, `console`, `crossterm` and `anstream` read it themselves and `termcolor` does under `ColorChoice::Auto`, so none of those is judged) with no source of the package naming `NO_COLOR`, and `var_os("NO_COLOR").is_some()` (or `is_none`, `is_ok`, `is_err`), which counts an empty value when the convention counts it only when set and non-empty; (4) a `loop` in `main` with no `break` or `return`, or a `TcpListener`/`UnixListener::bind` or `axum::serve`, in a package whose sources never mention `ctrlc`, `ctrl_c`, `signal`, `SIGINT`, `SIGTERM` or `SetConsoleCtrlHandler` (Low confidence). **Breaking** by the default-on rule (the same finding id gains cases, so a repository can gain findings). Still experimental in the catalog: reported, never gating. Not judged: another language, a flag handed to a child process, a handler or `-` named only in another package, and any dynamic probe (the binary is not run).

## 0.13.0 - 2026-10-29 (proposed) - BREAKING

**`OPS-012` (a command-line tool follows the platform's conventions) is built** (PM task 2026-10-29, ROADMAP M15, the static half of the OPS-011 follow-up). A Rust rule in the language-neutral module, default on in `portfolio` and `extended` (**Breaking** by the default-on rule: a project that does these gains findings), experimental in the catalog and so reported, never gating. From the source of each binary (`main.rs`, `src/bin/`; `syn`): a fixed `nano`/`vim`/`vi`/`nvim`/`emacs`/`pico`/`notepad` started in a file that never names `EDITOR` or `VISUAL`; a `.join(".name")` on the home directory in a file that never names XDG, `APPDATA`, `config_dir` or a directories crate (the advice is per platform: `%APPDATA%` on Windows, XDG on Linux only); `env::args` with two or more flags named in a package whose `Cargo.toml` names no argument parser; `-v` printing the version or `-V` setting verbosity (match arms and clap `short`). Low severity, Medium confidence. Not judged: other languages, an editor or parser named in another file, `format!`-built paths, and the declared-feature checks (M16). Patent sweep addendum: idea 21, no hit.

## 0.12.1 - 2026-10-29 (proposed)

**`[[cli_waiver]]` (PM task 2026-10-29, first user agentlife's `lane-restart`):** a repository waives named `OPS-011` probes for one binary (`bin`, `probes`, a required non-blank `reason`); the run prints `waived: ... <reason>`, the other probes are still judged, an entry without a reason or with an unknown probe is a configuration error, and a stale one is noted. The rule stays default-on.

## 0.12.0 - 2026-10-29 (proposed) - BREAKING

**`OPS-011` (a binary answers `--version`, `--help` and a bad flag, and does nothing else) is built** (CireSnave 2026-10-29: *"Get that CodeRipper rule in place."*; the case was `synapsed --version` starting the daemon and failing on a store path instead of printing a version). A Rust rule, default on in `extended` and `portfolio` (**Breaking** by the default-on rule: a project whose binaries do not meet the standard gains findings), experimental in the catalog and so reported, never gating. It builds the package's binaries from the committed tree (`cargo build --bins`, the build cache and the result cache apply) and runs each through `--version`, `-V`, `--help`, `-h` and an unknown flag, each in a fresh temporary directory with an empty environment (only `PATH` and the Windows system variables; `HOME`, `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`, the `XDG_*` roots and the temp directory point into the sandbox) and a 10-second timeout. It expects: exit 0 and a version line containing the manifest version; usage on stdout and exit 0 for help; exit 2 for the unknown flag with the message on stderr and stdout empty; no ANSI escape sequence in any output (it is a pipe); no panic; and nothing created, changed or removed anywhere in the sandbox. A timeout (retried once) makes the finding Medium, any other failure Low. One finding per binary, subject = the binary's name, so an allowlist entry with a reason covers a binary that cannot answer. **Not checked, and said so:** `package.json` `bin` entries and Python console scripts; a missing-file argument; `$EDITOR`, config dotfiles in the home root, hand-rolled argument loops, `-v`/`-V` swapped, stdin `-`, `-y`/`-q`/`-v`, `NO_COLOR` and Ctrl-C (queued as ROADMAP M15 and M16). **Not in `classic`** unless named by id (its output is a contract). This is the first rule that runs the built binaries, not only the build scripts (`executes_project_code`; ROADMAP M4's `--untrusted` will refuse it). Proven by `conformance/OPS-011` (seeded defect: a state file written on every start) and by unit tests with one failing program per assertion. The patent sweep for the mechanism is NOT done (see `docs/PATENT-SWEEP.md`, 0.12 addendum).

## 0.11.0 - 2026-10-26 (proposed) - BREAKING

**`API-001` (breaking public changes since the last release) is run by cargo-semver-checks** (decision 285 M8: *"many checks are too valuable to delay, so if others haven't built them, we need to sooner rather than later"*). **Breaking** by the default-on rule (ROADMAP "Default-on policy" item 6, before 1.0 the second number): a Rust library that removes or changes a public item without the version bump the change needs now has a High finding when the tool is installed (`coderipper tools install cargo-semver-checks --install-tools`, a pinned, hash-verified standalone build for Windows, Linux and macOS, x86_64 and aarch64 except Windows arm); without it the rule is a reported gap and changes no exit code. The baseline is the newest release tag below `HEAD`, else the merge-base with the default branch; no tag, no default branch or no library target is a gap, never clean. The tool builds both sides (so build scripts and procedural macros run, and cargo may download dependencies); it runs with its target directory in a scratch directory, so nothing is written into the project. A report that cannot be read is an error. Findings are at the place the item was in the baseline. Real-tool tests (`tests/delegated_semver.rs`) and conformance fixtures (`conformance/API-001`).

**`API-007` (database migrations are safe step by step) is run by squawk** (decision 285 M8, the second of the "build now" list; also **Breaking** by the same policy). With squawk installed (`coderipper tools install squawk --install-tools`, a pinned, hash-verified single-file build for Windows x86_64, Linux and macOS, x86_64 and aarch64) it reads the tracked PostgreSQL migrations (a `.sql` file in a `migrations`/`migrate` directory or named like a versioned migration) and reports the changes that take a table offline or lose data: a dropped column or table (High), a changed column type, a rename, a `NOT NULL` column without a default (High), a constraint added without `NOT VALID`, an index built without `CONCURRENTLY` (Medium). squawk's style advice is excluded; a statement it cannot parse makes a clean result a gap; the repository's `.squawk.toml` is not read, but a `-- squawk-ignore` comment is honoured by the tool and every finding says how many exist. Without the tool the rule is a reported gap. Real-tool tests (`tests/delegated_squawk.rs`) and fixtures (`conformance/API-007`).

**`CON-006` (no lock-order cycles) is built** (decision 285 M8, the third of the "build now" list; default on, **Breaking** with the others). A native Rust rule, from the source alone (`syn`): it reports two functions that take the same locks in opposite orders by direct nesting, and a lock taken while it is held, at High severity and **Low confidence** (a possible deadlock). Locks are named by `self.field`, `self.a.b`, `self.shards[i]` or a `static`; guard lifetimes follow Rust's rules (a `let`-bound guard to the end of its block or `drop`, a temporary to the end of its statement, a plain `if` condition before the body). **Precision, honestly:** run over the default branch (the GitHub tarball, i.e. `origin/main`) of the 38 portfolio repositories that have Rust source: **2 findings in 2 repositories**, one a certain self-deadlock (`wasm-sandbox`: an `if let` scrutinee guard held while the body takes the same `Mutex`; issue filed) and one borderline (`lsp-bridge`: three tokio `RwLock`s read in different orders; a deadlock needs a pending writer; issue filed), none clearly false. A first run read local checkouts and reported a third site in `auth-framework` that exists only on `release/0.4.x` (real there, already reworked on main): a lesson, not a result. Too few findings to state a rate; it ships on because CireSnave's rule is default-on unless a rule produces many false positives. A lock reached through a call, a local variable or a parameter is not seen, so a pass is evidence about direct nesting only.

## 0.10.3 - 2026-10-26 (proposed)

**`--scope FILE`: one command over several repositories** (audit row C7, slice 2; CireSnave, 2026-10-25: *"CodeRipper was designed to work across multiple projects at once right from the start... Lets keep cross-repo abilities."*). `coderipper sweep --scope portfolio.list` (also `fast` and `check ID`) runs the command over each repository the file lists, one path per line (`#` comments, blank lines, relative paths read from the file's directory), each under its own `.coderipper.toml`. Output is grouped by repository (a marker line before each; `coderipper-scope` and `coderipper-scope-summary` lines in JSON), a repository that cannot be read is reported and the others still run, and the exit code is the worst (3 over 1 over 0). A list that names nothing, an unreadable list and `--scope` with `--project` are usage errors. **Not breaking** (a new option). With the result cache (0.9.54) an unchanged portfolio is mostly reused.

## 0.10.2 - 2026-10-26 (proposed)

**A result cache for the checks that build the project** (audit row C7, the "incremental cache" CireSnave's 2026-10-25 ruling and the PM's 2026-10-15 ruling name as the lever). `reachability`, `unused-return-values` and `unused-parameters` analyse the committed tree, so a second run over the same tree, toolchain, features and build environment now reads the stored result instead of building (this repository's `reachability`: 48 s, then 1 s). Correctness never depends on it: only a verdict is stored (never an error or a gap), a damaged or missing entry runs the check, and a run that reused or computed results through the store ends with one stderr line (`coderipper: result cache - N reused (same committed tree), M computed`); it is not a note on the result, so a clean run prints what it always did. Off alone with `CODERIPPER_RESULT_CACHE=off`; off with the build cache (`CODERIPPER_CACHE=off`), which is also the default for a library user. Key and limits in the README. **Not breaking** (no output changes except that stderr line). Also `Cargo.lock`: zip 9.1.1 to 9.1.2 (`dependency-freshness` reports a locked crate that is behind, and the repository sweep fails on it). **Design rules from the patent reading of US10459822B1** (`docs/PATENT-SWEEP.md`, hit 15; not a clearance, the whole-tree residual is on the decisions board before publication): the key stays the whole committed tree plus toolchain plus options, never a per-file, per-snippet or per-function hash; a partly changed tree reuses nothing; the store is local only. The member-level key and `--scope` follow.

## 0.10.1 - 2026-10-26 (proposed)

**CI: `portfolio.yml` installs 0.10.0** (`CODERIPPER_VERSION` 0.9.37 to 0.10.0, now on crates.io). The shared portfolio check of every repository that pins this workflow therefore runs the 0.10 rules, including the new `patent-sweep-record` (advisory) and `SEC-009` (a gap until checkov is installed). Callers pick it up by moving their workflow SHA to the merge commit. No code changes.

## 0.10.0 - 2026-10-26 (proposed) - BREAKING

**Breaking** (before 1.0 the second number is the major): two rules are new and **on by default**, so a project that was clean under 0.9.x can have a finding it did not have (ROADMAP "Default-on policy", item 6).

- **`SEC-009` (least privilege in infrastructure definitions) is run by checkov** (0.9.51). It runs when checkov is installed (`coderipper tools install checkov --install-tools`, a pinned, hash-verified standalone build, no Python needed); without it the rule is a reported gap and changes no exit code. It then reports wildcard and administrator grants and elevated container privileges in tracked Terraform, Dockerfile, Kubernetes, CloudFormation, ARM, Bicep and Serverless files. Checkov has no Apple-silicon build, so there the rule is always a gap.
- **`patent-sweep-record`** (0.9.53): a repository whose root `Cargo.toml` or `package.json` declares a publishable version and that has no `docs/PATENT-SWEEP.md` naming a version in its version line, with headings for ideas, queries and hits and no hit marked `unread`, is reported at Medium. Advisory (not a `--profile portfolio` blocking floor). Opt out with an `[[allow]]` entry and a reason, or `publish = false` / `"private": true`.

Also since 0.9.46, not breaking: `version-and-changelog` accepts a list item (`- 0.6.0-rc1 -- text`) as a changelog entry (0.9.48); `Cargo.lock` zip 9.1.1; the decision-285 rulings are recorded in `docs/` and `ROADMAP.md`; CodeRipper's own patent sweep is `docs/PATENT-SWEEP.md`. Before that, in 0.9.44, every run reports the share of rules that passed (a line on stdout in the extended profile, on stderr under `classic`; `rules_ran`, `rules_passed` and `rules_passed_percent` in the JSON summary): a script that parses the last line of stdout should look for the summary line instead.

## 0.9.53 - 2026-10-25 (proposed)

**`patent-sweep-record`: a release needs a patent sweep on record** (CireSnave, 2026-10-25: *"Ensure a patent sweep has been done of every one of our projects before release."*). A new language-neutral rule, default on, Medium: a repository whose root `Cargo.toml` or `package.json` declares a version that could be published and that tracks no `docs/PATENT-SWEEP.md`, or whose record is not a sweep (no heading for ideas, queries and hits), has a hit with the verdict `unread`, or names no version in the manifest's version line, is reported: a file that exists is not a sweep, and an unread hit is not clear. `publish = false` and `"private": true` are not judged; a project that will never be released opts out with an `[[allow]]` entry and a reason, which stays visible. **Breaking** for a consumer who upgrades (a default-on rule can add a finding to a clean project; before 1.0 that is the second number, but this rule is advisory and not in the `portfolio` blocking floor, which the PM sets). This repository has its own sweep on record (`docs/PATENT-SWEEP.md`).

## 0.9.52 - 2026-10-25 (proposed)

**Docs: CodeRipper's own patent sweep** (`docs/PATENT-SWEEP.md`, CireSnave's standing instruction of 2026-10-25: *"Ensure a patent sweep has been done of every one of our projects before release."*). Sixteen ideas swept, 25 queries, the independent claims of fourteen patents read; all fourteen are clear or designed around, and two watches are recorded. One (US20210200834A1, Atlassian) concerns the unbuilt hosted service only, not the command line or library: the PM read its claims from the USPTO page images and CireSnave ruled (decision 300 = A) that face B keeps one fixed runner image per job, no per-tool image selection by file property, the pinned-tarball fetch with no clone, and runs on user submission only (ROADMAP H9; hosted-service plan §3 item 10). Due-diligence read, not legal advice. No behaviour changes.

## 0.9.51 - 2026-10-25 (proposed)

**`SEC-009` is run by checkov** (audit row M7, CireSnave's ruling of 2026-10-25 that a good tool is supported whatever language it is written in). The pinned standalone checkov 3.3.30 build (no Python) is installed with `coderipper tools install checkov --install-tools` and run offline over the project's tracked Terraform, Dockerfile, Kubernetes, CloudFormation, ARM, Bicep and Serverless files, restricted to 37 checks that are the rule's wording (wildcard and administrator grants, privilege escalation, privileged containers, shared host namespaces, added capabilities, root containers). A `checkov:skip` comment is not honoured and the finding says so; an unparsed file or a project with no definition is a gap, never clean. Adds `checkov` to `tools.lock` (three platforms) and the rule to the extended profile; the README no longer says the rule is declined. The isolated Python-environment installer for tools with no binary stays on the roadmap.

## 0.9.50 - 2026-10-25 (proposed)

**Docs: decision 285 recorded** (CireSnave's rulings of 2026-10-25 on the sixteen audit calls). New: `docs/superpowers/plans/2026-10-25-decision-285-rulings.md` (the 70% default-on threshold and the performance rules' positions, the valuable-and-unbuilt checks, the provenance record schema, the dashboard stack options, the cross-repository and result-cache plan) and `docs/hosted-service/sandbox-hardening-gate.md` (the checklist CireSnave approves before any submitted repository is built). The terms of service are marked approved; H3 and H8 are recorded in the hosted-service plan; ROADMAP rows C7, M8, M12, H2 to H8, R1 to R3, T1 and T2 carry their rulings. Also `Cargo.lock`: zip 9.1.0 to 9.1.1 (the `dependency-freshness` rule reports a locked crate that is behind, and the repository sweep fails on it). No behaviour changes.

## 0.9.48 - 2026-10-25 (proposed)

**`version-and-changelog` accepts a list item as a changelog entry** (found by the auth-framework lane's portfolio run): a line such as `- 0.6.0-rc1 -- what changed` now names its version, as `## 0.6.0-rc1` and `[0.6.0-rc1]` already did; before, every version-bump PR to a repository whose changelog is a bullet list got "has no entry for Cargo.toml version X". The whole-token rule is unchanged (`0.6.0` is still not an entry for `0.6.0-rc1`, nor the reverse), and prose that merely mentions a version is still not an entry.

## 0.9.46 - 2026-10-25 (proposed)

**`patent-index-match`: a yarn or pnpm entry that is the repository's own code is not a registry package, and a bare `npm:` alias names its real package** (review of #164). A yarn Berry `name@workspace:`, `link:`, `file:`, `portal:`, `exec:`, tarball or git descriptor, and a pnpm key whose version is a path or an address, no longer match an index name as if they were the registry package of that name (`name@workspace:pkg` with `version: 0.0.0-use.local` was read as `name` at 0.0.0-use.local); `alias@npm:real` with no range reads as `real`. Red-first tests for both.

## 0.9.44 - 2026-10-25 (proposed)

**Every run reports the percentage of rules passed** (audit row M3, CireSnave's ruling of 2026-10-25). Of the rules that ran to a verdict, the share that found nothing: the human output ends with `rules passed: N of M that ran (P%)`; the JSON summary line gains `rules_ran`, `rules_passed` and `rules_passed_percent` (`null` when no rule ran), and the coverage line's `run` object gains `passed_percent`. Rules that did not apply or could not run are in neither count; coverage (how much of the catalogue CodeRipper can check) is a separate number, defined beside it in the README. Never changes the exit code. **Not breaking** (new keys only); the golden summary-key test is updated. `--min-coverage` stays on the roadmap.

## 0.9.43 - 2026-10-25 (proposed)

**`patent-index-match` reads `yarn.lock`, `pnpm-lock.yaml` and `gradle.lockfile`** (audit row C8). Yarn classic and Berry (an `npm:` alias names the real package), pnpm lockfile versions 5, 6 and 9 (`packages` only; `snapshots` repeats it), and Gradle's `group:artifact:version=configs` lines. Like the other lockfiles, a package only a lockfile names is graded one step below the index's action at low confidence, and a package a manifest declares is reported once, from the manifest. **`go.sum` is not read, on purpose:** the pinned index lists crates, PyPI, npm and Maven names only, so a Go lockfile could match nothing; the note says so rather than implying coverage.

## 0.9.42 - 2026-10-25 (proposed)

**Docs: hosted v1 runs with DOC-010 off and osv-scanner limited to the OSV host (CireSnave's H6 ruling); `buf` and `zizmor` re-verified for network and plugin use by reading the code.** No behaviour changes.

## 0.9.41 - 2026-10-25 (proposed)

**`ci-protection-presence`: a ruleset-only default branch is audited for force-push and deletion** (audit row C5, part). A branch that passes on its rulesets alone (no classic protection) now gets one `Medium` finding for each of `non_fast_forward` and `deletion` that no active rule for the branch (`rules/branches/<branch>`) provides, read on its own so one present does not hide the other missing. One more GET in that case. A branch with classic protection is audited from its classic settings as before. Still open in C5: required reviews and signed commits, required-check names matched against the workflows, non-default branches, non-GitHub forges, rate limits, and telling a taken-over fork from a plain one.

## 0.9.40 - 2026-10-25 (proposed)

**Docs only: the per-member footprint design records that part B is not needed.** Fuel's first hosted Linux run of the pinned portfolio job (fuel#499) used about 6 GB of a 145 GB root filesystem with 80 GB free, and a Windows measurement of the build cache for 45 members gave 3.8 GB; the "14 GB" runner was an assumption. Part A (`[[skip_member]]`, 0.9.35) stands. `docs/member-footprint-design.md` has the numbers and the condition for reopening.

## 0.9.39 - 2026-10-25 (proposed)

**`portfolio.yml` installs 0.9.37, and checks it did** (PM task, 2026-10-25; the change that follows each publish). `CODERIPPER_VERSION` moves from 0.9.24 to 0.9.37 (published and tagged, f26b741e), which brings `dependency-freshness`, `[[skip_member]]` and the licence-holder ruling to every caller that moves its SHA. New step: after the install, `coderipper --version` must equal `coderipper $CODERIPPER_VERSION` or the job fails and prints both (a caller once pinned a SHA that installed 0.9.20 while saying 0.9.24). A test keeps the step in the file.

## 0.9.38 - 2026-10-25 (proposed)

**`portfolio.yml` logs disk use** (PM task, 2026-10-25): two log-only steps, `df -h /` and `du -sm` of the build directory, `~/.cargo` and the binary cache, before the install and (`if: always()`) after the sweep. Fuel's portfolio job runs out of disk on a hosted runner and a Windows measurement of the sweep's build cache (3.8 GB for Fuel's 45 members) does not explain it; callers cannot add steps to a reusable workflow, so the numbers have to come from here. No behaviour change; callers pick it up by moving their SHA.

## 0.9.37 - 2026-10-25 (proposed)

**`licence`: a person as copyright holder is Info, not Medium** (PM task, 2026-10-25). CireSnave's standing ruling, relayed by the PM: *"All licenses/copyrights can name me"*. Both findings (a licence file, and source-file headers) keep their text but are Info and say it is a standing ruling, not a defect. The "no licence file" High, the manifest mismatch and the missing-SPDX Medium are unchanged. Checks that deny at Medium no longer fail on a holder line.

## 0.9.36 - 2026-10-25 (proposed)

**Scheduled audits** (audit row C6, part): `.github/workflows/scheduled-audits.yml` runs `dependency-freshness`, SUP-002, SEC-006 and DOC-010 daily and on dispatch, built from this repository's source, and fails when a rule did not run. Each passes locally at 07ff52e. The portfolio-wide schedule in `portfolio.yml` waits for a published release with `dependency-freshness` (0.9.28 or later); npm, pip, Maven and Gradle lockfiles stay open.

## 0.9.35 - 2026-10-25 (proposed)

**`[[skip_member]]`: a declared, reasoned skip of a workspace member** (PM task 2026-10-25; Fuel's portfolio job fails on members that need an SDK a hosted runner lacks). An entry in the repository's `.coderipper.toml` (`name`, `reason`, `owner`, optional `needs` = an executable that must be absent from `PATH`) keeps `--workspace` from building that member. Each run says `<member>: not judged (declared skip): <reason> (owner ...)` and counts it; it is not a failure. A `needs` tool that is present means the member is built. An entry naming no package is an `allowlist` finding. `WorkspaceRun` gains `members_skipped` (the struct is `#[non_exhaustive]`). The build-cache bound (design part B) is not in this change. Design: `docs/member-footprint-design.md`.

## 0.9.34 - 2026-10-25 (proposed)

**Docs only: design for the per-member footprint limit** (PM task, 2026-10-25; Fuel's portfolio job overflows a hosted runner and fails on members that need an SDK). `docs/member-footprint-design.md`: a declared, reasoned `[[skip_member]]` entry that reports a member "not judged" without failing the job, and a build cache bounded during a run. No code changed. 0.9.33 is proposed by #156; the PM allocates the number.

## 0.9.32 - 2026-10-25 (proposed)

**Docs only: audit row D3 closed.** The `reachability/mod.rs` limitation note said a `--project` nested in a larger repo analyzes the enclosing root; it analyzes the nested crate (diagnostic paths re-based from the workspace root), and a directory that is not a package's own is refused with the member names. Comment change only.

## 0.9.31 - 2026-10-25 (proposed)

**Tag every publish** (issue #94, "Tag the source"). `tools/release-tag.sh tag <version> --push` tags the commit a published version came from, read from the crate's `.cargo_vcs_info.json`; `check` fails when origin lacks the newest published version's tag, and `release-drift.yml` now runs it. `RELEASING.md` records the process. Tags `v0.9.20` and `v0.9.24` were created (annotated, pushed, read back). **No other version is tagged**: 27 of the 34 published versions carry no vcs info and 5 name a commit that is not in the repository, and a tag on a guess would be false.

## 0.9.30 - 2026-10-24 (proposed)

**Docs only: audit rows D1 and half of D3.** The eleven built rules (API-006, DOC-005, DOC-009, DOC-010, SEC-006, SUP-001, SUP-002, SUP-004, SUP-008, SUP-011, WSP-001) no longer read "no check yet" on their `docs/rules` pages; each row says native or delegated and what is checked, taken from the rule's `checked_today`. The README's `patent-index-match` row now says lockfiles and Gradle version catalogs are read (0.9.24) and names the four lockfile kinds that are not. Also D2: the stale status lines of the audit-host, multi-language and programme, three-faces and triage documents, the coverage doc's settings-audit row, and two supersession notes (performance, security). Still open: the `reachability/mod.rs` doc comment (a code change) and the provenance plan's data-file counts, which this lane could not re-verify.

## 0.9.29 - 2026-10-24 (proposed)

**Docs only: the spec-to-roadmap audit** (PM task, 2026-10-24). `ROADMAP.md` gains 53 rows for work that a spec, plan or README describes and the roadmap did not name (16 need a decision), each with its source, a status checked against the code, and what is read in full against in part. Three stale statements in `ROADMAP.md` are corrected (six native Rust checks, the unreproducible "96", `dependency-freshness` built). No code changed.

## 0.9.28 - 2026-10-24 (proposed)

**New check `dependency-freshness`** (coverage doc section 4, build order 6): is every package in `Cargo.lock` at the newest version the registry has? CireSnave's rule, CLAUDE.md 9: *"I want all of my projects' dependencies on their most recent versions at all times."*

- Reads `cargo update --dry-run --verbose` (never writes the lockfile): a package with a newer compatible version, and one a manifest requirement holds below a newer release, is one finding each. A dependency a workspace member declares is **Medium**, a transitive one **Low**. `Adding` lines are not staleness.
- Network tier (a sweep check), meant for a schedule: a PR would turn red the day any dependency published. A package that cannot move is waived with `[[allow]]` (`check = "dependency-freshness"`, `file = "Cargo.lock"`, `symbol` = the package name) and its reason.
- No `Cargo.lock` is one Info finding; a cargo failure, or output with no sign the dry run finished, is an error, never clean. Cargo only: npm, pip, Maven and Gradle lockfiles are not read.
- **`coderipper sweep` runs one more check**, so a sweep with `--deny info` now fails on any stale dependency. This repository's lockfile was brought current in the same change.
- Tests: `each_package_behind_is_one_finding_and_adding_is_not_staleness`, `a_fresh_lockfile_is_empty_and_a_cargo_failure_is_an_error`, `no_lockfile_is_an_info_finding_not_a_clean_run`, `updating_and_unchanged_behind_lines_are_read_and_the_rest_is_not`.

## 0.9.27 - 2026-10-24 (proposed)

**`ci-protection-presence` also audits repository settings** (coverage doc section 4, build order 5, the settings-audit extension).

- One **Medium** finding per setting that differs from CireSnave's (6.5; CLAUDE.md 3 and 4): `allow_squash_merge` must be on, `allow_merge_commit` and `allow_rebase_merge` off, `delete_branch_on_merge` on; and on a branch with classic protection `enforce_admins` must be **off** (admins keep a bypass, 2026-09-18), `allow_force_pushes` and `allow_deletions` off.
- A branch governed only by rulesets gets the repository-level audit alone (ruleset `non_fast_forward`/`deletion` rules are not read yet).
- **A fork is skipped** with one Info finding (6.5: forks he has not taken over are not his). A taken-over fork is not told apart.
- Reading needs admin on the repository: a reply without a field, or a failed `branches/<b>/protection` read, is an error, never clean. **A caller whose token lacks admin now gets an error from this check where it got a verdict before.** The portfolio sweep runs as the PM with admin; the repo's own CI already skips the rule.
- Tests: `each_setting_that_differs_is_its_own_finding_the_positive_control`, `the_portfolios_own_settings_are_clean_the_negative_control`, `admins_who_can_bypass_are_the_wanted_state_not_a_finding`, `merge_settings_a_token_cannot_see_are_an_error_never_clean`, `a_protection_reply_without_a_setting_is_an_error_and_a_failed_read_is_one_too`, `a_fork_is_not_audited_at_all`, `a_branch_governed_only_by_rulesets_gets_the_repository_audit_alone`.

## 0.9.26 - 2026-10-24 (proposed)

The `ci.yml` self-call is repinned to `portfolio.yml@ee9c3950`, the merge of 0.9.25, whose workflow installs 0.9.24 (the pin before it ran 0.9.20). No code change.

## 0.9.25 - 2026-10-24 (proposed)

**CI: `portfolio.yml` installs coderipper 0.9.24** (published 2026-10-24, read back from crates.io), so callers get `patent-index-match` on agent-safety-bed 1.4.0 and its lockfile and Gradle version catalog reading. No code change. Callers repin to the merge of this change.

## 0.9.24 - 2026-10-24 (proposed)

**`patent-index-match` reads lockfiles and Gradle version catalogs**, so a package no manifest declares (a transitive dependency) can match the index. Follow-up to the rule's "cannot see" list; 0.9.23 (the 1.4.0 index data) ships in the same release.

- Read: `Cargo.lock`, `package-lock.json` (lockfileVersion 1, 2 and 3), `poetry.lock`, `uv.lock`, `Pipfile.lock`, and `*.versions.toml` (`[libraries]` as `"g:a:v"`, `{ module }` or `{ group, name }`; read as a manifest, so a match there is graded like a declared dependency). Not read: `yarn.lock`, `pnpm-lock.yaml`, `go.sum`, `gradle.lockfile`.
- **A locked match is graded one step below the index's action and at low confidence** (High becomes Medium; Medium and Low become Low), because the repository may not choose a transitive package: a transitive `avoid` entry is a Medium finding and cannot fail the portfolio gate (floor High); a declared one still does. The summary says "locked package ... declared by no manifest read, so probably transitive". **This is a design choice** (agreed with the PM, 2026-10-24): the repository may not control a transitive package, so a lockfile alone does not carry the full grade. **A package that is both declared and locked grades as declared**: it is reported once, from the manifest, at the index's own action and medium confidence (a declared `avoid` is High and does fail the portfolio gate), and the lockfile entry adds nothing.
- A locked package that any manifest in the repository declares is reported once, from the manifest (at the declared grade, see above); one package in two lockfiles (or two versions) is one finding. The repository's own packages are not judged: a Cargo package with no `source`, a package-lock entry outside `node_modules/` or marked `link`, a uv package whose source is `virtual` or `editable`.
- The note now reads "N lockfile(s) read, M locked package(s) compared"; a repository with only lockfiles is no longer "not applicable". An unparseable lockfile is a note, as for manifests.
- Tests: `a_gradle_version_catalog_is_read_in_all_three_library_forms`, `a_locked_package_no_manifest_declares_is_one_step_lower_and_low_confidence`, `a_package_a_manifest_declares_is_reported_once_from_the_manifest`, `the_projects_own_packages_in_a_lockfile_are_not_judged`, `package_lock_v3_v1_and_pipfile_lock_are_read`, `an_unparseable_lockfile_is_a_note_not_a_clean_run`.

## 0.9.23 - 2026-10-24 (proposed)

**`patent-index-match` reads agent-safety-bed 1.4.0** (was 1.2.0; upstream commit `3074b3c`, `data/patent-tiers.PIN` records version, commit and sha256, and the data, `LICENSE` and `NOTICE` are copied from that commit unchanged: upstream's copyright line now reads "Thinker's Journal" where our 1.2.0 copy said "Eric Evans").

- **What changes in findings:** the same 66 entries and the same package names (74 names, 18 entries with none), but 1.4.0 re-graded the evidence behind each entry ("reported, not verified" where no patent database was searched), so the `action` of 28 entries moved: 29 entries are now `flag-for-review` (was 1), `allow` 13 (was 26), `allow-with-conditions` 14 (was 21), `separate-component` 7 (was 15), `avoid` still 3. A dependency that was silent (`allow`) or Low can now be Medium: e.g. `cadical`, `llama_cpp_2`, `stable-baselines3`, `pyro`, `swe-bench` are Medium. `avoid` (High) is unchanged: `gensim` and the other two. **The portfolio floor for the rule is High, so no caller's gate changes**; the extended profile reports more Medium findings.
- Tests: the two tests that named severities (`cargo_names_match_by_hyphen_underscore_and_by_package_rename`, `pyproject_pep621_optional_groups_and_poetry_are_read`) now expect the 1.4.0 grades; `the_pinned_index_matches_its_pin` checks the new sha256.

## 0.9.22 - 2026-10-24 (proposed)

The `ci.yml` self-call is repinned to `portfolio.yml@e40c8557`, the merge of 0.9.21, whose workflow installs 0.9.20 (the pin before it ran 0.9.6). No code change.

## 0.9.21 - 2026-10-24 (proposed)

**CI: `portfolio.yml` installs coderipper 0.9.20** (published 2026-10-24, read back from crates.io), the first release whose Rust checks do not need a target to link, so callers building with `DOCS_RS=1` (baracuda) stop failing with "linking with cc failed". Callers repin to the merge of this change.

## 0.9.20 - 2026-10-24 (proposed)

**Fix: the Rust checks no longer need the target to link.** `unused-return-values`, `unused-parameters` and `reachability` built with `cargo build --all-targets`, so with `DOCS_RS=1` a bin or example that needs a native part the skipped build script did not produce (baracuda's `kernels-sys` symbols) compiled but failed with "linking with cc failed", and the rules reported the crate as unbuildable (baracuda run 40098520475: 21 errors at 0.9.6, 4 at 0.9.14). They now run `cargo check` (same lints, no codegen, no link). Test: `a_target_that_cannot_link_does_not_break_the_analysis` (a bin calling an undefined extern symbol: red on `cargo build`, green on `cargo check`, and the lint is still reported). The cross-run staleness tests now look in `.rmeta` as well as `.rlib`, since `check` writes metadata only.

## 0.9.19 - 2026-10-24 (proposed)

Docs only: the provenance design (0.9.15) corrected after the files were opened and the coverage rule made strict, as the PM's routing and the fuel lane's question needed. **The 0.9.15 headline, "the portfolio keeps its data well", is withdrawn:** the first scan counted a record as covering a file if it sat in the same directory or merely mentioned the directory anywhere, so a record that names other files hid the files it does not name (the fuel lane was right that nothing sits next to `qwen3_0_6b_cpu_reference.json`). With the rule that a record covers a file only if it names the file or shares its stem: **42 of 75 data files and fixtures are not named by any provenance record** (24 in fuel, 6 in Humboldt, 3 in unisql, 2 in auth-framework, 1 in each of six more), 32 right on inspection, 5 unclear, 5 not; and 2 of the 33 that are named lack a content hash (0.9.15 said 4, then 3). Also: `vulkane/vulkan.jpeg` is a painted Vulcan, not the Khronos logo; `baracuda/assets/barracuda.png` is a photograph-grade image with no source; auth-framework's `test_private_key.pem` is described by its README. The incident conclusion stands: the one detector is the network licence check.

## 0.9.18 - 2026-10-24 (proposed)

**Fix (urgent): `portfolio.yml` installed a stale CodeRipper.** `CODERIPPER_VERSION` was 0.9.6 on `main` while 0.9.7 to 0.9.14 were published, so no workflow SHA could install `public_api_crates`, the ratchet wording, `--summary` or any later fix (Unpopped found it; the previous repin notice pointed at a SHA that installed 0.9.6). It is now **0.9.14**, the newest published release. **It cannot drift again unnoticed:** `tests/portfolio_version.rs` checks, always and offline, that the version is well formed and not ahead of `Cargo.toml` (it would not install), and, in the new scheduled `release-drift.yml` (daily, and on demand), that it **equals the newest non-yanked version crates.io lists**; a workflow behind the newest release fails with "every fix since is unreachable from CI; bump CODERIPPER_VERSION and repin the callers". Control: the drift that happened (0.9.6 against 0.9.14) fails, equal passes, ahead (unpublished) fails, a malformed version fails; the index reader ignores a yanked version. The comparison is with what is *published*, not with `Cargo.toml`, because a docs-only or workflow-only version is not published and the workflow must not name it. **Every caller must repin** to the SHA of this change's merge; after each future publish the workflow bump is the next change.

## 0.9.14 - 2026-10-24 (proposed)

Fix (commy reported the portfolio job red on `main`): the `lint-suppression` ratchet on a run of the default branch now says **`ratchet not applicable: HEAD is the merge-base ... (a run on the default branch)`**, not `ratchet not run: ...`. There is no branch to compare on `main`, so nothing failed to run; `portfolio.yml` fails a job on any "not run:" note, and only a text exception for this one note kept `main` green. A real gap (no base found, a base file that cannot be read or parsed) still says `ratchet not run`, and the job still fails on it. Tests: the note for each case, and `the_ratchet_on_the_default_branch_is_not_applicable_not_a_rule_that_did_not_run` (a repository whose `origin/main` is its own `main` says not applicable; the same repository on a branch that adds a suppression runs the ratchet and reports it).

**commy's red run has a second cause that no release fixes:** commy calls `portfolio.yml@c665210`, the 0.8.19-era workflow, whose guard predates the exception added in 0.8.22 (it fails on the default-branch note). A repository on that pin must repin to a `portfolio.yml` of 0.8.22 or later; the 0.9.14 wording makes the exception unnecessary for the next pin.
## 0.9.15 - 2026-10-24 (proposed)

Docs only: `docs/superpowers/plans/2026-10-24-provenance-rules.md`, the design and measurement for family 2 (provenance and licence of committed data and code), with CireSnave's standing rule as its premise and fuel's recorded Qwen-AWQ incident as its test. **The incident has exactly one detector, a check of the licence a record claims against the licence the source publishes (network, sweep tier)**: replayed on the pre-rewrite commit, the static rules are silent on it (the record existed, named a pinned revision, and claimed a compatible licence) except that the record carried no hash of the committed bytes. Six rules (SUP-014 to SUP-019) with precision notes; a census of the portfolio's 45 committed data files (*0.9.19 corrects both counts and withdraws the "well kept" reading: 42 of 75 data files are named by no provenance record, 2 of 33 named files lack a hash*), a sample of 21 of 42 branch-URL provenance comments (8 right), and a check that no real third-party copyright holder in a header is uncredited. Nothing is built; the scripts are kept under `provenance-prototype/` as `.txt`.

## 0.9.13 - 2026-10-24 (proposed)

Docs only: the re-sample of the unzeroized-secret rule (SEC-011) outside auth-framework, as the PM asked after 0.9.12. A fresh 30 drawn from 54 findings in 12 repositories is **14 of 30 where a reviewer would act (47%), 19 of 30 right (63%)**; the first sample's 83% was 22 parts auth-framework in 30. **Withdrawn:** 0.9.12's headline and plan said the rule clears the gate and ships default-on; it does not, and is opt-in until a matcher that drops the bare field name `token` clears a fresh 30 outside both samples. The retraction is made in the design document's headline, catalogue row, results table, section 6, plan and open points, and in the 0.9.12 entry below. A v4 matcher (77% on the same 30, 10 of 13) is recorded as a hypothesis only, because it was read off that sample. The sampler and table generator are added under `security-prototype/`.

## 0.9.12 - 2026-10-24 (proposed)

Docs only: `docs/superpowers/plans/2026-10-24-security-crypto-rules.md`, the design and measurement for family 1 (security and cryptography) of the ROADMAP's next rule families. Eleven rules with precision notes; a throwaway detector validated against **seven recorded incidents** in auth-framework and synapse (it separates the commit before from the commit after on five, and the document says the other two are not statically checkable); a measurement on eight repositories. ~~One rule clears the precision gate (unzeroized secret fields: 25 of 30 where a reviewer would act)~~ *withdrawn in 0.9.13: 22 of those 30 were auth-framework; a re-sample outside it is 14 of 30, and no rule clears the gate*; weak algorithms are an inventory (all ten are protocol-mandated); the name-based log and hard-coded-constant rules do not clear. Its findings in auth-framework were reported privately (one was a real vulnerability, fix in progress) and are not printed. Nothing is built; the detector and controls are kept under `security-prototype/` as `.txt`.

## 0.9.11 - 2026-10-24 (proposed)

Fix (Humboldt#131, run 538a60c): `portfolio.yml` no longer installs into `~/.cargo/bin`. On a shared self-hosted runner `cargo install coderipper` failed with "failed to move ... coderipper.exe to ...\.cargoin\coderipper.exe: Access is denied (os 5)": parallel runs share that directory and a running executable cannot be replaced; the compile also took 13m43s on every run.

- Each run builds with `--root $RUNNER_TEMP/coderipper-build` and prepends the resulting `bin` to `PATH` (through `GITHUB_PATH`).
- The finished tree is published to a version-keyed cache, `${CODERIPPER_BIN_CACHE:-$HOME/.cache/coderipper-bin}/<version>-<os>-<arch>`, by a rename: a run that finds a usable copy compiles nothing; two runs that race both finish with a complete copy (the loser removes its staging copy, and one that was moved *into* the winner's directory is taken back out); a half-written tree is never visible. A cached binary is used only if `--version` reports exactly the pinned release; a damaged copy is cleared and rebuilt. Nothing is written over a binary that may be running.
- On a GitHub-hosted runner the cache directory is restored and saved by `actions/cache` (v6.1.0, the pin `ci.yml` already uses), keyed on the OS, architecture and version; a self-hosted runner just keeps the directory.
- Checked locally with a fake `cargo` that sleeps three seconds: two concurrent runs both exit 0 and publish one copy (two builds, as expected, since both started before either finished); a third run uses the cache and calls `cargo` zero times; no staging directory is left behind; a copy whose `--version` is wrong is replaced; `~/.cargo` is never created. **Not yet run on a Windows self-hosted runner**: the Windows path differs only in the `.exe` name, and the run that proves it is two concurrent pull-request runs on the Humboldt runner.
## 0.9.10 - 2026-10-24 (proposed)

Docs only: `docs/superpowers/plans/2026-10-24-performance-rules-precision.md`, the precision sample of the performance (`PRF`) candidate rules (the PM's task: 30 per rule). **No rule clears the gate** (at least 80% true positives on at least 30 findings): the allocation-in-loop family is 0 of 42, `to_owned`/`to_vec` 6 of 20, linear-search forms 1 of 22, `vec-new-then-push` 7 of 9 (a census), `unbuffered-io` 2 of 2 (a census). The detector (a throwaway, with two positive controls and a seeded order), every verdict and its reason, and the rule changes that follow are in the document; the detector and controls are kept under `perf-prototype/` as `.txt`. Restates the design document's 12-finding sample as 3 of 12 under the stricter rubric. Nothing is built.
## 0.9.9 - 2026-10-24 (proposed)

A package with no `src/` is analysed from its cargo targets (the PM's ruling after baracuda's `examples`, which has only `[[bin]]` entries with a `path` at the package root and made all three Rust checks die with a bare "os error 3" until 0.9.6 named the cause). `cargo metadata` gives each library and binary target's root file (`Package::roots`, libraries first); when `src/` does not exist the rewrite reads and rewrites those root files, and the sentinels (positive controls) go into them: `reachability` and `unused-return-values` into the first root, `unused-parameters` into every root. Checked on the real layout: `check reachability`, `unused-parameters` and `unused-return-values` on baracuda's `examples` at 8e162f7 now run and report no issues (unused-parameters notes that `cutlass_gemm.rs` and `forge_hello.rs` are not built: their `required-features` are off).

**Still refused, by name:** a root file outside `src/` that declares a file module (`mod helper;`), because the module's file would be neither rewritten nor read and the result would look complete and not be; the error names the file and the module. Tests: `a_package_whose_targets_are_outside_src_is_analysed_from_its_cargo_targets` (the layout, all three checks run, a seeded unused function and an ignored return are found) and `a_target_outside_src_with_a_file_module_is_refused_by_name_not_half_read`.

## 0.9.8 - 2026-10-23 (proposed)

The `ci.yml` self-call is repinned to `portfolio.yml@66d358e1`, the merge of 0.9.7 (its workflow installs 0.9.6, which has `DOCS_RS` handling and the "not judged" note; the pin before it ran 0.8.19).

`--summary` (the PM's request, to track a portfolio sweep): per-rule counts for a run. Human: a totals line and one line per rule after the findings. JSON: one `coderipper-rule-summary` line before the `coderipper-summary` line, with stable fields `reason`, `project`, `findings`, `waived`, `declared_public`, `opt_outs`, `errors` and `rules` (`rule`, `findings`, `waived`, `declared_public`, `severities`). `opt_outs` is `[[allow]]` waivers plus `public_api`/`public_api_crates` declarations: the visible opt-outs of the default-on policy. A rule that ran clean has no row. No exit code changes. Tests: `tests/summary_cli.rs` (absent without the flag; human; JSON order and fields, totals equal the rows' sums) and two unit tests.

## 0.9.7 - 2026-10-23 (proposed)

`[reachability] public_api_crates`: declare a whole crate's public items as API with one entry (baracuda's second sweep, run 40025510102: 753 `reachability` findings, of which 725 of the 730 that parse are in crates with `publish` not `false`, and 12 of 12 sampled are `pub` items reachable through a `pub mod` tree, so intended API). `{ crate = "<package>", reason = "..." }` in the **workspace root's** `.coderipper.toml` covers every `reachability` finding of that cargo package, printed as `[declared-public]` with the reason. **Each member's own run reads it** (the nearest enclosing workspace root's file, entries naming that package only), so a run by hand of one member, `--project crates/x`, agrees with `--workspace` and with CI (PersistAnt had reproduced CI member by member and got different results). Why a new key: a member's finding paths are relative to the member, and a member's run reads only the member's own file, so the root's `public_api` globs could not name one crate. A name that is no package of the workspace is reported by name by the `--workspace` run. Tests: three unit tests, `a_crate_level_public_api_declaration_covers_one_package_of_a_workspace` (one package covered, the other not; a misspelt name is reported) and `a_member_run_by_hand_agrees_with_the_workspace_run_on_a_root_only_declaration` (root-only config, two members, `--project` on each).

`portfolio.yml` moves `CODERIPPER_VERSION` to 0.9.6 (the first release with `DOCS_RS` handling, the "not judged" note and the protoc step's companion fix); the `ci.yml` self-call is repinned in a follow-up, to the merge of this change.

**Decision pending (not changed here): should `reachability` treat the public items of a published library as API by default?** Evidence is in the PR description; the default is unchanged.
## 0.9.6 - 2026-10-23 (proposed)

Fix (baracuda's second sweep, run 40025510102): the Rust checks assert nothing for a member that cannot be built, and the output did not say which.

- `portfolio.yml` sets `DOCS_RS=1` for the sweep step: a build script that honours the docs.rs convention skips its native or CUDA toolchain (nvcc), so such a crate compiles on a hosted runner. **The analysis is then static only**: it reads the source and does not prove the native part builds.
- A `--workspace` run now ends with a note naming each member that could not be built or run ("not judged (...): a, b"), beside the per-check errors it already printed (`WorkspaceRun::members_failed`, a new field on a `#[non_exhaustive]` struct). A member whose build still fails (a build script that ignores `DOCS_RS`) stays an error, so the job fails on it instead of passing for the others.
- A package whose targets are outside `src/` (only `[[bin]]` entries with a `path` at the package root: baracuda's `examples`) failed with a bare "The system cannot find the path specified (os error 3)". It now says "<dir> has no `src/` directory: this package's targets ... are outside `src/`, which the Rust checks do not support yet, so none of them was judged". **Supporting such a package is not done**; it needs the targets' `src_path` from `cargo metadata`.
- `portfolio.yml` installs `protobuf-compiler` on a GitHub-hosted Linux runner, so a crate whose build script needs `protoc` (mlmf's `prost-build`, run 40034994212) can be judged; not on a self-hosted runner. The "not judged" note above is the other half of that finding: a crate that still cannot be built is named, not left as an unlabeled exit 3.
- Tests: `a_member_that_cannot_be_built_is_named_as_not_judged_and_docs_rs_lets_it_be_read` (the member fails without `DOCS_RS`, is named, and is analysed with it) and `a_package_whose_targets_are_outside_src_says_so_instead_of_a_bare_os_error`.
## 0.9.5 - 2026-10-23 (proposed)

Docs only: `ROADMAP.md` gains "Next rule families" (the PM's tasks carrying CireSnave's words, 2026-10-23): the default-on policy in eight points, and fourteen families, each with three candidate rules and the incident that is its positive control (or a statement that none was supplied). Family 13 (documentation) has four tiers: deterministic checks including doc-comment presence and staleness, a prose-lint tier (native Rust set, optional Vale shell-out), metrics, and an off-by-default LLM tier. Nothing is built.

## 0.9.4 - 2026-10-23 (proposed)

`ci-security`: the self-hosted-runner finding now says the job "names no GPU or other hardware it needs" and prints the `[[allow]]` entry that waives it (`file`, `symbol = "<job>:runner"`, a `reason` placeholder), so a lane need not look it up. The rule's doc states the precision: GPU words stand in for "needs our hardware", so a self-hosted job that needs other hardware (PersistAnt's power-loss rig) fires every time, by design, and is waived with a reason. No change in what fires.
## 0.9.3 - 2026-10-23 (proposed)

Docs only: `docs/superpowers/plans/2026-10-23-performance-rules.md`, the design for the `PRF` rule family (PM task from CireSnave 2026-10-23): eleven statically checkable rules with precision notes and positive controls, what is not checkable and must never become a rule, "performance claim without a benchmark" as a check, the scoping that keeps it quiet (`[perf] hot_paths`, opt-in until a precision gate), how it avoids contradicting the Clean Code rules, and a measurement on fuel, baracuda and Unpopped with 12 sampled findings judged one by one (4 of 12 true; the allocation-in-loop family 0 of 7). Nothing is built.
## 0.9.1 - 2026-10-23 (proposed)

Fix: `portfolio.yml` passes `--workspace` to the sweep when the root `Cargo.toml` has a `[workspace]` table. `--project .` on a virtual workspace root made `reachability`, `unused-return-values` and `unused-parameters` error ("not a package directory"; baracuda run 40013917052, the same layout as fuel), and on a root package that also has members (auth-framework, synapse) it analysed the root only and skipped the members without saying so. A repository with no `[workspace]` runs as before. Cost: a workspace run compiles every member (fuel measured 1742 s cold on 2026-10-15), so a large workspace's advisory job is slow on a cold runner; the build cache is the lever. Test: `a_virtual_workspace_root_runs_the_rust_checks_only_with_workspace` (without the flag the root errors; with it a seeded defect in a member is found). **Whose coverage changes** (layouts read from the forge on 2026-10-23): baracuda and fuel (virtual workspaces: the three Rust checks ran nowhere before, they now run on every member) and auth-framework and synapse (a root package plus members: only the root was analysed, the members now are), so their finding counts rise; lightbulb (one package) is unchanged. **Build cache:** on a GitHub-hosted runner the job now restores and saves CodeRipper's build cache (`actions/cache` v6.1.0 pinned to its SHA, keyed on the CodeRipper version and `Cargo.lock`; saved even when the sweep fails on a finding), so a repeat run compiles only what changed. A self-hosted runner keeps its own cache directory and skips the steps. First-run and cached times are NOT measured yet: the only figure on record is fuel's 1742 s cold whole-workspace reachability run (2026-10-15, local); the first hosted runs will be read and reported.

## 0.9.0 - 2026-10-23 (proposed) - BREAKING

**The default profile now runs every rule.** `coderipper`, `cargo coderipper`, `fast`, `sweep` and `check` with no `--profile` used to run the `classic` profile (the five original checks and `conflict-marker`); they now run `extended`: those, plus every language-neutral rule, the rules delegated to a tool (a tool that is not installed is a reported gap, never a failure; `--install-tools` fetches them), and the coverage report. CireSnave's ruling (board 144); the PM allocated 0.9.0 (pre-1.0, so the second number).

- **What a consumer sees:** a project that ran the default and fails on `--deny` (the default `info`, or `--deny medium`) can now fail on findings of rules it never ran, such as `WSP-001` (no CODEOWNERS), `SUP-001`, `SUP-004` and `SUP-008`. Nothing changes until it moves to 0.9.0.
- **To keep the old rule set:** pass `--profile classic`, or set `profile = "classic"` in `.coderipper.toml` or a wide config file. To fix instead of waive, see each finding; a waiver is an `[[allow]]` entry with a reason.
- **Unchanged:** `--profile portfolio` (what the portfolio's pull-request job runs) keeps its rule set and floors; the library's `Profile` type and `run_checks_in` are untouched (the profile is an argument there); exit codes and the JSON shape.
- Tests that were about the five original checks or the exit codes now pass `--profile classic` explicitly; the extended-profile tests are unchanged.
## 0.8.22 - 2026-10-23 (proposed)

Fix: `portfolio.yml` on a self-hosted Windows runner. `shell: bash` resolved to `C:\WINDOWS\system32\bash.exe`, the WSL launcher ("no installed distributions"), so the first step failed before coderipper ran (found by ThinkersFoundation-site#14, run 40005533893; it hit every private Windows repository). A new first step, run only on Windows in Windows PowerShell, finds Git for Windows' `bash.exe` (beside `git.exe`, else `C:\Program Files\Git\bin`), puts its directory first on `PATH` for the later steps, and fails with a message naming the missing install if neither exists. Linux and macOS runners skip it. The same workflow sets `RUSTC_WRAPPER` and `CARGO_BUILD_RUSTC_WRAPPER` to empty: a self-hosted runner account can inherit another user's `sccache` wrapper that it cannot execute, which failed `cargo install` (ThinkersFoundation run 40007365677, after the bash step passed). The "a rule did not run" guard also accepts the two base-comparing rules (the lint-suppression ratchet, version-and-changelog) saying they have no base on a run of the default branch: it made the advisory job red on every project's `main` with no findings (dirty-ranges run 40008349786). Every other rule that does not run still fails the job. Callers on a self-hosted Windows runner must try the new pin there before moving to it.

## 0.8.21 - 2026-10-23 (proposed)

The `ci.yml` self-call is repinned to `portfolio.yml@c665210`, the merge of 0.8.20, whose workflow installs 0.8.19 (the pin before it ran 0.8.17). No code change.

## 0.8.20 - 2026-10-23 (proposed)

CI pins moved to 0.8.19: `portfolio.yml` `CODERIPPER_VERSION`; the `ci.yml` self-call stays at its 0.8.17 pin until a follow-up repins it to this change's merge commit (the workflow at 0.8.19's merge still installs 0.8.17). No code change.

## 0.8.19 - 2026-10-23 (proposed)

`patent-index-match` reads Maven and Gradle (follow-up to the rule below, which listed them as not judged).

- `pom.xml` (each `<dependency>` by `groupId:artifactId`; a property placeholder is compared as written) and `build.gradle` / `build.gradle.kts` (each quoted `group:artifact[:version]` string) are compared with the index's 12 `maven` names (8 entries). Still not judged: Gradle version catalogs (`libs.versions.toml`), coordinates built from variables, lockfiles, transitive dependencies.
- Test: `maven_pom_and_gradle_coordinates_match` (a listed pom and Gradle dependency are found, `junit:junit` in both is not).

## 0.8.18 - 2026-10-23 (proposed)

Self-hosted and trigger lints (`docs/portfolio-ci-coverage.md` build order 5, second row), added to `ci-security`.

### Added
- `ci-security`: a job on a self-hosted or custom-labelled runner under `pull_request` or `pull_request_target` whose `if:` lacks the same-repo condition (`github.event.pull_request.head.repo.full_name == github.repository`, either operand order) is **High**; with the condition but no `environment:` it is **Medium** (CireSnave's self-hosted policy 5.1h; a private repository on the Free plan cannot have required reviewers, so waive the Medium there). A job limited by its own `if:` to other events is not judged, as for secrets.
- `ci-security`: a `pull_request` trigger whose `branches:` filter names neither `main` nor `master` (nor `*`/`**`), or whose `branches-ignore:` names one of them, is **Medium**.
- Planted cases both ways: `conformance/ci-security/defective` gains `selfhosted.yml` and `branches.yml`; unit tests for each finding with positive controls (reversed operand order, a release-only job, a push trigger, a hosted runner, a catch-all filter).

### Changed
- `ci.yml` calls `portfolio.yml` at `5e6e1d4` (0.8.17, the commit that merged #119) and `portfolio.yml` installs 0.8.17 (published).

### Not judged
- Repository visibility (a private repository is the waiver above) and the forge's approval setting; a runner chosen by an expression; the default branch's real name (main and master are assumed).

## 0.8.17 - 2026-10-23 (proposed)

`lint-suppression` ratchet (`docs/portfolio-ci-coverage.md` build order 5, first row; GOV-001 against the merge-base).

### Added
- Rule `lint-suppression`: the `allow` and the `ignore` attributes of the repository at `HEAD`, reasoned or not, are counted and compared with the tree the branch started from (the merge-base, found as `version-and-changelog` finds it). A kind whose total rose is a **Medium** finding at the first surplus attribute (advisory in `--profile portfolio` unless the repository lists the rule in `blocking`). Totals are compared, so a moved or renamed file adds nothing.
- Planted cases both ways: `conformance/lint-suppression/{defective,clean}` now carry a `.baseline`; unit tests for a rise in each kind, equal, fewer, a rename, a new file, no base, and an unparsable base file.
- `BaseTree::paths` (the base's tracked files).

### Not judged
- Owner and detector per suppression (an attribute has no field for them); suppressions in `Cargo.toml` `[lints]`; attributes in macro bodies. With no base (a shallow clone, the default branch itself) or a base file `syn` cannot parse, the ratchet is a note ("ratchet not run"), never a pass.

### Changed
- `ci.yml` calls `portfolio.yml` at `c0309d0` (0.8.16, the commit that merged #118) and `portfolio.yml` installs 0.8.16.

## 0.8.16 - 2026-10-23 (proposed)

Found by the first run of `portfolio.yml` (#117): this repository's two `SEC-002` `[[allow]]` entries were reported as "names a check that is not registered" under `--profile portfolio`.

### Fixed
- The allowlist's "not registered" judgement took the rule list of the profile being run, so an entry for a rule of another profile (a delegated rule under `portfolio` or `classic`) read as a typo. Over the built-in checks it now takes every rule of the build (each named once); a name that is no rule at all is still reported, and a run over a list of the caller's own checks still judges entries against that list alone (the documented gotcha, `tests/custom_check.rs`). Tests (each red before): `an_allow_entry_for_a_rule_outside_the_profile_is_not_called_unregistered` (the typo case is its positive control), `the_list_of_registered_rules_in_an_unknown_entry_names_each_rule_once`, `an_entry_naming_a_rule_of_a_profile_is_unknown_to_a_run_of_your_own_checks`.

## 0.8.15 - 2026-10-23 (proposed)

This repository calls its own `portfolio.yml` (build order 4, second half).

### Added
- Job `portfolio` in `ci.yml`: `uses: ciresnave/coderipper/.github/workflows/portfolio.yml@23406320fe7e4483bedfe41b5b71128a574d8718` (the commit that merged the workflow, 0.8.14), `permissions: contents: read`, no inputs and no secrets. Not a required check yet.

## 0.8.14 - 2026-10-23 (proposed)

Reusable workflow `portfolio.yml` (`docs/portfolio-ci-coverage.md` build order 4). Stacked on #115 (0.8.13).

### Added
- `.github/workflows/portfolio.yml`, a `workflow_call` workflow: installs the pinned release (`cargo install coderipper --version 0.8.13 --locked`, a version the caller cannot override) and runs `coderipper sweep --profile portfolio --deny medium --no-config --project .` plus `coderipper check SEC-002 --install-tools` (the profile holds the neutral rules only, so the secrets scan is its own step) on a full-history checkout. Input `runs-on` (default `ubuntu-latest`); `contents: read` only, no secrets declared; every step Bash; refuses a fork or `pull_request_target` run on a runner that is not GitHub-hosted (reachability builds the crate, so a `build.rs` would run there); fails the job when a rule printed "not run", except the Rust-only rules where there is no Rust crate and the skipped `ci-protection-presence`.

### Not done
- This repository does not call the workflow yet: a call pinned by SHA needs a commit of `main` that holds the file, so it follows this merge.

## 0.8.13 - 2026-10-22 (proposed)

Rule `version-and-changelog` (`docs/portfolio-ci-coverage.md` build order 3; PM ruling 2026-10-21 on EXPECTATIONS 6.6, and CireSnave 2026-09-17: "If a change is made and pushed, the version should change in some way. If it is a breaking change, the major version should change.").

### Added
- Rule `version-and-changelog` (neutral module, advisory in `--profile portfolio`; catalog record in `rules/wsp.toml`): Medium when the root `Cargo.toml` (`[package]`, else `[workspace.package]`) or `package.json` version is the base's; Medium when a version that changed, or a manifest the base lacked, has no entry in the root `CHANGELOG*`/`CHANGES*` file (a heading, or a line that begins with the version, naming it as a whole token) or the repository has no changelog.
- The base: the merge-base of `HEAD` with `origin/HEAD`, else `origin/main`, else `origin/master` (`RepoView::base`). A coverage gap, never clean, when none exists, when the history is missing (a shallow clone: `fetch-depth: 0`), and on the default branch where `HEAD` is the base.
- Planted cases both ways: `conformance/version-and-changelog/{defective,clean}`, each with a `.baseline`, and unit tests with a positive control for each finding and for the whole-token entry match.

### Changed
- The conformance runner records a fixture's `.baseline` commit as `origin/main`, as a clone would have it, so a rule that asks where a branch left the default branch finds it.
- The neutral module claims twelve rules (was eleven).

### Not judged
- The `version-deferred` label (a forge fact; the PM's gate allocates the number and re-checks at merge), which component a breaking change moved, manifests below the root, `pyproject.toml`, lock files, and what an entry says.

## 0.8.12 - 2026-10-22 (proposed)

Rule `ci-security` (PM task from CireSnave 2026-10-22: "Require approval before workflows from outside collaborators run. Don't give pull-request-triggered jobs on a public repo any repository secrets. Prefer hosted runners for public repos wherever a job doesn't need our GPU."). Stacked on #113 (0.8.11).

### Added
- Rule `ci-security` (neutral module, advisory in `--profile portfolio`; catalog record in `rules/sup.toml`): High for a job of a `pull_request`/`pull_request_target` workflow that reads `secrets.NAME` (not `GITHUB_TOKEN`), `secrets: inherit`, or a top-level `env` secret; High for a `pull_request_target` job whose `actions/checkout` takes the pull request head; Medium for a job on a self-hosted or custom-labelled runner that mentions no GPU/CUDA/Vulkan/ROCm/OpenCL; Low for a workflow with no top-level `permissions:`.
- Planted cases both ways: `conformance/ci-security/{defective,clean}` (one defective workflow per finding) and unit tests with a positive control for each.
- Dependency `serde_yaml_ng` 0.10 (workflow YAML).

### Not judged
- Repository visibility (run the rule over the public repositories) and the forge setting "require approval for outside collaborators" (a settings-audit read, not a file); a runner chosen by an expression; secrets reached through composite actions or called workflows.

### Changed
- The neutral module claims eleven rules (was ten).

## 0.8.11 - 2026-10-22 (proposed)

Rule `licence` (PM task, `docs/portfolio-ci-coverage.md` build order 1; EXPECTATIONS 6.4 and 6.4a).

### Added
- Rule `licence` (neutral module, `--profile portfolio` floor High; catalog record in `rules/sup.toml`): **High, the only blocking finding:** no licence file at the repository root (`LICENSE*`, `LICENCE*`, `COPYING*`, `UNLICENSE*` or a `LICENSES/` directory). **Medium, reported and never blocking** (the owner's words that expectations "are not gates that have to be verified before we proceed"): a manifest `license` identifier (`Cargo.toml`, `package.json`, `pyproject.toml`) that no licence file is named for (one generic `LICENSE` is accepted for any, its text is not read); source files with no `SPDX-License-Identifier` in their first 15 lines; a header naming a licence other than the project's with no `NOTICE` or `THIRD-PARTY` origin record; a copyright line naming the owner as a person. The per-file findings are one finding per repository naming the count and the first five files.
- Planted cases both ways: `conformance/licence/{defective,clean}` and unit tests for each finding, the root-only search, expression parsing, and the skipped directories.

### Changed
- The neutral module claims ten rules (was nine); the coverage report counts one more covered rule per language.
- Every tracked source file now starts with `SPDX-License-Identifier: MIT OR Apache-2.0` (126 files; the new rule found 124 of 127 without one on this repository), and `LICENSE-MIT` names the organisation, Thinker's Journal, as holder instead of a person (EXPECTATIONS 6.4).
- Conformance fixtures `patent-index-match/{clean,defective}/requirements.txt`: `requests` 2.32.0 to 2.33.0 (dependabot #111, folded in; the fixtures only name a package, nothing is installed).

### Not judged
- The licence text itself; whether a copied file's header is its origin's; another owner's personal name (the list of personal forms is built in); manifests below the root; markup and data files.

## 0.8.10 - 2026-10-22 (proposed)

`--profile portfolio`: the profile a repository's pull-request check runs (build order step 4 of `docs/portfolio-ci-coverage.md`, first part).

### Added
- Profile `portfolio` (`Profile::Portfolio`): the five original checks and every language-neutral rule (`SUP-001`, `SUP-004`, `SUP-011`, `DOC-005`, `DOC-009`, `WSP-001`, `lint-suppression`, `conflict-marker`, `patent-index-match`), **without** the rules delegated to a tool (no network, no tool to install). It fails the run by a floor per rule (`coderipper::portfolio_floor`) instead of one `--deny`: `SUP-004` and `version-consistency` at Medium, `conflict-marker` at Low, `patent-index-match` at High (its `avoid` action); every other rule (including `SUP-001`, `WSP-001` and `ci-protection-presence`, until the owner rules them blocking) reports and never blocks, per the owner's words that expectation-class rules "are not gates that have to be verified before we proceed". `--deny none` still never fails on a finding. A waiver is an `[[allow]]` entry as in every profile.
- Key `blocking = ["RULE-ID", ..]` in a repository's `.coderipper.toml`: rules that repository has made blocking for itself under `--profile portfolio` (a finding at Low or above fails the run). A repository flips a rule once it is clean for it; it never lowers a default floor; an unknown rule id is a usage error.
- Tests: `tests/portfolio_cli.rs` (an `avoid` dependency blocks; `flag-for-review` and conditions entries are shown but pass; `--deny none`; a delegated rule is a usage error; the floor table).

### Notes
- Still to come in this step: the reusable workflow `portfolio.yml` that calls the profile pinned by SHA, and the licence and version-and-changelog rules joining the floor table.
- Any data file a check pins by hash must be hashed over its LF bytes: a Windows checkout converts text to CRLF (coderipper#110 failed on Windows for that reason).

## 0.8.9 - 2026-10-22 (proposed)

Rule `patent-index-match`: a declared dependency the patent-safety index lists (PM task from CireSnave 2026-10-22: "get CodeRipper working with it").

### Added
- Rule `patent-index-match` (neutral module, extended profile; catalog record in `rules/sup.toml`): each dependency declared in `Cargo.toml`, `package.json`, `pyproject.toml` (PEP 621, dependency groups, Poetry) or `requirements*.txt` is compared by registry name with the `match` lists of the agent-safety-bed index. By the entry's action: `avoid` is High, `flag-for-review` Medium, `allow-with-conditions` and `separate-component` Low, `allow` is not reported. A finding carries the entry's id, tier, licence and note, and says **index match**: not legal advice, not a patent clearance, and a dependency with no match is not cleared. Waivable with an `[[allow]]` entry. Not judged: Maven and Gradle (the index lists Maven names; no parser reads them yet), lockfiles and transitive dependencies, a technique written in the project's own source, the 18 index entries that list no package name.
- The index is pinned in the crate: `data/patent-tiers.json` from agent-safety-bed 1.2.0 (commit `5bd779f`), `data/patent-tiers.PIN` (version, commit, sha256) and the upstream `LICENSE` and `NOTICE` (Apache-2.0; credit kept). No network at lint time. A test fails if the data and the pin disagree.
- Planted cases both ways: `conformance/patent-index-match/{defective,clean}` (a `gensim` requirement, an `avoid` entry, must be found; unlisted dependencies must not) and unit tests for every manifest, normalisation (PEP 503, hyphen/underscore, scoped npm, `package` and `npm:` aliases), `ray[rllib]` extras, and the wording.

### Changed
- The neutral module claims nine rules (was eight); the coverage report counts one more covered rule per language.

## 0.8.8 - 2026-10-21 (proposed)

Rule `SUP-004`, dependencies only from published sources (PM task, CireSnave 2026-10-21: the rule against relative path dependencies should be a CI check; EXPECTATIONS 6.8).

### Added
- Rule `SUP-004` (neutral module, extended profile; catalog record already in `rules/sup.toml`, now with `checked_today`): a Cargo path dependency that leaves the repository is High
  (dependencies, dev-, build-, workspace, target tables, `[patch]`, `[replace]`, a `.cargo/config.toml` `paths` override); a `git =` dependency is Medium and is waived through an
  `[[allow]]` entry; a Rust `include!`, `include_str!`, `include_bytes!` or `#[path]` that leaves its crate is Medium, and High when it leaves the repository (a `CARGO_MANIFEST_DIR` join is reported only then: a test finding its workspace is not a published-crate defect);
  an npm `file:`/`link:` dependency leaving the repository is High, an npm git or URL spec Medium. Fixture and vendored directories are skipped. Not judged: other ecosystems,
  run-time `..` joins, `version` on in-repository path dependencies, a registry allowlist.
- Planted cases both ways: `conformance/SUP-004/{defective,clean}` and unit tests for each table and shape.

### Measured
- Over `origin` default branches of 36 checkouts under `C:\Projects` (a `git archive` of each; 3 more, `claude-code-wrapper`, `llm-bench`, `unpopped-harness`, could not be archived and are not counted; run by `check SUP-004 --profile extended` at 0.8.8): **144 findings in 9 repositories, 27 clean.** 24 are `git =` dependencies (candlelight 11, lightbulb 9, baracuda 2, mlmf 2: each needs a named blocker or the published version), 2 are High (`commy` `gen_certs.rs`, `include_str!("../tests/...")` from the repository root), and 118 are Medium `include*!` that leave their crate (fuel 97, mlmf 9, PersistAnt 5, baracuda 4, Humboldt 2, Unpopped 1). Those 118 are real packaging defects for a published crate but many are in `tests/` (they pass in the workspace); no path dependency outside a repository was found. A first run also flagged 40 `CARGO_MANIFEST_DIR` joins that stay inside the repository; those are no longer reported.

## 0.8.7 - 2026-10-21 (proposed)

Which portfolio rules a CI job can check, and the design of one check set (PM task). Documentation only.

### Added
- `docs/portfolio-ci-coverage.md`: every rule in `CLAUDE.md` and `CIRESNAVE-EXPECTATIONS.md` classified CI / partly / no, what covers it today, the build list, and a `portfolio` profile with one reusable workflow.

## 0.8.6 - 2026-10-21 (proposed)

`cargo deny check` runs in CI (PM task). No source change.

### Added
- `deny.toml`: advisories deny (yanked too) with no ignore entries; licences limited to a permissive allow-list (MIT, Apache-2.0
  with or without the LLVM exception, ISC, BSD-3-Clause, Zlib, Unlicense, 0BSD, Unicode-3.0, CDLA-Permissive-2.0); unknown registries
  and git sources denied. Before this file `cargo deny` allowed no licence at all and reported 200 errors.
- CI job `deny` (`EmbarkStudios/cargo-deny-action`, pinned to the v2.1.1 commit).

## 0.8.5 - 2026-10-21 (proposed)

Lock-only security update (PM task). No source change.

### Security
- `Cargo.lock`: `rustls-webpki` 0.103.15 -> 0.103.16 (RUSTSEC-2026-0382/0383) and `webpki-roots` 1.0.9 -> 1.0.10 (RUSTSEC-2026-0381, distrusted emSign roots). `cargo audit` reports no vulnerabilities after the update; `webpki-root-certs` is not in this lock.

## 0.8.4 - 2026-10-20 (proposed)

ROADMAP refreshed to `main@ea58701` (0.8.3); it was dated 2026-10-14 (0.6.14). Documentation only.

### Changed
- `ROADMAP.md`: items 1-3, 5-7 marked done, the group5 audit recorded, and the WSP-001 / DOC-009 / SUP-008 / SUP-001 bar re-measured over
  33 fresh clones (was 27): 11 / 0 / 97 / 7 findings, against 24 / 21 / 326 on 2026-10-14. The default-ruleset release number is now
  the next Breaking one, not 0.7.0 (0.7.0 shipped the protocol enums).

## 0.8.3 - 2026-10-20 (proposed)

The compiled-in checks are one closed set (PM task, group5 audit row 3). Not breaking: see the note on `Finding::check_id`.

### Added
- `builtin::BuiltinCheck` (`ALL`, `id`, `from_id`, `is_proven`, `build`). `registered_checks()` and the Rust module's list of proven claims
  (a hand-kept `PROVEN` string list) are derived from it; a new variant makes the compiler name each `match` that must decide, and a test
  ties each variant's id to the id of the check it builds.

### Not done
- `Finding::check_id` and `AllowEntry::check` stay strings: findings arrive from external modules and delegates with ids that are not built-in
  checks, so a closed enum there would reject them. The audit's "breaking" mechanism applies only if that is wanted.

## 0.8.2 - 2026-10-20 (proposed)

GitHub's error wording is read in one place (PM task, group5 audit row 7). Not breaking.

### Added
- `ApiError::kind()` and `ApiErrorKind` (`EmptyRepository`, `PlanLimit`, `Other`). `ci-protection-presence` matched `message.contains("empty")` and
  `contains("Upgrade to GitHub Pro")` inline; it now asks `kind()`, so a reworded GitHub answer is one edit with one test, not two silent flips.

## 0.8.1 - 2026-10-20 (proposed)

The fixture contract is checked over the whole `conformance/` tree (PM task, group5 audit row 8). Not breaking.

### Added
- A test enumerates `conformance/` from the git index and fails for a directory that is not a catalog id, a rule missing its `defective`
  or `clean` twin, a defective fixture expecting nothing, a clean twin expecting a finding, or an expectation naming another rule. The runner
  already refused these, but only for a rule it was asked to run.

## 0.8.0 - 2026-10-20 (proposed, Breaking)

A rule's domain, languages and scopes are enums, not strings (PM task, group5 audit row 9).

### Changed (Breaking)
- `Rule::domain()` returns `Domain` (21 variants; `Domain::as_str` gives the three letters), `Applicability::languages` is `Vec<Language>`
  (`any`, `rust`, `typescript`, `python`) and `Applicability::scopes` is `Vec<CodeScope>` (`first_party`, `third_party`). A record naming
  anything else now fails to parse, naming the bad value, instead of loading and matching nothing. Module protocol language strings are
  unchanged: they come from external modules.

## 0.7.4 - 2026-10-20 (proposed)

The configuration keys `profile`, `deny` and `message-format` are the same enums as the flags (PM task, group5 audit row 4).
Stacked on nothing; not breaking.

### Changed
- `Profile`, `Deny` and `MessageFormat` are defined once, in `config.rs`, and derive both `serde::Deserialize` (kebab-case) and
  `clap::ValueEnum`. The three string arrays, the `one_of` check and the second parse of each value in `cli.rs` are gone, so the file and the
  flag cannot drift apart. The accepted spellings are unchanged.
- A wrong value in a configuration file now fails inside the TOML parse, so the message carries the file, the line and serde's list of the
  accepted values (`unknown variant "sometimes", expected one of ...`) in place of the hand-written sentence. Still an error in every build.

## 0.7.3 - 2026-10-18 (proposed)

An absence is declared by the constructor, not guessed from the words of the summary (PM task, group5 audit row 3; approved option (a) on 2026-10-18).
Stacked on 0.7.2 (#97), which is stacked on 0.7.1 (#98).

### Added
- `Finding::absence(check_id, severity, confidence, project, summary, detail, positive_control)`: the only constructor of an absence claim, and the
  control is a parameter, so it cannot be built without one. `Finding::claim` (`Claim::Presence` | `Claim::Absence`) is a new field, serialized only
  for an absence (`"claim":"absence"`), so every existing report and the golden file are unchanged and a finding from an older module reads back as a
  presence.
- `validate()` now refuses a declared absence whose control is missing or blank, including one that arrived over the wire from a module, whatever its
  summary says. The English word list (zero, no, none, missing, unreachable, 0) stays as the backstop for a presence finding that says otherwise; a
  test pins that it still fires on one built with `Finding::new`.

### Changed
- `--message-format json`: a finding line gains the key `"claim":"absence"` when the check declared an absence (an additive key; a presence finding's
  line is byte-for-byte what it was). `tests/message_format.rs` pins the new key set for `unused-return-values`.
- The checks whose finding is "something is not there" declare it: `doc009` (no glossary), `wsp001` (no CODEOWNERS / no default owner), `sup001`
  (lockfile not committed), `sup011` (vendored directory not excluded; no word in the list catches "does not exclude"), and the struct-literal findings of
  `reachability`, `unused-parameters`, `unused-return-values` and `ci-protection-presence`. Findings that report a defect read at a place keep
  `Finding::new` (their control, where they have one, is still set with `.positive_control`).

## 0.7.2 - 2026-10-18 (proposed)

A check names its rule through a generated constant, so a misspelt rule id does not compile (PM task, group5 audit row 4). Stacked on 0.7.1 (#96).

### Changed
- `build.rs` writes a `pub const` for every rule id in `rules/*.toml` (`WSP-001` -> `rule_id::WSP_001`, `conflict-marker` ->
  `rule_id::CONFLICT_MARKER`) and fails on an id that is not an identifier or collides with another. The 17 rule-id constants of the neutral
  module, the delegated tools and the Rust checks (and `reachability`'s `id()`) are those constants instead of string literals. No behaviour change.
- Shown: `crate::rule_id::WSP_0O1` in `wsp001.rs` is `error[E0425]: cannot find value` and `cargo check` exits 101; restored, it builds.
- Not converted: the rule ids written as literals inside tests (they assert what the constants say) and `"allowlist"` (a stale-entry finding, not a rule).

## 0.7.1 - 2026-10-18 (proposed)

A bad shipped `tools.lock` fails the build, not only a test or a run (PM task, group5 audit row 1). (#96).

### Changed
- `build.rs` reads `src/tools/tools.lock` and runs the lock rules over every entry (64-hex hashes, safe name/version/file/member components, the
  platform form, a licence, an allowed source scheme, archive/member/file_sha256 together, no duplicate name and platform); a violation panics the
  build with the entry named. The rules moved to `src/tools/lock_rules.rs`, a file with no crate dependencies that `build.rs` includes by `#[path]`
  and `ToolsLock::parse` calls, so the build-time check and the run-time parser cannot drift. Messages are unchanged. New build dependency: `toml`.
- Shown, not just argued: with the first entry's `sha256` set to `deadbeef`, `cargo build` fails (exit 101, "gitleaks 8.30.1: sha256 must be 64 hex
  digits"); restored, it builds. Before this the same lock built and the `embedded()` panic waited for the first `cargo test` or the first run.

## 0.7.0 - 2026-10-18 (proposed, Breaking)

The module protocol's closed sets are types, not strings (PM task, group5 audit row 2; EXPECTATIONS 6.16: a closed set is checked where the data
enters, not where it is used).

### Changed (Breaking)
- `RuleClaim::status` is `ClaimStatus` (`ImplementedNative`, `Delegated`, `NotApplicable`), and `Request::unit` / `Request::run_tier` are
  `RequestUnit` (`Package`, `Repository`) and `RequestTier` (`Fast`, `Sweep`), instead of `String`. The JSON is the same strings
  (`"implemented-native"`, `"package"`, `"fast"`, ...), so a conforming module is unaffected; a hello or request with any other value is now a parse
  error. The conformance runner's separate "status the protocol does not define" check is gone (it cannot happen); a test replaces it at parse time.
- `Request`, `Hello`, `RuleClaim` and `Capabilities` are `deny_unknown_fields`: a module that sends a key the protocol does not define fails its
  hello instead of being half-read. (Findings and rule results are unchanged: they may still carry extra keys.)
- Test: `tests/protocol_closed_sets.rs` (written first and red: all three accepted the bad input).

## 0.6.43 - 2026-10-17 (proposed)

A committed merge conflict is a finding, in every profile (PM task after baracuda#197 merged `<<<<<<< HEAD` into `docs/DEVICE-TESTS.md` and every
gate passed).

### Added
- Rule `conflict-marker` (neutral module; catalog record in `rules/gov.toml`; **runs under `classic` too**, the one neutral rule that does): a tracked
  text file with a line that starts with seven `<` and a space (or is those seven alone) opens a conflict region, a line that starts with seven `>`
  and a space closes it, and a line of seven `=` counts only between them (alone it is a Markdown heading underline). One High finding per region at
  its first line; a region never closed, and a closing marker with no opening, are findings too. Not read, so that quoting the markers is not a
  defect: a fenced code block in a Markdown file, anything under `fixtures`, `testdata`, `conformance`, `snapshots`, `golden` or a vendored
  directory, patch files (`.patch`, `.diff`, `.rej`, `.orig`), a file with a NUL byte, a file over two megabytes. Git is asked once which files hold a
  candidate line (`git grep`), so a large repository is not read blob by blob (fuel, 1809 files: seconds, not minutes). A `classic` run in a
  directory that is not a git repository with a commit does not take it (nothing to read); `extended` still errors there, as the other neutral rules do.
- Planted case both ways: `conformance/conflict-marker/{defective,clean}` (a conflict in a page at line 5; the same text in a fenced block, and a
  heading underline, reported as nothing), unit tests for each shape (CRLF, never-closed, stray close, fence and tilde fence, fixtures, vendored,
  patch, binary, six characters, indented), and a CLI test that `classic` and `extended` both report it and `--deny` fails the run.

### Changed (a behaviour change in `classic`)
- **The default profile now runs a sixth rule.** `classic` was the five original checks and nothing else; it now also runs `conflict-marker`, so a
  repository with a committed conflict marker fails a plain `coderipper fast` (exit 1 at `--deny info`), and `--coverage` counts one more rule that ran
  ("5 clean" where a one-crate run said 4). A repository with no marker sees no other change.

### Measured
- Over the 27 default-branch clones of 2026-10-17: the rule found `baracuda` `docs/DEVICE-TESTS.md:4` (the merge that started this; region lines 4-8) in a
  clone taken before baracuda#199 removed it; **re-fetched after #199, all 27 are clean**, including coderipper itself.

## 0.6.42 - 2026-10-16 (proposed)

Q14 triage (PM task; CireSnave 2026-10-16: "Go with your recommendation" = no breaking release yet).

### Changed
- **A `reachability` finding on an item written `pub` says so and names both cures.** In the 2026-10-16 sample of 30 findings
  (`auto-discovery`, `mlmf`), 28 were rooted in a `pub` item of a published library (true as stated, not dead) and 2 were private dead
  code, but the text was the same for both. The detail now adds: "It is written `pub`: if it is this library's API, declare it under
  `[reachability] public_api` with a reason; if not, remove it or narrow it to `pub(crate)`." Only with a library; the summary and the
  set of findings are unchanged. Planted both ways in `tests/reachability.rs`.

### Added
- `docs/portfolio-triage/2026-10-16-q14-remeasurement.md`: WSP-001, DOC-009 and SUP-008 re-measured (11, 3 and 176 findings against 24,
  21 and 326 on 2026-10-14; every one a true defect in the repository's own files, so no rule changed), every repository-level rule on
  `baracuda`, `synapse` and `fuel`, the reachability read, and the measured bar.

## 0.6.41 - 2026-10-16 (proposed)

A Rust lint suppression or ignored test says why (CireSnave 2026-10-16: *"That same general dislike applies to
#[allow(dead_code)] as well.  That should also be a last resort."*). Extended profile only; the default set is unchanged.

### Added
- Rule `lint-suppression` (neutral module, extended profile; catalog record in `rules/gov.toml`, related to GOV-002 and TST-005):
  a tracked `.rs` file's `#[allow(..)]` / `#![allow(..)]` (also inside `cfg_attr`) or `#[ignore]` with no reason is a Low finding at
  its line. `allow(lints, reason = "..")` and `#[ignore = ".."]` pass; a comment beside the attribute is not a reason. `dead_code`
  is named, with the cure (delete, use, or gate with `cfg`). Every attribute that does carry a reason is counted in a run note
  (`lint-suppression: N allow and M ignore attribute(s) carry a reason and pass`), so the debt stays visible when nothing is
  reported. Read with `syn`, at `HEAD`; vendored directories are skipped; a file that does not parse is a note. Not read: macro
  bodies, `#[expect]`, `[lints]` tables, other languages.
- Planted case both ways: `conformance/lint-suppression/{defective,clean}` (a bare `allow(dead_code)` and a bare `#[ignore]`
  reported at their lines; the same shapes with reasons, a commented-out attribute and an attribute inside a string reported as
  nothing), plus unit tests for each shape, `cfg_attr`, blank reasons, `expect`, vendored code and an unparsable file, and a CLI test
  that classic does not run it.

### Changed
- CodeRipper's own sweep (`--profile extended --all-features --deny info`) stays exit 0 with the same 4 waived and no new waiver:
  the four attributes the rule found in this repository (`clippy::permissions_set_readonly_false` twice,
  `clippy::too_many_arguments`, `clippy::zombie_processes`) now say why.
- `Verdict::Noted` (internal): a neutral rule can return notes beside its findings.

## 0.6.40 - 2026-10-16 (proposed)

No `#[allow(dead_code)]` attribute is left in the repository (CireSnave 2026-10-16: *"That same general dislike applies to
#[allow(dead_code)] as well.  That should also be a last resort."*). 25 removed; none was a last resort.

### Changed
- Tests: `tests/common/mod.rs` held two helpers and 13 test files allowed dead code because each used a subset. `MANIFEST` is now
  `tests/common/manifest.rs`, included by the 9 files that use it (`#[path]`); `tests/cli_help.rs` used neither and no longer
  includes `common`.
- `src/build_cache`, `src/config.rs`: items only the command-line program uses are `#[cfg(feature = "cli")]` (or
  `any(feature = "cli", test)` when unit tests use them) instead of allowed. Without the `cli` feature `ConfigFile` accepts the
  five keys only the CLI reads (`redact`, `install-tools`, `cache`, `tools`, `limits`), whatever their value, and does not look
  at them; every other unknown key is still an error, and with `cli` behaviour is unchanged. `tests/config_cli_only_keys.rs`
  pins both builds, and CI runs it without `cli`.
- `src/cargo_json.rs`: `RawTarget.name` and `.kind` exist only in test builds, the only place they are read.
- `src/session.rs`: `release_lock_for_test`, called by nothing, is deleted.

## 0.6.39 - 2026-10-15 (proposed)

CodeRipper's whole extended sweep on itself runs in CI.

### Added
- `.github/workflows/ci.yml`: job `coderipper-sweep` runs `sweep --profile extended --all-features --deny info --install-tools`
  over the repository with its history fetched. Measured on this branch's tree: exit 0, no finding, 4 waived (1 declared public
  API, 3 history-only SEC-002 placeholders), 17 fixture findings not reported. Not a required check yet; the existing
  `coderipper` job (`fast --deny medium`, classic) is unchanged.
- `--skip-rule ID=WHY` (repeatable, on `fast`, `sweep` and `check`) and the library's `run_checks_in_with_tools_skipping` /
  `run_workspace_in_with_tools_skipping`: a built-in check is not called, and the run carries a note
  `<ID> not run: skipped on request (<WHY>)`. A name that is no built-in check, and a skip with no reason, are errors. The CI job
  uses it for `ci-protection-presence` alone: the job's token (`contents: read`) cannot read branch protection (run 38841727909
  exited 3, "cannot read ciresnave/coderipper from GitHub"), and a personal access token is not to be given to a job. That check
  runs in the PM's sweep under an admin account. Existing functions are unchanged.
- The sweep job fails when any rule did not run: an unavailable delegated tool is a note, which never changes the exit code, so the
  job reads the notes on stderr and fails on any `not run: ... is not available` (review finding on #89; the other notes - API-006 without a changed .proto, the skip we asked for, the fixtures count - stay notes).

## 0.6.38 - 2026-10-15 (proposed)

The three SEC-002 findings CodeRipper's own sweep reported in `src/` (measured on 0.6.36 at ba87887).

### Fixed
- `src/module/delegate/gitleaks.rs` (test) and `src/redact.rs` (a doc comment) no longer hold a key-shaped literal at HEAD: the
  test joins its placeholder at run time, and the doc comment names the marker without writing it.

### Changed
- `.coderipper.toml` waives SEC-002 for those two files, with a reason, an owner and a removal condition. The scan reads
  history, and the two commits that introduced the literals (24cb9f1a, f8c78dbe) stay in it, so editing HEAD cannot clear
  the finding; none of the three is a secret (a made-up AWS-key-shaped placeholder in a test; a marker line with no key body).

## 0.6.37 - 2026-10-15 (proposed)

A repository's own conformance fixtures are no longer reported when it is swept (CodeRipper's own `sweep --deny info` exited 1,
16 of 20 findings from `conformance/**`; measured on 0.6.36 at ba87887).

### Changed
- A finding located at `conformance/<rule>/<variant>/...`, where `<variant>` holds an `expect.toml`, is left out of the run
  and counted in one stderr note ("N finding(s) in this repository's own conformance fixtures ... were not reported"). Such a
  fixture is defective on purpose, and the repository that holds it is not the project it describes. Nothing else is
  excluded: the same files anywhere else, or under `conformance/` without an `expect.toml`, are reported as before.
  `coderipper conformance` is unaffected (it runs a fixture as the project root, where its paths are not under `conformance/`).
  This also takes the `clean` DOC-010 fixture's `/docs/guide.md` out of a repository-wide run (a root-relative link resolves to
  the repository root there, not to the fixture's).

### Added (tests only)
- `suppression` tests: a defective fixture finding is left out and counted; a same-shaped file outside `conformance/`, one
  without `expect.toml`, and a `..` path are all kept.

## 0.6.36 - 2026-10-15 (proposed)

The uncommitted-changes note and `--project`'s help no longer overclaim (review findings on #86).

### Fixed
- The note counted only tracked edits, so a fix that existed only as an untracked file produced no warning although the
  worktree checks do not see it; untracked (not ignored) files are now counted.
- The note and `--project`'s help said "the checks" analyse `HEAD`; only `reachability`, `unused-return-values` and
  `unused-parameters` do. `version-consistency` and API-006 read the working tree, and both texts now say so.

## 0.6.35 - 2026-10-15 (proposed)

`--help` and the run say that the committed tree is what is analysed.

### Added
- `--project`'s help states that the checks analyse the committed tree (`HEAD`) in a throwaway worktree and that uncommitted
  changes are not analysed.
- A run over a project with tracked files that have uncommitted changes prints one stderr note with the count, so a fix that
  is only in the working tree is not mistaken for a check that found no change (found on `fuel`). stdout and JSON are unchanged.

### Added (tests only)
- `tests/cli_help.rs` (the phrases in `--project`'s own help block) and `tests/cli.rs` (a clean tree has no note, a dirty one
  names the count).

## 0.6.34 - 2026-10-15 (proposed)

`--features`, `--all-features` and `--no-default-features` reach the build step.

### Added
- The three flags on `fast`, `sweep` and `check` (so on `--workspace` too). A target whose `required-features` are off is not built
  and so could not be checked; they are appended to the `cargo build -p <package>` of `unused-parameters`, `unused-return-values` and
  `reachability`. Library: `CheckContext::cargo_features` and the public `CargoFeatures`. The module `Request` carries them
  (`cargo_features`, left out of the wire message when default). With `--workspace` a named feature must exist on every member
  (`--all-features` has no such limit).

### Added (tests only)
- `tests/cargo_features.rs` and `tests/cli.rs::features_flags_reach_the_build_so_a_gated_bin_is_checked`; unit tests of the argument list.

## 0.6.33 - 2026-10-15 (proposed)

`unused-parameters` names a target cargo did not build instead of blaming the code.

### Fixed
- **A bin with `required-features` that are off was reported as "`unused_variables` is suppressed crate-wide".** Such a target is never
  compiled, so its sentinel cannot be reported; the run failed with a cause that was false (found on `fuel`, whose
  `fuel-capture-fixtures` has `required-features = ["capture"]`). The check now records which target roots cargo built, and says
  `target not built: required-features=[capture]` in the finding's positive control and as a run note for one it did not. A
  built target whose sentinel is missing still fails the run, and so does a run in which no target root was proven.
  The target is matched by its exact path (not a path with the same tail), the manifest's `./` spellings and the nested
  `src/bin/<name>/main.rs` layout are understood, and the note is a run note (`run_noted`), not only a process statistic.

### Added (tests only)
- Two cases in `tests/unused_parameters.rs`: a gated bin beside a built lib (the message is asserted), and a crate-wide
  `allow(unused)` in the built lib beside a gated bin (still an error).

## 0.6.32 - 2026-10-14 (proposed)

Planted variants for `reachability` (the false-negative control of decision 180, completing the Rust checks).

### Fixed
Three spellings of a dead `pub` item that the line-based `pub` -> `pub(crate)` downgrade did not see, so rustc's `pub` exemption hid
them and the check reported nothing (a false negative; no live item was ever reported dead by these):
- **`pub union`.** `union` was not a declaring keyword; the union stayed `pub`, and what was reported instead was its two never-read
  fields. The union is now downgraded and reported by its own name.
- **An outer attribute on the item's own line** (`#[derive(Debug)] pub struct S;`, `#[inline] pub fn f() {}`). The line began with
  `#[`, not `pub`, so it was never downgraded. Leading attributes on the line are now skipped (a proc-macro entry point is still
  left `pub` by the guard, which runs first).
- **A tab after `pub`** (`pub<TAB>fn f() {}`); only a space was recognised.

### Documented
- The downgrade is line-based: a second item on a line after another (`pub fn a() {} pub fn b() {}`) and a `pub` separated from its
  keyword by a line break stay exempt and are not reported. `rustfmt` writes neither. Listed with the other known limitations.

### Added (tests only)
- `tests/reachability_variants.rs` (10 cases: every function qualifier, the item kinds (struct, enum, alias, const, static, trait, module, union), a union, a method in an impl, a one-line
  module, attributes and whitespace before `pub`, every way an item is live: path, rename, glob, const expression, type position,
  bound; and the items the compiler keeps for the linker or the author) and four unit tests in the rewriter.

## 0.6.31 - 2026-10-14 (proposed)

A seeded defect and clean twin for `ci-protection-presence`, the one rule that had none (it asks GitHub, which a fixture cannot make answer).

### Added
- **Conformance fixtures may name an `origin`** (`origin = "<url>"` in `expect.toml`): the runner gives the fixture's repository that
  remote, for a rule that asks which forge repository it is in. Nothing else changes for a fixture without the key.
- `tests/fixtures/forge/ci-protection-presence/{defective,clean}` and `tests/ci_protection_fixture.rs`: the pair is run through the
  conformance runner and the Rust module over a stub `Github` that answers, from the real response shapes, for `acme/unprotected`
  (enforces nothing: reported at `github:branch-protection`) and `acme/protected` (three required contexts: clean). Controls: the same
  fixtures fail when the stub answers the other way round, and a fixture with no `origin` is `Errored`, never proven.
- The Rust module does NOT claim `proof` for this rule: a real run of those fixtures would ask the real GitHub about repositories that
  do not exist. The rule is exercised end to end by the test, not earned as a claim.

## 0.6.30 - 2026-10-14 (proposed)

Planted variants for the Rust checks (the false-negative control of decision 180, continued from 0.6.29).

### Fixed
- **`unused-return-values` mistook a caller on its callee's own line for a recursive call.** A call is dropped as recursion when
  it lies inside the callee's own span, which was compared by line only, so `fn callee() -> i32 { 1 } fn caller() { callee(); }`
  lost the discarded call and the function was not reported. The span is now bounded by column as well (in the rewritten source,
  where the tag attributes shift every later column on the line); a genuinely recursive call is still dropped.

### Documented
- **`unused-parameters` reads parameter sites from `src/` only.** An unused parameter in `tests/`, `examples/` or `benches/` is
  built and rustc warns, but it is not reported, the boundary `unused-return-values` already documented. Pinned by a test; widening
  it is a product decision, not made here.

### Added (tests only)
- `tests/unused_parameters_variants.rs` (8 cases: every function spelling, inherent-impl spelling, nesting and `cfg`, shadowing,
  every way of reading a parameter, allow/expect at every level, trait-impl shapes, target scope) and
  `tests/unused_return_values_variants.rs` (5 cases). `version-consistency` already had a case per documented shape and
  `reachability` was not covered by this pass. Every case passed when written except the two named above.

## 0.6.29 - 2026-10-14 (proposed)

CodeRipper's own run on itself, part 3: planted variants for the five language-neutral rules (the false-negative control of decision 180).
Each rule's conformance fixture seeds one shape of its defect, so a rule can be "proven" and still miss a sibling shape.

### Fixed
- **SUP-001 misread `cargo` with global flags before the subcommand.** `cargo -q build`, `cargo --quiet test`, `cargo -v clippy`,
  `cargo --color always build`, `cargo -Z unstable-options test`, `cargo --config k=v test` and `cargo +nightly -q build` were not seen
  (the subcommand was looked for in the first word after `cargo` and an optional `+toolchain`), so a CI step that resolves afresh passed.
  Seen now; `cargo --locked build` and `cargo -q --locked test` stay clean.

### Added (tests only)
- `src/module/neutral/variants.rs`: 15 table-driven cases over SUP-001 (every cargo command spelling, lockfile kind and
  application shape), SUP-011 (every vendor directory name against every analysis configuration), WSP-001 (nine defect shapes, nine
  clean ones), DOC-009 (glossary names and the term shapes) and DOC-005 (status spellings, supersession both ways). Every other
  case passed when written; the case that did not is the SUP-001 one above.

## 0.6.28 - 2026-10-14 (proposed)

CodeRipper's own run on itself, part 2: the eight `unused-return-values` findings and the three standing waivers (decision 180).

### Fixed
- **`unused-return-values` no longer tracks a function that hands back its closure's result** (`fn with<T>(f: impl FnOnce() -> T) -> T`,
  also `F: FnOnce() -> T` and `where` forms, `&dyn Fn(..) -> T`, `Box<dyn FnOnce() -> T>`). `T` is whatever the closure returns, so a
  caller whose closure returns nothing discards nothing; the three scoped helpers in this repository (`with_session`, `with_config`,
  `with_policy`) were reported for it. `fn id<T>(t: T) -> T` and `fn make<T: Default>() -> T` are still tracked. Seen in the
  `unused-return-values` clean fixture.
- **This repository's own code:** the remaining findings were call sites that discard a value on purpose; each is now `let _ = ...`,
  the form the check defines as a deliberate discard (`redact`, `resolve`, `embedded`, `apply`, `fake_repo_dir`, the session test
  `build`, and `augment` in `conformance.rs`).
- **`.coderipper.toml` holds no `[[allow]]` waiver any more:** `augment` and `with_session` are fixed in code and by the rule above;
  `run_checks_in` is declared in `[reachability] public_api` (its reason is printed with the finding).

## 0.6.27 - 2026-10-14 (proposed)

CodeRipper's own run on itself (CireSnave, decision 180: "If our testing tool doesn't pass its own tests, that's the first thing to fix"):
the findings of `coderipper sweep --profile extended --deny info` on `main@1087468` that were true, fixed.

### Fixed
- **`git worktree add failed` names git's reason** (issue 75). The checkout the reachability and return-value checks analyse is made with
  `git worktree add`; a failure used to be the bare sentence, so a CI flake carried no cause. The error now includes git's first lines.
- **This repository's own findings (no code change):** `.github/CODEOWNERS` (WSP-001); `--locked` on the nine `cargo` commands of
  `ci.yml` and `live-tools.yml` that lacked it (SUP-001); the `docs/rules/README.md` link `con.md` to the file that exists,
  `concurrency.md` (DOC-010; `con` is a reserved Windows device name); `rustls` 0.23.45 to 0.23.46 in `Cargo.lock` (SUP-002,
  RUSTSEC-2026-0346 to 0349); the two fixture `vendor` directories named in `codecov.yml` (SUP-011).

## 0.6.26 - 2026-10-14 (proposed)

### Changed
- **This repository's workflows declare `permissions: contents: read`** at the top of `ci.yml`, `coverage.yml` and `live-tools.yml`
  (the 11 SUP-008 `excessive-permissions` findings of the 2026-10-14 portfolio sweep; the actions were already pinned by #68). No job
  needs more: none writes to the repository, comments or uses an OIDC token. No code changed.

## 0.6.25 - 2026-10-14 (proposed)

### Changed (a loosening: fewer DOC-009 findings)
- **DOC-009 asks for a glossary only from ten documentation pages.** A repository with one to nine `.md` pages under `docs/` or `doc/` and no
  glossary is now "not applicable" (a coverage gap, never a finding); a glossary that exists is still judged for repeated terms. The
  2026-10-14 sweep of 27 repositories found nine of DOC-009's 21 findings on one to six pages. Not configurable: a repository that
  disagrees waives the finding with a reason. The seeded defect under `conformance/DOC-009/defective` now has ten pages.

### Fixed
- **`coderipper check <RULE>` for a rule that does not apply is no longer "failed to run (no_verdict)" with exit 3.** The host treated a
  module that skipped every requested rule as having given no verdict; for an in-process module (the language-neutral rules) a skip
  of the one rule asked for is the answer, "not applicable". An external module that skips everything is still `no_verdict`.

## 0.6.24 - 2026-10-14 (proposed)

### Documentation
- **Portfolio triage of WSP-001, DOC-009 and SUP-008** (ROADMAP item 3; `docs/portfolio-triage/2026-10-14-wsp001-doc009-sup008.md`): one list per
  rule over 27 repositories at their default branch, each classed true defect, rule too strict or waiver with a reason. Report only; no
  other repository was changed. No code changed.
- **ROADMAP item 5 is marked DONE** (the configuration layers shipped in 0.6.21).

## 0.6.23 - 2026-10-14 (proposed)

### Added
- **`[reachability] public_api` in `.coderipper.toml`** (ROADMAP item 6(a), option B as ruled by the PM). A list of `{ file, symbol?,
  reason }` entries declaring what a library intends as public API; `file` is a path or glob (`*`, `?`, `**`). Matching
  `reachability` findings do not fail the run and are printed as `[declared-public/<severity>]` (not hidden, reason shown); JSON
  `waived` items carry `"declared_public": true` (the new field `Waived::declared_public` is `false` and omitted for ordinary
  waivers). An entry that matches nothing warns, by name, as an `allowlist` Info finding. The repository's own file may set it
  (`[reachability]` joins the list of keys a repository may set); a wide file may too, with the same warning and
  `(declared by <file>)` as for `[[allow]]`, and is not judged stale there. `reason` is required and an unknown key is an error.

## 0.6.22 - 2026-10-14 (proposed)

### Changed (a loosening: fewer `reachability` findings in a workspace that uses `[patch]` or `[replace]`)
- **`reachability` sees a dependent that reaches the package through `[patch]` or `[replace]`** (ROADMAP item 6(b); the 0.6.8
  limit). `cargo metadata --no-deps` gives a dependency written `engine = "0.1"` no `path`, so a member redirected to the
  workspace copy by the root manifest's `[patch.<source>]` or `[replace]` table rescued nothing. The root manifest is now read
  and an entry whose `path` is a workspace member's directory makes that member's dependents count, exactly as a `path`
  dependency does. **A run that reported such items now reports fewer findings.** Not seen, as before: a patch through a
  git source, to a directory outside the workspace, or set in `.cargo/config.toml`.
- **Not changed: ROADMAP item 6(a)** (an intended public API with no in-repo consumer is still reported). Every `pub` item of a
  library is public API by definition, and the check exists to find the ones nothing uses, so no signal in the source separates
  "intended for outside users" from "dead"; any rule that did (a `pub use` at the root, a doc comment) would hide real dead items.
  The answer stays the reasoned per-file `[[allow]]` waiver in README "Waiving a finding". Whether a project should declare its
  published surface (a `[reachability] public_api` list) is a design question for the PM, not built here.

## 0.6.21 - 2026-10-14 (proposed)

### Added
- **Configuration layers** (ROADMAP item 5; design `docs/superpowers/plans/2026-10-13-configuration-layers.md`). A `.coderipper.toml`
  is now also read from `/etc/coderipper/`, `/etc/` and the home directory (Windows: `%ProgramData%\coderipper\`, `%ProgramData%\`,
  `%USERPROFILE%`), weakest first, then the repository's file, then the environment, then flags. Wide files may set defaults for
  `deny`, `profile`, `message-format`, `redact`, `install-tools`, `[cache]` and `[tools]` (the environment variables keep winning),
  ceilings under `[limits]` (combined by minimum and reported, not enforced by CodeRipper itself), and `[[allow]]`, `[[tracks]]`,
  `[buf]`. New flags `--config <path>` / `--no-config` (and `CODERIPPER_CONFIG` / `CODERIPPER_NO_CONFIG`). A run that read a wide
  file names it on stderr.
- **A waiver from a wide file** is applied with one stderr warning per file ("waivers belong in each repository"), printed with
  `(waived by <file>)` (JSON `waived_by`; `Waived::source`), and never reported as a stale entry.

### Changed
- **The repository's file is restricted to an allowlist** of keys: `[[allow]]`, `[[tracks]]`, `[buf]` and the new `profile`. Every key
  that is new in this release is wide-file-only, so **no existing repository file changes meaning**; a known key outside the list
  is skipped with a warning, an unknown key is an error as before.

### Notes
- Behaviour with no wide file present is unchanged, and a library caller sees no wide file (the command-line program reads them).
  One difference for a library caller: a repository file that sets a *known* wide-only key (`deny`, `[cache]`, ...) is now
  accepted and ignored by the library (it was an unknown-key error); the command-line program warns and skips it.
- Not built from the design yet: the run-time stop at `max_runtime_minutes` (the ceilings are reported), a dotless
  `/etc/coderipper/coderipper.toml` spelling, and pinning a profile against a repository's choice.

## 0.6.20 - 2026-10-14 (proposed)

### Security
- **Every GitHub Action in `ci.yml`, `coverage.yml` and `live-tools.yml` is pinned to a commit SHA** with its tag as a trailing
  comment (21 `uses:` lines; the PM's pin patch was regenerated on top of 0.6.19 because #67 added a job, and `live-tools.yml`
  was pinned the same way). This closes the SUP-008 triage finding this lane raised against itself. No code changed.

## 0.6.19 - 2026-10-14 (proposed)

### Added
- **CodeRipper runs on its own source in CI** (the `coderipper` job of `ci.yml`: `coderipper fast --deny medium`), as every Rust
  repository of the portfolio is meant to (ROADMAP item 7; the PM approved `--deny medium` and the default profile). The first
  run read 12 findings, 4 of them Medium or worse; each is dispositioned, none by configuring a path out of the analyser:
  - `set_cache_config`'s `bool` ("false if a configuration was already set") was discarded by the command line: it now says so on
    stderr and keeps the earlier configuration, instead of ignoring it (behaviour change only in a process that configured the
    cache before the command line did, which no command-line run does);
  - three are waived in the new `.coderipper.toml` with their reasons (`run_checks_in`, a public entry point with no in-crate
    caller; `augment`, a recursion whose result the top call reads from a table; `with_session`, generic over a closure that
    returns nothing). The 8 Low findings are printed and do not fail the job.

## 0.6.18 - 2026-10-14 (proposed)

### Added
- **`coderipper conformance --redact`.** The same switch as on `fast`, `sweep` and `check` (0.6.11): known secret shapes are removed
  from what a fixture's build or tool wrote before a reason is printed, in `human` and `json` output alike, and a reason passes
  through the redaction once more on its way out. It changes no verdict and no exit code, and it is off by default. This removes the
  "`conformance` has no `--redact`" limit of 0.6.11. Tested with a fixture whose defective project fails to build with a message
  that quotes `API_KEY=<value>`: the value is in the output without the flag (the control) and is not with it, and the exit code
  is 3 either way.
- **Under `--redact`, an error that stops a whole run is redacted too**, for every subcommand that has the switch: the final
  `coderipper: <error>` line (a fixture's unreadable `expect.toml`, whose parse error quotes the line it stopped at) went to
  stderr as it came, and now goes through the same redaction. Without the switch nothing changes.
  Not covered: a caller of the library's `conformance::run` gets the verdict strings as they were made; only the command line
  redacts them.

## 0.6.17 - 2026-10-14 (proposed)

### Fixed
- **SEC-002 no longer fails a repository that tracks a path longer than 260 characters on Windows.** Measured with gitleaks 8.30.1
  over a repository whose one commit holds a 292-character path: git warns `unable to access '<directory>/.gitattributes': Filename
  too long` for each directory of the path, gitleaks logs each warning as an error and stops reading, and the wrapper (correctly)
  reported "gitleaks did not read the history", so the rule was a failure (exit 3), never a result. The tool's own `git` is now run
  with `core.longpaths=true`, set through git's environment configuration (`GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_0`,
  `GIT_CONFIG_VALUE_0`): neither the repository's nor the user's config file is modified (the user's global config is still read, as before; git older than 2.31 ignores these variables and the scan fails loudly as it did). With it the same repository scans 1
  commit and reports what it holds (the seeded secret is found, in a new test against the real tool). No other tool is changed: only
  gitleaks was observed to fail this way.

## 0.6.16 - 2026-10-14 (proposed)

### Fixed
- **`reachability` runs on a procedural-macro crate whose entry point is written inside `cfg_attr` (the remainder of issue 54).**
  0.6.7 kept `#[proc_macro]`, `#[proc_macro_derive(..)]` and `#[proc_macro_attribute]` functions `pub` but did not read an
  attribute written as `#[cfg_attr(<predicate>, proc_macro_derive(..))]`, so that crate still exited 3. The three names are now
  recognised after the predicate of a `cfg_attr`, on one line or spread over several (strings and `//` comments are skipped; the
  predicate itself is not read, because `proc_macro` is also a cfg name). The rewrite cannot evaluate the predicate, so a function
  whose attribute applies only under some cfg is kept `pub` in every build: that can hide a dead item in such a function, never
  report a live one. Every other `pub` item in the crate is still downgraded. This supersedes the 0.6.7 sentence that says an
  attribute inside `cfg_attr` is not recognised (that entry now points here).
- No backport branch: published 0.6.9, 0.6.11 and 0.6.14 already carry the 0.6.7 fix, and the line is pre-1.0 (users take the newest
  0.6.x), so this ships in the next 0.6.x publish.

## 0.6.15 - 2026-10-14 (proposed)

Documentation only; no code or behaviour change.

### Added
- **`ROADMAP.md`: where CodeRipper should go, in order, and who consumes it** (the PM's request; the PM edits it). Nine items,
  from the remainder of issue 54 (a proc-macro entry point written inside `cfg_attr` still exits 3) through the triage that gates
  the all-rules-by-default breaking release, to the hosted service, which waits for CireSnave's go. It records what a search of the
  portfolio's tracked CI files found: no repository runs CodeRipper in its CI yet, this one included.

## 0.6.14 - 2026-10-13 (proposed)

Documentation only; no code or behaviour change.

### Added
- **Design: runner isolation** (`docs/superpowers/plans/2026-10-13-runner-isolation.md`, nothing built, nothing measured). Replaces
  the container-first v1 stance: a hardware-virtualised microVM is the required isolation (plain containers not admitted,
  emulation a measured fallback); requirements R1-R10; a threat model; a comparison of container, gVisor, Firecracker, Cloud
  Hypervisor, QEMU/KVM, Hyper-V, Windows Sandbox, WSL2 and emulation with the evidence for each and what is not known; a
  default-deny network with a proxy-side domain allowlist and per-repository approvals recorded in the report; the host-side agent as
  the attack surface; result integrity for runners the operator does not run; a per-host-class and per-stack launch gate with its
  fixtures; and the measurement spike (CodeRipper's own build: 46 s in a hardware-virtualised Linux guest, 2 min 36 s native on
  Windows, 3 runs each, caveats in section 9; the emulation arm was attempted, is blocked (no user-mode emulator is registered) and stays unmeasured by ruling, see section 9.3). The terms' section 4 now says the network is limited to an allowlist and the report
  lists approved domains.

## 0.6.13 - 2026-10-13 (proposed)

Documentation only; no code or behaviour change.

### Added
- **Design: configuration layers** (`docs/superpowers/plans/2026-10-13-configuration-layers.md`, nothing built). One file name,
  `.coderipper.toml`, searched in `/etc/coderipper/`, `/etc/` and the home directory and combined; after those the repository's
  file (restricted to an allowlist of keys: `[[allow]]`, `[[tracks]]`, `[buf]`, `profile`; every other key warns and is skipped,
  an unknown key is an error); after that the environment variables; command-line flags last. Limits are ceilings combined by
  minimum and are enforced from outside the job's container. `[[allow]]`/`[[tracks]]` in a wider file are applied with a warning
  and marked in the report. Requested by CireSnave, 2026-10-13.

## 0.6.12 - 2026-10-13 (proposed)

Documentation only; no code or behaviour change.

### Changed
- **Hosted service: the terms of service are now a template each hosting site renders (v2), and the queue ordering is a weight
  with aging.** `docs/hosted-service/terms-of-service-draft.md` is rewritten after CireSnave's 2026-10-13 answers and two outside
  reviews of v1: the operator (the site that hosts a CodeRipper service) fills in `{OPERATOR_NAME}`, `{CONTACT}`,
  `{GOVERNING_LAW}`, `{MIN_AGE}`, `{LIABILITY_CAP}` and its own retention values; an intake authorisation text with `{repository}`
  and `{username}`; a processing licence; run-time (30 minutes by default) and resource limits; third-party runners disclosed;
  takedown, privacy, secret, abuse and vulnerability reports as separate processes; a narrow compensation clause that excludes the
  operator's own fault; copyright complaints and retention are the hosting site's, not CodeRipper's. Part D records an Arizona and
  federal desk check of the liability and compensation clauses, with what it found and what it did not. **Not legal advice; no
  lawyer has reviewed it.** `docs/superpowers/plans/2026-10-12-hosted-service.md` section 10 replaces "fewest runs, then least
  resource" with a weight (recent wall-clock use, expected run time, a site adjustment) that is lowered every N minutes (default
  10) while a job waits, so a heavy job is delayed but not delayed forever. Nothing is built.

## 0.6.11 - 2026-10-13 (proposed)

### Added
- **`--redact` (and `coderipper::redact::set_policy(RedactionPolicy::On)`): third-party output is redacted on its way out.**
  Step 2 of the redaction pass designed in `docs/superpowers/plans/2026-10-12-redaction-pass.md`; **off by default, so no run's
  output changes unless it is asked for.** With it on:
  - a failed build script is reported as a fixed sentence built from what cargo says about it, never from what the script
    printed: `build script of foo-sys 0.1.0 failed (exit status 101); its output is withheld` (the package, its version and the
    exit status are each checked against a strict grammar and dropped when they do not fit);
  - the stderr of an external module or of a delegated tool that crashed is not repeated: `...; its stderr is withheld` after
    the exit status; a module's unreadable hello is not echoed either;
  - every other third-party string that reaches a message (a compiler error's text, a note, the stderr a delegated tool wrote on
    an exit that was not a failure, a line of a tool's report) goes through `redact::text` **before** its length cut, so a cut
    cannot leave half a secret; `redact::text` also removes ANSI escapes and zero-width characters first, so a secret split by one
    cannot hide;
  - the whole `RunResult` (errors, notes, each finding's summary, detail, project, subject, member, positive control and location
    file, each waiver's reason) goes through it once more at the end of each library run function, and one note says how many
    placeholders the result holds, counted from the result itself.
  A result built or deserialised elsewhere is covered by the new `RunResult::redact()`.
  Redaction never changes an exit code, a finding's existence or an error's existence. It removes known keys and shapes only
  (see 0.6.10): **a secret with no recognisable key or shape is not removed, and no output is called clean.** The policy is a
  process-wide setting (set it once at start-up), as the build cache's configuration is.
  Not done: a compiler message that quotes the project's own source (`compile_error!`, `include_str!`) is redacted, never
  withheld; a build script's name and version are shown as cargo's line gave them (a process that writes an unindented
  `error: failed to run custom build command for ...` line to cargo's stderr can choose them, within a strict character set); a
  path that contains a user name is not touched; `conformance` has no `--redact`; the hosted service's own scratch-prefix
  stripping and its escaping on render are the hosted front end's.

## 0.6.10 - 2026-10-13 (proposed)

### Added
- **`coderipper::redact`: a function that removes known secret shapes from third-party text** (`redact::text(&str) -> (String,
  usize)`, and `redact::REDACTED`). It is the first of two steps of the redaction pass designed in
  `docs/superpowers/plans/2026-10-12-redaction-pass.md`; **nothing calls it yet**, so no output of any run changes. The second
  step wires it into the sinks (a build failure's `Cause:`, an external module's stderr, a delegated tool's stderr, notes) behind
  a `--redact` policy that is off for local runs and on in the hosted service.
  It replaces: the userinfo of a URL (`scheme://user:password@host`, also when the password holds `/`, `?` or `#`); the value
  after a key whose name contains `token`, `secret`, `password`, `passwd`, `pwd`, `apikey`, `credential`, `privatekey`,
  `session` or `cookie`, or has `auth`, `oauth`, `authkey` as a whole segment (`KEY=value`, `KEY: value`, `"key": "value"`,
  also with escaped quotes); the credential of an `Authorization:` header (plain or quoted, as in JSON), and of a bare
  `Bearer`/`Basic` word that looks like a token (16+ characters with a digit, or 32+); a `PRIVATE KEY` block (also a PGP block,
  and a block whose start was cut off); a 32+ character word that follows such a key with no separator; and a short list of
  token shapes (GitHub, AWS access key ids, npm, crates.io, Slack). **It removes known keys and shapes only: a secret with no
  recognisable key or shape is not found, and the output is never "clean".** It over-redacts on purpose (`tokenizer=bpe` loses
  `bpe`, `ssh://git@host` loses `git`, an unlisted `Authorization` scheme loses two words) but leaves a developer's diagnostics
  alone (`crate::auth::Claims`, `src/session.rs:12:5`, ``expected token: `;` ``). It is idempotent. The count it returns is an
  upper bound on values (a value two rules both recognise counts twice). Known gaps: a multi-parameter `Digest` header keeps
  its later parameters; `--password hunter2` (a flag, a space) is not seen unless the word is long; a URL password that
  starts with a digit and holds `/`, `?` or `#` reads as a port. It does not remove paths that contain a user name.

## 0.6.9 - 2026-10-13 (proposed)

### Fixed
- **`reachability` no longer says an item "has zero callers" when it has callers that are themselves reported.** After the
  `pub` downgrade a whole chain lights up: a private helper called only by an unused public function was reported as
  "`children_of` has zero callers found within this crate" although `eval_recipe` calls it (the 2026-10-12 sweep read 9 of 32
  sampled findings of this kind). When every item that mentions the candidate is itself a finding of the same run, the
  summary now reads "`children_of` is reachable only from `eval_recipe`, which has no callers found within this crate" (up to
  three names, then a count), and the detail says to deal with those first. A candidate nothing mentions, or that anything not
  reported mentions, keeps the old text. The set of findings, their locations, subjects and severities, and the exit code are
  unchanged; only the `summary` of such findings (and the `detail`) differ, so a script that matched the old sentence for these
  items must allow for the new one. A type is not attributed to its own methods (an inherent impl lends its header to them).
  Two dead items that only call each other (`fn a() { b() }`, `fn b() { a() }`) are said to be reachable only through each
  other, since "which has no callers" would be false of either. Matching is by identifier, like the rest of the rescue, so a
  name shared with another item can make the list longer than the truth, and a word that appears only in a string literal that
  is itself a path, or in `{name}` inside a string, counts as a mention; an `impl` header and a `use ... as` alias are not
  read, so the list can also be incomplete. Applies to a package with a library (the only case the rescue pass runs).

## 0.6.8 - 2026-10-13 (proposed)

### Changed (a loosening: fewer `reachability` findings in a workspace)
- **`reachability` counts what the workspace members that depend on a package use.** A member was judged alone, so a library's
  API that only its sibling crates call was reported as having "zero callers" (the 2026-10-12 sweep read 32 findings in four
  repositories: 9 were live through another member or through trait dispatch, all 8 sampled from `unisql` among them). The
  check now also reads every `.rs` file of the members that depend on the package under test, directly or through other
  members, by `path` dependency of any kind (normal, dev, build), and does not report an item one of those files names. A member
  that does not depend on the package rescues nothing, and a dependent's library counts as well as its bins and tests, so an
  item a middle crate re-exports to a third is still seen. **A run that reported such items now reports fewer findings (and a
  `--deny` run that failed only on them now passes).**
- The note on a finding says how many candidates were set aside only because of a member, and lists the members that
  depend on the package.
- Matching stays by identifier, as for bins and tests, so it can hide a dead item that shares a name with something a dependent
  member names (it does not report a live item as dead). A dependency redirected with `[patch]` or `[replace]` is not seen
  (cargo metadata gives it no path), so a member that reaches the package that way rescues nothing.

## 0.6.7 - 2026-10-13 (proposed)

### Fixed
- **`reachability` runs on a procedural-macro crate (issue 54).** The check downgrades every top-level `pub` to `pub(crate)`
  to see what nothing uses, and it did that to `#[proc_macro]`, `#[proc_macro_derive(..)]` and `#[proc_macro_attribute]`
  functions too; rustc requires those to stay `pub` ("functions tagged with `#[proc_macro_derive]` must be `pub`"), so every
  proc-macro crate was "could not run" with exit 3 (found on `vulkane_derive` in the 2026-10-12 portfolio sweep). The three entry
  points now stay `pub`; every other `pub` item in the crate is still downgraded, so a dead helper next to them is still
  reported. A crate with a proc-macro member that exited 3 only for this reason now runs. The attribute is recognised line by
  line: on its own line, stacked with other attributes, `//` or `/* */` comments, doc comments or blank lines, spread over
  several lines, or followed by the item on the same line; an attribute written inside `cfg_attr` is not (that crate still exits
  3; **fixed in 0.6.16**). An attribute whose brackets never balance is given up on at the next line starting with `pub ` or `#[`.
  `unused-return-values` and `unused-parameters` were run on a minimal proc-macro crate and were not affected.

## 0.6.6 - 2026-10-12 (proposed)

### Documented
- **Hosted service: terms of service draft, scheduling, and the community-post route.**
  `docs/hosted-service/terms-of-service-draft.md` is a plain-language draft of the terms and the takedown/abuse text for scan-by-URL
  (public repositories the user can push to; a no-network sandbox; results shown and posted by the user), with every legal assumption
  marked and the contact left as `{CONTACT}`: **a draft for CireSnave's approval, not legal advice and not reviewed by a lawyer.**
  `docs/superpowers/plans/2026-10-12-hosted-service.md` gains §10 (the queue is ordered by fewest runs in the last month, then least
  resource used, per CireSnave; capacity limits for runners that are his own VPS and home PCs, with stricter isolation because they
  are his machines) and §6.1 (route A agreed with the Community lane: the user posts from their own session via a URL-fragment
  prefill and an idempotency key; no service credential). Nothing is built; no behaviour changes.

## 0.6.5 - 2026-10-12 (proposed)

### Documented
- **Hosted service: sandbox, abuse policy and community-post scope (design only).**
  `docs/superpowers/plans/2026-10-12-hosted-service.md` turns CireSnave's board-180 answers into a design: face A (results uploaded
  from the user's own CI, first) and face B (scan by URL, later); a per-job isolated runner whose build phase has **no network at
  all**, so a hostile `build.rs` cannot exfiltrate and a build that needs the network is a reported gap, never a pass; fair-share
  ordering by an account's trailing usage (no quota) with a per-account concurrent-job cap of 2 stated as a safety measure; v1 B
  scans only a public repository the logged-in user can push to; retention 30 days by default, user-settable, ceiling 365. It
  records the Community lane's finding that **no service can post as a user today** (browser sessions only, no idempotency key, no
  per-account post rate limit) and what that does to the "results post to community" ruling. Nothing is built; no behaviour changes.

## 0.6.4 - 2026-10-12 (proposed)

### Documented
- **Redaction pass (design only).** `docs/superpowers/plans/2026-10-12-redaction-pass.md` lists every place third-party build and tool
  output reaches CodeRipper's output, what is done to it today (ANSI, control and bidi characters removed, length cuts; **no secret
  redaction anywhere**), and a design for the hosted service: say less first (fixed sentences for build-script and tool failures),
  redact known key and URL-userinfo shapes second, one function plus a final pass at serialisation. Nothing is built; a developer's
  local output is unchanged.
- **Known limit of 0.6.3's set-aside note.** `reachability` prints "N dead-code candidate(s) were set aside" as a note only when the
  check has no findings at all (`findings.is_empty()`). When every finding is waived, the note is not printed; the count is in the
  findings' `detail`, as before. Untested: DOC-010's findings limit is applied after the web-root reading, and no test sets
  `max_findings` for it.

## 0.6.3 - 2026-10-11 (published)

### Changed (a loosening, not a fix)
- **`reachability`'s "N dead-code candidate(s) were set aside" is a note, not a finding.** When a library's own bin, test, example or
  bench names every candidate, the check used to report one `Info` finding saying so. Under the 0.5.0 default (`--deny info`: any
  finding fails) that bookkeeping line made a crate with nothing wrong exit 1. It is now a note (stderr, and `notes` in the JSON summary),
  so **a run that exited 1 on only that line now exits 0**. The count is still printed on every run that sets candidates aside and finds
  nothing else; when there are findings, the count is still in their `detail`.
- API: `Check::run_noted` (default: `run` and no notes) lets a check say something that is not a finding; `RuleOutcome::notes`; the human
  line for a clean run now reads `(see the N note(s) on stderr)` instead of `(N not run: see the notes on stderr)`, because a note is not
  always a rule that did not run.

## 0.6.2 - 2026-10-11 (proposed)

### Added
- **A build that fails now says why.** `reachability`, `unused-parameters` and `unused-return-values` used to end a crate whose build
  failed with "build reported real compiler error(s) (or failed without any diagnostic), so no result can be trusted" and nothing else
  (a SvelteKit app's `backend/` crate on a machine with no OpenSSL: a `-sys` build script panicked, and the message never said so). The
  message now ends `Cause: <...>`: the first real compiler error with its code and place (`error[E0425]: cannot find value ... (src/main.rs:2)`),
  or cargo's own first `error:` line and the first two lines the failed build script printed (`error: failed to run custom build command
  for `openssl-sys v0.9.117` | Could not find directory of OpenSSL installation, ...`), on one line, control characters removed, cut at
  400 characters. Exit codes are unchanged (3); the strings in the JSON summary's `errors` now carry the cause too, which is third-party
  build output (paths, environment values a build script printed): a hosted service must treat it as untrusted text.
- **Text output names where a finding is.** `[Medium/High] app — actions/checkout@v4 is not pinned to an immutable hash at
  .github/workflows/ci.yml:24 (SUP-008)`: ` at <file>:<line>` (or ` at <file>` for a finding about a whole file) before the check id, for
  findings and waived findings alike. Eight findings of one rule used to print eight identical lines; the JSON always had the location. A
  finding line still ends in `(<check id>)` (a waived one then adds `-- reason: ...`). A script that matched the whole summary text of a text line exactly must allow for the suffix (the
  JSON output is the stable interface).

## 0.6.1 - 2026-10-11 (proposed)

### Fixed
- **DOC-010 no longer reports a root-relative link in an HTML page as dead.** lychee resolves `/branding/a.png` against the repository
  root, which is how GitHub reads a Markdown file and not how a site serves a page (a SvelteKit app serves it from `static/`, an Astro
  site from `public/`). The 2026-10-11 dogfood run reported two such links in a SvelteKit `app.html` as dead although both files exist
  under `static/`. In a `.html`/`.htm` page, a link that is written root-relative on its line is now **not judged** (the repository does
  not say where the web root is): it is not a finding, the run says how many it did not judge, and a run with nothing else to say is
  the usual gap, never clean. A dead relative link in the same page is still a finding, and a root-relative link in Markdown is still read
  against the repository root. Found by reading the link on its line, not by guessing a web root. Known limits: a line that holds both a relative and a root-relative
  reference to the same file leaves both not judged; a form the reader does not recognise (an unquoted value ending in another character, a
  non-ASCII spelling the page escapes) is still read against the repository root, the old behaviour.

### Documented
- README: where `.coderipper.toml` is read from. A root file is **not** applied to crates discovered below the root (each reads its
  own); that errs toward more findings, never fewer, and `--deny` is one setting for the whole run.

## 0.6.0 - 2026-10-11 (proposed)

### Breaking
- **A project root with no tracked `Cargo.toml` is "no Rust crate at this path", not an error.** `coderipper fast`/`sweep`/`check` and
  `cargo coderipper` used to ask cargo about the root, fail four times (`cargo metadata ... could not find Cargo.toml`) and exit 3 on any
  repository that is not a crate at its root (found by the 2026-10-11 dogfood run: a Rust-free site and a SvelteKit app with its crate in
  `backend/` both exited 3 and were never audited). They now exit 0 (or 1 on a finding) with a note, and the four crate-reading rules are
  *not run here* in the coverage. A script that treated that exit 3 as "this repository is not Rust" must read the note instead.
- **Crates below the root are discovered and checked** (git index, never a disk walk): each cargo workspace found is checked as `--project`
  would check its members, and the run says which crates it found. A nested crate that does not build is exit 3 as before. `--workspace`
  does the same when the root has no crate.

### Added
- `Check::requires_rust_crate()` (default `false`; the four crate-reading built-ins return `true`): the host does not run such a check at a
  root with no crate. A check that is not about Rust (`ci-protection-presence`, your own) is unaffected.
- `LanguageCoverage::claimed_unproven_ids` and a `claimed_unproven_ids` key in the `coderipper-coverage` JSON line.

### Fixed
- **The coverage list no longer calls a built rule "not yet implemented".** `ci-protection-presence` (built, no conformance fixture yet
  because it needs a forge) was listed under "Not yet implemented"; it is now listed under "Implemented, not yet proven". `gaps` in the JSON
  line is unchanged (it still holds both).

## 0.5.0 - 2026-10-09 (proposed)

### Breaking
- **A finding fails the run by default.** `--deny` used to be off: findings were printed and the run exited 0 unless the caller passed
  `--deny <level>`. It now defaults to `info`, so **any finding makes the run exit 1** (CireSnave's rulings Q7 and Q9, 2026-10-08).
  **To restore the old behaviour pass `--deny none`** (findings are printed, the run never fails on one); `--deny medium` (or any level)
  still lets findings below it through. A CI job or script that ran `coderipper` or `cargo coderipper` without `--deny` and expected
  exit 0 now exits 1 when the project has a finding. The stale-waiver and unknown-check findings of the allowlist (check `allowlist`) are
  findings like any other and fail the run too. Exit 3 (a check could not run) still outranks exit 1.
- **An `[[allow]]` entry with a key it does not know is now an error.** Before 0.5.0 such an entry (an old `note = "..."`, a misspelt `line = 5`)
  loaded and suppressed; now the file does not load, the run reports `allowlist: ...` and exits 3. An existing `.coderipper.toml` whose entries
  carry only `check`, `file`, `symbol` and `reason` loads and behaves as before (apart from the default exit code above).
- **Coverage gaps never fail a run** (unchanged and now stated as a rule): a rule not implemented for a language, or whose tool is not
  installed, is reported as not run, never as a finding.

### Added
- **Waivers.** An `[[allow]]` entry now needs only `check`, `file` and `reason`. `symbol` is optional (omitted: every finding of the check in
  the file), and a new optional `lines = "12"` / `"10-20"` (inclusive) narrows an entry to findings reported at those lines. A finding with no
  line is never matched by an entry with `lines`. A `lines` value that is not a line or a range (`"0"`, `"20-10"`, `"+5"`, `"5 - 7"`) is an error
  rather than a wider waiver.
- **A waived finding is shown, not dropped.** Human output prints `[waived/<severity>] ... -- reason: <reason>` for it; `RunResult::waived`
  (a new public field; `Waived { finding, reason }`) carries it; it never counts toward exit 1. The stale-entry judgement is unchanged (an
  entry that waived at least one finding is live).
- **JSON schema addition:** a `coderipper-waived` line per waived finding (the `Finding` fields, plus `waiver_reason`), and `waived` (a count) in
  the `coderipper-summary` line. Existing keys and lines are unchanged.
- The roadmap records the `SEC-009` (checkov) decision: no Python prerequisite and no `pip` installer; the rule stays a reported coverage gap
  until checkov ships a standalone binary or another route is justified (README, "Rules waiting on a tool").

## 0.4.5 - 2026-10-09 (proposed)

### Added
- **API-006 (schema changes obey evolution rules), run by [buf](https://github.com/bufbuild/buf)** (Apache-2.0; multi-language P3, PR 7)
  under `--profile extended`: `buf breaking`, category `FILE`, over the project's Protocol Buffers schemas, comparing the **working tree**
  with a **baseline** commit. Local-tier: the baseline is a commit of the local repository and the invocation names no remote (not measured with
  the network blocked); no token reaches the tool.
- **The baseline** is the git ref named by `[buf] baseline = "<ref>"` in `.coderipper.toml` (new; a branch, tag or commit, normally the last
  release), else the merge-base of `HEAD` with `origin/HEAD` (`origin/main`, `origin/master`). A ref that was named and does not resolve is an
  error (`config_invalid`); a ref that starts with `-` is refused, never passed to git.
- **Nothing compared is a gap, never clean** (PM ruling). Asked to compare a tree with itself, buf answers with nothing, and that is not a
  check. The rule is reported as **not run** when there is no `origin` and no named ref, when a shallow clone lacks the merge-base, when **no
  `.proto` file differs** from the baseline (a run on the default branch with a clean tree, where the merge-base of `HEAD` with itself is
  `HEAD`; a branch that changes no schema), when the files that differ are **not below a module that has schema on both sides** (measured: buf
  is not given them, exits 0 and prints nothing), when the baseline holds no `.proto` file below a module (a schema that is new cannot break
  anyone), and when every schema was removed from the tree. Uncommitted schema changes are compared with the baseline, also on the default
  branch. A rename away from `.proto` counts as a deletion (`git diff --no-renames`).
- **Modules.** buf needs the same modules on both sides and refuses a module with no schema (measured: "input contained 1 images, whereas
  against contained 2"; "had no .proto files"). The roots are read from each side's own `buf.yaml`/`buf.work.yaml` (the baseline's from
  git) and the modules compared are those with schema on both sides: a module added since the baseline cannot break anyone and is skipped; a
  module that had schema in the baseline and has none now is a removal buf cannot report, so the result carries a note and a clean result is
  **not run**.
- **The repository cannot silence the rule.** Measured with buf 1.73.0: a `buf.yaml` with `breaking: ignore:` drops the findings, at the tree and
  at the baseline, nested ones too. Both are replaced by a configuration given on the command line (`--config` and `--against-config`).
  An inline `// buf:breaking:ignore` comment is not a feature of buf 1.73.0 (measured: the finding comes back); `buf:lint:ignore` is lint's
  and the rule does not run lint. The project's way to suppress a finding is the `.coderipper.toml` allowlist.
- **Module roots** (where a file's imports are relative to) are read from the tracked `buf.yaml`/`buf.yml`/`buf.work.yaml` files: a v1 file's
  directory, a v2 file's directory or the `path:` of each of its `modules:`, a work file's `directories:`; else the project's directory. buf is
  given the top of the working tree as its input and these as its modules (measured: a module root is only found that way when the tool
  runs outside the project). A project that sits in a subdirectory of a repository, or a linked worktree, is compared the same way and its
  findings are named relative to the project.
- **A schema buf cannot compile makes the rule a gap** (a missing import, which includes a Buf Schema Registry dependency since no
  token reaches the tool; a syntax error). Measured: buf builds all of the schemas or none; with one file that does not compile it prints
  `COMPILE` entries (exit 100) and **no violation at all**, even for a real breaking change in another file. So the rule is reported as
  **not run**, naming the file, never clean.
- **A file that was deleted has no `path` in buf's output** (`FILE_NO_DELETE`); it is named from buf's message when exactly one file of the
  baseline or the index matches, else the finding is kept without a location.
- **Only the project's files are reported:** buf walks the module roots on disk, so a finding is kept only for a file the index tracks or the
  baseline had (a removed file is reported by its baseline name; there is no skip list for these: a package of the project called `target` or
  `vendor` is judged). A violation left out is counted, the result says so, and a clean result then becomes **not run**. Output past the findings
  limit is an error.
- **One finding per violation**, at the file and line buf prints, severity and confidence from the catalog (high), subject buf's rule id
  (`FIELD_SAME_TYPE`, `FILE_NO_DELETE`, ...). buf's text is third-party text: control characters are removed and the length is cut.
- **buf 1.73.0 is pinned** for `x86_64-windows`, `x86_64-linux`, `aarch64-linux`, `aarch64-macos` and `x86_64-macos`, as the publisher's bare
  binaries; the hashes are the ones in the release's `sha256.txt`, re-checked against the downloaded files. (`windows_arm64` is not pinned,
  as for gitleaks and osv-scanner.)
- **Conformance and tests.** The fixture runner can give a fixture a history: a `.baseline/` directory is committed first, the fixture proper
  on top of it. `conformance/API-006/` (a field retyped from `int32` to `string`, and a compatible new field) is proven against the real
  buf. `tests/delegated_buf.rs` installs the pinned buf and covers the cases above against it. Every repository in the tests is built by the
  test: **pull-request CI depends on no web site and no day's data** (it downloads the pinned buf from its GitHub release once).

### Known limits
- Only Protocol Buffers: other schema formats (Avro, JSON Schema, OpenAPI, GraphQL) need their own tools. buf's lint and its `WIRE` and
  `WIRE_JSON` breaking categories are not run.
- Whether a retyped field was in use anywhere is not known; the rule reports the schema change.
- A project with a Buf Schema Registry dependency, or with schemas that import a file kept outside its modules, is a gap until the import can
  be resolved (no registry token reaches the tool).
- The `buf.yaml` reader is small (v1 directory, v2 `path:` entries in block or flow style, work-file `directories:` as a block list). A form
  it does not know yields no root; with no root at all the project's directory is the module, and imports that do not resolve make the
  rule a gap, never a clean pass.
- A stray schema in an untracked or ignored directory below a module root that conflicts with the project's (the same message declared twice)
  stops buf from building anything: the rule is a gap. (buf's `--exclude-path` is relative to the tool's working directory, which is a scratch
  directory by design, so it could not be used.)
- Not exercised: a `--workspace` run (the baseline is read from the git history of the checkout the rule is given, and `[buf] baseline` from the
  `.coderipper.toml` in it; a test covers `--project <member>` only).
- A malformed or dependency-bearing `buf.lock` in the project makes buf fail (an error, not a gap). A schema inside a git submodule is not
  tracked by the project's index and is not compared. The configuration is passed twice on the command line, so very many modules (hundreds)
  could reach a Windows command-line limit.
- A path containing `#` or `,` cannot be given to buf (its input syntax): the rule is a gap there.
- Only gitleaks, osv-scanner, lychee, zizmor and buf are pinned. checkov (PyPI only) needs a Python-prerequisite decision.

## 0.4.4 - 2026-10-09 (proposed)

### Added
- **SUP-008 (CI workflows pinned and least-privileged), run by [zizmor](https://github.com/zizmorcore/zizmor)** (MIT; multi-language
  P3, PR 6) under `--profile extended`. zizmor is given the project's *tracked* GitHub Actions files (`git ls-files`): the workflows in
  `.github/workflows/` and every `action.yml`/`action.yaml` (not those below `node_modules`, `vendor`, `third_party` or `target`). It runs
  **offline**, so the rule needs no network and no token and is not sweep-tier. Of what zizmor reports only the three audits that are this
  rule's wording are kept: `unpinned-uses` (an action or reusable workflow referenced by a tag or branch, not an immutable hash), `unpinned-images` (a `container:`,
  `services:` or `docker://` image not pinned to a digest) and
  `excessive-permissions` (the broad default token, `write-all`, `read-all`, a workflow-level write scope). Every other zizmor audit
  (template injection, credential persistence, dangerous triggers, ...) is another rule's business and is not reported as SUP-008.
- **One finding per zizmor finding**, at the file and line of its primary location, severity and confidence zizmor's own, subject the
  `uses:` value (or, for a permission, the place in the file: `permissions`, `jobs.build`). Workflow text is third-party text: control
  characters are removed and the length is cut.
- **The stricter `pedantic` persona is used, on purpose.** Measured against zizmor 1.30.1, its default persona does not report
  `permissions: write-all` or an unpinned `container:` image at all; `pedantic` reports both (and, on the workflows probed, nothing for the audits kept
  that the `auditor` persona adds). A test pins the reason: it fails if the default persona starts reporting `write-all`.
- **A workflow zizmor could not read makes a clean result a gap.** Measured: a workflow with a YAML syntax error beside a valid one is skipped
  with a warning and zizmor exits **0** with `[]`. zizmor is run quietly (`-q`: stderr then holds only warnings and errors, so a long log
  cannot push the warning out of the part kept), and a warning that an input failed to parse, validate or load beside an exit 0 is a partial read (another audit's warning is not): findings carry a note, and a
  clean result is reported as **not run** (`SUP-008 not run: ...`), never clean. A project whose every workflow is unreadable (zizmor exits 3,
  "no inputs collected") and a project with no workflow or action are gaps too.
- **The analysed repository cannot silence the rule** (design 5.2). Measured: a committed `zizmor.yml` or `.github/zizmor.yml` with
  `rules: unpinned-uses: disable: true` drops the findings, and so does a `# zizmor: ignore[unpinned-uses]` comment. `--no-config` and
  `--no-ignores` are passed; a finding that carries such a comment is still reported and says that CodeRipper does not honour the comment.
  (`.gitignore` does not hide a file named on the command line; measured.) The project's way to suppress a finding is the
  `.coderipper.toml` allowlist.
- **zizmor 1.30.1 is pinned** for `x86_64-windows`, `x86_64-linux` (gnu), `aarch64-linux` (gnu), `aarch64-macos` and `x86_64-macos` (every
  build the release has), as the publisher's archives (`.zip` on Windows, `.tar.gz` elsewhere). **The release lists no checksum file**: the
  archive hashes are the SHA-256 digests GitHub computed for each release asset (`assets[].digest` of the release API), re-checked against the
  downloaded archives; the lock pins the extracted executable's hash too.
- **Many workflows are read whole**: file names are passed to zizmor in groups that fit a command line (a Windows command line is cut at
  32 767 characters); a test with 250 workflows checks that none is lost at a seam.
- **Conformance and tests.** `conformance/SUP-008/` (a workflow using `actions/checkout@v4`, and the same workflow pinned to the commit
  the `v4.2.2` tag names) is proven by the fixture runner against the real zizmor. `tests/delegated_zizmor.rs` installs the pinned zizmor
  and runs the fixtures and the CLI against it, including the cases above. zizmor runs offline and every workflow in the tests is written
  by the test, so **pull-request CI depends on no web site and no day's data** (it downloads the pinned zizmor from its GitHub release once).

### Changed
- The delegation framework's run result now carries the tool's stdout (`ToolRun::stdout`), for a tool with no option to write its report to
  a file; the `git ls-files` listing lychee used is now shared (`tracked_listing`). No behaviour change.

### Known limits
- zizmor cannot know what a job *needs*: a workflow-level `contents: write` is reported even when it is needed (the finding carries zizmor's
  confidence), and the allowlist is how to say so. The rule is about *declared* permissions.
- A write scope declared on one job (`jobs.x.permissions: contents: write`) is **not** reported: zizmor treats that as the right way to ask
  for it. A workflow-level `read-all` is reported.
- A `uses:` with no `@ref`, an expression (`${{ matrix.action }}`) or a workflow that is not UTF-8 makes zizmor stop for the whole project
  (exit 1, "no audit was performed"): the rule is then an **error** naming the cause, never clean, and no other file is judged until it is fixed.
- A tracked `action.yml` that is not a GitHub action (another tool's configuration with that name) is warned about by zizmor and keeps
  the rule a gap ("could not read part"). Fixture and example directories are not excluded.
- Whether a pinned hash is the commit the tag names (zizmor's `impostor-commit` audit) and other online audits are not run: the rule is
  offline.
- Workflows outside `.github/workflows` are not read (GitHub does not run them); `dependabot.yml` and pre-commit files are not audited.
- A finding that zizmor reports twice for one cause (a workflow with no `permissions:` block is reported at the workflow and at each job)
  is two findings, with different subjects.
- Only gitleaks, osv-scanner, lychee and zizmor are pinned. buf follows; checkov (PyPI only) needs a Python-prerequisite decision.

## 0.4.3 - 2026-10-09 (proposed)

### Added
- **DOC-010 (documentation links resolve), run by [lychee](https://github.com/lycheeverse/lychee)** (Apache-2.0 OR MIT; multi-language
  P3, PR 5) under `--profile extended`. lychee is given the project's *tracked* Markdown, HTML and text documents (`git ls-files`, so hidden directories such as
  `.github/` are included and a virtualenv, a build directory or another worktree is not; a tracked file below `node_modules`, `vendor`,
  `third_party` or `target` is left out), extracts their links and checks each: a link to a file or directory must exist (a root-relative `/docs/a.md` is resolved against the project root) and a web page
  must not answer `404 Not Found` or `410 Gone`. **The rule is sweep-tier** (it requests web pages): `coderipper sweep --profile extended`
  runs it, `coderipper check DOC-010 --profile extended` runs it with the network permitted, `fast` leaves it alone.
- **One finding per dead link**, at the document and the line it is on, subject the target (project-relative for a local one),
  severity `Low`, confidence `High` for a local target that does not exist and `Medium` for a web page that answered 404 or 410 (a
  site can answer a bot differently from a reader). Link text is third-party text: control characters are removed and the length is cut.
- **A link that could not be settled is not a good link.** A timeout, a failed connection, a rate limit (`429`), a `403` from a site that turns
  bots away and a `5xx` are *not judged*: not findings, and not evidence the link works. A run where no link is dead but some were not
  judged, a project with no links, and a run where lychee skipped a document it could not read (it exits 0 and says so only on stderr:
  measured with a file that is not UTF-8) are each a **coverage gap** (`DOC-010 not run: ...`), never a clean rule. When dead links are found beside
  links that were not judged, every finding says how many.
- **The analysed repository cannot silence the rule** (design 5.2). Measured against the real tool: a committed `lychee.toml` with
  `exclude = [".*"]` makes lychee exit 0 having excluded every link, and a `.lycheeignore` or `.gitignore` entry hides links or whole documents.
  `lychee.toml` and `.lycheeignore` are only read from the working directory, and lychee is run from an empty scratch directory (with `--config`
  naming an empty file as a second guard and `--no-ignore`), so none is read; the project's way to suppress a finding is the `.coderipper.toml` allowlist.
- **lychee 0.24.2 is pinned** for `x86_64-windows`, `x86_64-linux` (gnu), `aarch64-linux` (gnu), `aarch64-macos` and `x86_64-macos`, as
  the publisher's archives (`.zip` on Windows, `.tar.gz` elsewhere); the archive hashes are the ones in the `.sha256` file the publisher
  ships beside each asset, re-checked against the download, and the lock pins the extracted executable's hash too. (The musl, arm, i686
  and windows-arm builds are not pinned.)
- **Conformance and tests.** `conformance/DOC-010/` (a README linking a guide that is not there, and a clean twin whose every link is to a
  file in the fixture) is `needs_network = true` (unproven without `--network` or the tool, not failed) but requests nothing from the
  web. `tests/delegated_lychee.rs` installs the pinned lychee and runs the fixtures and the CLI against it, including the cases above; its
  only link to a closed port is `127.0.0.1:9` ("connection refused", the same every day). **Pull-request CI does not depend on any web site** (no link check requests one; the tests download the pinned lychee from its GitHub release once);
  the one test that does (a gone page on github.com, a live one on example.com) is `#[ignore]` and runs only in the scheduled, non-required
  `.github/workflows/live-tools.yml`.

### Known limits
- Only a `404`/`410` or a missing local target is *dead*. A domain that no longer resolves is reported by lychee as a failed connection, which
  cannot be told from a machine that is offline, so it is *not judged* (it makes the result a gap rather than a finding).
- `#fragment` links are not checked (the anchors a renderer generates differ from lychee's), nor are mail addresses, other URL schemes,
  or links inside code blocks. The rule is sweep-tier even for a project whose links are all local: a run without the network permission
  reports it as a gap and does not run lychee `--offline`.
- Links to `localhost` or `127.0.0.1` (a development server) cannot be reached from a checker and so are *not judged*: a README that
  mentions one keeps the rule a gap on every run, unless a dead link is found beside it.
- A root-relative link (`/docs/a.md`) is resolved against the project root, the way GitHub reads it. A site served from a subdirectory
  (`static/`, `public/`) reads `/css/x.css` differently, and such a link is reported as a missing file.
- Only tracked documents are read (an untracked or ignored file is not), a file system that ignores case can say a link exists that
  does not on Linux, and the file named in a finding is the repository's own text (control characters in it are removed in the detail).
- A busy project's links to rate-limited hosts (GitHub, say) may keep the rule a gap on some days. CodeRipper does not pass lychee a
  `GITHUB_TOKEN` (the tool runs with a scrubbed environment).
- Only gitleaks, osv-scanner and lychee are pinned. zizmor and buf follow, one per release; checkov (PyPI only) needs a Python-prerequisite decision.

## 0.4.2 - 2026-10-09 (proposed)

### Added
- **SUP-002 (dependencies free of known advisories) and SEC-006 (no known exploitable dependency vulnerabilities), run by
  [osv-scanner](https://github.com/google/osv-scanner)** (Apache-2.0; multi-language P3, PR 4) under `--profile extended`. osv-scanner reads
  the lockfiles, manifests and SBOMs it knows anywhere under the project (but not inside a `node_modules` directory, even with `--no-ignore`), resolves each package to an exact version and asks osv.dev
  which advisories cover it. **Both rules are sweep-tier (they use the network):** `coderipper sweep --profile extended` runs them,
  `coderipper check SUP-002 --profile extended` runs one with the network permitted, and `fast` leaves them alone.
- **One finding per advisory group** (advisories that osv-scanner treats as the same flaw) of one package in one lockfile, at the lockfile
  (no line), subject `ecosystem:name@version`, so the allowlist can name `check` + `file`. Severity follows the group's highest CVSS
  score (9.0 and up `Critical`, 7.0 `High`, 4.0 `Medium`, below `Low`); an advisory with no score is `High`, not hidden. Confidence is
  `Medium` for SUP-002 and `Low` for SEC-006. Advisory text is third-party text: control characters are removed and each summary is cut short.
- **SEC-006 is narrower than SUP-002, and it does not know whether the flaw is reachable.** It reports the groups scored 7.0 or higher
  (or unscored) as a "known high-severity advisory" (or "known advisory with no severity score"), never as "exploitable", and every
  SEC-006 finding says reachability is not analysed. The rule's record says the same in `checked_today`; the rule keeps its id. Reachability is call analysis, which osv-scanner offers only for Go and Rust and does
  by building the project's code (a Rust build script runs), so it is not run. A SEC-006 finding means a serious advisory covers the pinned
  version, not that the project can be attacked through it.
- **The analysed repository cannot silence these rules** (design 5.2): osv-scanner reads an `osv-scanner.toml` in every directory it
  scans (its `IgnoredVulns` drop advisories) and honours `.gitignore`; CodeRipper runs it with `--config` naming an empty file and
  `--no-ignore`, so neither a committed `osv-scanner.toml` nor a `.gitignore` entry hides a lockfile or an advisory. Both were measured
  against the real tool (an ignored advisory vanishes from a plain osv-scanner report and stays in CodeRipper's).
- **A partial read is not a clean result.** osv-scanner names a lockfile it could not read only on stderr (measured: exit 127 when nothing
  else was found, exit 1 beside advisories); that is `tool_failed` (exit 3) naming the file, and so is an empty report with anything on
  stderr. Findings that were read beside an unreadable lockfile are still reported, each with a note that part of the project was not judged. Exit `128` (no package sources found) is a **coverage gap**, not a clean rule: nothing was judged. Exit `1` with no findings in
  the report, an unreadable report, or any other exit code is a failure.
- A run that asks for both rules runs osv-scanner once (the two rules share the report).
- **osv-scanner 2.6.0 is pinned** for `x86_64-windows`, `x86_64-linux`, `aarch64-linux`, `aarch64-macos` and `x86_64-macos`, as the
  publisher's bare binaries; the hashes are those in its `osv-scanner_SHA256SUMS`. (The windows-arm64 build is not pinned yet.)
- **Conformance: `many = true` in `expect.toml`.** An expectation is met by exactly one finding; a rule that reports what an outside
  database holds today reports a number a fixture cannot fix, so an expectation may say "one or more at this file". None is still a miss
  and a finding elsewhere is still unexplained. `conformance/SUP-002/` and `conformance/SEC-006/` (a lockfile pinning lodash 4.17.15, and a
  clean twin) use it and `needs_network = true`: they are `unproven` without `--network` (and without the tool), not failed.
  `tests/delegated_osv.rs` installs the pinned osv-scanner and runs the fixtures and the CLI against it; it needs the network
  (`CODERIPPER_SKIP_NETWORK_TESTS=1` skips it). **Pull-request CI does not depend on the day's database for a clean answer**: the clean
  twin's one package is a name in the fixtures' own scope that no registry holds (so no advisory can ever list it), and the defective
  twin pins lodash 4.17.15 (advisories for years; `many = true` absorbs new ones). A real package with no advisory today is checked by
  a canary test (`#[ignore]`) that only the new scheduled, non-required workflow `.github/workflows/live-tools.yml` runs; its failure is a signal.
- Library: `DelegatedModule` now declares `needs_network` (its osv-scanner rules do).

### Changed
- **A rule that needs the network no longer runs in a `fast` run under `--profile extended`**; before, no rule delegated to a tool needed
  one. `check <id>` naming such a rule gives it the network, as naming a network check always has.

### Known limits
- Only gitleaks and osv-scanner are pinned. lychee, zizmor and buf follow, one per release; checkov (PyPI only) needs a Python-prerequisite decision.
- The advisory database is queried live, so a result is true of the day it was read; a lockfile's packages are judged by exact version
  and osv-scanner's extractors (Cargo.lock, package-lock.json, yarn/pnpm, poetry/pip, go.mod, Gemfile.lock, and the rest it supports).
  A manifest without a lockfile may be resolved through deps.dev (osv-scanner's default), which is not deterministic.
- CodeRipper runs osv-scanner with a scrubbed environment, which has no proxy variables (not measured): a machine that reaches osv.dev
  only through `HTTPS_PROXY` should get `tool_failed`, never a clean rule.

## 0.4.1 - 2026-10-09 (proposed)

### Added
- **The delegation framework, and the first delegated rule: SEC-002 (no committed secrets), run by [gitleaks](https://github.com/gitleaks/gitleaks)**
  (multi-language P3, PR 3b). `coderipper::module::DelegatedModule` runs rules through tools the cache installs. It resolves the tool
  through a `tools::ToolEnv` (lock, cache, consent, fetcher), runs it with a scrubbed environment, an empty scratch directory as its working
  directory and the run's limits (the process tree is killed on a timeout), and reads only its machine-readable output. A tool whose
  output cannot be read is `tool_output_unreadable`, never "no findings"; a non-zero exit is `tool_failed` with the tail of stderr.
- **SEC-002 under `--profile extended`**: `gitleaks git` scans the whole history (a secret committed and later deleted is still
  reported), with `--redact`, so the value of a secret is never read into CodeRipper and cannot reach a finding, a log or the JSON output.
  Each leak is a `High`-severity finding at `file:line`, subject the file (so the allowlist can name `check` + `file`), confidence
  `Medium` (`Low` for gitleaks' generic entropy rule). It sees gitleaks' own rule set at the pinned version, and nothing outside git.
- **A missing tool is a coverage gap, never a failure.** Not pinned, no build for this platform, or not installed and not allowed to be
  (`consent_not_given`): the rule is reported as not run (`coderipper: note: SEC-002 not run: ... run: coderipper tools install gitleaks
  --install-tools`), counted as skipped in the coverage line, and the exit code is untouched. An error is reserved for a tool that was
  asked for and could not be had or trusted: a `checksum_mismatch`, a refused archive, an install that failed after `--install-tools`.
  The coverage figure does not count the claim as covered while the tool is not installed on that machine.
- **The analysed repository cannot silence SEC-002** (design 5.2): gitleaks is run with its default rules only, `--ignore-gitleaks-allow`,
  and the repository's **git directory** as its source, so a committed `.gitleaksignore`, `.gitleaks.toml` or `gitleaks:allow` comment is not
  honoured. The project's way to suppress a finding is the `.coderipper.toml` allowlist (check + file), which is visible in review.
- **Scope**: a project in a subdirectory of a repository gets the leaks under it, named relative to it (history is the repository's);
  a path that is not a git repository with a commit is `tool_failed` (exit 3), never a clean rule; a relative `project_root` is made absolute.
- **A scan that did not read the history is a failure**: gitleaks exits 0 with an empty report when the `git` it runs fails (a repository
  git calls unsafe, say). Its log is checked as a guard: an `ERR` line, no commit count, or "0 commits scanned" in a repository that has
  commits makes the rule `tool_failed` (exit 3). The log can only turn "nothing found" into a failure, never the reverse.
- **A shallow clone is a gap too**: gitleaks sees only the commits that were fetched, so an empty report from a shallow repository (CI's
  default checkout) is reported as not run (`git fetch --unshallow`), never as clean. Findings in a shallow clone are still reported.
- A run in which a rule did not run for want of its tool no longer prints a bare `coderipper: no issues found`: it says
  `no issues found by the rules that ran (N not run: see the notes on stderr)`, and the JSON summary line gains a `notes` array
  **only then** (every other run keeps exactly its keys). The coverage line's `not applicable here` count now reads `not run here`.
- **`--install-tools` and `--tools-lock FILE` on `fast`, `sweep`, `check` and `conformance`** (and so `cargo coderipper`). Without
  `--install-tools` a run downloads and writes nothing.
- **gitleaks 8.30.1 is pinned** for `x86_64-windows`, `x86_64-linux`, `aarch64-linux`, `aarch64-macos` and `x86_64-macos`. The archive
  hashes are those in the publisher's `gitleaks_8.30.1_checksums.txt`; each member's hash was computed from the verified archive.
- `coderipper conformance --module delegated` runs the SEC-002 fixtures (`conformance/SEC-002/`, a seeded committed key and a clean twin
  holding a placeholder) against the real tool. Without the tool the rule is `unproven`, not failed. `tests/delegated_gitleaks.rs` installs
  the pinned gitleaks and runs the fixtures and the CLI against it; it needs the network once (`CODERIPPER_SKIP_NETWORK_TESTS=1` skips it).
- Library: `RuleStatus::Unavailable`, `RuleResult::tool_unavailable`, `TOOL_UNAVAILABLE` (only a built-in, in-process module may report a
  tool gap; from an external module it is a `protocol_mismatch`), `RunResult::notes`, `tools::ToolEnv`, `run_checks_in_with_tools` and
  `run_workspace_in_with_tools`. `run_checks_in` and `run_workspace_in` keep their signatures and use `ToolEnv::from_environment()`,
  which has no consent to install.

### Changed
- `coderipper tools list` now lists gitleaks (the shipped lock is no longer empty).

### Known limits
- Only gitleaks is pinned. osv-scanner, lychee, zizmor and buf follow, one per release; checkov (PyPI only) needs a Python-prerequisite decision.
- SEC-002 reads history through git, so it needs a git repository (a project outside git is `tool_failed`); a secret gitleaks has no pattern for is not seen.
- A `Composite` of a `DelegatedModule` with an external module counts as not in process, and then refuses a tool gap as a protocol mismatch: compose built-in modules only.

## 0.4.0 - 2026-10-09 (proposed)

### Breaking
- `coderipper::tools::ToolEntry` and `ToolError` are now `#[non_exhaustive]`, and gained fields (`archive`, `member`, `file_sha256`) and a
  variant (`ArchiveRefused`): struct-literal construction and exhaustive matches outside the crate stop compiling. Pre-1.0, so the
  second number moves (0.3.x was never published). Later additions are non-breaking.

### Added
- **Tools can be installed from an archive** (multi-language P3, PR 3a): a lock entry may name `archive = "tar.gz"` or `"zip"`, the
  `member` to take out (a `/`-separated path of safe components) and `file_sha256`, the member's SHA-256; the three go together or the
  lock is `tools_lock_invalid`. `sha256` is then the archive's (what a publisher's checksum file lists). The archive is hashed before it
  is opened, unpacked in memory, and the member is hashed against `file_sha256` before anything is written; the installed file is
  re-checked against `file_sha256` each time it is resolved. Only the member is written.
- **Archives are refused whole** (`archive_refused`, exit 3 from the CLI) if any entry, wanted or not, has a name that could leave the
  directory (`..`, absolute, a drive, a backslash, a NUL, not UTF-8) or is a symlink, hardlink, device or fifo; so is an archive with two
  entries of the member's name (tar only: the zip reader folds a repeated name into the last entry, so a zip duplicate is not
  reported, but the member's hash still binds what is written) or whose member is not a regular file. A member over the size limit
  is `tool_install_failed`, not truncated; the decompressed tar stream is bounded too, and reaching its bound is an error (a cut on an
  entry boundary would otherwise leave later entries unchecked). An archive that is not the declared kind, or lacks the member, is `tool_install_failed`.
- New dependencies, all pure Rust: `flate2`, `tar` (without `xattr`), `zip` (the library only reads zips; the tests write them).
- `ToolEntry` and `ToolError` are now `#[non_exhaustive]` (they gained fields/variants; this makes the next addition non-breaking).
  Building `ring` needs a C compiler; nothing else here does.
- README: a note that `ring` (C and assembly, through `ureq`'s `rustls`, `cli` feature only) is the one non-Rust dependency, and that
  replacing it is wanted.

### Changed
- `coderipper tools install NAME` with a name the lock does not have is now exit **2** (usage), not 3, and nothing is installed even for
  the valid names given beside it. The message still begins `tool_missing`. A tool known for other platforms only stays exit 3.

### Known limits
- The shipped lock is still empty: the delegations (gitleaks, osv-scanner, lychee, zizmor, buf; checkov needs a pip installer) follow.
- `--install-tools` is still only on `tools install`; it joins the run commands with the first delegation.

## 0.3.7 - 2026-10-09 (proposed)

### Added
- **The tool cache** (multi-language P3, PR 2): the isolated place, and the policy, for the external tools delegated rules will use.
  `coderipper::tools` (`ToolsLock`, `ToolCache`, `Consent`, `Fetcher`, `DefaultFetcher`, `ToolError`) and `coderipper tools list` /
  `coderipper tools install [NAME...] --install-tools [--tools-lock FILE]`.
  - Tools live at `<tools dir>/<tool>/<version>/<file>` (an absolute path); the tools directory is `CODERIPPER_TOOLS_DIR`, else
    `tools` beside the build cache. Nothing is written to a `PATH` or any system location.
  - `tools.lock` pins, per tool and platform, version, source, SHA-256 and SPDX licence. Names that become path components are
    validated (no separators, no `..`, no Windows reserved names, unique ignoring case); a source is `https://`, `http://` on this
    machine (userinfo and backslashes refused), or an absolute `file://` path (no percent-decoding). An `https://` download may
    redirect but never down to `http://`.
  - Nothing is fetched, written or deleted without `--install-tools`; otherwise the error is `consent_not_given` with the exact
    command (naming the same `--tools-lock` when one was given). A download is hashed before anything is written, so a mismatch is
    `checksum_mismatch` and leaves no file. An installed file that no longer matches its lock entry is reported and left alone
    without consent, and replaced with it. Other codes: `tool_missing`, `tool_unavailable_for_platform`,
    `download_failed`, `tool_install_failed`, `tools_lock_invalid`.
  - New dependencies: `sha2`, and `ureq` (rustls) behind the `cli` feature for the download.

### Known limits (deliberate for this PR)
- The shipped lock is empty: no tool is pinned until the delegation PR that needs it. The installer handles a single-file binary
  only; archives, `cargo install`, `npm` and `pip` installers come with the first tool that needs them.
- There is no interactive prompt and no user configuration file yet; `--install-tools` is the only consent. `tools bundle` and
  `tools install --from` (offline sets) are not built; a `file://` source covers a mirror for now.
- `--install-tools` is accepted by `coderipper tools install` only; the run commands do not take it until a delegated rule uses a tool.

## 0.3.6 - 2026-10-09 (allocated by the PM)

### Added
- **The language-neutral module and its first five rules** (multi-language P3, PR 1), behind `--profile extended`: `SUP-001` (a
  committed lockfile for an application; CI installs in the strict mode), `SUP-011` (vendored code excluded from analysis
  configuration), `DOC-005` (decision-record status and supersession links), `DOC-009` (a glossary exists; no term defined twice),
  `WSP-001` (a default code owner). Each reads the tracked files at `HEAD`, reports under its catalog id, honours the
  `.coderipper.toml` allowlist, and is proven by a seeded-defect fixture and a clean twin under `conformance/<rule>/`. Each is a
  narrow reading of a broader rule; the module documents what it does not see (for example, `WSP-001` judges the owner, not an
  inventory with importance and stage of life, and `DOC-009` does not judge how terms are used). The `DOC-009` fixture proves the
  missing-glossary reading; the duplicate-term reading is covered by unit tests only.
- `coderipper::module::NeutralModule`, `coderipper::module::Composite` (modules asked as one, with one allowlist and one rule-id set),
  `coderipper::Profile`, `run_checks_in` and `run_workspace_in` (the existing `run_checks` and `run_workspace` are `Classic`),
  `coverage::for_missing_module_with`.
- `coderipper check <RULE> --profile extended` runs one neutral rule; `coderipper conformance --module neutral` judges them.
- Coverage is derived per rule and language from what each claim really reads. A claim may name its languages
  (`RuleClaim::languages`); `SUP-001` reads Cargo and npm-family manifests only, so a Python or Go project shows it as not yet
  implemented, while the other four (which read file kinds, not languages) are covered for every language. A workspace run takes the
  rules once, at the workspace root.
- Catalog records gain `checked_today`: what the check judges now and what it does not. The five records say so (`WSP-001` judges a
  default owner, not an inventory with importance and stage of life; `DOC-009` a glossary's existence and duplicate terms, not
  usage; `DOC-005` status and supersession links, not references to enforcing rules; `SUP-001` Cargo and npm only).

### Known limits (found by the independent audit; deliberate for this PR)
- A tracked file that cannot be read is skipped by the rule that wanted it, not reported as an error (only a project that is not a
  git repository with a commit is an error for every rule).
- `check <RULE> --profile extended` for a rule that does not apply here (for example `DOC-005` in a repository with no decision
  records) fails the run as "no verdict", because the host treats a module that skips every requested rule as having given none.
- Files are read one `git show` at a time; a project below the repository root sees only its own directory (so a repository-level
  `CODEOWNERS` above it is not seen).

### Changed
- `--profile extended` now also runs these rules, so it can report findings (and, with `--deny`, fail) where it only printed the
  coverage report before. `classic`, the default, is byte-for-byte what it was.

## 0.3.5 - 2026-10-09 (proposed; the PM allocates the number at gate time)

### Added
- **The coverage report and `--profile`** (multi-language design sections 4.4, 6.4, 10.2). `coderipper::coverage` joins the rule
  catalog with what a module claims and has *proven* (a claim with a conformance proof is covered; one without is counted as a
  gap, not as coverage) and with this run's outcomes (clean, findings, skipped, could not run). `--profile classic|extended`
  (default `classic`) and `--coverage[=full]` print it: one line per language, a "Not yet implemented" section, and one
  `coderipper-coverage` JSON line per language before the summary line.
- **A gap never fails a run** (the owner's ruling): coverage is information about CodeRipper's own incomplete implementation, worded
  as such, and no exit code depends on it. Exit codes are exactly as before.
- **One stderr line for a language with no module** under `classic`: `coderipper: typescript: 14 source files found, not
  checked (classic profile; see --profile extended)`, for a manifest plus at least one source file among the files git tracks.
  Stdout and JSON are unchanged, so nothing that reads them changes.
- `RunResult::outcomes` (how each requested rule fared). The built-in Rust module marks the four claims its conformance fixtures
  prove; a test fails if that list and the fixtures disagree.
- No behaviour change under `classic` apart from the stderr line.

## 0.3.4 - 2026-10-08 (proposed; the PM allocates the number at gate time)

### Added
- **`coderipper::conformance` and `coderipper conformance`**: how a module earns a claim to cover a rule (multi-language design
  section 6.2). Each rule has a seeded-defect project and a clean twin under `conformance/<rule>/`, each with an `expect.toml`
  naming where the rule must report (or, for the twin, nothing). The runner copies each into a throwaway git repository, runs the
  one rule, and calls it `proven` only if every expectation is met, nothing unexplained is reported, and the twin is silent.
  A claim with no complete pair is `no fixture`; one that needs the network is `unproven` unless `--network`. `--require-proven`
  turns anything short of proven into exit 1 (for a CI job).
- Fixtures for the four original checks that can run offline: `reachability`, `unused-parameters`, `unused-return-values`,
  `version-consistency`. `ci-protection-presence` needs a GitHub repository and stays claimed but not earned. A test puts the defect
  into a clean twin and requires the real module to fail it, so the passes are not vacuous.
- No behaviour change for the other commands or `run_checks`.

## 0.3.3 - 2026-10-08 (proposed; the PM allocates the number at gate time)

### Added
- **The whole rule catalog**: `Catalog::builtin()` now holds 221 rules: the 216 of the owner's knowledge base (informed by it, written
  in CodeRipper's own words: statement, rationale, remediation, applicability, related rules; every one `experimental`) and the five
  original checks. Records are in `rules/<domain>.toml` (the concurrency domain is `concurrency.toml`: `con` is a reserved device name
  on Windows).
- **`docs/rules/`**: a guide page per domain (the problem, the practices, the cautions, and a table of where each rule is checked, which
  says "no check yet" for everything that is not implemented), the verification model, the contested-guidance register and language
  profiles, the lessons behind the rules, and a reading list.
- Nothing the command or `run_checks` does has changed: the catalog is read by nothing yet, and no new rule runs.

## 0.3.2 - 2026-10-08 (proposed; the PM allocates the number at gate time)

### Added
- **`coderipper::catalog`**: the rule catalog of the multi-language design (P2, first PR). A `Rule` is a record in CodeRipper's own
  words (statement, rationale, scope, network, unit, default severity, `kb_refs`, aliases, lifecycle); `Catalog::builtin()` loads the
  records under `rules/`, and `Catalog::parse` loads any set of record files strictly (an unknown or missing field, a blank statement, an
  ID or alias claimed twice is an error naming the file). The five existing checks have records, and a test fails if a record and its
  check disagree about scope, network or unit.
- `Scope`, `Unit` and `Network` can be deserialized (`local` / `network` for `Network`).
- No behaviour change for the command or `run_checks`: nothing reads the catalog yet.

## 0.3.1 - 2026-10-08 (proposed; the PM allocates the number at gate time)

### Added
- **`coderipper::module`**: the module protocol of the multi-language design (P1). A `Module` answers `describe` and `check`; the five
  built-in checks now run behind `RustModule`, and `ExternalModule` speaks the protocol to a child process (JSON lines, a scrubbed
  environment, wall-clock and output limits, the whole process tree killed on a timeout). `reconcile` is what the host believes: a rule
  that got no verdict (crash, hang, silence, a wrong count, a missing summary) is an error, never a clean run. `run_module` runs any
  module through the same validation and allowlist as `run_checks`.
- No behaviour change for the command or `run_checks`: every existing test passes unedited and the JSON output of a fixed fixture is
  byte-identical before and after. Nothing selects an external module yet (discovery and the catalog are later phases).

## 0.3.0 - 2026-10-06

CodeRipper becomes three things at once: a library, a command that is also a cargo subcommand, and (planned, not in this
crate) a hosted service. This is the library's first release.

### Breaking
- **The data types are `#[non_exhaustive]`**: `Finding`, `Location`, `Severity`, `Confidence`, `FindingError` (and its variant),
  `CheckContext`, `Scope`, `Unit`, `Network`, `Tier`, `ApiError`, `CacheConfig`, `RunResult`, `WorkspaceRun`, `RepoRef`, `GhCli` and the
  four unit-struct checks, so a field or variant can be added later without breaking anyone. Build them with `Finding::new` +
  `.location()` / `.subject()` / `.positive_control()` / `.member()`, `Location::new`, `CheckContext::new`, `ApiError::new`,
  `RunResult::new`, `RepoRef::new`, `CacheConfig::new` + `.with_wait()` / `.with_max_bytes()`, and `ReachabilityCheck::new()` (and the
  other built-in checks), and match enums with a wildcard arm.
- **`Check` and `Github` are `Send + Sync`**, so checks run on a thread pool or in a service. A method added to either later will
  have a default body.
- **`CheckContext::new(project_root)`** takes the project alone; the portfolio root (which no built-in check reads) defaults to the
  project's parent and is set with `.portfolio_root(path)`. `Check::scope()` has a default (`Scope::Project`).
- **An unknown check id is an error**, no longer a clean empty run: the library returns it in `errors`, the command exits 2 with the
  list of checks. Two checks sharing an id are refused.
- **Exit codes.** `coderipper` now exits 0 (clean), 1 (a finding at or above `--deny`), 2 (usage error: bad flag, unknown check id,
  a project path that cannot be read) or 3 (the audit could not be completed: a check could not run, or the command failed). Before, a
  check that could not run and a bad project path both exited 1, and findings could not fail a run at all.
- The command-line plumbing in the library (`UnitFilter`; in `build_cache`: `config_from_env`, `status`, `prune_to_cap`, `take_stats`,
  `render_stats`, `MARKER`, `RepoDirInfo`, `BuildStats`) is no longer public (it was only ever for the binaries).

### Added
- **`run_checks_with`**: run your own `Check` (or any list of checks) through the host's validation and the project's allowlist.
  `run_checks` is now a wrapper over it.
- **`cargo coderipper`**: a second binary; a bare `cargo coderipper` is `fast`.
- **`--deny <info|low|medium|high|critical>`** on `fast`, `sweep` and `check`. Off by default (findings print but never fail the run, like
  clippy warnings); CI writes `--deny medium`.
- **`--message-format json`**: one JSON object per line with a `"reason"` (`coderipper-finding`, `coderipper-summary`).
- `--version`; `coderipper::anyhow` and `coderipper::serde_json` (re-exports of the types in the trait signatures); `Debug` / `Clone` /
  `PartialEq` / `Hash` where a library user expects them.
- A `cli` cargo feature (default on) gates `clap` and the two binaries: `default-features = false` is the library alone.
- Crate documentation, `#![warn(missing_docs)]`, three runnable examples (each run by a test), golden files for the `Finding` JSON and
  the `.coderipper.toml` format, and `rust-version = "1.89"` (where `File::try_lock`, used by the build cache's lock, was stabilised;
  1.88 fails to compile it, measured).
- CI jobs: docs (`-D warnings`), the library alone, MSRV, `cargo package`, and cargo-semver-checks on pull requests.

### Changed
- The absence-claim rule in `Finding::validate` matches whole words: "10 threads", "version 1.0" and "casino" are no longer rejected as
  claims of an absence.
- The `serve` subcommand, which only ever said "not implemented", is hidden from `--help`.
- The package no longer ships `docs/`, `.github/`, `codecov.yml` or `tests/`; the README, the package description and the command's
  `--help` no longer promise dependency checking or a server, which do not exist.

### Fixed
- Windows: the `\?\` prefix of a canonicalized path no longer appears in messages and JSON.

## 0.2.13 - 2026-10-04

### Changed
- Dependency `toml` 0.9 to 1.

## 0.2.12 - 2026-10-03

### Added
- **`--workspace`** on `fast`, `sweep` and `check`: runs the checks over every member of the cargo workspace containing the project (the root or any
  member's directory). Checks that judge the whole repository (`version-consistency`, `ci-protection-presence`) run once, at the workspace root;
  checks that judge a package run once per member, in sorted order, each exactly as `--project <member>` runs them. Findings carry a new optional
  `Finding.member` (the cargo package name) and lead with it. A member that fails does not stop the others (the run still exits non-zero). stderr shows
  `coderipper: member <name> (i/n)` per member and a one-line summary.
- **Cost to know about:** while a `--workspace` run is in progress it holds the build cache's lock for that repository, so another coderipper run on the
  same repository builds without the cache (correct, slower) and says so. Not measured yet on a large workspace.

## 0.2.11 - 2026-10-03

### Added
- The session checkout used by `--workspace` (no command used it yet in this version): one git checkout for a whole run, each member rewritten in place and
  restored with `git checkout`, never with an old mtime (which would let a dependent link against the rewritten build); a failed restore is retried, then
  stops the run; the session holds the cache lock for the run and builds in `<cache dir>/target`.

### Fixed
- Two tests that CI found over-strict: one matched a random temporary directory name (a flake that could fail any run, not caused by this version), and one
  assumed a git behaviour (a file rewritten with identical bytes gets a new mtime on restore) that holds on git 2.55 but not on CI's git versions.

## 0.2.10 - 2026-10-03

### Added
- `Check::unit()` (`Unit::Package` or `Unit::Repository`) says whether a check judges one cargo package or the whole repository
  (`version-consistency` and `ci-protection-presence` judge the repository), `UnitFilter` lets the host run one kind or the other, and
  `package::workspace_members` lists a workspace's members in a stable order. Groundwork for `--workspace`; no CLI change yet.

## 0.2.9 - 2026-10-03

### Fixed
- **A build could be served another CodeRipper run's compiled copy of the package it was analysing (affects 0.2.7 and 0.2.8
  only).** Each check makes its checkout and rewrites it *before* taking the build-cache lock, and cargo calls a unit fresh when
  no source file is newer than the cached artifact. A run that waited for the lock (or was slow to reach it) while another run
  built the same unit into the shared cache therefore reused that run's compile, rewrite and diagnostics, and reported that
  run's findings with no error. The merged code was **reproduced** to do this (a test with two checkouts of one repository and
  a shared cache: the second build reported the first build's unit as fresh, for two builds of the same kind and for a
  `--lib` build followed by an `--all-targets` build). It needs two separate `coderipper` processes overlapping on the same
  repository; one process running several checks in a row, and any run with `CODERIPPER_CACHE=off`, were not affected.
  Right after taking the lock a run now sets every file of its checkout to the current time, so cargo recompiles the package.
  If that is impossible the run builds without the cache, releases the lock, and says why.
  **If you ran two overlapping runs on one repository with 0.2.7 or 0.2.8, discard the later run's findings and re-run it alone.**

## 0.2.8 - 2026-10-03

### Changed
- Codecov's pull-request statuses are informational: coverage is a signal, not a gate (`codecov.yml`). `coverage.yml` says what the
  number covers.

## 0.2.7 - 2026-10-03

### Added
- **Shared dependency-build cache.** The checks that compile your package build in a persistent target directory per repository and
  toolchain, outside your project (`CODERIPPER_CACHE_DIR`, else `%LOCALAPPDATA%\coderipper\build`, `$XDG_CACHE_HOME/coderipper/build`
  or `~/.cache/coderipper/build`), guarded by CodeRipper's own lock with a bounded wait, with a size cap (`CODERIPPER_CACHE_MAX_GB`,
  20) and `coderipper cache status|prune`. Measured on `fuel-core`: 234 s cold, 90 s warm. `CODERIPPER_CACHE=off` disables it. Each run
  that built something ends with one stderr line (`coderipper: cache <dir> - N units fresh, M compiled`).
  *Known defect in this version: see 0.2.9.*

## 0.2.6 - 2026-10-02

### Added
- A separate, non-required `Coverage` workflow (cargo-llvm-cov, uploaded to Codecov).

## 0.2.5 - 2026-10-02

### Added
- **`ci-protection-presence`** (sweep tier, network): reports a repository whose default branch requires no status checks. Reads
  `enforcement_level` and the required contexts, and repository rulesets, never GitHub's `protected` flag (which is true for branches that
  enforce nothing).

## 0.2.4 - 2026-10-02

### Added
- **`version-consistency`**: the packages of a project share one version, except a package that tracks another project's (`[[tracks]]`).

## 0.2.3 - 2026-10-02

### Added
- A workspace member can be a project (`--project <member directory>`): the checkout holds the whole repository, only the member is
  rewritten and built, and findings are package-relative.

## 0.2.2 - 2026-10-02

### Changed
- `reachability` analyses packages with a library plus binaries: it builds only the library and does not report an item that a bin, test,
  example or bench reaches by name.

## 0.2.1 - 2026-10-02

### Added
- **`unused-parameters`** (Rust): function parameters rustc's `unused_variables` reports as unused.

## 0.2.0 - 2026-10-02

### Added
- Allowlist entries that no longer suppress anything are reported as `Info` findings (stale-this-run detection).

## 0.1.0 - 2026-10-01

### Added
- The host (the `Check` trait, the `Finding` schema, the allowlist, the CLI: `fast`, `sweep`, `check`) and the first checks:
  **`reachability`** (dead code, including `pub` items rustc skips) and **`unused-return-values`**.
