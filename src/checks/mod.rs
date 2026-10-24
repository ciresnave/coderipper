// SPDX-License-Identifier: MIT OR Apache-2.0
//! The built-in checks. Each module documents what its check judges, how, and what it cannot see.

pub mod ci_protection_presence;
pub mod dependency_freshness;
pub mod reachability;
pub mod unused_parameters;
pub mod unused_return_values;
pub mod version_consistency;
pub use ci_protection_presence::CiProtectionPresenceCheck;
pub use dependency_freshness::DependencyFreshnessCheck;
pub use reachability::ReachabilityCheck;
pub use unused_parameters::UnusedParametersCheck;
pub use unused_return_values::UnusedReturnValuesCheck;
pub use version_consistency::VersionConsistencyCheck;
