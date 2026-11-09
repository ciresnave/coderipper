kind: Fixed: version-consistency per-interface mode no longer treats 0.0.z as a major release
---
**A project at `0.0.z` no longer asks its `[[interface]]` crates for its number** (found by FireLine: a root at 0.0.13 raised four High findings against interface crates that keep their own versions). `0.0.z` has no major number, so only `X.0.0` and `0.Y.0` count as major releases in the per-interface mode. Fixtures: a root at `0.0.13` with interface crates at their own versions is clean, and a root at `0.4.0` still reports an interface crate behind it. Without `[[interface]]` nothing changes.
