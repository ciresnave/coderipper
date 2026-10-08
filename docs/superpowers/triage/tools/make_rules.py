#!/usr/bin/env python3
"""Build rules.tsv from the pass-1 outputs, applying labels_override.tsv, and mark which rules have per-language cells.

The overrides replace labels (and one note) that repeated the knowledge base's wording. They are applied to the committed copies in
raw/ too, so no verbatim label is shipped anywhere. Idempotent. Run from the triage directory.
"""
import csv


def load(p):
    with open(p, encoding="utf-8", newline="") as fh:
        return list(csv.reader(fh, delimiter="\t"))


def save(p, rows):
    with open(p, "w", encoding="utf-8", newline="") as fh:
        csv.writer(fh, delimiter="\t", lineterminator="\n").writerows(rows)


over = {(r[0], r[1]): r[2] for r in load("labels_override.tsv")[1:]}
for path in ["raw/pass1_G1.tsv", "raw/pass1_G2.tsv", "raw/pass1_G3.tsv", "raw/pass1_G4.tsv", "raw/pass1_G5.tsv",
             "raw/pass1_all.tsv", "raw/pass2_input.tsv"]:
    rows = load(path)
    head = rows[0]
    changed = 0
    for r in rows[1:]:
        for col in ("label", "note"):
            if col in head and (r[0], col) in over and r[head.index(col)] != over[(r[0], col)]:
                r[head.index(col)] = over[(r[0], col)]
                changed += 1
    save(path, rows)
    print(path, changed, "overrides applied")

rules = load("raw/pass1_all.tsv")
head = rules[0]
cells = {r[0] for r in load("raw/pass2_input.tsv")[1:]}
out = [head + ["per_language_triage"]]
for r in rules[1:]:
    cls = r[head.index("class")]
    done = "done (rust, typescript, python" + (", neutral" if cls == "N" else "") + ")"
    out.append(r + [done if r[0] in cells else f"deferred (class {cls})"])
save("rules.tsv", out)
print(len(out) - 1, "rules written")
