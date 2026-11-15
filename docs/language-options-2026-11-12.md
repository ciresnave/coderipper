# Language options: TypeScript, JavaScript, C, C++, CUDA, Slang (for CireSnave)

**Written 2026-11-12 for the PM task "languages and Rust completeness"; read at `origin/main` 9da0f91 (0.43.59).** Docs only. The order of build is the PM's to rule
after reading this; the lane's proposal is in `ROADMAP.md` items 11 to 17. Numbers say whether they were **measured** (and how) or are **unmeasured** guesses.

CireSnave, 2026-11-12: "We commonly do TypeScript, JavaScript, C, C++, Slang, and CUDA builds. Are those all on CodeRipper's roadmap to add support for?"
**Answer today:** TypeScript yes (item 8; Python next to it); JavaScript only as a side effect of TypeScript; **C, C++, CUDA and Slang were not on it**, and are now rows 13 to 16.

## 1. Where each language stands (measured, `docs/rules/COVERAGE-CENSUS-2026-11-12.md`)

| Language | Language-aware checks | What a run says today |
| :--- | ---: | :--- |
| Rust | 102 of 230 applicable rules | a coverage line; 128 rules "not yet implemented" |
| TypeScript | 0 (32 of 221 via the language-neutral rules) | "no module for typescript, only the language-neutral rules" |
| JavaScript | 0 | the same line, as `js`, `jsx`, `mjs` and `cjs` count as TypeScript source (`src/coverage.rs`) |
| C, C++, CUDA, Slang, HLSL, GLSL | 0 (31 of 183 language-neutral rules run on any repository) | **nothing**: the unchecked-language note knows only python and typescript, so these are not even named |

## 2. How each language would be parsed

CodeRipper reads Rust with `syn` (pure Rust). Every other language needs a parser that exists on crates.io as a **version** (portfolio rule: no path dependencies,
no new `git =` dependencies). All figures below were read from the crates.io API on 2026-11-12.

| Language | Parser (crate) | Version, released | Licence | Downloads | Notes |
| :--- | :--- | :--- | :--- | ---: | :--- |
| (all tree-sitter languages) | `tree-sitter` (the runtime) | 0.27.2, 2026-11-07 | MIT | 55.3 M | one runtime and one query API for every grammar below; each grammar crate depends only on `tree-sitter-language` and its generated C parser is compiled by `cc` at build time |
| C | `tree-sitter-c` | 0.24.2, 2026-04-22 | MIT | 13.5 M | maintained under the `tree-sitter` organisation |
| C++ | `tree-sitter-cpp` | 0.23.4, 2024-11-11 | MIT | 13.9 M | same organisation; no release in a year |
| CUDA | `tree-sitter-cuda` | 0.21.2, 2026-09-15 | MIT | 142 k | `tree-sitter-grammars` organisation |
| Slang | `tree-sitter-slang` | 0.3.1, 2025-07-29 | MIT | 19.6 k | one maintainer (the same as the HLSL and GLSL grammars) |
| HLSL | `tree-sitter-hlsl` | 0.2.0, 2025-03-23 | MIT | 85.8 k | same maintainer |
| GLSL | `tree-sitter-glsl` | 0.2.0, 2025-03-16 | MIT | 182.9 k | same maintainer |
| TypeScript | `tree-sitter-typescript` **or** `oxc_parser` | 0.23.2, 2024-11-11 **or** 0.158.0, 2026-11-09 | MIT, MIT | 19.9 M, 11.1 M | `oxc_parser` is pure Rust (needs rustc 1.97; this crate is at 1.89, and CireSnave permitted raising it, 2026-10-08), a typed AST that also parses JavaScript and JSX; `swc_ecma_parser` 47.0.1 (Apache-2.0, 43.6 M) is the other pure-Rust option |
| JavaScript | `tree-sitter-javascript` **or** `oxc_parser` (the same crate as TypeScript) | 0.25.0, 2025-09-01 | MIT | 17.5 M | |
| Python (for comparison, not this task) | `ruff_python_parser` or `tree-sitter-python` | 0.0.22, 2026-11-12 (pre-1.0 line); 0.25.0, 2025-09-11 | MIT, MIT | 1.5 M; 20.6 M | |

**Provenance and licence (portfolio rule 6.4a).** Every parser above is MIT (swc: Apache-2.0), each a crates.io release of an upstream repository named in its
manifest (`github.com/tree-sitter/...`, `tree-sitter-grammars/tree-sitter-cuda`, `theHamsta/tree-sitter-{slang,hlsl,glsl}`, `oxc-project/oxc`). A grammar crate ships its generated
`parser.c`; we would **depend on the published crate, not copy it**, so there is nothing to verify "against an original" beyond the crate's own checksum (Cargo.lock) and
`cargo deny`'s licence and advisory gates, which already run here. Not checked in this note: the licences of every transitive crate of `oxc_parser` (`cargo deny check licenses` after adding it is the check), and the notices each grammar repository carries for text it derived from.

**One cost every tree-sitter grammar adds:** a C compiler at build time (`cc`). The note's experiment compiled all eight grammars with the MSVC toolchain on this machine; CodeRipper
today installs with `cargo install` needing only Rust, so `cargo install coderipper` would newly need a C toolchain on a user's machine. `oxc_parser` has no such cost.

### Measured: do the grammars parse each other's code? (a scratch crate, not committed)

Eight hand-written samples of about three lines each (one per language) were parsed with each of the eight grammars (`tree-sitter` 0.27.2); a cell is `ok` when the
tree has no error node. **A sample this small shows what fails, not what works at scale; it is not a corpus run.**

| sample \ grammar | c | cpp | cuda | slang | hlsl | glsl | typescript | javascript |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| C | ok | ok | ok | ok | ok | ok | 6 errors | 6 errors |
| C++ (templates, lambda, trailing return) | 10 errors | ok | ok | ok | ok | 9 errors | 16 errors | 17 errors |
| CUDA (`__global__`, `<<<...>>>`, `__shared__`) | 4 errors | 3 errors | ok | 1 error | 3 errors | 4 errors | 11 errors | 9 errors |
| Slang (interface, generics, `ParameterBlock`) | 4 | 4 | 4 | **1 error** | 3 | 4 | 10 | 10 |
| HLSL (`cbuffer`, `register`, `mul`) | 4 | 4 | 4 | 1 | **1 error** | 4 | 7 | 7 |
| GLSL | 2 | 2 | 2 | 1 | 1 | ok | 4 | 4 |
| TypeScript (generics, enum, `as`) | 9 | 7 | 7 | 8 | 8 | 9 | ok | 8 errors |
| JavaScript (`?.`, `??`, private field, class expression) | 6 | 7 | 8 | 8 | 8 | 6 | ok | ok |

What it says: **the CUDA grammar parsed the C, C++ and CUDA samples without an error, so C, C++ and CUDA can share one family (and one `tree-sitter-cuda` dependency is
a candidate for all three; the C++ grammar remains the faithful one for C++)**; **the TypeScript grammar parsed the JavaScript sample**, so TypeScript and JavaScript are one family; the GLSL
grammar parsed its sample cleanly, but **my Slang and HLSL samples each left one error in their own grammar** (not investigated: it may be the sample or the grammar,
and it is the first thing a Slang slice must settle); C code parses under every C-family grammar, but C++ does not parse as C. Nothing here measures speed, memory, or behaviour on real
repositories.

## 3. What the rules need from each language

A rule needs one of four things. The first three are available from a syntax tree; the fourth is not.

1. **Files and text** (licences, CODEOWNERS, CI files, comments by regex): language-neutral, already running (31 rules).
2. **A syntax tree** (a function with too many parameters, a forbidden call by name, an empty test, a `catch` that swallows): the tree-sitter or oxc trees give it. Roughly
   the "source only" rows of the census (41 of the 105 mechanical Rust rules have this shape for Rust; the same rules recur per language with a different node vocabulary).
3. **Git history** (EVO-*, change-coupling): language-neutral; needs only file paths.
4. **Types and name resolution** (TYP-*, API-*, "a public signature names a third-party type", "unused import"): a syntax tree cannot give it. Rust gets it from rustc's own diagnostics
   and `cargo metadata`. TypeScript can get it from `tsc` (a delegated tool; Node on the runner) or from `oxc_semantic` (in-process, scope and symbol resolution, not types; its crate page was not read for this note);
   C and C++ from `clang-tidy` or `clang` (`clang-sys` 1.9.1, Apache-2.0, needs libclang installed); CUDA from `clang` with `--cuda-gpu-arch` (the compile is the expensive step and
   needs the CUDA headers; running a kernel needs a GPU); Slang from `slangc` (a delegated tool: the compiler is the reference for what is valid Slang). Delegation is the
   portfolio's established route for rules a tool already does well (CireSnave 2026-10-25: "support any good tool whatever language it is written in").

## 4. First rules per language

Every language starts with the language-neutral 31 (they need no parser). Then, by the tree alone:

| Language | First rules (tree only) | First delegated tools |
| :--- | :--- | :--- |
| TypeScript, JavaScript | TYP-006 (non-null assertions `!` and `any`), ERR-* (empty `catch`, floating promises by syntax), RDB-004 (nesting, parameters), TST-003 (a `test(...)` with no `expect`), SEC-004 (`eval`, `innerHTML`), CON-005 (unawaited promise) | `eslint`, `tsc --noEmit` (needs Node on the runner: unmeasured cost), `npm audit` / osv-scanner (SUP-002 already runs osv on lockfiles) |
| C | SEC-* (`gets`, `strcpy`, `sprintf`: banned-function list), TYP-006 analogue (unchecked `malloc`), RDB-004, ERR-* (ignored return values of listed functions) | `clang-tidy`, `cppcheck` (GPL-3.0 by its repository's licence, not re-read here; run as a separate process: a licence question for the PM, nothing is linked) |
| C++ | the C rules, plus raw `new`/`delete`, `catch (...)`, C-style casts, `using namespace` in headers | `clang-tidy` |
| CUDA | the C++ rules, plus an unchecked `cuda*` return (the same shape as `unused-return-values`), a `__global__` kernel with no bounds guard on `threadIdx`/`blockIdx` indexing, `cudaMemcpy` size by `sizeof` of a pointer | `compute-sanitizer` (runs the kernel: needs a GPU, self-hosted runner) |
| Slang, HLSL, GLSL | PRF-* syntax (a loop with a non-constant bound inside a pixel shader, dynamic indexing), RDB-004, unused `cbuffer` fields by name | `slangc` (compile errors as findings), `glslangValidator`, `dxc` |

Which of these are right is an open design question, not a measurement; the lane would pick them from the catalog's `any` rules read against a real C/CUDA/Slang repository
of CireSnave's choosing, which this note did not do (**unmeasured**: no such repository was read).

## 5. Lane-day estimates (all unmeasured)

| Work | Estimate | Basis |
| :--- | ---: | :--- |
| Slang/HLSL/GLSL, one slice (parse, find the first 3 rules, a conformance fixture each) | 3 to 5 days | the lane's experience of one rule plus fixtures; **unmeasured for this language**. The experiment above (a parser matrix) took one session. |
| TypeScript + JavaScript module (P4 plan: 9 to 22 days); JavaScript adds 3 to 6 | 12 to 28 | the programme plan's figure; **unmeasured** |
| C module | 8 to 18 | by analogy with the TypeScript plan; **unmeasured** |
| C++ on top of C | 6 to 14 | **unmeasured** |
| CUDA on top of C++ | 5 to 12 | **unmeasured** |
| Slang + HLSL + GLSL | 6 to 14 | **unmeasured**; the grammars are the youngest dependencies in the plan |
| Rust mechanical rules (105) | 40 to 90 | sizes from the 2026-11-07 per-rule guess (10 S, 41 M, 54 L); **unmeasured** |

To turn a figure into a measurement, build one slice per family and record the days; the lane will do that if the PM orders it.

## 6. Questions for the PM or CireSnave

1. **Parser family for TypeScript and JavaScript:** `oxc_parser` (pure Rust, typed, raises the minimum Rust to 1.97) or `tree-sitter-typescript` (a C build dependency, shared with every other language)? The lane leans to **tree-sitter for all of them** so one query layer serves every language, and to `oxc` only if a rule needs the typed AST.
2. **Is a C compiler on the user's machine acceptable** for `cargo install coderipper`? If not, only `oxc` and delegated tools are possible.
3. **Which real repository stands for each of C, C++, CUDA, Slang** so the first rules are read against code CireSnave actually writes?
4. **`cppcheck` (GPL-3.0, run as a separate process, never linked)**: allowed as a pinned delegated tool, as `gitleaks` and `zizmor` are?

— lane: coderipper
