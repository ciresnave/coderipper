# Pass-2 classification vocabulary (CodeRipper triage, per-language cells)

You decide, for ONE language, how each listed rule could be covered by CodeRipper. CodeRipper is an auditor that calls
mature third-party tools as external processes and builds its own check only where no default tool covers a rule. You are
NOT verifying tools (a script does that later); you are proposing, honestly.

## Status: exactly ONE per rule per language

- `implemented-native`  CodeRipper would write the check itself (no mature tool covers it, or it needs CodeRipper's own
                        facts). For a rule the language-neutral module handles, use this with tool `-` and note `neutral module`.
- `delegated`           A named tool checks it; CodeRipper maps the tool's findings to the rule. REQUIRES a tool.
- `not-applicable`      The rule does not apply to this language (REQUIRES a reason).
- `NOT-COVERED`         A known gap: no good tool you are confident of, and not worth writing yet (REQUIRES a reason).

HONESTY RULE: prefer `NOT-COVERED` with a reason over guessing a plausible tool name. Never invent a tool, a lint code or a
flag. If you are not sure a tool exists or does what you say, either use `NOT-COVERED`, or give it with confidence `low`.
A tool must be: actively maintained as far as you know, open-source or free to run as an external process, and machine-readable
(SARIF or JSON output) or at least stable text. Prefer tools that ship with the language's standard toolchain.

## Columns (TAB-separated, UTF-8, LF, first line is the header)

`id	language	status	tool	tool_src	coordinate	tool_rule	output	executes_code	confidence	note`

- `id`: the rule id, as in the input file. One row per input rule (plus the extra `neutral` rows if asked).
- `language`: the language you were given (or `neutral`).
- `tool`: the tool's usual name (`ruff`, `cargo-deny`), or `-`.
- `tool_src`: `kb` if the rule's `kb_tools` field in the input names that tool; `agent` if it comes from your own knowledge; `-`.
- `coordinate`: where a script can look the tool up. EXACTLY one of: `crates:<crate>` | `npm:<package>` | `pypi:<package>` |
  `github:<owner>/<repo>` | `builtin:<name>` (ships with the language toolchain: rustc, clippy, rustfmt, tsc, the Python
  interpreter) | `-`. Use `github:` when the tool is distributed as a binary or you are not sure of the registry name.
- `tool_rule`: the specific lint/rule/flag that evidences the rule (`clippy::cognitive_complexity`, `ruff:F401`,
  `eslint:no-unused-vars`) or `-` if the whole tool is the evidence or you do not know. Do not guess codes.
- `output`: `sarif` | `json` | `text` | `api` | `unknown`.
- `executes_code`: `yes` if running it executes project code, build scripts, config files or plugins (ESLint config is
  JavaScript; cargo builds run build scripts; most type checkers do not); else `no`.
- `confidence`: `high` | `medium` | `low` that the tool really covers the rule well (not just touches it).
- `note`: at most 15 words, your own words; for `not-applicable` and `NOT-COVERED` this is the REASON.

No tabs or newlines inside a field. Do not copy the knowledge base's sentences; your notes are in your own words.
