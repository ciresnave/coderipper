kind: Fix
---
**`zip` 9.4.0 to 9.4.1 in `Cargo.lock`** (a patch release of the archive reader the tool installer uses for the zip-packaged tools: gitleaks, lychee, zizmor, checkov on Windows). Found by the installed 0.39.0 portfolio sweep (`dependency-freshness`, Medium: "zip is locked at 9.4.0 and 9.4.1 is available"). No source change; the tool-install tests (`tools::` archive and download tests) pass on the new version.
