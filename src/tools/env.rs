//! [`ToolEnv`]: everything a delegated rule needs to get hold of its tool, in one value the caller builds once.

use super::{
    current_platform, tools_dir_from_env, Consent, DefaultFetcher, Fetcher, ToolCache, ToolEntry,
    ToolError, ToolStatus, ToolsLock,
};
use std::path::PathBuf;
use std::sync::Arc;

/// The lock to read, the cache to install into, whether installing is allowed, and how to download: what turns a tool's name
/// into the absolute path of an executable ([`ToolEnv::resolve`]).
///
/// [`ToolEnv::from_environment`] is what a library user gets by default: the lock built into this binary, the cache the
/// environment names (`CODERIPPER_TOOLS_DIR`), and **no consent**, so nothing is downloaded or written until the caller says
/// [`Consent::Granted`] (the CLI's `--install-tools`).
#[derive(Clone)]
pub struct ToolEnv {
    lock: ToolsLock,
    cache: Option<ToolCache>,
    consent: Consent,
    fetcher: Arc<dyn Fetcher + Send + Sync>,
    platform: String,
}

impl std::fmt::Debug for ToolEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolEnv")
            .field("cache", &self.cache)
            .field("consent", &self.consent)
            .field("platform", &self.platform)
            .finish_non_exhaustive()
    }
}

impl ToolEnv {
    /// The embedded lock, the environment's tools directory, this platform, and no consent to install.
    pub fn from_environment() -> Self {
        Self {
            lock: ToolsLock::embedded(),
            cache: tools_dir_from_env(&|k| std::env::var(k).ok()).map(ToolCache::new),
            consent: Consent::NotGiven,
            fetcher: Arc::new(DefaultFetcher),
            platform: current_platform(),
        }
    }

    /// Reads tools from this lock instead of the one built in.
    pub fn lock(mut self, lock: ToolsLock) -> Self {
        self.lock = lock;
        self
    }

    /// Installs into (and runs from) this tools directory.
    pub fn cache_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.cache = Some(ToolCache::new(root.into()));
        self
    }

    /// Whether installing a missing tool is allowed.
    pub fn consent(mut self, consent: Consent) -> Self {
        self.consent = consent;
        self
    }

    /// Downloads with this fetcher (tests supply their own).
    pub fn fetcher(mut self, fetcher: impl Fetcher + Send + Sync + 'static) -> Self {
        self.fetcher = Arc::new(fetcher);
        self
    }

    /// Resolves for this platform (`x86_64-linux`) rather than the one running.
    pub fn platform(mut self, platform: impl Into<String>) -> Self {
        self.platform = platform.into();
        self
    }

    /// The lock entry for `tool` on this platform.
    pub fn entry(&self, tool: &str) -> Option<&ToolEntry> {
        self.lock.for_platform(tool, &self.platform)
    }

    /// Whether `tool` is installed here and matches the lock. Reads only: never downloads, never writes.
    pub fn is_installed(&self, tool: &str) -> bool {
        match (&self.cache, self.entry(tool)) {
            (Some(cache), Some(entry)) => cache.status(entry) == ToolStatus::Installed,
            _ => false,
        }
    }

    /// The absolute path of `tool`, installing it first when it is missing and consent was given.
    pub fn resolve(&self, tool: &str) -> Result<PathBuf, ToolError> {
        let Some(cache) = &self.cache else {
            // Without a place to install there is nothing to run: that is an absence unless the user asked for an install.
            return Err(match self.consent {
                Consent::NotGiven => ToolError::ConsentNotGiven {
                    tool: tool.to_string(),
                    version: self
                        .entry(tool)
                        .map_or_else(|| "?".to_string(), |e| e.version.clone()),
                },
                Consent::Granted => ToolError::InstallFailed {
                    tool: tool.to_string(),
                    version: "-".to_string(),
                    reason: "no tools directory could be derived; set CODERIPPER_TOOLS_DIR"
                        .to_string(),
                },
            });
        };
        cache.ensure(
            &self.lock,
            tool,
            &self.platform,
            self.consent,
            self.fetcher.as_ref(),
        )
    }
}
