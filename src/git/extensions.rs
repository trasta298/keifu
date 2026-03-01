//! Git extension configuration for libgit2.

use anyhow::{Context, Result};

/// Configure extra repository extensions that libgit2 should accept.
///
/// This must run before opening repositories.
pub fn configure_git_extensions() -> Result<()> {
    unsafe { git2::opts::set_extensions(&["relativeworktrees"]) }
        .context("failed to configure libgit2 supported extensions (relativeworktrees)")
}
