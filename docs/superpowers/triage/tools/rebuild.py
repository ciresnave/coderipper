#!/usr/bin/env python3
"""Rebuild every derived file in this directory from the committed raw agent outputs, in the right order.

  1. make_rules.py          rules.tsv (+ label overrides applied to raw/)
  2. normalise_cells.py     a first pass without tool facts, to learn which tools the final cells name
  3. lookup_tools.py        facts about those tools from public registries (network; the date of the lookup is recorded)
  4. verify_delegations.py  each delegated claim against the tool's own docs or source (network; local rustc/clippy)
  5. normalise_cells.py     the final cells.tsv and normalisation_log.tsv
  6. make_readme.py         README.md (every number computed)

Needs: Python 3, the `gh` CLI logged in (read-only GETs), `cargo` with clippy (for the local lint lists), network. The knowledge base is
NOT needed here (check_provenance.py is the only script that reads it). Run from anywhere:  python tools/rebuild.py
"""
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
TRIAGE = os.path.dirname(HERE)


def run(*args, cwd=TRIAGE):
    print("+", " ".join(args), file=sys.stderr)
    subprocess.run([sys.executable, *args], cwd=cwd, check=True)


def main():
    run("tools/make_rules.py")
    stage = tempfile.mkdtemp(prefix="triage_stage_")
    empty = os.path.join(stage, "empty_tools.tsv")
    with open(empty, "w", encoding="utf-8", newline="") as fh:
        fh.write("coordinate\tfound\tlicence\tlicence_class\tlatest_version\tlatest_release\trepo\trepo_archived\trepo_last_push\t"
                 "maintained\tlicence_file_head\tlicence_file_flag\n")
    run("tools/normalise_cells.py", "raw", stage, empty, "spotcheck_overrides.tsv")
    run("tools/lookup_tools.py", ".", os.path.join(stage, "cells.tsv"))
    run("tools/verify_delegations.py", "verification_delegated.tsv", os.path.join(stage, "cells.tsv"))
    run("tools/normalise_cells.py", "raw", ".", "tools.tsv", "spotcheck_overrides.tsv", "verification_delegated.tsv")
    run("tools/make_readme.py")
    shutil.rmtree(stage, ignore_errors=True)


if __name__ == "__main__":
    main()
