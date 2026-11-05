// SPDX-License-Identifier: MIT OR Apache-2.0
//! The built-in checks. Each module documents what its check judges, how, and what it cannot see.

pub mod architecture;
pub mod ci_protection_presence;
pub mod cli_basics;
pub mod cohesion;
pub mod dependency_freshness;
pub mod reachability;
pub(crate) mod rust_sources;
pub mod size_coupling;
pub mod unused_parameters;
pub mod unused_return_values;
pub mod version_consistency;
// One name per line, sorted, so adding a check edits its own line and not a wrapped list.
#[rustfmt::skip]
pub use architecture::{
    ApiBudgetCheck,
    ComponentCyclesCheck,
    ComponentGraphCheck,
    DataOwnershipCheck,
    DivergentChangeCheck,
    DriftCheck,
    InternalsCheck,
    InvariantFieldsCheck,
    LayersCheck,
    ModuleCyclesCheck,
    PassThroughCheck,
    PublicSurfaceCheck,
    StabilityCheck,
    VolatileDetailCheck,
};
pub use ci_protection_presence::CiProtectionPresenceCheck;
pub use cli_basics::CliBasicsCheck;
pub use cohesion::CohesionCheck;
pub use dependency_freshness::DependencyFreshnessCheck;
pub use reachability::ReachabilityCheck;
pub use size_coupling::SizeCouplingCheck;
pub use unused_parameters::UnusedParametersCheck;
pub use unused_return_values::UnusedReturnValuesCheck;
pub use version_consistency::VersionConsistencyCheck;
