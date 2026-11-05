// SPDX-License-Identifier: MIT OR Apache-2.0
//! The compiled-in checks as one closed set. Everything that used to list them by hand (the registration order, the module's
//! list of proven claims, the profile each runs in) is derived from the one table below, so **adding a check is one new row** and
//! nothing else in this file changes.
//!
//! A row is `Variant => RULE_ID_CONSTANT, proven, classic, CheckType;` with the variant's one-line doc comment above it:
//! - `RULE_ID_CONSTANT`: the constant in `crate::rule_id` generated from `rules/*.toml` (`ARC_008` for `ARC-008`);
//! - `proven`: whether the claim is earned, i.e. it has a seeded-defect fixture and a clean twin under `conformance/` (see
//!   [`BuiltinCheck::is_proven`]);
//! - `classic`: whether the `classic` profile runs it. The `classic` output is a contract fixed before these checks existed, so a
//!   check added later is `false`; `extended` and `portfolio` take every check;
//! - `CheckType`: the type in `crate::checks` with a `new()` that builds it.
//!
//! The six legacy rows come first, in the order the `classic` profile's output is fixed to. Every other row follows, sorted by id
//! (a test enforces it): put a new row at its place in that order, not at the end, so changes that add different rules do not
//! edit the same lines. Nothing depends on the run order of these (the registration test below compares it with `ALL`).

use crate::check::Check;
use crate::checks;

macro_rules! builtin_checks {
    ($($(#[$doc:meta])* $variant:ident => $id:ident, $proven:literal, $classic:literal, $check:ident;)+) => {
        /// One of the checks compiled into CodeRipper.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum BuiltinCheck {
            $($(#[$doc])* $variant,)+
        }

        impl BuiltinCheck {
            /// Every built-in check, in the order they run.
            pub const ALL: [BuiltinCheck; [$(BuiltinCheck::$variant),+].len()] = [$(BuiltinCheck::$variant),+];

            /// The check's stable id: what `Check::id` returns for the check this builds.
            pub fn id(self) -> &'static str {
                match self {
                    $(Self::$variant => crate::rule_id::$id,)+
                }
            }

            /// Whether its claim is earned: it has a seeded-defect fixture and a clean twin under `conformance/`, run by this
            /// repository's own tests (`tests/conformance.rs` fails if this and the fixtures disagree). The host trusts a built-in
            /// module's `proof` because that CI proved it; there is no per-build lock file for code that is compiled into the binary.
            pub fn is_proven(self) -> bool {
                match self {
                    $(Self::$variant => $proven,)+
                }
            }

            /// Whether the `classic` profile runs it (the checks that existed when its output was fixed); `extended` and `portfolio`
            /// run every check.
            pub fn in_classic(self) -> bool {
                match self {
                    $(Self::$variant => $classic,)+
                }
            }

            /// A fresh instance of the check.
            pub fn build(self) -> Box<dyn Check> {
                match self {
                    $(Self::$variant => Box::new(checks::$check::new()),)+
                }
            }
        }
    };
}

builtin_checks! {
    /// `reachability`.
    Reachability => REACHABILITY, true, true, ReachabilityCheck;
    /// `unused-return-values`.
    UnusedReturnValues => UNUSED_RETURN_VALUES, true, true, UnusedReturnValuesCheck;
    /// `unused-parameters`.
    UnusedParameters => UNUSED_PARAMETERS, true, true, UnusedParametersCheck;
    /// `version-consistency`.
    VersionConsistency => VERSION_CONSISTENCY, true, true, VersionConsistencyCheck;
    /// `ci-protection-presence`.
    CiProtectionPresence => CI_PROTECTION_PRESENCE, false, true, CiProtectionPresenceCheck;
    /// `dependency-freshness`.
    DependencyFreshness => DEPENDENCY_FRESHNESS, false, true, DependencyFreshnessCheck;

    // The checks below the six legacy ones: one family at a time, sorted by id, so a new row lands in its own place and two
    // changes adding different rules do not touch the same lines.
    /// `ARC-001`.
    ComponentGraph => ARC_001, true, false, ComponentGraphCheck;
    /// `ARC-002`.
    ComponentCycles => ARC_002, true, false, ComponentCyclesCheck;
    /// `ARC-003`.
    VolatileDetail => ARC_003, true, false, VolatileDetailCheck;
    /// `ARC-005`.
    Stability => ARC_005, true, false, StabilityCheck;
    /// `ARC-006`.
    ApiBudget => ARC_006, true, false, ApiBudgetCheck;
    /// `ARC-007`.
    DataOwnership => ARC_007, true, false, DataOwnershipCheck;
    /// `ARC-008`.
    Layers => ARC_008, true, false, LayersCheck;
    /// `ARC-009`.
    Drift => ARC_009, true, false, DriftCheck;
    /// `MOD-001`.
    PublicSurface => MOD_001, true, false, PublicSurfaceCheck;
    /// `MOD-002`.
    ModuleCycles => MOD_002, true, false, ModuleCyclesCheck;
    /// `MOD-003`.
    Internals => MOD_003, true, false, InternalsCheck;
    /// `MOD-004`.
    Cohesion => MOD_004, true, false, CohesionCheck;
    /// `MOD-005`.
    SizeCoupling => MOD_005, true, false, SizeCouplingCheck;
    /// `MOD-006`.
    DivergentChange => MOD_006, true, false, DivergentChangeCheck;
    /// `MOD-008`.
    PassThrough => MOD_008, true, false, PassThroughCheck;
    /// `MOD-010`.
    InvariantFields => MOD_010, true, false, InvariantFieldsCheck;
    /// `OPS-011`.
    CliBasics => OPS_011, true, false, CliBasicsCheck;
}

impl BuiltinCheck {
    /// The built-in check with this id, if there is one.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.id() == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_variant_builds_the_check_that_carries_its_id() {
        for check in BuiltinCheck::ALL {
            assert_eq!(check.build().id(), check.id(), "{check:?}");
            assert_eq!(BuiltinCheck::from_id(check.id()), Some(check));
        }
        assert_eq!(BuiltinCheck::from_id("no-such-check"), None);
    }

    #[test]
    fn the_registration_order_is_the_order_of_all() {
        let ids: Vec<_> = crate::registered_checks().iter().map(|c| c.id()).collect();
        let all: Vec<_> = BuiltinCheck::ALL.iter().map(|c| c.id()).collect();
        assert_eq!(ids, all);
        assert!(!all.is_empty());
    }

    #[test]
    fn rows_after_the_legacy_six_are_sorted_by_id() {
        let ids: Vec<_> = BuiltinCheck::ALL.iter().skip(6).map(|c| c.id()).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(
            ids, sorted,
            "keep the rows of `builtin_checks!` sorted by id"
        );
    }

    #[test]
    fn the_readme_lists_every_rule_row_in_id_order_and_every_new_check_has_one() {
        // The README rows are prose, so they are written by hand; what is checked is the layout that keeps parallel changes from
        // editing the same lines: the run of `| `ID` |` rows that holds the Rust rules is sorted, and each check has a row.
        let readme = include_str!("../README.md").replace(
            "
", "
",
        );
        let ids: Vec<&str> = readme
            .lines()
            .filter_map(|l| l.strip_prefix("| `"))
            .filter_map(|l| l.split_once("` |").map(|(id, _)| id))
            .filter(|id| {
                id.contains('-') && id.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            })
            .collect();
        for c in BuiltinCheck::ALL.iter().skip(6) {
            assert!(ids.contains(&c.id()), "README.md has no row for {}", c.id());
        }
        let rust: Vec<&str> = ids
            .iter()
            .copied()
            .skip_while(|id| *id != "CON-006")
            .collect();
        let mut sorted = rust.clone();
        sorted.sort_unstable();
        assert_eq!(
            rust, sorted,
            "keep the README's Rust rule rows sorted by id"
        );
    }

    #[test]
    fn ids_are_unique_and_the_classic_set_is_the_six_legacy_checks() {
        let mut ids: Vec<_> = BuiltinCheck::ALL.iter().map(|c| c.id()).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "a check id appears twice");
        // the `classic` profile's output is a contract: exactly these, in this order, and a new check must not join them
        let classic: Vec<_> = BuiltinCheck::ALL
            .iter()
            .filter(|c| c.in_classic())
            .map(|c| c.id())
            .collect();
        assert_eq!(
            classic,
            [
                "reachability",
                "unused-return-values",
                "unused-parameters",
                "version-consistency",
                "ci-protection-presence",
                "dependency-freshness"
            ]
        );
    }
}
