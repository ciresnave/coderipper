#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Stamp a version onto a change: the one step that touches the files every parallel lane would otherwise collide on.

    python tools/stamp-version.py <version> [--date YYYY-MM-DD] [--root DIR] [--dry-run]

A lane's branch edits only its own files plus fragments:

    changelog.d/<name>.md          the CHANGELOG entry, without a heading
    patent-sweep.d/<name>.md       the patent-sweep rows and addendum, with placeholders for the numbers

When the PM gives the lane its version number, the lane runs this script. It

 1. sets the version in Cargo.toml, Cargo.lock and (when the minor changes) the README install snippet;
 2. turns the changelog fragments into one `## <version> - <date> (proposed, <kind>)` entry above the newest one;
 3. numbers the patent-sweep fragments from what the file already holds (hits, queries, ideas), inserts the rows after the last hit
    row and appends the addendum at the end of docs/PATENT-SWEEP.md;
 4. deletes the fragments it used.

Formats are in changelog.d/README.md and patent-sweep.d/README.md. Standard library only. Line endings of each file are kept.
"""
import argparse
import datetime
import os
import re
import sys


def read(path):
    with open(path, "rb") as f:
        raw = f.read().decode("utf-8")
    return raw.replace("\r\n", "\n"), "\r\n" in raw


def write(path, text, crlf):
    with open(path, "wb") as f:
        f.write((text.replace("\n", "\r\n") if crlf else text).encode("utf-8"))


def fragments(directory):
    if not os.path.isdir(directory):
        return []
    return [
        os.path.join(directory, n)
        for n in sorted(os.listdir(directory))
        if n.endswith(".md") and n.lower() != "readme.md"
    ]


def die(msg):
    print(f"stamp-version: {msg}", file=sys.stderr)
    sys.exit(2)


def set_cargo_version(root, old, new, dry):
    changes = []
    p = os.path.join(root, "Cargo.toml")
    t, crlf = read(p)
    m = re.search(r'^version = "' + re.escape(old) + r'"', t, re.M)
    if not m:
        die(f"Cargo.toml has no `version = \"{old}\"` line")
    t2 = t[: m.start()] + f'version = "{new}"' + t[m.end():]
    changes.append(("Cargo.toml", p, t2, crlf))
    p = os.path.join(root, "Cargo.lock")
    if os.path.exists(p):
        t, crlf = read(p)
        pat = re.compile(r'(name = "coderipper"\nversion = ")' + re.escape(old) + '"')
        if not pat.search(t):
            die("Cargo.lock has no coderipper package at the old version")
        changes.append(("Cargo.lock", p, pat.sub(lambda m: m.group(1) + new + '"', t, count=1), crlf))
    old_minor = ".".join(old.split(".")[:2])
    new_minor = ".".join(new.split(".")[:2])
    p = os.path.join(root, "README.md")
    if old_minor != new_minor and os.path.exists(p):
        t, crlf = read(p)
        pat = re.compile(r'(coderipper = \{ version = ")' + re.escape(old_minor) + '"')
        if not pat.search(t):
            die(f"README.md has no install snippet at {old_minor}")
        changes.append(("README.md", p, pat.sub(lambda m: m.group(1) + new_minor + '"', t, count=1), crlf))
    return changes


def parse_changelog_fragment(path):
    text, _ = read(path)
    head, sep, body = text.partition("\n---\n")
    if not sep or not head.startswith("kind:"):
        die(f"{path}: expected a first line `kind: ...`, a line `---`, then the entry")
    return head[len("kind:"):].strip(), body.strip("\n")


def stamp_changelog(root, version, date, frags):
    if not frags:
        die("no changelog.d/*.md fragment: every pushed change needs a CHANGELOG entry")
    kinds, bodies = [], []
    for f in frags:
        k, b = parse_changelog_fragment(f)
        kinds.append(k)
        bodies.append(b)
    kind = next((k for k in kinds if k.lower().startswith("breaking")), kinds[0])
    entry = f"## {version} - {date} (proposed, {kind})\n\n" + "\n\n".join(bodies) + "\n\n"
    p = os.path.join(root, "CHANGELOG.md")
    t, crlf = read(p)
    i = t.find("\n## 0.")
    if i < 0:
        die("CHANGELOG.md has no version heading to put the entry above")
    return ("CHANGELOG.md", p, t[: i + 1] + entry + t[i + 1:], crlf)


def next_numbers(sweep):
    hits = [int(m.group(1)) for m in re.finditer(r"^\| (\d+) \| [A-Z]{2,3}[0-9A-Z]", sweep, re.M)]
    queries = [int(m.group(1)) for m in re.finditer(r"^\| Q(\d+) \|", sweep, re.M)]
    ideas = [int(m.group(1)) for m in re.finditer(r"\*\*Idea (\d+)", sweep)]
    ideas += [int(m.group(1)) for m in re.finditer(r"^(\d+)\. \*\*", sweep, re.M)]
    return max(hits, default=0), max(queries, default=0), max(ideas, default=0)


def fill(text, version, date, hit, q, idea):
    line = ".".join(version.split(".")[:2])

    def num(prefix, base):
        def sub(m):
            return str(base + int(m.group(1)))
        return sub

    text = re.sub(r"\{\{HIT(\d+)\}\}", num("HIT", hit), text)
    text = re.sub(r"\{\{Q(\d+)\}\}", num("Q", q), text)
    text = re.sub(r"\{\{IDEA(\d+)\}\}", num("IDEA", idea), text)
    return text.replace("{{VERSION}}", version).replace("{{LINE}}", line).replace("{{DATE}}", date)


def stamp_sweep(root, version, date, frags):
    if not frags:
        return None
    p = os.path.join(root, "docs", "PATENT-SWEEP.md")
    t, crlf = read(p)
    hit, q, idea = next_numbers(t)
    rows, addenda = [], []
    for f in frags:
        text, _ = read(f)
        r = re.search(r"<!-- rows -->\n(.*?)(?=<!-- addendum -->|\Z)", text, re.S)
        a = re.search(r"<!-- addendum -->\n(.*)\Z", text, re.S)
        if not r and not a:
            die(f"{f}: expected a `<!-- rows -->` section, an `<!-- addendum -->` section, or both")
        if r:
            rows.append(fill(r.group(1).strip("\n"), version, date, hit, q, idea))
        if a:
            addenda.append(fill(a.group(1).strip("\n"), version, date, hit, q, idea))
        # the next fragment of the same stamp continues the numbers
        hit += max([int(m) for m in re.findall(r"\{\{HIT(\d+)\}\}", text)], default=0)
        q += max([int(m) for m in re.findall(r"\{\{Q(\d+)\}\}", text)], default=0)
        idea += max([int(m) for m in re.findall(r"\{\{IDEA(\d+)\}\}", text)], default=0)
    if rows:
        lines = t.split("\n")
        new = [r for blk in rows for r in blk.split("\n") if r.strip()]
        for is_query, pat, what in (
            (False, r"^\| \d+ \| [A-Z]{2,3}[0-9A-Z]", "hit"),
            (True, r"^\| Q\d+ \|", "query"),
        ):
            mine = [r for r in new if bool(re.match(r"^\| Q\d+ \|", r)) == is_query]
            if not mine:
                continue
            last = max((n for n, l in enumerate(lines) if re.match(pat, l)), default=None)
            if last is None:
                die(f"docs/PATENT-SWEEP.md has no {what} row to insert after")
            lines[last + 1:last + 1] = mine
        t = "\n".join(lines)
    if addenda:
        t = t.rstrip("\n") + "\n\n" + "\n\n".join(addenda) + "\n"
    return ("docs/PATENT-SWEEP.md", p, t, crlf)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("version")
    ap.add_argument("--date", default=datetime.date.today().isoformat())
    ap.add_argument("--root", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
    ap.add_argument("--dry-run", action="store_true")
    a = ap.parse_args()
    if not re.fullmatch(r"\d+\.\d+\.\d+", a.version):
        die(f"`{a.version}` is not MAJOR.MINOR.PATCH")
    root = os.path.abspath(a.root)
    cargo, _ = read(os.path.join(root, "Cargo.toml"))
    m = re.search(r'^version = "(\d+\.\d+\.\d+)"', cargo, re.M)
    if not m:
        die("Cargo.toml has no version line")
    old = m.group(1)
    if old == a.version:
        die(f"Cargo.toml already says {old}")
    cl_frags = fragments(os.path.join(root, "changelog.d"))
    ps_frags = fragments(os.path.join(root, "patent-sweep.d"))
    changes = set_cargo_version(root, old, a.version, a.dry_run)
    changes.append(stamp_changelog(root, a.version, a.date, cl_frags))
    sweep = stamp_sweep(root, a.version, a.date, ps_frags)
    if sweep:
        changes.append(sweep)
    for name, _, _, _ in changes:
        print(("would change " if a.dry_run else "changed ") + name)
    if a.dry_run:
        print(f"{old} -> {a.version}; {len(cl_frags)} changelog and {len(ps_frags)} patent-sweep fragment(s)")
        return
    for _, path, text, crlf in changes:
        write(path, text, crlf)
    for f in cl_frags + ps_frags:
        os.remove(f)
    print(f"stamped {old} -> {a.version}; removed {len(cl_frags) + len(ps_frags)} fragment(s)")


if __name__ == "__main__":
    main()
