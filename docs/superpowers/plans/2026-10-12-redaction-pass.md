# Redaction pass for echoed third-party output — Design

> **Status: built in two steps (0.6.10: `redact::text`; 0.6.11: the policy, the sinks, `--redact`).** The text below is the
> design as written on 2026-10-12; where the build differs, the CHANGELOG entry for 0.6.11 says how. Requested by the PM on 2026-10-12 as the precondition named in the 0.6.2 audit (D5) for the
> hosted service (`2026-10-04-three-faces.md` §3). Facts below are read from `main` at `3f649d1` (0.6.3); each names the file so it can be
> re-checked.

## 1. The problem

CodeRipper runs other people's build scripts and tools and then **prints some of what they wrote**. On a developer's machine that is the
developer's own output. In a hosted service the reader is a third party and the writer is an attacker or an unlucky project, so any
secret a build script echoed (`KEY=value`, a registry URL with a password in it) would be published with our name on it.

## 2. Where third-party text reaches output today

| # | Surface | Where | What is done to it now | Secret redaction |
|---|---------|-------|------------------------|------------------|
| 1 | `Cause:` in the "build reported real compiler error(s)" message (`reachability`, `unused-parameters`, `unused-return-values`) | `src/cargo_json.rs` `failure_cause`, `cargo_cause`, `one_line` | ANSI removed, control and bidi characters replaced, whitespace collapsed, cut at 400 characters | **none** |
| 2 | A compiler diagnostic's message, place and first note (the first branch of `failure_cause`) | same | same | **none** |
| 3 | An external module's failure: `...; stderr: <tail>` (up to 4096 bytes of stderr) | `src/module/external.rs` (`with_stderr`, `STDERR_TAIL_BYTES`) | trimmed only; **this design has not traced whether a later stage cleans it** (`clean_note` in `src/module/mod.rs` is applied to notes) | **none** |
| 4 | A delegated tool's stderr tail, kept for the tool's log and used in error details | `src/module/delegate/mod.rs` (`stderr`, the `Err((kind, detail))` path) | not traced | **none** |
| 5 | Notes from a module | `src/module/mod.rs` `clean_note` | control and bidi replaced, cut at 500 characters, at most 20 per rule | **none** |
| 6 | Finding text quoting source (a dependency name, a workflow line, a link) | each check | per check | **none**, and it is the project's own file, not build output |

Rows 1–5 are the ones a *build or tool* can fill with anything. Surface 6 is the submitter's own repository content: it is not a secret
we can know about, but it is still untrusted text on a public page (escape on render; the hosted plan already says so).

**Measured, not guessed:** the 0.6.2 dogfood case, a `-sys` build script that panicked, produced a cause containing the script's own
words ("Could not find directory of OpenSSL installation"). Nothing in that run echoed a secret; the exposure is what *could* be
echoed (build scripts commonly print `cargo:warning=` lines and environment-derived paths and values). That is the reason for a pass
before the hosted face exists, not a reported incident.

## 3. Principle: emit less first, redact second

A pattern list never covers every secret (a base64 string with no key beside it is invisible). So:

1. **Primary control: in hosted mode, say less.** A build-script failure becomes a fixed sentence built from facts *we* know:
   `build script of <package> <version> failed (exit status N); its output is withheld`. The package name and version come from cargo's
   own `error: failed to run custom build command for ...` line, validated against `[A-Za-z0-9_-]+` and a version grammar; anything that
   does not match is dropped, not echoed. The same for an external module's stderr: the exit status and the fixed sentence, never the
   tail. A compiler error keeps its code and place (`error[E0425]: ... (src/main.rs:2)`): rustc's message is generated from the
   submitter's source, so the redaction pass is applied to it too (row 2), but its code and place are not.
2. **Secondary control: a redactor on every remaining string.** Defence in depth for what the fixed sentences do not cover.
3. **Local runs are unchanged by default.** A developer needs the full cause. The policy is opt-in locally (`--redact`) and always on in
   the hosted service.

## 4. What the redactor removes

Applied to the text *after* ANSI/control/bidi cleaning (so an escape sequence cannot split a pattern) and *before* the length cut (so a
cut cannot leave half a secret):

- **URL userinfo:** `scheme://user:password@host` and `scheme://token@host` → `scheme://<redacted>@host`.
- **`KEY=value` and `KEY: value`** where the key, case-folded, contains `token`, `secret`, `password`, `passwd`, `pwd`, `apikey`,
  `api_key`, `auth`, `credential`, `private_key`, `session`, `cookie` → value replaced. Value runs to the next whitespace, or the closing
  quote if quoted.
- **Header shapes:** `Authorization: ...`, `Bearer <...>`, `Basic <...>`.
- **Well-known token shapes**, a short list kept in one table with a source comment each (GitHub `gh[pousr]_`, AWS `AKIA`/`ASIA`, npm,
  crates.io, Slack `xox`, private-key `-----BEGIN ... PRIVATE KEY-----` blocks). The list is deliberately short; it is not a scanner,
  and the real scanner (gitleaks, already delegated, SEC rules) is where completeness lives.
- **High-entropy long tokens** (≥ 32 characters, base64/hex alphabet, mixed case or digits) only when adjacent to a key from the list
  above; never on their own (a hash in a lockfile is not a secret and the false positives would hide real errors).

Replacement is the fixed text `<redacted>`; the redactor returns the **number of replacements**, and the run says
`N value(s) were redacted from third-party output` as a note. A cause that was redacted must not be the only evidence for an
"all clear": redaction can only make the text shorter, never turn a failure into a pass.

## 5. Where it is applied: one sink, not six

Applying it per call site is how a surface gets missed (the enumeration in §2 was needed to find them). Two layers:

1. **One function `redact::text(&str) -> (String, usize)`** in a new module, called from `one_line` (`cargo_json.rs`), `clean_note`
   (`module/mod.rs`), and the place that builds external/delegate failure details (rows 3–4).
2. **A final pass at the serialisation boundary**: the JSON/text writers apply the policy to every string in `errors`, `notes` and
   each finding's `detail` when the policy is on. A surface added later is covered by this layer even if its author forgot layer 1.
   A test builds a `RunResult` containing a fake secret in each field and asserts none survives serialisation.

Library surface (additive; a new variant or field goes last): `RedactionPolicy { Off, On }` carried in the run options, default
`Off`; the CLI flag `--redact`; the hosted service sets `On`. The `build_cache::CacheConfig` precedent (an embedder controls it) is the
model for how the embedder sets it.

## 6. Tests first (the order of the PRs)

- Fixture secrets are **assembled at runtime** (`concat!`/`format!`), never written as literals: CodeRipper's own CI runs gitleaks and
  zizmor over this repository, and a literal fake token trips them.
- **Positive control:** each pattern has one string that is redacted and one lookalike that is not (a lockfile hash, `author=name`).
- **Cut-boundary test:** a secret that straddles the 400/500-character cut is not half-printed.
- **Escape-splitting test:** `ghp_` + ANSI escape + remainder is still redacted.
- **Real tool test:** a build script that prints `echo "cargo:warning=TOKEN=<fake>"` and then fails, run through the actual
  `reachability` build path, and the same with an external module that writes a fake URL with userinfo to stderr.
- **Mutation checks:** remove one pattern, confirm the mutation applied (a no-op reads as "caught nothing"), confirm a named test fails.
- **Not-silent test:** redaction never changes an exit code and always leaves the failure's existence in the output.

## 7. Limits, stated up front

- It cannot find a secret with no recognisable key or shape. The claim is "known shapes and keys are removed", never "output is clean".
- It does not remove **paths** that contain a user name (`C:\Users\<name>\...`). The hosted service runs in a scratch directory it
  controls, so this is a separate, smaller question: strip the scratch prefix, not the redactor's job.
- It does not make **findings text** safe to render: that is escaping on render, owned by the hosted front end.
- It does not decide *whether* to run strangers' build scripts at all (the hosted plan's option B). This pass is necessary for B and
  useful for A (uploaded JSON is also third-party text, and the same redactor can be applied on receipt).

## 8. Decisions for the PM

1. **Default:** off locally, on in hosted (recommended), or on everywhere (a BREAKING change: the `Cause:` text a developer relies on
   would change; Q14-style ruling needed)?
2. **Withhold vs redact for build-script output in hosted mode:** the design withholds (fixed sentence). Is losing the "could not find
   OpenSSL" hint acceptable there? (Local runs keep it.)
3. **Version and sequencing:** one PR for the redactor and its tests (additive, no behaviour change: policy `Off`), a second wiring it
   into the sinks and adding `--redact`. Both are additive (a patch-level bump under the project's rule); the PM allocates the numbers.
