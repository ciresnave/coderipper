// SPDX-License-Identifier: MIT OR Apache-2.0
//! Where a delegated-tool test installs the real pinned tool.
//!
//! By default a throwaway tempdir, so a test never sees another run's files. When `CODERIPPER_TEST_TOOLS_DIR` names a directory
//! (CI points it at an `actions/cache` keyed by `src/tools/tools.lock`), that directory is used instead and kept between runs, so
//! a pinned tool is downloaded once and not once per test binary per run. The cache is not trusted: `ToolEnv::resolve` hashes an
//! installed tool against the lock and installs it again when the bytes differ.

use std::path::{Path, PathBuf};

pub enum ToolsRoot {
    Temp(tempfile::TempDir),
    Kept(PathBuf),
}

impl ToolsRoot {
    pub fn new() -> Self {
        match std::env::var_os("CODERIPPER_TEST_TOOLS_DIR").filter(|v| !v.is_empty()) {
            Some(dir) => {
                let dir = PathBuf::from(dir);
                std::fs::create_dir_all(&dir).unwrap();
                ToolsRoot::Kept(dir)
            }
            None => ToolsRoot::Temp(tempfile::tempdir().unwrap()),
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            ToolsRoot::Temp(dir) => dir.path(),
            ToolsRoot::Kept(dir) => dir,
        }
    }
}
