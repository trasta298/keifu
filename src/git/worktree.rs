//! Linked worktree listing

use std::path::PathBuf;

use anyhow::Result;
use git2::Repository;

#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub name: String,
    pub path: PathBuf,
    /// Branch checked out in the worktree (None when detached or unreadable)
    pub branch: Option<String>,
}

impl WorktreeInfo {
    /// List linked worktrees (the main working tree is not included)
    pub fn list_all(repo: &Repository) -> Result<Vec<Self>> {
        let mut worktrees = Vec::new();
        for name in repo.worktrees()?.iter().flatten() {
            let Ok(worktree) = repo.find_worktree(name) else {
                continue;
            };
            // Read the per-worktree HEAD file directly: this runs on every
            // auto-refresh, and opening each worktree as a Repository would
            // re-parse config/odb state just to learn the branch name.
            let head_path = repo.path().join("worktrees").join(name).join("HEAD");
            let branch = std::fs::read_to_string(head_path).ok().and_then(|head| {
                head.trim()
                    .strip_prefix("ref: refs/heads/")
                    .map(|s| s.to_string())
            });
            worktrees.push(WorktreeInfo {
                name: name.to_string(),
                path: worktree.path().to_path_buf(),
                branch,
            });
        }
        Ok(worktrees)
    }
}
