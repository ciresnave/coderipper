# SEC-002 reviewed waivers: design note

**Status:** proposed (design only; no code in this PR). **Ask:** the PM, 2026-11-11. **Rows:** SEC-002, OPS-006.

## Problem

SEC-002 runs `gitleaks git` over the whole history with an empty ignore file on purpose: a repository must not be able to silence its
own secret scan (README, "Rules run by a tool: SEC-002"). So a *verified* false positive in a repository's history can never be
cleared from that repository. SecretAnt#4 is the first case: prose in `CHANGELOG.md` that the `generic-api-key` rule reads as a key
(entropy 3.73, no secret), present in two commits on main. Rewording the current line cannot help, because the scan reads history.

## Design

A waiver list that lives **in CodeRipper's own repository**, reviewed by the PM through a normal PR, and shipped inside the binary
(`include_str!` of `waivers/secrets.toml`), so a scanned repository can neither add nor edit an entry. A new entry reaches users with
the next release; until then the finding stands.

### An entry

```toml
[[waiver]]
repo       = "ThinkersJournal/secretant"            # GitHub owner/name of the scanned repository
commit     = "6e944684ca91408834c7f0d8b8d317cd2076e35e"   # full 40-hex SHA, never abbreviated
file       = "CHANGELOG.md"
rule       = "generic-api-key"                       # the gitleaks RuleID
line       = 5
reviewer   = "pm"                                    # who read the line and judged it
reviewed   = "2026-11-11"
evidence   = "prose in the changelog; entropy 3.73; no credential; SecretAnt#4"
expires    = "2027-02-11"                            # at most 90 days after `reviewed`
```

All nine fields are required; a missing or malformed field makes the whole list a startup error of the binary's tests, not a silent
skip (the list is parsed in a unit test and in `coderipper doctor`).

### Matching: exact fingerprint only

gitleaks already names a finding by `commit:file:rule:line` (its `Fingerprint`). An entry applies to a leak iff **all** of `commit`,
`file`, `rule`, `line` equal the leak's and `repo` equals the scanned repository's identity. There is no glob, no prefix, no
"any line", no "any commit", and **never a waiver of a rule or of a path**: a waiver for `generic-api-key` in `CHANGELOG.md` at one
commit says nothing about line 6, the next commit, or another rule. A moved line is a new finding, deliberately, so a real secret
pasted next to a waived line is still reported.

`repo` comes from the scanned repository's `origin` URL, normalised to `owner/name` (the same normalisation the portfolio gate uses).
No `origin`, or one that does not parse, means no waiver applies. The commit SHA must exist in the scanned history, which is what ties
the waiver to the content that was actually reviewed (a SHA names a tree).

### Visible, not silent

A waived leak is **not dropped**: it becomes an `Info` finding on SEC-002 (`waived: reviewed by pm 2026-11-11, expires 2027-02-11:
<evidence>`), so the run says what it accepted and the coverage line counts it. It does not gate (Info is below every `--deny` that
SEC-002 is run with). This is the same shape as the tool-native waivers of #276 (Info, with reason, owner and expiry; GOV-002).

### Expiry

An entry past `expires` stops applying and the leak is a Critical finding again, plus one Info finding naming the lapsed entry, so a
lapse is a nudge to re-review (or to rewrite history), never a surprise. A waiver cannot be renewed by editing `expires` alone in
review: the PR must change `reviewed` and `evidence`, which a unit test checks against the previous entry of the same fingerprint.

### What is deliberately out of scope

- **Secrets.** A waiver is for a verified false positive. A real credential is rotated and the history rewritten; it is never waived.
  `evidence` must say why it is not a credential, and the review checklist rejects "known", "test key" and "old".
- **Other tools** (osv-scanner, zizmor, buf): their own waiver paths exist (#276, ruling 8).
- **A per-repo opt-in.** There is none; that is the point.

## Tests (when built)

1. exact fingerprint match waives and emits the Info finding; changing any one of repo, commit, file, rule, line leaves the leak Critical;
2. a waiver whose commit is absent from the scanned history applies to nothing;
3. an expired entry restores the Critical finding and adds the lapse note;
4. malformed list (short SHA, missing field, `expires` > `reviewed` + 90 days, duplicate fingerprint) fails a unit test;
5. `.gitleaksignore` in the scanned repository still has no effect (existing test).

## First entries (to be added with the code)

Both from `gitleaks git` over `ThinkersJournal/secretant` main, `generic-api-key`, `CHANGELOG.md`, line 5 (gitleaks fingerprints, read
back from a local scan, entropy 3.7254806 on each):

- `6e944684ca91408834c7f0d8b8d317cd2076e35e:CHANGELOG.md:generic-api-key:5`
- `f1cc76b01a0e9afd3f1d00c95ba3dd3602fff4a1:CHANGELOG.md:generic-api-key:5`

## Questions for the PM

1. **Identity of `repo`:** `ThinkersJournal/secretant` (the forge's name, from `origin`), not the `ciresnave/` path the local checkout
   uses. Confirm.
2. **Expiry cap:** 90 days proposed. Longer, or none for history that can never change?
3. **Reviewer:** PM only, or any lane other than the one whose repository it is?

— lane: coderipper
