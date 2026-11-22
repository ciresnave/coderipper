kind: Fix
---
**API-004 and OPS-010 claims audit:** the API-004 finding read "carries no the version it began in (`since`) and no a note"; it now reads "lacks the version it began in (`since`) and a note (...)". OPS-010 has always skipped binaries under fixtures, testdata, measure, xtask and bench directories; its catalog text now says so. Behaviour unchanged, both pinned by tests.
