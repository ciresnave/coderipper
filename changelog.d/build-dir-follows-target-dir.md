kind: Fix
---
**A cargo `build.build-dir` setting no longer pulls a build out of the build cache.** With Cargo 1.100 a user's own config can keep a build's intermediate files in a separate directory; the cache slot, its lock and the delegated `cargo semver-checks` scratch directory assumed they were written inside `CARGO_TARGET_DIR`. CodeRipper now sets `CARGO_BUILD_BUILD_DIR` to the same directory wherever it sets `CARGO_TARGET_DIR`. Five lib tests that failed on a machine with such a setting pass.
