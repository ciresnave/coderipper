// SPDX-License-Identifier: MIT OR Apache-2.0
//! Names that more than one module needs and that none of them owns, so that no module has to depend on another only for a constant
//! (`MOD-002`: `allowlist` and `architecture` named `config::FILE_NAME` while `config` uses both of them).

/// The one file name, in every place it is searched.
pub(crate) const FILE_NAME: &str = ".coderipper.toml";
