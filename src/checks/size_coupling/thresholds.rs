// SPDX-License-Identifier: MIT OR Apache-2.0
//! The figures `MOD-005` compares with, and where they came from.

/// Thresholds and the sentence a finding uses to say where they came from.
pub(crate) struct Thresholds {
    /// Code lines at which a file is reported at Info (the corpus's pooled p95).
    pub file_lines: usize,
    /// Code lines at which a file may be reported at Low if it is also coupled (the pooled p99).
    pub file_lines_high: usize,
    /// Other modules a file names at or above which, alone, it is reported at Info (pooled p99).
    pub fan_out: usize,
    /// Fan-out at or above which a file that is also very large counts as coupled, and is reported at Low (pooled p95).
    pub fan_out_coupled: usize,
    /// Fan-in at or above which a file that is also very large counts as coupled (pooled p95).
    pub fan_in_coupled: usize,
    /// Fields plus inherent methods at or above which a type is reported (pooled p99).
    pub type_members: usize,
    pub source: &'static str,
}

impl Thresholds {
    pub(crate) fn basis(&self) -> String {
        format!("The thresholds are {}", self.source)
    }
}

/// Percentiles of the pooled corpus: 1,263 production files and 3,706 structs of Fuel, auth-framework, Unpopped, KISS (conformance),
/// PersistAnt, configurant, httpant, releaseant and coderipper, each at its `origin/main` on 2026-11-01, measured by this check's own
/// metrics (design note `docs/superpowers/specs/2026-11-01-arc-mod-checks-design.md`, section 3.1 and the table in the MOD-005 change).
/// Code lines: p95 1,035, p99 2,234. Fan-out: p95 7, p99 14. Fan-in: p95 6 (used beside size only). Members: p99 30.
pub(crate) const CORPUS: Thresholds = Thresholds {
    file_lines: 1035,
    file_lines_high: 2234,
    fan_out: 14,
    fan_out_coupled: 7,
    fan_in_coupled: 6,
    type_members: 30,
    source: "percentiles of the pooled corpus of nine of our own codebases (1,263 files, 3,706 types at origin/main on 2026-11-01), not of this project alone: size p95 and p99, fan-out p95 and p99, fan-in p95 (beside size only), type members p99",
};
