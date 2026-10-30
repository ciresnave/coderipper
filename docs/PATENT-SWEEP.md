# Patent sweep: coderipper 0.9 @ 0ad0026

Date: 2026-10-25; Run by: the coderipper lane, Claude Sonnet 5.5; Method: AI-assisted search and read; **NOT legal advice; NOT a freedom-to-operate opinion.**
Version lines covered: **0.9** (swept at 0.9.46, commit `0ad0026` on `main`) and **0.10**, **0.11** and **0.12** (addenda below). Process: `C:\Projects\PATENT-SWEEP-PROCESS.md`. This is CodeRipper's own sweep of its own ideas; it is not `patent-index-match` (which compares a project's dependencies with an index) and does not replace it.

## Ideas swept

The mechanisms, in plain claim-like sentences (what the code does that might be new or unusual, read from the source at this commit):

1. **Reachability by compiler diagnostics.** For a Rust crate, build a throwaway checkout whose public items are rewritten so the compiler's own dead-code analysis can see which have no caller, read the compiler's diagnostics, and report public items with no caller inside the crate; an item with a declared external consumer is waived by name.
2. **The same route for discarded return values and unused parameters:** rewrite the analysed package so a normally silent lint is forced to warn, build, and read the JSON diagnostics.
3. **A persistent build cache for the builds the checks run:** one target directory per toolchain and per source repository, shared by throwaway checkouts at different paths, guarded by an advisory lock taken before the build tool's own lock with a bounded wait and a holder-pid sidecar file, and pruned least-recently-used under a size cap, never deleting from a directory that lacks a marker file.
4. **A session checkout for a multi-member workspace run:** one checkout kept for the run; a member's source is rewritten in place and restored afterwards with a new modification time ("never restore an old mtime"), so the build tool recompiles it from the original bytes while untouched siblings stay fresh.
5. **A delegated-tool framework:** a tool pinned by name, version and platform in an embedded lock with two hashes (archive and extracted member); installed only with the user's consent; the archive read in memory and refused whole if any entry could escape the directory; run as a child process with a scrubbed environment, an empty scratch directory, limits and a process-tree kill; its machine-readable output mapped to findings; a missing tool is a reported coverage gap, never a pass or a failure.
6. **An absence claim must carry what was searched.** A finding that asserts an absence ("no changelog entry", "no CODEOWNERS file") carries a record of the search, and a host-side validator rejects an absence claim without it or without a positive control.
7. **Coverage earned by proof.** A module's claim to cover a rule counts only when a conformance fixture pair (a seeded defect reported at an exact file and line, and a clean twin that reports nothing) passes against the real tool; otherwise it is reported as claimed but unproven.
8. **A pass percentage kept apart from coverage.** Each run reports the share of the rules that ran to a verdict that found nothing, distinct from the share of a catalogue that can be checked at all.
9. **The repository cannot silence a delegated rule.** The tool's own configuration files and inline ignore comments are replaced or switched off by command-line configuration, and a finding that carried such a comment is still reported and flagged.
10. **An allowlist whose waivers are visible and checked for staleness** by whether the rule ran to completion and found nothing at that place.
11. **A dependency-to-patent-caution index match.** Compare a project's declared dependencies with an index of patent cautions per package and report the entry's action (avoid, flag for review, separate component, allow), stating that a match is not a patent opinion and no match is not a clearance.
12. **A version and changelog gate.** Compare the manifest version at HEAD with the version at the merge-base with the default branch, and require a changelog entry that names the new version as a whole token.
13. **Redaction of third-party tool output** before it reaches a terminal or a report: secret-shaped values replaced, control characters removed, length cut, and a build script's output withheld for a fixed sentence.
14. **A default-branch protection audit** reading the forge's branch-protection and ruleset interfaces, distinguishing "protection absent" from "not readable".
15. **A precision gate for enabling a rule by default:** a shuffled, seeded 30-finding sample judged against a rubric across at least three repositories, the sample and the judge recorded beside the rule (a documented process, not implemented in code).
16. **The hosted service design (not built):** clone a submitted repository, run the delegated tools in a throwaway virtual machine from an image that bundles them, and return a size-capped, redacted result to a dashboard.

## Queries (source, query, date)

All on 2026-10-25. Source: Google Patents through a web search restricted to `patents.google.com` (and, for the first batch, unrestricted web search); claims read through the shared tool `C:\Projects\tools\patent-claims.py` (Google Patents' public page). **Coverage is limited:** 25 queries, up to 10 results each, one search engine; USPTO, Espacenet and WIPO full-text search were not used (not reachable by this lane's tools); non-US filings were not searched separately. Google Patents refused automated fetches (HTTP 503) for about an hour and a quarter mid-sweep and then recovered.

| # | Idea | Query (abridged) |
|---|---|---|
| Q1 | 1 | method detecting unused public functions in a software library by compiling with rewritten visibility and compiler dead code warnings |
| Q2 | 1 | detecting unused exported functions in program, dead code analysis public interface not referenced |
| Q3 | 1 | source code analysis rewrite code so compiler reports unused public symbols, build copy of project, parse compiler diagnostics to identify dead code |
| Q4 | 3 | persistent build cache shared target directory keyed by toolchain repository lock least recently used pruning compiled artifacts |
| Q5 | 3 | build cache compiled artifacts stored by key toolchain version reused across builds source repository eviction |
| Q6 | 3 | exclusive lock file build directory shared between processes holder process id sidecar file timeout waiting compiler cache directory eviction size limit |
| Q7 | 4 | incremental build rebuild decision based on file modification timestamps restore source file after temporary modification new timestamp so dependents rebuild |
| Q8 | 5 | static analysis tool plugin framework external process pinned checksum verified install scrubbed environment findings mapping record |
| Q9 | 5 | static analysis external tool downloaded verified by hash executed in child process results normalized into common finding record |
| Q10 | 5 | downloaded archive validated before extraction reject entire archive if any entry path traversal symbolic link, extract single member verify hash |
| Q11 | 6 | static analysis finding of absence must include record of what was searched negative result evidence |
| Q12 | 6 | code analysis tool result must state what was searched to support a claim of absence, validator rejects finding without evidence of search |
| Q13 | 7 | code analysis rule coverage proven by conformance fixtures seeded defect clean twin claim of coverage |
| Q14 | 8 | computing coverage of rules implemented by plugins, report of rules not implemented versus implemented and verified |
| Q15 | 9 | repository cannot disable code scanning rule configuration override command line replace project config suppression comments ignored |
| Q16 | 9 | static analysis suppression comments in source code ignored by pipeline, repository owner cannot disable security rule, policy enforcement override |
| Q17 | 10 | suppressed warnings allowlist stale entry detection analysis result no longer reported remove baseline entry |
| Q18 | 11 | compare software project dependencies against index of patent risk entries package name action avoid flag review |
| Q19 | 11 | software dependency open source component patent risk database lookup flag component warn developer |
| Q20 | 12 | compare version of manifest at merge-base with branch head changelog entry required release gating pull request check |
| Q21 | 12 | verify software package version bump and changelog entry in pull request compared with base branch automated check |
| Q22 | 13 | redact secrets from third-party tool output before display control characters length cap scanner results sanitization |
| Q23 | 13 | code review tool sanitizes output of third party analysis, removes secrets and control characters from build log before presenting to developer |
| Q24 | 14 | repository branch protection audit required status checks default branch policy compliance scanning code hosting platform |
| Q25 | 15 | enabling an analysis rule by default after sampling findings and measuring precision of true positives threshold sample size |

## Hits

Only patents whose **independent claims were read in full** are listed. Titles seen in result lists that were not read are not hits and are not claimed as cleared; the search coverage above is the limit of this sweep.

| # | Patent | Title | Assignee | Status (Google Patents) | Claim(s) read | Overlap | Our difference | Verdict |
|---|---|---|---|---|---|---|---|---|
| 1 | US12159126B1 | Systems and methods for identifying unused code | USAA | Active, est. expiry 2042-09 | 1 (and 2 to 8) | Reports unused code portions. | Claim 1 requires tracking the execution of code that is currently executing, finding a branch that does not run because a conditional is on a hardcoded value, and a graphical user interface. CodeRipper never executes or profiles the analysed code and reads compiler diagnostics of a rewritten copy; it has no execution tracking, no hardcoded-condition test, no GUI. | clear |
| 2 | US11868747B2 | System and method for dynamic dead code analysis | JPMorgan Chase | Active | 1 and 9 | Reports methods never used. | Claim 1 is a runtime method: a list of all method names loaded into a hash set, a pointcut around each invoked method, removing the name on invocation. CodeRipper is static and has no runtime hook. | clear |
| 3 | US7086063B1 | System and method for file caching in a distributed program build environment | Electric Cloud | Expired 2024-04 | 1 | A cache across build jobs. | Expired. Claim 1 also requires scheduling build jobs in parallel across nodes, predicting and preloading files, and a policy that caches header files and flushes object files after each job; CodeRipper's cache is one target directory per toolchain and repository, with a lock and least-recently-used pruning. | clear |
| 4 | US7526755B2 | Plug-in pre- and postconditions for static program analysis | Microsoft | Expired 2026-04 | 1 | A static checker that calls third-party plug-in code. | Expired. In-process checker with plug-in conditions over an object file; CodeRipper runs tools as child processes. | clear |
| 5 | US20210200834A1 (found by Q9; idea 16 had no query of its own) | Asynchronous static analysis system for a collaborative software development environment | Atlassian | Active (application; grant status not checked) | 1 | Claim 1: clone a selected branch of a repository to a local directory, analyse it to find a property (such as a file extension), select from an image database an image packaging a static analysis tool for that property, instantiate each image with a pointer to the directory, store results in a database. **Our hosted-service design (idea 16, not built) clones a submitted repository, picks the tools for the languages found, runs them from a virtual-machine image that bundles them, and stores a result.** | The command-line tool does none of this: it runs pinned tool binaries named by the rule, from a lock, as child processes, with no image database and no results database. The hosted design differs in that the tools are chosen by rule and language, not by property, and the result store is a dashboard; whether "an image packaging a tool selected by a property of the cloned files" reads on it was not settled by this lane's reading. **The PM then read the claims from the USPTO page images (decision 300, CireSnave ruled A, 2026-10-25; archive entry 300 has the reading): not too close as designed; claim 9 is broad; the grant status was not checked, so re-read at grant or when Google Patents answers.** Face B is held to a design rule (ROADMAP H9; hosted-service plan §3 item 10): one fixed runner image per job, never a step that selects per-tool images from an image store by a file property, the pinned-tarball fetch (no clone), triggered by user submission only. | design-around (by that design rule; read by the PM) |
| 6 | US10540176B2 | Method and system for controlling software risks for software development | Sonatype | Active, est. expiry 2036-01 | 1 | Looks a component up by a key and applies a policy action. | Claim 1 requires determining that a component is new to a software repository, matching risks before adding it, a stored policy with pass and does-not-pass actions per stage of enforcement, and adding the component to the repository on a pass. `patent-index-match` reads a project's declared dependencies and prints the index entry's action; there is no software repository being added to, no "new to the repository" step and no quarantine. | clear |
| 7 | US20170315903A1 | Systems and methods for analyzing violations of coding rules | MathWorks | Active, est. expiry 2036-10 | 1 | Reports coding-rule violations. | Claim 1 designates a violation as justified automatically when a corresponding code verification check finds no undesired behaviour. CodeRipper never justifies a finding by a check: a waiver is a written entry with a human reason, printed with the finding. | clear |
| 8 | US9535821B1 | Displaying violated coding rules in source code | MathWorks | Active, est. expiry 2033-07 | 1 | Coding-standard checks with a list of violations. | Claim 1 requires determining properties of execution paths of the code and one of a set of display forms (a tree view, a list view, an indicator in the code). CodeRipper's checks do not analyse execution paths; they read compiler diagnostics and syntax, and print text. | clear |
| 9 | US9160762B2 | Verifying application security vulnerabilities | IBM | Active, est. expiry 2033-06 | 1 | Validates static-analysis results. | Claim 1 generates mock objects from a vulnerability call trace and runs a unit test to validate a finding. CodeRipper does not. | clear |
| 10 | US12061903B2 | Software development quality assessment | Microsoft | Active, est. expiry 2043-01 | 1 | Counts conformance and nonconformance and computes a rate: our pass percentage (idea 8) is a share of rules that found nothing among those that ran, which a reader could call a quality score from the two counts. | Claim 1 also requires ranking two quality standards, or two source codes, against each other by those metrics (and scores for developers or tools in the alternatives). CodeRipper reports one run's percentage and ranks nothing. **Re-sweep if a feature ranks projects or rule sets by the pass percentage** (for example a cross-repository `--scope` summary ordered by it). | clear, with that watch |
| 11 | US11212298B2 | Automated onboarding of detections for security operations center monitoring | Microsoft | Active, est. expiry 2039-10 | 1 | Onboards a detection algorithm automatically when its finding volume is low, rejects it when high, and otherwise by a sampled true-positive rate: close in spirit to our precision gate (idea 15), including "an exact rule with few findings may ship default-on". | The claim is a system with a volume thresholder (onboard below one threshold, reject above another) and a performance evaluator. Our gate is a written policy applied by people: it has no rejection threshold on volume (a noisy rule is opt-in, never rejected), requires a fixture proof both ways for the low-volume case, and is not implemented in software. **Re-sweep before any software decides default-on status from counts.** | design-around (by not being a system, and by omitting the reject-on-volume step) |
| 12 | US7568109B2 | System for software source code comparison | IPX | Expired 2026-05 | 1 | Compares source code with patent databases. | Expired; the claim is a corpus-comparison system built on concept frequencies. | clear |
| 13 | US7725417B2 | Method and system to analyze rules based on popular query coverage | eBay | Active, est. expiry 2028-05 | 1 | A percentage of rule coverage. | The claim is about search queries and keyword-matching rules in a marketplace database. | clear |
| 14 | US9552272B1 | Utility to instantly protect sensitive information for an application log at runtime | Symantec | Active | 1 | Redacts sensitive data in log messages. | Claim 1 intercepts a log method call of a second running application and recursively examines composite messages. CodeRipper post-processes the text a tool already printed. | clear |

## Open points and watches

- **Nothing in this record is waiting on a decision.** Hit 5 (US20210200834A1, Atlassian) concerned the unbuilt hosted service (face B) only; the PM read its claims from the USPTO page images and CireSnave ruled (decision 300 = A) that face B is built to the design rule in the verdict above. It blocks nothing in the command-line tool or the library. **Re-read hit 5 at grant, or when Google Patents answers for it** (its grant status was not checked), and re-sweep if face B's design changes how tools are chosen or bundled.
- **Watches (not hits):** hit 10 (re-sweep if the pass percentage is used to rank) and hit 11 (re-sweep if default-on status is decided by software from finding counts).

## Addendum: the 0.10 line (2026-10-26, 0.10.0 @ 2d5f055)

Version line covered by this addendum: **0.10** (0.10.0 is `main` at `2d5f055`, which is 0.9.53, plus this release PR: the version number, the changelog and this addendum). The sweep above was made at 0.9.46 (`0ad0026`); five changes have merged since, read from `git log 0ad0026..2d5f055` and `git diff --stat 0ad0026..2d5f055 -- src`:

| Change | Mechanism | New capability family? | Sweep result |
|---|---|---|---|
| `SEC-009` run by checkov (0.9.51, `src/module/delegate/checkov.rs`, `tools.lock`) | The delegated-tool framework of idea 5, applied to one more tool: a pinned, hash-verified standalone download; the same scrubbed child-process run; the tool's JSON mapped to findings by a fixed list of 37 check ids; a `checkov:skip` comment not honoured and flagged (idea 9); a report with parse errors makes a clean result a gap (the "not judged is not clean" rule of idea 5). | No: ideas 5 and 9 again; a mapping table and a different tool's output, no new mechanism. | Hits 1 to 14 stand. Hit 5 (US20210200834A1) is about choosing tool images by a property of the cloned files; the CLI still names the tool by rule from a lock, so nothing changes (ROADMAP H9 keeps the hosted design to its rule). |
| `patent-sweep-record` rule (0.9.53, `src/module/neutral/patent_sweep.rs`) | A file-presence and shape check: the record exists, names the manifest's version line, has ideas, queries and hits headings, no `unread` verdict. | No: the same family as `version-and-changelog` (idea 12): a text rule over tracked files. | No new patent area; the record's own checks are plain text matching. |
| `version-and-changelog` accepts a list item (0.9.48) | A wider text matcher for a changelog entry. | No: idea 12. | Hits stand. |
| Decision-285 documents, `ROADMAP.md`, `Cargo.lock` zip 9.1.1 | Documentation and a dependency bump. | No. | None. |
| The record itself (0.9.52) | Documentation. | No. | None. |

The 0.10.0 release therefore adds no capability family, and the ideas, queries, hits and limitations above are unchanged. **Not covered by this addendum:** the result cache (keyed on the committed tree) and `--scope`, which are built after 0.10.0 and get their own addenda before they are released; the hosted service, which is unbuilt and held to ROADMAP H9.

## Addendum: the result cache (0.10.2, 2026-10-26)

Version line covered by this addendum: **0.10**. The result cache (`src/result_cache.rs`) is a new mechanism, so it is swept before it is released. Written by the coderipper lane; the same method and limits as above.

**Idea 17 (the result cache).** For the checks that build the project and analyse its committed tree, store each check's result (its findings and notes) under a key made of the git tree hash of `HEAD`, the project's path in its repository, the CodeRipper version, the check, the toolchain, the cargo features and the build environment variables; a later run with the same key reads the stored result back instead of analysing again, and says so in a note. The store is a local directory (`<build cache root>/results/`), entries are JSON files, an error or gap is never stored, and a damaged entry is a miss.

Queries (2026-10-26, Google Patents through a web search restricted to `patents.google.com`; also the Q8 and Q9 result lists above, which surfaced the Synopsys patent):

| # | Idea | Query (abridged) |
|---|---|---|
| Q27 | 17 | static analysis results cached and reused when source code unchanged, key derived from commit hash or file content hash and analyzer version, skip re-analysis |
| Q28 | 17 | incremental static analysis reuse previous analysis results for unchanged files of a repository store results keyed by content digest |

Hits (independent claims read in full through `patent-claims.py`):

| # | Patent | Title | Assignee | Status (Google Patents) | Claim(s) read | Overlap | Our difference | Verdict |
|---|---|---|---|---|---|---|---|---|
| 15 | US10459822B1 | Iterative static analysis using stored partial results | Amazon Technologies | Active | 1 (and dependent 2 to 4) | Claim 1: perform a static analysis of first source code including a first code snippet; generate metadata describing the result; apply a hashing function to the snippet; store the metadata in a data store in association with the hash; on a second request for source code including a second snippet "at least similar" to the first, hash it, find the match, obtain the stored metadata, and generate the second result incorporating it. **A cache of analysis results looked up by a hash of the code is the same shape.** | Our key is not a hash of a code snippet: it is the git tree hash of the whole committed repository combined with the check, the CodeRipper version, the toolchain, the features and the build environment, and it matches only an identical key (never a "similar" snippet). Our stored value is the check's whole result for that tree, returned as the result, not metadata incorporated into a larger result for source code that merely includes the snippet. Whether an identical whole tree counts as a "snippet", and a stored result as "metadata describing the result", is a question of claim construction this lane cannot settle. | design rule (read by the PM from the USPTO PDF, claims 1, 5 and 13; **not a clearance**) |
| 16 | US12093163B1 | Accelerating static program analysis with summary reuse | Black Duck Software | Active, est. expiry 2042-12 | 1 and 5 | Partition an analysis into work units; for a unit with a stored record, retrieve its summary instead of analysing; analyse the rest; store the new summaries. Our per-check results could be called work units. | Claim 1 requires the records to be in a **network-accessible summaries repository**. Ours is a local directory of the machine running the check, never shared or served; and it stores a check's whole result, not summaries of defects per work unit of an interprocedural analysis. **Design rule: the result cache stays local; a network-accessible or shared store is not built without a new sweep.** | design-around (by the design rule) |
| 17 | US11941379B1 | Accelerating static program analysis with artifact reuse | Synopsys | Active, est. expiry 2042-09 | 1 and 6 | Reuses stored artifacts instead of regenerating them. | Claim 1 gathers artifacts for an interprocedural analysis, finds some with a record in a network-accessible artifact repository, and writes an "emit database" of artifact identifiers. Ours has no artifact repository and no emit database. | clear |
| 18 | US9378013B2 | Incremental source code analysis | Semmle | Active | 1 | Incremental analysis of a later snapshot. | Claim 1 is built on full and incremental build graphs, a keep graph, and merging incremental analysis artifacts into the full set. Ours re-analyses everything on a miss and reuses nothing partial. | clear |
| 19 | US9830335B1 | Snapshot manifest for static analysis results | Semmle | Expired | 1 | Compares results of consecutive snapshots by identifiers. | Expired; the claim computes per-file results files and compares manifests with a previous snapshot in a data storage system over a network. | clear |

**Hit 15 as read by the PM (2026-10-26; claims 1, 5 and 13 from the USPTO PDF; specification, priority and expiry unread).** Claim 1 needs a hash of a code snippet and a second, similar snippet; claims 5 and 13 need a hash of a "portion" of the source code. The cache keys on the whole committed tree plus toolchain and options, locally: a whole tree is arguably a "portion", so it is **not clearly outside claims 5 and 13**. This is a residual risk, not a clearance, and it is on the decisions board before 0.10.2 is published. **Design rules for the result cache, fixed by this reading:** (1) the key stays the whole committed tree plus toolchain plus options; (2) **no per-file, per-snippet or per-function hash keys**; (3) **no reuse of a prior result for a partly changed tree** (no partial or "similar" matches); (4) no shared or remote store (also hit 16). Any change to these rules needs a new sweep. The command-line tool and every other mechanism are unaffected.

## Addendum: `--scope` (0.10.3, 2026-10-26)

Version line covered by this addendum: **0.10**. `--scope FILE` (`run_scoped` in `src/cli.rs`) is swept before release, though it is a small mechanism.

**Idea 18 (`--scope`).** Run one command over each repository a list file names (one path per line, comments and blank lines ignored, relative paths read from the list's directory), each repository analysed on its own under its own configuration; print a marker before each repository's output and a closing count; a repository that cannot be read is reported and the others still run; the exit code is the worst of them.

| # | Idea | Query (2026-10-26, Google Patents through a web search restricted to `patents.google.com`) |
|---|---|---|
| Q29 | 18 | run code analysis over a list of repositories from a configuration file, aggregate results per repository and exit status |

| # | Patent | Title | Assignee | Status (Google Patents) | Claim(s) read | Overlap | Our difference | Verdict |
|---|---|---|---|---|---|---|---|---|
| 20 | US8806441B2 | Static code analysis | IBM | Expired, est. 2032-08 | 1 | Runs analysis from a request and merges tool outputs. | Expired. Claim 1 routes a request to a set of tools chosen by their capability for the categories requested, merges their outputs through tool-agnostic abstractions, performs a deep analysis on categories with many false positives, and reconfigures the tools from user feedback; `--scope` does none of this: it repeats the same command over a list. | clear |
| 5 (again) | US20210200834A1 | Asynchronous static analysis system for a collaborative software development environment | Atlassian | see hit 5 | 1 | The result-list in Q29 describes running tools over every repository of an organization; claim 1 is the clone, choose-image-by-file-property and store flow. | `--scope` clones nothing, chooses no image and stores no result in a database: it runs the local command per directory. Hit 5's design rule (ROADMAP H9) is unchanged. | design-around (as hit 5) |

Results listed by the search but whose claims were not read (for example WO2015085261A1, US8843882B1 on repository metrics) are not hits and are not cleared; the search limits above apply. A loop over a list of directories is common practice in shell scripts and build tools, and no patent found claims it as such.

## Addendum: the 0.11 line (2026-10-26, 0.11.0)

Version line covered by this addendum: **0.11**. Two delegations and one native rule are added over 0.10 (decision 285 M8), read from `src/module/delegate/semver.rs`, `src/module/delegate/squawk.rs` and `src/module/neutral/lock_order.rs`:

| Change | Mechanism | New capability family? | Sweep result |
|---|---|---|---|
| `API-001` run by cargo-semver-checks (`semver.rs`, `tools.lock`) | The delegated-tool framework of idea 5 applied to another tool: a pinned, hash-verified standalone download; a scrubbed child-process run with the tool's target directory in a scratch directory; the tool's text report parsed into findings at the place an item was in the baseline. The baseline is the newest release tag below `HEAD`, else the merge-base with the default branch (the same two ways idea 12 and the schema rule choose a baseline). | No: idea 5 again; choosing a release tag as the baseline is a git query, and comparing a crate with a git revision is what the tool does. | Hits 1 to 14 stand (hit 5 is about choosing tool images by a file property; `API-001` names its tool by rule). No new query: no new mechanism. |
| `API-007` run by squawk (`squawk.rs`, `tools.lock`) | Idea 5 again: the tracked migration files are selected by path and name, given to the tool with its style rules excluded and an empty configuration file, and the JSON report is mapped to findings by rule; a statement the tool cannot parse makes a clean result a gap (idea 5's "not judged is not clean"); `squawk-ignore` comments are counted and said on the findings (idea 9 in the one case where the tool cannot be told to ignore them). | No. | Hits 1 to 14 stand. |
| `CON-006` lock-order cycles (`lock_order.rs`) | **Idea 19, new:** from Rust source alone, name a lock by the path that reaches it (`self.field`, a `static`), record an edge `held -> acquired` whenever a guard is held while another lock is acquired (a `let`-bound guard to the end of its block or `drop`, a temporary to the end of its statement, an `if` condition ended before the body, an `if let` scrutinee held through it), and report a cycle of the repository's edges (and a lock taken while held) as a possible deadlock. | **Yes** (a static source analysis, not the delegated-tool framework). | Queries Q30 to Q32 and hits 21 to 26 below: none clears a claim read as reading on it; the area is heavily patented, so this is not a clearance. |

**Idea 19 queries (2026-10-26, Google Patents through a web search restricted to `patents.google.com`):**

| # | Idea | Query (abridged) |
|---|---|---|
| Q30 | 19 | static analysis detect potential deadlock by building lock acquisition order graph from source code and finding cycles |
| Q31 | 19 | lock order inversion detection program analysis lock graph nested lock acquisition held locks edge deadlock warning |
| Q32 | 19 | source code analysis mutex guard lifetime scope temporary until end of statement lock held while acquiring second lock report |

| # | Patent | Title | Assignee | Status (Google Patents) | Claim(s) read | Overlap | Our difference | Verdict |
|---|---|---|---|---|---|---|---|---|
| 21 | US8230437B2 | Methods and apparatus for detecting deadlock in multithreading programs | IBM | Expired (fee) | 1 | A resource graph whose cycles indicate deadlock, built from static analysis. | Expired. Claim 1 builds an invocation graph with a single root over the functions of the program, computes resource sets at each node and refines the resource graph by post-processing. `CON-006` builds no invocation graph and follows no call: it reads direct nesting inside one function. | clear |
| 22 | US8763003B2 | Methods and apparatus for detecting deadlock in multithreading programs | IBM | Expired 2026-03 | 1 | Same family as hit 21; claim 1 is the invocation graph, resource graph and cycle steps without the later limitations. | Expired, and the same missing element: no invocation graph. | clear |
| 23 | US8448175B2 | Deadlock detection in a parallel program | IBM | Expired (fee) | 1 | A lock graph and cycle detection. | Expired. Claim 1 records lock events while the program runs, adds "gate lock" events and thread-creation order relations; `CON-006` never runs the program. | clear |
| 24 | US8661450B2 | Deadlock detection for parallel programs | IBM | Expired (fee) | 1 | A lock graph and deadlock detection. | Expired. Claim 1 deletes locks that are no longer used from a graph built from a running program's lock operations; `CON-006` is static. | clear |
| 25 | US20070143766A1 | Deadlock detection in a computing execution environment | IBM | Abandoned | 1 | Deadlock detection. | Abandoned; claim 1 is a second thread monitoring lock attempts against a time threshold at run time. | clear |
| 26 | US9052967B2 | Detecting resource deadlocks in multi-threaded programs by controlling scheduling in replay | VMware | Active, est. expiry 2032-04 | 1 | Identifies potential deadlocks from lock and release events. | Claim 1 records an execution in a virtual machine and replays it under a hypervisor with a changed schedule; `CON-006` records and replays nothing. | clear |

The search found six patents, all IBM or VMware filings on call-graph or runtime approaches; **static lock-graph deadlock detection is a well-worked field with many filings and a large academic literature (the first search also returned papers on static deadlock analysis for C, Java bytecode and Go), so a patent this search did not surface is possible. This is a first-pass read, not a clearance.** The mechanism built reads direct nesting within a function only, which is narrower than the call-graph analyses above.

The 0.11 line therefore adds no capability family beyond ideas 5, 9 and 12 **and idea 19 (swept above)**, and the ideas, queries, hits and limitations above are unchanged. **Not covered by this addendum:** the result cache and `--scope` (their own addenda, released as 0.10.2 and 0.10.3 if the board allows), and the hosted service (ROADMAP H9).

## Addendum: the 0.13 line (2026-10-29, 0.13.0)

Version line covered by this addendum: **0.13**. One rule is added over 0.12, read from `src/module/neutral/ops012.rs`: `OPS-012`. Swept 2026-10-29 by the coderipper lane (Claude Sonnet 5.5), same method and limits as above (Google Patents through a web search restricted to `patents.google.com`).

**Idea 21 (`OPS-012`).** Parse the source of a command-line program and report, from the syntax tree and the package manifest, four departures from platform convention: a fixed editor program started with no mention of the editor environment variables; a dotfile joined onto the home directory where the platform has a configuration directory; an argument loop with no argument-parser dependency; and the version and verbose short flags swapped.

| # | Idea | Query (2026-10-29) |
|---|---|---|
| Q36 | 21 | static analysis source code detect hardcoded text editor instead of environment variable EDITOR command line tool convention violation |
| Q37 | 21 | analyzing source code to detect configuration file stored in user home directory instead of platform standard configuration directory |
| Q38 | 21 | static code analysis detect hand-written command line argument parsing loop rather than argument parser library, flag naming convention check |

**Positive controls:** weak. Each query returned general static-analysis patents (for example US5854924A, US9032376B2, CN101017458A), so each reaches the static-analysis area; none returned a patent on convention checking, and that absence is a statement about these three queries only.

No patent on point was surfaced by Q36 to Q38. The results listed (US20070100820A1, CN101017458A, US5854924A, US9672015B2 and others) were **not read**: they are not hits and carry no verdict, and none is cleared. No row is added to the hits table. **Limits specific to this idea:** three queries, one search engine, no USPTO, Espacenet or WIPO full-text search, no citation chasing. **Result: no hit; this is not a clearance.** The 0.13 line adds no other mechanism: hits 1 to 32 and ideas 1 to 20 are unchanged. **Watch:** re-sweep if `OPS-012` starts to rewrite code, learn conventions from a corpus, or judge another language.

## Addendum: the 0.14 line (2026-10-30, 0.14.0)

Version line covered by this addendum: **0.14**. No rule is added over 0.13; `OPS-012` (`src/module/neutral/ops012.rs`) gains four cases, judged only for a feature the binary offers. Swept 2026-10-30 by the coderipper lane (Claude Sonnet 5.5), same method and limits as above (Google Patents through a web search restricted to `patents.google.com`).

**Idea 21, extended.** From the syntax tree and the package manifest of a command-line program: a long option (`--yes`, `--quiet`, `--verbose`) offered without its conventional short form; standard input read and files opened with no operand `-` for standard input; a colour crate that reads no `NO_COLOR` with no source naming it, or a presence-only test of it; and a program that runs until stopped with no signal handling in its package.

| # | Idea | Query (2026-10-30) |
|---|---|---|
| Q39 | 21 | static analysis command line program detect flag -q -v short option missing for long option convention check source code |
| Q40 | 21 | detecting program that ignores NO_COLOR environment variable or lacks signal handler for interrupt, static analysis of source code |
| Q41 | 21 | analyzing command line tool source to determine whether standard input is accepted via dash operand, command line interface convention conformance |

**Positive controls:** weak, as for Q36 to Q38. Each query returned general static-analysis patents (for example US7340726B1 and US5854924A), so each reaches the area; none returned a patent on convention checking, and that absence is a statement about these three queries only.

No patent on point was surfaced by Q39 to Q41. The results listed (US7617492B2 "Extensible command line parsing", US20080155565A1, US20020083216A1, US7340726B1, US5854924A, US10915424B2 and others) were **not read**: they are not hits and carry no verdict, and none is cleared. No row is added to the hits table. **Limits specific to this idea:** three queries, one search engine, no USPTO, Espacenet or WIPO full-text search, no citation chasing. **Result: no hit; this is not a clearance.** The 0.14 line adds no other mechanism: hits 1 to 32 and ideas 1 to 21 are unchanged.

## Limitations

- AI-assisted search and read; **not legal advice and not a freedom-to-operate opinion**. A lawyer's review is a separate, later purchase.
- One search engine, 25 queries, up to 10 results each; no USPTO, Espacenet or WIPO full-text searches, no non-US filings searched on their own, no citation chasing. A patent the queries did not surface is not cleared by this record.
- Claims are machine-converted text from Google Patents; status and expiry are Google Patents' estimates, not the official record. The record names claims 1 and the independent claims listed; dependent claims were skimmed, not analysed.
- Several ideas (the rewritten-visibility reachability check, the mtime-restoring session checkout, the entire-archive refusal, absence claims that carry their search, conformance-earned coverage) returned no on-point patent in any query; that is an observation about these searches, not a clearance.
- The ideas were written by the lane that owns the project, from the source at this commit; a mechanism it overlooked is not swept.
- Re-sweep when CodeRipper adds a new family of capability (a new mechanism, not a bug fix), and at least yearly.

## Addendum: the 0.12 line (2026-10-29, 0.12.0)

Version line covered by this addendum: **0.12**. One rule is added over 0.11, read from `src/checks/cli_basics/`: `OPS-011`. Swept 2026-10-29 by the coderipper lane (Claude Sonnet 5.5), same method and limits as above (Google Patents through a web search restricted to `patents.google.com`; claims read serially through `patent-claims.py`).

**Idea 20 (`OPS-011`).** Build the project's binaries from the committed tree; run each over a fixed set of standard flags (`--version`, `-V`, `--help`, `-h`, an unknown flag) in a fresh temporary directory with an empty environment and a timeout; check the exit codes and output streams against a stated standard; and compare a listing of the directories the process could write to before and after. A new capability family (running the built artifact over a fixed probe set).

| # | Idea | Query (2026-10-29) |
|---|---|---|
| Q33 | 20 | automated testing executable run in sandbox compare file system listing before and after execution detect unexpected file writes |
| Q34 | 20 | smoke test command line program invoke with standard flags version help unknown option verify exit code and output stream |
| Q35 | 20 | build binary from committed source tree in temporary checkout execute with empty environment and timeout verify behavior of built artifact |

**Positive controls (what each query is known to reach):** Q33 returned US20160156658A1 and US9239922B1, whose claims do record a state before and after running code, so the query reaches that area. Q34 returned US8145988B2 and US6434738B1, whose claims do test a program's output and exit code, so it reaches command-line testing. Q35 has only a weak control: it returned papers on rebuilding from a committed tree and checking the artifact, so it reaches the topic, but it returned no patent on it, and that absence is a statement about this one query only.

Q35 returned no patent on point (papers on reproducible builds and one workload-integrity application, WO2026080403A1, whose claims were not read: it is not a hit and is not cleared).

| # | Patent | Title | Assignee | Status (Google Patents) | Claim(s) read | Overlap | Our difference | Verdict |
|---|---|---|---|---|---|---|---|---|
| 27 | US8145988B2 | Command line testing | IBM | Expired, est. 2031-01 | 1, 8 and 15 (all independent) | Tests an application's command-line output. | Expired. Each independent claim compares two tables of named columns (expected output against a description of it) and alerts on a column difference; `OPS-011` has no tables, no columns and no stored expected output, only a fixed standard for five flags. **Missing element: the two tables of named columns and the comparison of their columns.** | outside as read |
| 28 | US6434738B1 | System and method for testing computer software | an individual | Expired | 1, 6, 7 and 13 (all independent) | Compiles then runs a program and compares its exit code and output (dependent claims 3, 10, 15). | Expired. Every independent claim receives a program code fragment and forms a concatenated program by prepending or appending code blocks before compiling; `OPS-011` builds the whole committed package and receives no fragment. **Missing element: receiving a code fragment and forming a concatenated program from it.** | outside as read |
| 29 | US20160156658A1 | Method and system for automatic detection and analysis of malware | Verisign | Active, est. expiry 2031-09 (application; grant status not checked) | 1 and 10 (both independent) | Baseline before, execute a file, state after, compare: the same shape as our directory listing before and after. | Claims 1 and 10 require a memory baseline of volatile and non-volatile memory, changed processes with timestamps, rootkit-concealment behaviours, a determination that the file is malicious and a timeline of the malware's activities. `OPS-011` reads no memory or process state, judges no maliciousness and builds no timeline; it lists the files of one empty directory tree. **Missing elements: the memory baseline, the changed-process analysis with timestamps, the rootkit-concealment behaviours and the malicious determination.** | outside as read |
| 30 | US9239922B1 | Document exploit detection using baseline comparison | Trend Micro | Active | 1, 10, 20 and 27 (all independent) | Records behaviours (file system among them) of running code and compares with a baseline. | Every independent claim opens a document file in an application and compares a recorded behaviour pattern with a pattern file from a known-malicious or known-benign document, with weights in claims 10 and 20. `OPS-011` opens no document, keeps no pattern file and weighs nothing. **Missing element: the pattern file of a known-malicious or known-benign document compared with the recorded behaviours.** | outside as read |
| 31 | US20200004962A1 | Identification and extraction of key forensics indicators of compromise using subject-specific filesystem views | IBM | Active, est. expiry 2041-06 (application; grant status not checked) | 1, 8 and 15 (all independent) | Captures the file creation, deletion and modification an untrusted process performs, in an overlay on a writable base: a file-activity record of a run, as our before and after listing is. | Every independent claim requires read-only filesystem overlays deployed on a writable base filesystem, a determination that the subject is **malicious**, and a predetermined protective action on that determination. `OPS-011` uses no overlay (a throwaway directory listed before and after), makes no maliciousness determination and takes no protective action. **Missing elements: the read-only overlay over a writable base, the maliciousness determination, the protective action.** | outside as read |
| 32 | US10043001B2 | Methods and apparatus for control and detection of malicious content using a sandbox environment | Invincea | Active, est. expiry 2034-06 | 1, 4 and 13 (all independent) | Runs a program in a sandbox environment. | Every independent claim concerns a second application started by a first application inside the sandbox attempting to modify a **registry key**, judged anomalous against a set of allowed behaviours specific to the first application (claims 1 and 13 then define a hash signature of the second application's file; claims 1 and 4 terminate it). `OPS-011` starts one binary, watches no registry, holds no per-application allowed-behaviour set and signs nothing. **Missing elements: the second application started by the first, the registry-key attempt, the allowed-behaviour set.** | outside as read |

Results listed but whose claims were not read (CN104200161A, US7010782B2, US8037360B2, EP1179776A1, WO2026080403A1) are not hits and carry no verdict: they were not read. **Limits specific to this idea:** three queries, one search engine, no USPTO, Espacenet or WIPO full-text search, no citation chasing; the sandboxed-execution-with-filesystem-diff area is crowded (malware analysis in particular), so a claim this search did not surface may read on it. **Result: six patents read (hits 27 to 32), each outside as read on `OPS-011` for the missing element named in its row; this is not a clearance.** The 0.12 line adds no other mechanism: hits 1 to 32 and ideas 1 to 19 are unchanged. **Watch:** re-sweep if `OPS-011` or a follow-up (M15, M16) starts to judge a binary malicious, keep a stored expected-output table, or record a process or memory baseline.
