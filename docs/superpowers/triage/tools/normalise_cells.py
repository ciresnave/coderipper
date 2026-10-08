#!/usr/bin/env python3
"""Normalise the per-language triage cells that three independent agents filled in, and join the pieces.

Why this exists: the three language agents and the neutral-column agent each decided, alone, how to treat the 24 rules that
need no parser (class N). They disagreed (one delegated to gitleaks where another wrote "neutral module"). The rules below make
the cells consistent. They are applied mechanically and listed in the output, so nothing is changed silently.

  R1  A cell that is not `delegated` has `output` = `-` (the agents' vocabulary had no "none" value).
  R2  For a class-N rule whose neutral cell is `delegated` to tool T: a language cell that is `native` or that delegates to the
      SAME tool T becomes `implemented-native`, tool `-`, note "via the neutral module, which delegates to T" (one owner per
      language-independent check).
  R3  For a class-N rule whose neutral cell is `NOT-COVERED` (no language-independent form), a language cell that says
      "neutral module" has that note replaced by "ecosystem-specific manifest parsing in the language module"; its status stands.
  R4  A language cell that delegates to a tool DIFFERENT from the neutral cell's tool stays as it is (an ecosystem-specific tool).
  R2b For a class-N rule whose neutral cell is `implemented-native`, a language cell that is `implemented-native` gets the note
      "via the neutral module" (the neutral module owns it for every language).
  R5  Corrections from the spot-check and the independent audit (spotcheck_overrides.tsv: id, language, status, note, reason and an
      optional confidence) replace a cell's status, note and confidence. Each is logged with its reason.
  R6  `builtin:tsc` is not a language built-in (it ships in the npm package `typescript`): the cell's coordinate becomes
      `npm:typescript`.

Each output cell also gets an `owner`, decided from the data and never from the free-text note: `via-neutral` (the language-neutral
module owns the check for every language), `language-native` (the language module would have to write it), `tool` (delegated),
or `-` (not applicable, or a gap).

Also joins, per delegated cell, the strength of its verification (verification_delegated.tsv): `strong` = a specific lint, code or
flag was found in the tool's own docs or source; `weak` = only a keyword in its README; `-` = not a delegated cell.

Usage: python normalise_cells.py RAW_DIR OUT_DIR TOOLS_TSV [OVERRIDES_TSV VERIFICATION_TSV]
(licence_notes.tsv, if present beside TOOLS_TSV, adds a `licence_note` per tool.)
Writes OUT_DIR/cells.tsv (297 rows) and OUT_DIR/normalisation_log.tsv. Standard library only.
"""
import csv
import sys

LANGS = ["rust", "typescript", "python"]


def load(path):
    with open(path, encoding="utf-8", newline="") as fh:
        return list(csv.DictReader(fh, delimiter="\t"))


def main(raw, out, tools_tsv, overrides_tsv=None, verification_tsv=None):
    inp = {r["id"]: r for r in load(f"{raw}/pass2_input.tsv")}
    neutral = {r["id"]: r for r in load(f"{raw}/pass2_neutral.tsv")}
    cells = {lang: {r["id"]: r for r in load(f"{raw}/pass2_{lang}.tsv")} for lang in LANGS}
    log = []

    def note(rule, lang, why):
        log.append({"id": rule, "language": lang, "change": why})

    for lang in LANGS:
        for rid, c in cells[lang].items():
            if c["status"] != "delegated" and c["output"] != "-":
                c["output"] = "-"  # R1 (not logged per cell: it applies to every non-delegated row)
            if rid not in neutral:
                continue
            n = neutral[rid]
            if n["status"] == "delegated":
                same_tool = c["status"] == "delegated" and c["tool"] == n["tool"]
                native_via_neutral = c["status"] == "implemented-native"
                if same_tool or native_via_neutral:
                    before = f'{c["status"]}:{c["tool"]}'
                    c.update(status="implemented-native", tool="-", tool_src="-", coordinate="-", tool_rule="-", output="-",
                             note=f'via the neutral module, which delegates to {n["tool"]}')
                    note(rid, lang, f"R2 {before} -> implemented-native via neutral ({n['tool']})")
                elif c["status"] == "delegated":
                    note(rid, lang, f'R4 kept: ecosystem-specific {c["tool"]} (neutral uses {n["tool"]})')
            elif n["status"] == "implemented-native" and c["status"] == "implemented-native":
                c["note"] = "via the neutral module"  # R2b
                note(rid, lang, "R2b note: native via the neutral module's native check")
            elif n["status"] == "NOT-COVERED" and "neutral module" in c["note"]:
                c["note"] = "ecosystem-specific manifest parsing in the language module"
                note(rid, lang, "R3 note replaced: neutral cell is NOT-COVERED")

    if overrides_tsv:
        for o in load(overrides_tsv):
            c = cells[o["language"]][o["id"]]
            before = c["status"]
            c["status"], c["note"] = o["status"], o["note"]
            if o.get("confidence"):
                c["confidence"] = o["confidence"]
            if o["status"] != "delegated":
                c.update(tool="-", tool_src="-", coordinate="-", tool_rule="-", output="-")
            note(o["id"], o["language"], f"R5 {before} -> {o['status']}: {o['reason']}")

    for lang in LANGS:
        for rid, c in cells[lang].items():
            if c["coordinate"] == "builtin:tsc":
                c["coordinate"] = "npm:typescript"  # R6
                note(rid, lang, "R6 coordinate builtin:tsc -> npm:typescript")

    strength = {}
    if verification_tsv:
        for v in load(verification_tsv):
            key = (v["language"], v["id"])
            strength[key] = "strong" if (v["strength"].startswith("strong") or strength.get(key) == "strong") else "weak"
            if v["result"] != "confirmed":
                strength[key] = "UNCONFIRMED"
    import os
    notes_path = os.path.join(os.path.dirname(os.path.abspath(tools_tsv)), "licence_notes.tsv")
    lic_notes = {r["coordinate"]: r["note"] for r in load(notes_path)} if os.path.exists(notes_path) else {}
    tools = {r["coordinate"]: r for r in load(tools_tsv)}
    cols = ["id", "domain", "label", "evidence_tiers", "class", "language", "status", "tool", "tool_src", "coordinate", "tool_rule",
            "output", "executes_code", "confidence", "note", "owner", "licence", "licence_file_flag", "licence_note", "latest_version", "latest_release", "maintained", "verified"]
    rows = []
    for rid, r in inp.items():
        for lang in ["neutral"] + LANGS:
            if lang == "neutral":
                c = neutral.get(rid)
                if c is None:
                    continue
            else:
                c = cells[lang][rid]
            t = tools.get(c["coordinate"], {})
            neutral_row = neutral.get(rid)
            if c["status"] == "delegated":
                owner = "tool"
            elif c["status"] == "implemented-native":
                via = lang != "neutral" and neutral_row is not None and neutral_row["status"] != "NOT-COVERED"
                owner = "via-neutral" if via else ("neutral-native" if lang == "neutral" else "language-native")
            else:
                owner = "-"
            rows.append({
                "id": rid, "domain": r["domain"], "label": r["label"], "evidence_tiers": r["evidence_tiers"],
                "class": r["class"], "language": lang, "status": c["status"], "tool": c["tool"], "tool_src": c["tool_src"],
                "coordinate": c["coordinate"], "tool_rule": c["tool_rule"], "output": c["output"],
                "executes_code": c["executes_code"], "confidence": c["confidence"], "note": c["note"],
                "owner": owner, "licence": t.get("licence", ""), "licence_file_flag": t.get("licence_file_flag", ""),
                "licence_note": lic_notes.get(c["coordinate"], ""),
                "latest_version": t.get("latest_version", ""),
                "latest_release": t.get("latest_release", ""), "maintained": t.get("maintained", ""),
                "verified": strength.get((lang, rid), "-") if c["status"] == "delegated" else "-",
            })
    with open(f"{out}/cells.tsv", "w", encoding="utf-8", newline="") as fh:
        w = csv.DictWriter(fh, fieldnames=cols, delimiter="\t", lineterminator="\n")
        w.writeheader()
        w.writerows(rows)
    with open(f"{out}/normalisation_log.tsv", "w", encoding="utf-8", newline="") as fh:
        w = csv.DictWriter(fh, fieldnames=["id", "language", "change"], delimiter="\t", lineterminator="\n")
        w.writeheader()
        w.writerows(log)
    print(f"{len(rows)} cells, {len(log)} logged changes")


if __name__ == "__main__":
    main(*sys.argv[1:6])
