#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Rewrite the old kebab-case rule names in a project's CodeRipper configuration to their numbered ids.

CodeRipper 0.44.0 renamed its fourteen slug-named rules to numbered ids in their domains (`reachability` is `MOD-011`, ...). The old
names are accepted for one release, with a warning, so nothing breaks the day you upgrade; this tool moves a project over:

    python tools/migrate-rule-ids.py [--dry-run] [PATH ...]

With no PATH it reads `.coderipper.toml` in the current directory. A `.toml` file is rewritten where a name is a rule: an
`[[allow]]` entry's `check = "..."` and the strings of `blocking = [...]`. Any other file (a CI workflow, a script, a README) has
`coderipper ... check <old>` and `--skip-rule <old>=...` rewritten. A section such as `[reachability]` is a settings table, not a rule
name, and is left alone. The tool is idempotent, keeps each file's line endings, and prints one line per change. `--dry-run` prints
them and exits 1 when something would change, 0 when nothing would (use it as a CI gate).
"""
import re
import sys

# old name -> numbered id. tests/catalog.rs checks this table against the catalog's aliases, so it cannot drift.
RENAMES = {
    "unused-return-values": "ERR-012",
    "lint-suppression": "GOV-010",
    "conflict-marker": "GOV-011",
    "reachability": "MOD-011",
    "unused-parameters": "RDB-014",
    "patent-index-match": "SUP-014",
    "dependency-freshness": "SUP-015",
    "licence": "SUP-016",
    "ci-security": "SUP-017",
    "version-consistency": "WSP-010",
    "ci-protection-presence": "WSP-011",
    "version-and-changelog": "WSP-012",
    "patent-sweep-record": "WSP-013",
    "security-support-statement": "WSP-014",
}

NAMES = "|".join(sorted((re.escape(k) for k in RENAMES), key=len, reverse=True))
CHECK_LINE = re.compile(r'^(\s*check\s*=\s*)(["\'])(' + NAMES + r')\2', re.M)
QUOTED = re.compile(r'(["\'])(' + NAMES + r')\1')
BLOCKING = re.compile(r"^(\s*blocking\s*=\s*\[)(.*?)(\])", re.M | re.S)
CLI_CHECK = re.compile(r"(\bcoderipper\b[^\n]*?\bcheck\s+)(" + NAMES + r")(?![\w-])")
CLI_SKIP = re.compile(r"(--skip-rule[ =])(" + NAMES + r")(?=[=\s])")


def migrate_toml(text):
    changes = []

    def check(m):
        changes.append((m.group(3), RENAMES[m.group(3)]))
        return f"{m.group(1)}{m.group(2)}{RENAMES[m.group(3)]}{m.group(2)}"

    text = CHECK_LINE.sub(check, text)

    def blocking(m):
        def one(q):
            changes.append((q.group(2), RENAMES[q.group(2)]))
            return f"{q.group(1)}{RENAMES[q.group(2)]}{q.group(1)}"

        return m.group(1) + QUOTED.sub(one, m.group(2)) + m.group(3)

    return BLOCKING.sub(blocking, text), changes


def migrate_other(text):
    changes = []

    def sub(m):
        changes.append((m.group(2), RENAMES[m.group(2)]))
        return m.group(1) + RENAMES[m.group(2)]

    text = CLI_CHECK.sub(sub, text)
    text = CLI_SKIP.sub(sub, text)
    return text, changes


def main(argv):
    dry = "--dry-run" in argv
    paths = [a for a in argv if not a.startswith("--")] or [".coderipper.toml"]
    changed = 0
    for path in paths:
        try:
            raw = open(path, "rb").read()
        except OSError as e:
            print(f"{path}: cannot read: {e}", file=sys.stderr)
            return 2
        text = raw.decode("utf-8")
        new, changes = (migrate_toml if path.endswith(".toml") else migrate_other)(text)
        for old, num in changes:
            print(f"{path}: {old} -> {num}")
        if changes:
            changed += 1
            if not dry:
                open(path, "wb").write(new.encode("utf-8"))
    if not changed:
        print("nothing to migrate")
    return 1 if dry and changed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
