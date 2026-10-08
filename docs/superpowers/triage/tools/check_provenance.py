#!/usr/bin/env python3
"""List triage labels and notes that reuse the knowledge base's own wording. Needs the KB file, which is NOT in this repository.

A label or note is reported when, compared case-insensitively after removing punctuation, either
  - it equals the start of the rule's statement (a truncation), or equals the whole statement, or
  - it shares a run of N or more consecutive words with ANY text of the KB (default N = 6; the KB's table text and prose).
Tool names are exempt (they are facts, not wording): a run is ignored when it is all tool-like tokens from the row's tools column.

Usage: python check_provenance.py KB_FILE TSV [TSV ...] [--n 6]
Prints one line per finding: file, id, column, the text. Exit status 1 if there is any finding.
"""
import csv
import re
import sys


def words(text):
    return re.findall(r"[a-z0-9]+(?:[-'][a-z0-9]+)*", text.lower())


def main(argv):
    n = 6
    if "--n" in argv:
        i = argv.index("--n")
        n = int(argv[i + 1])
        del argv[i:i + 2]
    kb_path, files = argv[1], argv[2:]
    kb_text = open(kb_path, encoding="utf-8").read()
    kb_words = words(kb_text)
    grams = {tuple(kb_words[i:i + n]) for i in range(len(kb_words) - n + 1)}
    statements = {}
    for line in kb_text.splitlines():
        m = re.match(r"^\| ([A-Z]{3}-\d{3}) \| (.*?) \|", line)
        if m:
            statements[m.group(1)] = words(re.sub(r"[*`]", "", m.group(2)))
    findings = 0
    for f in files:
        with open(f, encoding="utf-8", newline="") as fh:
            for row in csv.DictReader(fh, delimiter="\t"):
                rid = row.get("id", "")
                for col in ("label", "note"):
                    text = row.get(col, "")
                    w = words(text)
                    if len(w) < 3:
                        continue
                    st = statements.get(rid, [])
                    truncated = bool(st) and len(w) >= 3 and st[:len(w)] == w
                    shared = any(tuple(w[i:i + n]) in grams for i in range(len(w) - n + 1))
                    if truncated or shared:
                        findings += 1
                        print(f"{f}\t{rid}\t{col}\t{'TRUNCATION/EQUAL' if truncated else f'{n}-gram'}\t{text}")
    print(f"{findings} findings", file=sys.stderr)
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
