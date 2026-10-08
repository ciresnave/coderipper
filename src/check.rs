//! The [`Check`] trait, what a check declares about itself, and the [`CheckContext`] it runs against.

use crate::finding::Finding;

/// Which repos a check needs to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[non_exhaustive]
pub enum Network {
    /// Reads only the machine it runs on (the repository, its build): a fast-tier check. Spelled `local` in a catalog record.
    #[serde(rename = "local")]
    LocalOnly,
    /// Calls a registry or the GitHub API: a sweep-tier check, rate-limited and slow. Spelled `network` in a catalog record.
    #[serde(rename = "network")]
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
    /// The tier a check with this network behaviour runs in.
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
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CheckContext {
    /// The project being checked (its directory).
    pub project_root: std::path::PathBuf,
    /// Where `Portfolio`-scope checks look for the project's siblings.
    pub portfolio_root: std::path::PathBuf,
}

impl CheckContext {
    /// A context for the project at `project_root`. The portfolio root, where a check that reads sibling projects looks
    /// for them, defaults to the project's parent directory (the project itself when it has none); change it with
    /// [`CheckContext::portfolio_root`]. No built-in check reads it.
    pub fn new(project_root: impl Into<std::path::PathBuf>) -> Self {
        let project_root = project_root.into();
        let portfolio_root = project_root
            .parent()
            .map_or_else(|| project_root.clone(), std::path::Path::to_path_buf);
        Self {
            project_root,
            portfolio_root,
        }
    }

    /// Sets where checks that read sibling projects look for them.
    pub fn portfolio_root(mut self, portfolio_root: impl Into<std::path::PathBuf>) -> Self {
        self.portfolio_root = portfolio_root.into();
        self
    }
}

/// One pluggable audit. Run it through the host with [`crate::run_checks_with`], which validates its findings and applies
/// the project's allowlist; calling [`Check::run`] directly skips both.
///
/// `Send + Sync` are supertraits so a check can run on a thread pool, an async runtime or a service; a check that holds
/// an `Rc` or a `RefCell` must hold an `Arc` or a `Mutex` instead.
pub trait Check: Send + Sync {
    /// Stable identifier, e.g. `"reachability"`. Used in `coderipper check <id>`, in every `Finding`'s
    /// `check_id`, and as the key for allowlist entries.
    fn id(&self) -> &'static str;

    /// Which repositories the check reads (see [`Scope`]). Nothing in the host reads it yet, so it defaults to the
    /// project; a check that reads sibling projects returns `Scope::Portfolio`.
    fn scope(&self) -> Scope {
        Scope::Project
    }

    /// Whether the check leaves the machine (see [`Network`]); this decides its [`Tier`].
    fn network(&self) -> Network;

    /// The tier the check runs in: derived from [`Check::network`], override only with a reason.
    fn tier(&self) -> Tier {
        self.network().tier()
    }

    /// What one run of this check judges (see [`Unit`]). Most checks judge one package.
    fn unit(&self) -> Unit {
        Unit::Package
    }

    /// Run the check and return whatever it found, RAW: do not apply the project's allowlist. The
    /// host suppresses, because only it can tell which allowlist entries went
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
