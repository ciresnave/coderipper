# Configuration layers: a user/system `.coderipper.toml`, a restricted repo file, environment variables, flags (design)

> **Status: built in 0.6.21 (proposed), except what section 9 and the CHANGELOG list as not built** (the run-time stop at a limit, the
> dotless spelling in `/etc/coderipper/`, profile pinning). Written 2026-10-13 from a conversation with CireSnave the same day;
> sections 3 to 8 are what was built, and `README.md` ("Configuration layers") is the user's description. **Open items are in section 9.**

## 1. What exists now (`origin/main` at `a1e8732`, read from the source, not by running the binary)

- **`.coderipper.toml` at a repository root** (`src/allowlist.rs`, `deny_unknown_fields`): `[[allow]]` (waivers: `check`, `file`,
  `reason`, optional `symbol` and `lines`), `[[tracks]]` (a package that follows another project's version) and `[buf]` (`baseline`).
  It is a *policy file about the repository*, not a settings file for CodeRipper. Under `--workspace` each member's own file applies.
- **Flags** (`clap`): `--deny`, `--profile`, `--project`, `--workspace`, `--redact`, `--install-tools` and others.
- **Environment variables**, all about the build cache and the tool cache: `CODERIPPER_CACHE`, `CODERIPPER_CACHE_DIR`,
  `CODERIPPER_CACHE_MAX_GB`, `CODERIPPER_CACHE_WAIT_SECS`, `CODERIPPER_TOOLS_DIR`.
- The redaction policy is a process-wide setting set by `--redact` or `redact::set_policy`.
- **Nothing** reads a file outside the repository, and nothing lets a user set defaults once for all repositories.

## 2. What CireSnave asked for (2026-10-13)

1. A user-level file holding defaults for every command-line option that is a default, and for what the environment variables set,
   read **before** the repository's file, so the repository can override it; environment variables read after the files so they win;
   **flags last, so they override everything.**
2. **The repository's file is an allowlist: only keys explicitly marked repo-settable are honoured, all others are blocked.** Fail
   closed.
3. **One file name, searched in three places, in order:** `/etc/coderipper/`, `/etc/`, `~/`. (An "operator file" and a "user file"
   are the same thing in a different directory, not two formats.)
4. **`[[allow]]` and `[[tracks]]` may appear in the user-wide file.** CireSnave's words: *"it's actually wrong. The settings should be
   in each repo. However, it's quick and convenient and we shouldn't force users to do things the right way."* The correct response to
   a wrong-but-convenient use is **a warning, then proceed** (section 6).
5. **A repository's file can never override a limit.** Enforcement of limits is **outside the container** the code and CodeRipper
   run in (section 7).

## 3. The files and how they combine

**Search.** CodeRipper looks for a file named `.coderipper.toml` in, in this order, **`/etc/coderipper/`**, **`/etc/`**, **the
user's home directory**. (On Windows `/etc` has no meaning: the equivalents are `%ProgramData%\coderipper\` and
`%USERPROFILE%`; a Windows-only machine-wide location is `%ProgramData%\.coderipper.toml`.) **Every file found is read and
combined; the first one found does not hide the rest.** Later files are stronger, so a user's file overrides `/etc`. Then the
repository's file, then the environment, then flags.

| # | Layer | Strength | May set |
|---|---|---|---|
| 1 | Built-in defaults | weakest | - |
| 2 | `/etc/coderipper/.coderipper.toml` | | everything |
| 3 | `/etc/.coderipper.toml` | | everything |
| 4 | `~/.coderipper.toml` | | everything except that a **limit** can only be lowered (section 5) |
| 5 | the repository's `.coderipper.toml` (per member under `--workspace`) | | **only the keys in section 4.2**; others warn and are skipped |
| 6 | environment variables | | settings (section 4.1), and limits only downward |
| 7 | command-line flags | strongest | per-invocation values and defaults |

**Two switches for reproducibility** (the same repository must give the same answer on CI and on a laptop, or the report must say
why not): `--config <path>` / `CODERIPPER_CONFIG` reads exactly that one file *in place of* layers 2 to 4; `--no-config` /
`CODERIPPER_NO_CONFIG` reads none of them. Neither affects layer 5.

**The report says what was read.** One note at the head of the run lists each file read and each key skipped, for example:
`configuration: /etc/coderipper/.coderipper.toml, ~/.coderipper.toml, ./.coderipper.toml (2 keys skipped: see warnings)`. A result is
never produced from settings the reader cannot see.

## 4. What may be set where

### 4.1 Keys by kind

| Kind | Keys | Notes |
|---|---|---|
| **Defaults for flags** (key = flag name without `--`) | `profile`, `deny`, `redact`, `install-tools`, and each other flag that is a *default* | Per-invocation values (`project`, the member selection of `--workspace`, subcommands, paths named on the command line) are **not** settable: they have no meaning as a default. A table of every flag is part of the build of this. |
| **What the environment sets today** | `[cache] enabled, dir, max_gb, wait_secs`; `[tools] dir` | Each keeps its env var, which wins over the file. |
| **Limits** | `[limits] max_runtime_minutes`, `max_memory_mb`, `max_disk_mb`, `max_processes`, ... | **Ceilings**: section 5. Run-time default 30 minutes. |
| **Policy about a repository** | `[[allow]]`, `[[tracks]]`, `[buf]` | Repository file; also the wider files, with a warning (section 6). |
| **Hosted-service package settings** | queue step, rendered terms values, retention | Out of scope here: they belong to the hosted-service package, which reads this mechanism. |

### 4.2 What the repository's file may set: an allowlist

**v1 of the allowlist is exactly:** `[[allow]]`, `[[tracks]]`, `[buf]`, and `profile`. Everything else, **known or not**, is not
honoured from a repository's file:

- A **known key that is not on the allowlist** (for example `tools.dir`, `cache.*`, `redact`, `install-tools`, `deny`, anything under
  `[limits]`): a **warning that names the key and the file, and the key is skipped.** The run goes on.
- An **unknown key** (a typo, or a key from a newer version): a **hard error**, as today (`deny_unknown_fields`). A misspelt waiver must
  not be silently ignored.
- A key on the allowlist with a wrong type or value: a hard error.

Why an allowlist and not a list of forbidden keys: the repository is untrusted input in the hosted service. A forbidden-keys list
fails open for every key added later; an allowlist fails closed. The cases it keeps out, each a real route: `tools.dir` (or
`cache.dir`) pointing inside the repository, so CodeRipper runs a repository's own binary; `install-tools = true` (a repository
granting itself downloads and network); `redact = off` (a repository removing the redaction of its own output); `deny = none`; and a
limit raised.

Adding a key to the allowlist is a deliberate act and a CHANGELOG line. `profile` is on it because a profile is a statement about
the project's own rule set and the report names the profile that ran; `deny` is **not**, in v1, because a project's CI already
sets it with a flag and it can be added later without harm, whereas a key wrongly added cannot be taken back.

## 5. Limits are ceilings, combined by minimum

For every key under `[limits]` the **effective value is the minimum of the values every layer gave** (an unset layer gives
nothing). So `/etc/coderipper/.coderipper.toml` can say `max_runtime_minutes = 30` and no later file, no environment variable and
no flag can raise it; a user may lower it. A repository's file cannot set it at all (section 4.2).

- This is the only place a later layer cannot override an earlier one, and the rule is stated once, in the config reference.
- A flag that asks for a larger value than the ceiling gets the ceiling, and the run notes `max_runtime_minutes: 120 requested, 30
  allowed by /etc/coderipper/.coderipper.toml`.

## 6. `[[allow]]` and `[[tracks]]` in a user-wide file: a warning, then proceed

This is permitted, because it is convenient and CireSnave uses it, and because forcing the right way helps nobody. It is also
**the wrong place**, and the design says so every run:

- **One warning per run** (a note on the result, and on stderr), naming the file and the number of entries: `~/.coderipper.toml holds
  3 [[allow]] entries; waivers belong in each repository's .coderipper.toml, where they are reviewed with the code. Applied
  anyway.` The warning does not change the exit code.
- A wide waiver is **applied** the way a repository's is: the finding is still printed, with its `reason`, **and marked with the file
  it came from** (`waived by ~/.coderipper.toml`), so a reader of a result can tell a waiver nobody in the repository reviewed.
- **A wide entry that matches nothing in this repository is not a stale entry.** The stale-entry finding (a waiver that no longer
  matches) is for the repository's own file. Otherwise one waiver for one project would raise a finding in every other.
- **`file` is relative to the repository root,** as always; a `[[tracks]]` `manifest` likewise. An entry that names a file only one
  repository has simply never matches elsewhere.
- **In the hosted service the wide files are the operator's.** The job's home directory is empty (a fresh container), so a wide
  waiver is honoured only from `/etc/coderipper/` or `/etc/`, which the operator controls. That is a consequence of where the files
  live, not a special case in the code.

The reproducibility cost is the reason for the warning: a waiver that exists only on one machine means the same repository passes
there and fails on CI. `--no-config` is how a person checks it.

## 7. Enforcement is outside the container

CodeRipper and the code it checks run **in the same container** (hosted service v1). A limit CodeRipper reads from a file is
therefore a **request**: hostile code in the job can edit its own home directory, or simply ignore the value. So:

- The configuration states what the operator wants (`max_runtime_minutes = 30`, memory, disk, processes). **The orchestrator
  enforces it from outside**: the container's own memory and disk caps (cgroups, a size-limited writable layer), a timeout the
  orchestrator holds and a kill that does not depend on anything inside the job, a process-count limit, and egress controlled on
  the host.
- The operator's files live **outside the job's reach**: `/etc/coderipper/` of the host or an image layer the job cannot rewrite,
  and not the job's `~`. Where a container's `/etc` is writable by the job, CodeRipper's read of it is advisory and the orchestrator's
  limits are the ones that hold.
- CodeRipper's own checks of the limits (stopping itself at the run-time limit and reporting a gap that names the cap, as the
  hosted-service design already says) are a **courtesy that produces a good report**, not a security boundary.

## 8. Building it

1. A `config` module: parse each file into a typed struct with `deny_unknown_fields`, keep `Source` (path) on every value, combine
   in the order of section 3, return the effective settings **and** the list of files read and keys skipped. The existing allowlist
   reader moves behind it; `.coderipper.toml`'s current keys keep their meaning, so **no existing repository file changes**.
2. The CLI reads it before parsing flag defaults; flags are applied last. The environment variables keep working and keep winning
   over files.
3. Tests, each mutation-checked with the mutation confirmed to apply: a repository file setting `tools.dir` (skipped with a
   warning, and the tool cache stays where the user's file put it); the same for `redact`, `install-tools`, `deny`, a limit; an
   unknown key (error); `[limits]` minimum across `/etc`, `~`, env and flag; a wide `[[allow]]` (applied, warned, marked, not
   stale); `--no-config`; `--config`; and a hostile fixture whose `.coderipper.toml` tries to point `tools.dir` at its own
   directory and `install-tools` at true, which must change nothing.
4. Documentation: the README gains a configuration section; `docs/rules` is untouched.
5. **Semver:** additive (new files, new keys, new flags `--config`, `--no-config`). A minor change of the 0.n line; the PM
   allocates the number. Nothing is renamed or removed.

## 9. Open items

- **The flag table.** Which flags are "defaults" (and therefore keyed) needs the full flag list from `--help`; this design was
  written from the source and the README, not by running the binary.
- **`/etc/.coderipper.toml` as a name.** A dotfile in `/etc` is unusual and some tools ignore dotfiles; CireSnave named it this way
  and the design follows. A second spelling without the dot in `/etc/coderipper/` (`coderipper.toml`) would be harmless to add.
- **Does the repo file get `deny`?** Proposed no for v1 (section 4.2). A project that wants a stricter gate already passes a flag.
- **A per-member file's allowlist** is the same as the root's; a member's file cannot set anything a root's cannot.
- **Pinning.** An operator may want to *pin* `profile` against a repository's choice. v1 has no pin; the allowlist's keys are
  few, and the report names the profile that ran. Revisit when the hosted service has a real operator.
- **Wide waivers and a shared machine.** A user's `~/.coderipper.toml` affects every checkout of theirs, including ones
  they did not write; the per-run warning and the "waived by" mark are the only guard. If that proves too weak, `--no-config` in
  CI is the answer, not a prohibition.
