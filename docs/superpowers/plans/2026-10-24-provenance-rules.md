# Provenance and licence of committed data and code (family 2): design and measurement

> **Status: DESIGN with a measurement, for PM review. Nothing here is built.** The PM's task of 2026-10-24 (family 2 of the ROADMAP's "Next rule
> families"), carrying CireSnave's standing rule, quoted as `C:\Projects\CLAUDE.md` section 9 records it: *"I am not a plageurist."* — *trace every
> third-party file to its origin, verify our copy against it, and keep its licence, notices and credit.* The recorded incident to test against is fuel's
> Qwen-AWQ slice: a verbatim 8,728-byte slice of a GPL-3.0 checkpoint committed as `fuel-tensor/tests/fixtures/awq_qwen3_0_6b/slice.safetensors` into a
> repository licensed `MIT OR Apache-2.0`, with a provenance record that called the model licence Apache-2.0. It was removed from history on 2026-10-23.
>
> **Headline.** The incident has **exactly one detector: a check of the licence a record claims against the licence the source publishes**, which needs
> the network and belongs in the sweep tier. Every static rule below is silent on it, because the record existed, named the source and a pinned revision,
> and claimed a licence compatible with the repository's. The static rules are worth writing for a different reason: the portfolio's committed data is
> **small** (75 data files and fixtures in 32 repositories) and **mostly unrecorded** (42 of the 75 are not named by any provenance record, 24 of them example
> assets in fuel copied from candle; where a record exists it is good: 31 of the 33 files it names carry a content hash, and no third-party copyright holder
> in a file header was found uncredited, by a deliberately loose name match). The rules find the exceptions, and there are a lot of them.

## 1. The incident, replayed rule by rule

The record (read from the pre-rewrite commit `558864da`, which still exists in the fuel lane's clone) named `Orion-zhen/Qwen3-0.6B-AWQ` at revision
`9f607bee`, said *"model licence Apache-2.0, as for Qwen3"*, described the slice as a verbatim byte copy, and gave "first-MiB SHA-256 prefix `04529031`" of the
**source** file. Today the hub's metadata for that repository says `license: gpl-3.0` (checked with the Hugging Face connector on 2026-10-24); its base model,
`Qwen/Qwen3-0.6B`, says `apache-2.0`. The record had applied the base model's licence to a quantized checkpoint that carries its own.

| rule (section 2) | fires on the incident? | why |
|---|---|---|
| SUP-014 a data file with no record | **no** | the record existed, in the same directory |
| SUP-015 a record with no hash of the committed bytes | **yes**, but beside the point | the record carried a prefix of the *source* file, not a digest of the committed 8,728 bytes; a real defect (the copy cannot be verified), not the licensing one |
| SUP-016 a record that states no licence | no | it stated one, in prose |
| SUP-017 an upstream URL that names a branch | no | it named a pinned revision |
| SUP-018 a foreign-headed file whose holder is not credited | no | a safetensors file has no header |
| a static licence-compatibility matrix | **no** | the claim, Apache-2.0, is compatible with `MIT OR Apache-2.0`; the claim was wrong |
| **SUP-019 the claimed licence against the source's** (network) | **yes** | `apache-2.0` claimed, `gpl-3.0` published, for the exact repository the record names |

The later, corrected record in fuel (`fuel-transformers/tests/fixtures/qwen3_0_6b_awq_greedy.PROVENANCE.md`) says "the checkpoint is GPL-3.0 (its README
front matter ... say so) and its base model is Apache-2.0" and commits no weights. It passes SUP-019: the claim and the source agree.

## 2. The rules

| id (proposed) | rule | reads | tier |
|---|---|---|---|
| SUP-014 | **Committed data with no provenance record** | a tracked file with a data or binary extension (`safetensors`, `gguf`, `bin`, `npy`, `onnx`, `png`, `jpg`, `gif`, `ttf`, `wasm`, `pem`, `zip`, ...) of at least 1 KiB, or a text file of at least 100 KiB under a `fixture`/`data`/`corpus` directory, that **no provenance record names**: a record covers a file only if it names the file's basename or shares its stem (`x.PROVENANCE.md` for `x.json`); a record in the same directory that names other files covers nothing, and neither does the repository's own root `LICENSE`. A README that merely mentions the file (a usage example) does not state an origin, and is reported as the weaker class "named only in a README" | static |
| SUP-015 | **A record with no hash of the committed bytes** | a record that covers a data file and contains neither its SHA-256 nor its git blob id (the full value, or a prefix of at least 12 characters) | static |
| SUP-016 | **A record that states no licence** | a record with no SPDX id and none of the licence names | static |
| SUP-017 | **An upstream URL that names a branch** | `github.com/.../blob|tree|raw/(main|master|develop|HEAD)/...` or `raw.githubusercontent.com/.../main/...` in a header, a vendored file or a record, **next to a copying word** (copied, adapted, ported, derived, kernels from, regenerate) | static |
| SUP-018 | **A foreign-headed file whose holder is not credited** | a source file whose header names a copyright holder or SPDX id that is not the repository's own, with the holder named in no `NOTICE`, `THIRD_PARTY`, `LICENSE*` or README of the repository | static |
| SUP-019 | **A record's licence against the source's** | the licence a record claims for a source repository (an `owner/name` on the Hugging Face hub or a GitHub repository), compared with the licence the hub or the GitHub API publishes | network, sweep tier |

A **record** is any file named `PROVENANCE*`, `NOTICE*`, `ATTRIBUTION*`, `SOURCES*`, `CREDITS*`, `*.PIN` or `*.provenance`; the portfolio's practice is free-text
Markdown, plus `data/patent-tiers.PIN` here. The rules therefore read prose and tables, not a schema. **A schema would make SUP-015, SUP-016 and SUP-019
exact** (`source`, `revision`, `licence`, `sha256` as fields); proposing one is a decision for the PM, and the rules must work without it.

The existing `licence` rule (a licence file at the repository root, SPDX headers) and `SUP-011` (third-party code excluded from analysis) are neighbours,
not substitutes: neither looks at a data file or at what a record claims.

## 3. Not statically checkable

- **Whether a claimed licence is the real one.** That is SUP-019, over the network, and it is only as good as the hub's metadata (a model card can be wrong).
- **Whether a file is a derivative work**, and under what licence the derivative must be offered. The 8,728-byte slice is a verbatim copy; most real cases are not.
- **Whether a use is within the licence** (an attribution clause, a non-commercial clause, field-of-use limits). A matrix can say "GPL data in an MIT repository"; it cannot say what a project may do.
- **Whether an unlabelled image is the project's own.** SUP-014 asks for a record and cannot decide the answer (5 of the 42 findings below are the projects' own logos or output).
- **Code copied without a header**, or with the header stripped. That is similarity detection against an index of upstream code, a different tool.

## 4. Scoping and the default-on policy

- **Exclusions are configuration:** `[provenance] own = ["frontend/static/brand/"]` says "this directory is our own work" with a reason, which is the record for
  the project's own assets (and the answer to SUP-014's three own-logo findings); `data_extensions` adds or removes extensions.
- **A record may be one file for a directory** (as fuel and kiss-ref do for a fixture corpus) and a table row per file; the rules read both.
- **Severity.** SUP-014: Low (the file may be the project's own). SUP-015: Medium (the copy cannot be verified; the incident's record lacked it). SUP-016: Low.
  SUP-017: Info. SUP-018: High (a credit is owed). SUP-019: High (a stated licence is false). None blocks.
- **The network rule stays in the sweep tier**, as the policy says. It reads public metadata only, makes one request per distinct source, and reports a source it
  could not reach as a gap, never as a clean pass.
- **Default-on under the ROADMAP's gate** (at least 80% where a reviewer would act, on at least 30 findings, in at least three repositories): **none of the
  rules reaches the gate**. SUP-014 has 42 findings in 11 repositories (a census): **32 right, 5 unclear, 5 not, so 76% on firm verdicts, below 80%**, and 24 of them are
  in one repository. The other rules have 2, 4 and 42 raw findings. They can only qualify by the policy's second clause (exact on the syntax, nothing to
  sample): SUP-015 (2 findings, 2 right) and SUP-018 (no real finding left) qualify; SUP-014, SUP-016, SUP-017 do not.

## 5. Measurement

**Method.** Throwaway scripts (`provenance-prototype/*.txt`), run on `origin/main` of the 32 repositories under `C:\Projects` that have a git origin
(read through git plumbing, never a working tree): `scan.py` (data files and records), `m2.py` (hashes in records), `scan_urls.py` (branch URLs),
`scan_code.py` and `credit.py` (foreign headers and credit). Censuses are read whole; SUP-017 has 42 findings and 21 were judged in a seeded order
(`random.Random(2026)`), the stopping rule of the earlier samples reached (7 not right) at row 9. One reader judged; **every verdict has its reason
below**. **Control, run on the incident:** the same tests applied to fuel's pre-rewrite commit `558864da` give: SUP-014 silent (the record names the file), SUP-015 **fires** (neither the SHA-256 nor the git blob id of the 8,728 committed bytes is in the record), SUP-016 silent (it names Apache-2.0), SUP-017 silent (a pinned 40-hex revision, no branch URL). That is the section 1 table, measured and not assumed.

**Results.**

| rule | findings in the portfolio | judged | right | reading |
|---|---|---|---|---|
| SUP-014 | 42 (of 75 data files and fixtures) | 42 (census) | 32 yes, 5 unclear, 5 no (76% firm) | 14 named by nothing and 28 only by a README usage mention; fuel 24, Humboldt 6, unisql 3, auth-framework 2, and one each in 6 more; the 5 not are the projects' own logos, own generated output and a test key; the 5 unclear are unattributed logos and clip art: the case for `[provenance] own` |
| SUP-015 | 2 (of 33 files that a record names) | 2 (census) | 2 | 31 of the 33 carry a SHA-256, a prefix of one, or a git blob id; the 2 are fuel's two derived-weight fixtures (the WebAuthn fixture's record gives a 16-hex SHA-256 prefix, which the rule accepts; `qwen3_0_6b_cpu_reference.json`, which an earlier draft put here, has no record at all and is a SUP-014 finding) |
| SUP-016 | 4 of 7 records state no licence | 4 (census) | 1 | 2 are the owner's own KISS corpus and 1 is a NOTICE whose `.PIN` states it |
| SUP-017 | 42 in 9 repositories | 21 | 8 (38%) | READMEs, spec links and self-links are not provenance; every right one is a code comment, a vendored file or a NOTICE saying where it was copied from |
| SUP-018 | 175 raw header hits, **0 shown uncredited** | 29 holder groups | 0 | every real third-party holder (Meta, Apple, ggml, Tri Dao, FlashInfer, ...) has a word of its name in a notice-like file of its repository (a generous match: a short word such as `van` would pass); 7 of the 29 groups are not names at all but prose that mentions "copyright", and the project's own organisation names are the rest |
| SUP-019 | 1 (the incident) | 1 | 1 | the portfolio's current records agree with the hub (checked: the two Hugging Face repositories fuel's records cite) |

**What the numbers say.** (1) The portfolio does this for its code and its records, **and not for its data files**: 42 of 75 are unrecorded, and the largest group is 24 example assets in fuel copied from candle (images, two videos, a voice file, a font). (2) SUP-017's raw form is mostly links, and its
useful form is narrow: only a URL beside a copying word, in a code comment, vendored file or NOTICE. The 8 right ones all satisfy that; the narrowed rule is a
**hypothesis**, read off this sample, and needs its own. (3) SUP-018 needs a better holder extractor before it is a rule: 7 of the 29 holder groups were not names
at all, and the credit match is too generous to call "credited" a finding. (4) The one finding that matters most, the licence claim, is the one nothing static can make.

**Findings for the owning lanes** (none is a vulnerability), 42 files; the routed rows are the PM's:

- **fuel (24):** 21 example assets under `fuel-examples/examples/` (images, `test_video.mp4`, `hello.mp4`, `aoc.gif`, `voices.safetensors`, four bike photos), `roboto-mono-stripped.ttf` (a font whose licence must be kept), and `fuel-transformers/tests/fixtures/qwen3_0_6b_cpu_reference.json` (no record; the awq record beside it names a different file). Their README links to candle's examples: the origin is implied and no file states it or its licence. Plus the two bnb fixtures whose record carries no digest.
- **HumboldtUnifiedKidTracker (6):** the Granville Elementary School logo and five icons derived from it; permission to use a school's mark is unrecorded.
- **vulkane:** `vulkan.jpeg`, on inspection a painted Vulcan at an anvil in an image-generator style, **not** the Khronos logo an earlier draft guessed. **baracuda:** `assets/barracuda.png`, a photograph-grade underwater image. Neither has a source or licence recorded.
- **unisql (3):** `assets/Unicycle-*.png`, vector clip-art style, origin unstated. **amt:** `logo.png`, a rendered gear and lightning bolt. **candlelight:** `candlelight.gif`, an animated flame. Origins unstated.
- **Own work, a one-line record or `[provenance] own`:** auth-framework `auth-framework.png`, synapse `Synapse.jpeg`, KISS `conformance/corpus/ops-int-wrap.json`, lightbulb's golden JSON (self-describing: its own output, with a provenance block inside), and auth-framework's `test_private_key.pem` (its README says it was generated with `openssl genrsa`).

## 6. Plan

| order | rule | ships | needs first |
|---|---|---|---|
| 1 | SUP-015 record without a content hash | default-on (exact; 2 of 2) | read a record's table rows and prose for a SHA-256 or blob id; fixture pair |
| 2 | SUP-018 holder not credited | default-on once the extractor is fixed | an extractor that reads `Copyright (c) YEAR Holder` and SPDX only; 29 groups to re-check |
| 3 | SUP-014 data file that no record names | opt-in, then default-on if a fresh 30 outside fuel clears 80% | `[provenance] own`; the extension list; a fixture pair; the README-mention class at Info |
| 4 | SUP-019 licence against the source | opt-in, sweep tier | a Hugging Face and a GitHub lookup with a cache; the gap behaviour; a seeded fixture on a stub forge |
| 5 | SUP-016, SUP-017 | opt-in | the narrowed SUP-017 sampled on a fresh 30; SUP-016 only with the record schema |

**Open points for the PM.** (1) A record schema (`source`, `revision`, `licence`, `sha256`)? It makes three rules exact and changes nothing for existing records.
(2) Should SUP-019 also check a *dependency's* published licence against `cargo-deny`'s policy? That is SUP-003, a different tool. (3) The two lanes with
unrecorded logos and the one test key: route these as small tasks?

## 7. The verdicts, one row each

`right` = the detector is right; `acts` = a reviewer would change the repository (add a record, a digest, or a pinned reference).

#### SUP-014: committed data that no provenance record names (42 findings, a census; 32 right, 5 unclear, 5 not)

Rows are grouped by directory (the files in a group have the same verdict and the same reason). `class`: **none** = named by nothing; **README** = named only by a README that uses the file.

| repository, directory | files | class | right | acts | why |
|---|---|---|---|---|---|
| fuel `fuel-examples/examples/csm` | 1 (`voices.safetensors`) | README | yes | yes | voice prompts from the CSM example; no source or licence |
| fuel `fuel-examples/examples/flux/assets` | 1 | README | yes | yes | an image from the flux example; no source or licence |
| fuel `fuel-examples/examples/paddleocr-vl` | 6 (5 PNG, `test_video.mp4`) | README | yes | yes | OCR test images and a video; no source or licence |
| fuel `fuel-examples/examples/parler-tts` | 1 (`hello.mp4`) | none | yes | yes | a video; no source or licence |
| fuel `fuel-examples/examples/quantized/assets` | 1 (`aoc.gif`) | README | yes | yes | an animation; no source or licence |
| fuel `fuel-examples/examples/segment-anything/assets` | 3 | README | yes | yes | images; no source or licence |
| fuel `fuel-examples/examples/stable-diffusion/assets`, `stable-diffusion-3/assets` | 2 | README | yes | yes | generated or sample images; no source or licence |
| fuel `fuel-examples/examples/trocr/assets` | 2 | README | yes | yes | images; no source or licence |
| fuel `fuel-examples/examples/wuerstchen/assets` | 1 (`cat.jpg`) | README | yes | yes | a photograph; no source or licence |
| fuel `fuel-examples/examples/yolo-v8/assets` | 4 (bike photographs) | README | yes | yes | photographs; no source or licence |
| fuel `fuel-examples/examples/yolo-v8` | 1 (`roboto-mono-stripped.ttf`) | none | yes | yes | a font: its licence and notice must be kept |
| fuel `fuel-transformers/tests/fixtures` | 1 (`qwen3_0_6b_cpu_reference.json`) | none | yes | yes | reference output of a third-party model; the record beside it names a different file |
| HumboldtUnifiedKidTracker `frontend/static/branding` | 1 (`granville-logo.png`) | README | yes | yes | a school's logo: permission is a licence question |
| HumboldtUnifiedKidTracker `frontend/static/branding` | 5 (icons) | none | yes | yes | icons derived from the school's logo |
| vulkane `.` | 1 (`vulkan.jpeg`) | none | yes | yes | a painted Vulcan in an image-generator style; no source or licence |
| baracuda `assets` | 1 (`barracuda.png`) | README | yes | yes | a photograph-grade image; no source or licence |
| amt `.` | 1 (`logo.png`) | none | unclear | unclear | a rendered gear and lightning bolt; whose it is is not stated |
| candlelight `.` | 1 (`candlelight.gif`) | README | unclear | unclear | an animated flame; origin not stated |
| unisql `assets` | 3 (`Unicycle-*.png`) | none | unclear | unclear | vector clip-art style; origin not stated |
| KISS `conformance/corpus` | 1 (`ops-int-wrap.json`) | none | no | no | conformance vectors the project generates itself |
| auth-framework `.` | 1 (`auth-framework.png`) | README | no | no | the project's own logo |
| auth-framework `tests/fixtures` | 1 (`test_private_key.pem`) | README | no | no | its README says it was generated with `openssl genrsa` for tests |
| synapse `.` | 1 (`Synapse.jpeg`) | README | no | no | the project's own logo |
| lightbulb `tests/fixtures/model_fuel_golden/v1` | 1 (`tinyllama_f32_greedy.json`) | none | no | no | the project's own past output, with a provenance block inside the file |

#### SUP-017: an upstream URL that names a branch (42 findings; 21 judged, 8 right)

| # | site | right | acts | why |
|---|---|---|---|---|
| 1 | fuel `fuel-examples/examples/whisper/main.rs:2` | yes | yes | a header of code ported from OpenAI's whisper, pointing at `main` |
| 2 | fuel `fuel-examples/examples/stable-diffusion-3/README.md:50` | no | no | a README link |
| 3 | vulkane `vulkan_gen/README.md:7` | no | no | a README link to the spec file the generator reads |
| 4 | fuel `fuel-examples/examples/whisper/README.md:11` | no | no | a README link to sample audio |
| 5 | lightbulb `src/gguf/parser.rs:4` | no | no | a specification link in a doc comment |
| 6 | baracuda `crates/baracuda-kernels-sys/vendor/marlin/src/marlin_cuda_kernel.cu:130` | yes | yes | a vendored kernel citing the strategy it follows, at `main` |
| 7 | auth-framework `docs/DEPLOYMENT_GUIDE.md:530` | no | no | a link to the project's own docs |
| 8 | fuel `fuel-onnx/src/lib.rs:40` | no | no | a link to the project's own crate |
| 9 | auth-framework `sdks/python/pyproject.toml:58` | no | no | a link to the project's own SDK |
| 10 | KISS `spec/namespaces/vulkan.md:18` | no | no | a link to a reference implementation |
| 11 | unisql `measure/licences/jooq-NOTICE.txt:4` | yes | yes | a NOTICE that names its source by the `master` URL of a notice file |
| 12 | fuel `fuel-cpu-kernels/src/philox_kat.rs:12` | yes | yes | test vectors derived from random123, regenerated by a `curl` from `main` |
| 13 | fuel `fuel-examples/examples/moondream/README.md:8` | no | no | an install instruction |
| 14 | fuel `fuel-formats/src/gguf.rs:5` | no | no | a specification link |
| 15 | fuel `fuel-examples/examples/beit/main.rs:3` | yes | yes | a header of code ported from microsoft/unilm, pointing at `master` |
| 16 | fuel `README.md:26` | no | no | a link to the project's own crate |
| 17 | baracuda `crates/baracuda-kernels-sys/vendor/flash-attention/src/utils.h:285` | yes | yes | a vendored comment citing a line of cutlass on `master` (line numbers drift) |
| 18 | auth-framework `src/protocols/spiffe.rs:15` | no | no | a specification link |
| 19 | baracuda `crates/baracuda-kernels-sys/vendor/fuel-q8_1/quantized.cu:3389` | yes | yes | copied kernels attributed to a `master` URL |
| 20 | lightbulb `docs/M3_4_FLASHATTENTION_INTEGRATION.md:232` | no | no | a documentation link |
| 21 | baracuda `crates/baracuda-kernels-sys/vendor/flash-attention/src/static_switch.h:2` | yes | yes | a vendored header 'inspired by' a `main` URL |
