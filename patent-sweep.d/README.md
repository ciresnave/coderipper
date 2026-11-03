# patent-sweep.d: PATENT-SWEEP fragments

`docs/PATENT-SWEEP.md` numbers its hits, queries and ideas globally, so two lanes adding rows at once would both claim the next
number. Write the rows with placeholders, `patent-sweep.d/<rule>.md`:

    <!-- rows -->
    | {{HIT1}} | US... | ... |
    | Q{{Q1}} | ... |
    <!-- addendum -->
    ### {{VERSION}} ({{LINE}} line) - {{DATE}}
    ... idea {{IDEA1}} ...

- `{{HIT1}}`, `{{HIT2}}`..: hit numbers; `{{Q1}}`..: query numbers; `{{IDEA1}}`..: idea numbers. The script adds the file's current
  maximum to each, so fragments of one stamp continue each other's numbering.
- `{{VERSION}}`, `{{LINE}}` (`0.33`), `{{DATE}}`: filled from the stamp.
- Either section may be omitted. Rows go after the last hit row; the addendum goes at the end of the file.
- The sweep itself (queries, claims read with `tools/patent-claims.py`, element-by-element charts, watches) is still the lane's work;
  `patent-sweep-record` fails the build if a version line has no addendum.
