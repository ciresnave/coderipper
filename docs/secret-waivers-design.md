# SEC-002 reviewed waivers: design note

**Status:** built (2026-11-17, PM task; `src/module/delegate/gitleaks_waivers.rs`, `waivers/secrets.toml`). **Ask:** the PM, 2026-11-11. **Rows:** SEC-002, OPS-006.

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
expires    = "2027-02-11"                            # default 90 days after `reviewed`, hard cap 365
```

All nine fields are required; a missing or malformed field makes the whole list a startup error of the binary's tests, not a silent
skip (the list is parsed in a unit test and in `coderipper doctor`).

### Matching: exact fingerprint only

gitleaks already names a finding by `commit:file:rule:line` (its `Fingerprint`). An entry applies to a leak iff **all** of `commit`,
`file`, `rule`, `line` equal the leak's and `repo` equals the scanned repository's identity. There is no glob, no prefix, no
"any line", no "any commit", and **never a waiver of a rule or of a path**: a waiver for `generic-api-key` in `CHANGELOG.md` at one
commit says nothing about line 6, the next commit, or another rule. A moved line is a new finding, deliberately, so a real secret
pasted next to a waived line is still reported.

`repo` comes from the scanned repository's `origin` URL, a `git@host:owner/name(.git)` or `https://host/owner/name(.git)` remote normalised to `owner/name`.
No `origin`, or one that does not parse, means no waiver applies. The commit SHA must exist in the scanned history, which is what ties
the waiver to the content that was actually reviewed (a SHA names a tree).

### Visible, not silent

A waived leak is **not dropped**: it becomes an `Info` finding on SEC-002 (`waived: reviewed by pm 2026-11-11, expires 2027-02-11:
<evidence>`), so the run says what it accepted and the coverage line counts it. It does not gate (Info is below every `--deny` that
SEC-002 is run with). This is the same shape as the tool-native waivers of #276 (Info, with reason, owner and expiry; GOV-002).

### Expiry

An entry past `expires` stops applying and fails loudly: the leak returns as a **Medium** finding whose message says "waiver expired" and names the
entry. History that can never change is renewed by a new reviewed entry, never by "no expiry" (a waiver that never expires is how these rot). A waiver cannot be renewed by editing `expires` alone in
review: the PR must change `reviewed` and `evidence`, which a unit test checks against the previous entry of the same fingerprint.

### What is deliberately out of scope

- **Secrets.** A waiver is for a verified false positive. A real credential is rotated and the history rewritten; it is never waived.
  `evidence` must say why it is not a credential, and the review checklist rejects "known", "test key" and "old".
- **Other tools** (osv-scanner, zizmor, buf): their own waiver paths exist (#276, ruling 8).
- **A per-repo opt-in.** There is none; that is the point.

## Built as designed, plus one check (PM 2026-11-17)

All of the above, and a **stale-entry check**: an unexpired entry of the scanned repository whose commit is in the scanned history but which names no
finding any more is a `Medium` finding ("a secret-scan waiver names a finding that no longer exists"), so the list cannot silently rot (a
shallow clone that lacks the commit applies nothing and is not stale). `reviewer` is validated to be `pm`.

## Tests

1. exact fingerprint match waives and emits the Info finding; changing any one of repo, commit, file, rule, line leaves the leak Critical; a planted key on the line next to a waived fingerprint is still caught;
2. a waiver whose commit is absent from the scanned history applies to nothing;
3. an expired entry returns the finding as Medium with "waiver expired";
4. malformed list (short SHA, missing field, `expires` > `reviewed` + 365 days, duplicate fingerprint) fails a unit test;
5. `.gitleaksignore` in the scanned repository still has no effect (existing test).

## First entries (added, ruled by the PM 2026-11-17, `expires` 2027-02-15)

Both from `gitleaks git` over `ThinkersJournal/secretant` main, `generic-api-key`, `CHANGELOG.md`, line 5 (gitleaks fingerprints, read
back from a local scan, entropy 3.7254806 on each):

- `6e944684ca91408834c7f0d8b8d317cd2076e35e:CHANGELOG.md:generic-api-key:5`
- `f1cc76b01a0e9afd3f1d00c95ba3dd3602fff4a1:CHANGELOG.md:generic-api-key:5`

## Tuning of the delegated config: measured, not shipped (2026-11-17)

An additive allowlist on `generic-api-key` (`regexTarget = "secret"`, anchored `^...$` over algorithm names: `XChaCha20-Poly1305`, `AES-256-GCM`,
`SHA-256`...) clears the prose without touching the rule otherwise. Findings before/after over full history (gitleaks 8.30.1, `--redact`, the
production base config vs base + the allowlist): secretant (121 commits) 3/0, coderipper (452) 5/5, lightbulb (863) 15/15; the three removed are the same prose
line (`key, XChaCha20-Poly1305;`: two CHANGELOG commits and a lane-branch HANDOFF commit); a toy repository with a seeded key
`secret_key = "XChaCha20-Poly1305Zq93kLm2Pw8"` and a random `api_key` still reported both. **Not shipped in this change** because it
makes the two waivers above stale (secretant would go `Medium` on the stale check): adopt it as its own change that removes those two entries. The
three-repo sample is small. The regex measured (a TOML literal string, in `[[rules]] id = "generic-api-key"` / `[[rules.allowlists]]`):

    (?i)^(x?chacha\d+-poly\d+|aes-?\d{3}-(gcm|cbc|ctr|ccm|siv)|sha-?(1|224|256|384|512)|sha3-\d+|hmac-sha-?\d+|ed25519|rsa-\d+|ecdsa-p\d+|argon2(id|i|d)|blake[23][a-z0-9]*)$

## Decided (PM, 2026-11-11)

1. `repo` is the forge name from `origin` (`ThinkersJournal/secretant`); `git@` and `https://` remotes are normalised to that shape.
2. Expiry: 90 days by default, hard cap 365; never "none".
3. Reviewer: the PM only, recorded as `reviewer = "pm"` plus the evidence. A lane never waives its own repository; an entry reaches main
   only through the PM gate.

— lane: coderipper
