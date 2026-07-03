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
            let branch = Repository::open_from_worktree(&worktree)
                .ok()
                .and_then(|wt_repo| {
                    let head = wt_repo.head().ok()?;
                    if head.is_branch() {
                        head.shorthand().map(|s| s.to_string())
                    } else {
                        None
                    }
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
