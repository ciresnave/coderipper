//! The compiled-in checks as one closed set. Everything that used to list them by hand (the registration order, the module's
//! list of proven claims) is derived from [`BuiltinCheck`], so adding a check is one new variant and the compiler names each
//! `match` that must decide about it.

use crate::check::Check;
use crate::checks;

/// One of the checks compiled into CodeRipper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BuiltinCheck {
    /// `reachability`.
    Reachability,
    /// `unused-return-values`.
    UnusedReturnValues,
    /// `unused-parameters`.
    UnusedParameters,
    /// `version-consistency`.
    VersionConsistency,
    /// `ci-protection-presence`.
    CiProtectionPresence,
}

impl BuiltinCheck {
    /// Every built-in check, in the order they run.
    pub const ALL: [BuiltinCheck; 5] = [
        Self::Reachability,
        Self::UnusedReturnValues,
        Self::UnusedParameters,
        Self::VersionConsistency,
        Self::CiProtectionPresence,
    ];

    /// The check's stable id: what `Check::id` returns for the check this builds.
    pub fn id(self) -> &'static str {
        match self {
            Self::Reachability => crate::rule_id::REACHABILITY,
            Self::UnusedReturnValues => crate::rule_id::UNUSED_RETURN_VALUES,
            Self::UnusedParameters => crate::rule_id::UNUSED_PARAMETERS,
            Self::VersionConsistency => crate::rule_id::VERSION_CONSISTENCY,
            Self::CiProtectionPresence => crate::rule_id::CI_PROTECTION_PRESENCE,
        }
    }

    /// The built-in check with this id, if there is one.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.id() == id)
    }

    /// Whether its claim is earned: it has a seeded-defect fixture and a clean twin under `conformance/`, run by this repository's own
    /// tests (`tests/conformance.rs` fails if this and the fixtures disagree). The host trusts a built-in module's `proof` because
    /// that CI proved it; there is no per-build lock file for code that is compiled into the binary.
    pub fn is_proven(self) -> bool {
        match self {
            Self::Reachability
            | Self::UnusedParameters
            | Self::UnusedReturnValues
            | Self::VersionConsistency => true,
            Self::CiProtectionPresence => false,
        }
    }

    /// A fresh instance of the check.
    pub fn build(self) -> Box<dyn Check> {
        match self {
            Self::Reachability => Box::new(checks::ReachabilityCheck::new()),
            Self::UnusedReturnValues => Box::new(checks::UnusedReturnValuesCheck::new()),
            Self::UnusedParameters => Box::new(checks::UnusedParametersCheck::new()),
            Self::VersionConsistency => Box::new(checks::VersionConsistencyCheck::new()),
            Self::CiProtectionPresence => Box::new(checks::CiProtectionPresenceCheck::new()),
        }
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
        assert_eq!(all.len(), 5);
    }
}
