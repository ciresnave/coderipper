use crate::finding::Finding;

/// Which repos a check needs to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Scope {
    /// Only the project being checked.
    Project,
    /// The project being checked, plus other portfolio repos.
    Portfolio,
}

/// What one run of a check judges. A workspace run (`--workspace`) runs a `Package` check once per member and a
/// `Repository` check once for the whole workspace: a check that reads repository-wide facts (a workspace's
/// versions, a GitHub repository's settings) would otherwise repeat one finding per member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Unit {
    /// One cargo package: the project directory is a package's directory.
    Package,
    /// The whole repository / workspace.
    Repository,
}

/// Whether a check ever leaves the machine.
///
/// This is what actually decides run tier (see [`Check::tier`]), not [`Scope`] — every portfolio
/// repo already lives checked out locally, so a `Portfolio`-scope, `LocalOnly` check (reading
/// sibling checkouts on disk) is still fast. What's slow and rate-limit-sensitive is a registry or
/// GitHub API call, regardless of how many repos a check reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Network {
    LocalOnly,
    NetworkRequired,
}

/// The tier a check runs in, derived from [`Network`], not [`Scope`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Tier {
    /// Meant to run on every PR, or on demand next to `cargo clippy`.
    Fast,
    /// Meant for a periodic or PM-triggered run; includes every `NetworkRequired` check.
    Sweep,
}

impl Network {
    pub fn tier(self) -> Tier {
        match self {
            Network::LocalOnly => Tier::Fast,
            Network::NetworkRequired => Tier::Sweep,
        }
    }
}

/// The context a check runs against: which project (and, for `Portfolio`-scope checks, where to
/// find its siblings).
///
/// Build one with [`CheckContext::new`]: the struct is `#[non_exhaustive]`, so a struct literal is rejected outside
/// this crate and a field can be added later without breaking anyone.
///
/// ```compile_fail,E0639
/// use coderipper::check::CheckContext;
/// let _ = CheckContext { project_root: ".".into(), portfolio_root: ".".into() };
/// ```
#[non_exhaustive]
pub struct CheckContext {
    pub project_root: std::path::PathBuf,
    pub portfolio_root: std::path::PathBuf,
}

impl CheckContext {
    /// A context for the project at `project_root`; `portfolio_root` is where `Portfolio`-scope checks look for
    /// siblings (the project's parent directory is the usual choice).
    pub fn new(
        project_root: impl Into<std::path::PathBuf>,
        portfolio_root: impl Into<std::path::PathBuf>,
    ) -> Self {
        Self {
            project_root: project_root.into(),
            portfolio_root: portfolio_root.into(),
        }
    }
}

/// One pluggable audit. See `docs/superpowers/specs/2026-09-30-audit-host-design.md` §1-2 for the
/// design this trait implements.
pub trait Check {
    /// Stable identifier, e.g. `"reachability"`. Used in `--check <id>`, in every `Finding`'s
    /// `check_id`, and as the key for allowlist entries.
    fn id(&self) -> &'static str;

    fn scope(&self) -> Scope;

    fn network(&self) -> Network;

    fn tier(&self) -> Tier {
        self.network().tier()
    }

    /// What one run of this check judges (see [`Unit`]). Most checks judge one package.
    fn unit(&self) -> Unit {
        Unit::Package
    }

    /// Run the check and return whatever it found, RAW: do not apply the project's allowlist. The
    /// host suppresses (see `suppression`) because only it can tell which allowlist entries went
    /// stale. A check that would make an absence claim without a positive control must not
    /// construct that `Finding` at all — see `Finding::validate`, which the host calls on every
    /// finding before it reaches a report. Set `Finding::subject` to the symbol the finding is
    /// about, or it can never be allowlisted.
    fn run(&self, ctx: &CheckContext) -> anyhow::Result<Vec<Finding>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_only_is_always_fast_regardless_of_scope() {
        // The design's core claim (§1): scope and network are independent, and tier is decided by
        // network alone. A Portfolio-scope LocalOnly check must still land in Fast tier.
        assert_eq!(Network::LocalOnly.tier(), Tier::Fast);
    }

    #[test]
    fn network_required_is_always_sweep_regardless_of_scope() {
        assert_eq!(Network::NetworkRequired.tier(), Tier::Sweep);
    }
}
