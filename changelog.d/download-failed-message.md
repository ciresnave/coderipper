kind: Fix
---
**A tool download that fails says so in one line.** `download_failed: <tool> <version> from <url>: <reason>` now ends "(the download failed, it is not a finding in your code: re-run the job)", so a lane reading a red portfolio job that hit a GitHub releases outage does not mistake it for a finding. It is still a hard failure by design; the code `download_failed` the portfolio workflow matches on is unchanged.
