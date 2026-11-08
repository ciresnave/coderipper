kind: Fix
---
**A pinned tool download is tried three times.** A 5xx answer, a failed connection, a timeout or an unresolvable host is retried after 5 s and then 20 s (each attempt is logged on stderr); a 4xx, a bad URL or a download that keeps failing still fails as before, with the last error and the same `download_failed` code. The retry is inside the fetch, so a file whose hash does not match the lock is never retried: `checksum_mismatch` fails at once after a single fetch. Why: GitHub releases answered 504 for buf, checkov and lychee and failed a whole CI run each time.
