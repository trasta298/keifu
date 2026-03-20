//! Repository operation wrapper

use std::path::Path;
use std::time::SystemTime;

use anyhow::{Context, Result};
use git2::{Repository, Status};

use git2::Oid;

use super::{BranchInfo, CommitInfo};

pub struct GitRepository {
    pub repo: Repository,
    pub path: String,
}

impl GitRepository {
    /// Discover a repository from the current directory
    pub fn discover() -> Result<Self> {
        let repo = Repository::discover(".")
            .context("Git repository not found. Please run inside a Git repository.")?;
        let path = repo
            .workdir()
            .unwrap_or_else(|| repo.path())
            .to_string_lossy()
            .to_string();
        Ok(Self { repo, path })
    }

    /// Open a repository from a specified path
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let repo = Repository::open(path.as_ref())
            .context("Git repository not found at specified path.")?;
        let path_str = repo
            .workdir()
            .unwrap_or_else(|| repo.path())
            .to_string_lossy()
            .to_string();
        Ok(Self {
            repo,
            path: path_str,
        })
    }

    /// Get commit history (newest first)
    pub fn get_commits(&self, max_count: usize) -> Result<Vec<CommitInfo>> {
        let mut revwalk = self.repo.revwalk()?;
        revwalk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)?;

        // Include all branches
        for branch_result in self.repo.branches(None)? {
            let (branch, _) = branch_result?;
            if let Some(oid) = branch.get().target() {
                revwalk.push(oid)?;
            }
        }

        let mut commits = Vec::new();
        for oid_result in revwalk.take(max_count) {
            let oid = oid_result?;
            let commit = self.repo.find_commit(oid)?;
            commits.push(CommitInfo::from_git2_commit(&commit));
        }

        Ok(commits)
    }

    /// Get branch list
    pub fn get_branches(&self) -> Result<Vec<BranchInfo>> {
        BranchInfo::list_all(&self.repo)
    }

    /// Get the current HEAD name
    pub fn head_name(&self) -> Option<String> {
        self.repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(|s| s.to_string()))
    }

    /// Get the current HEAD commit OID
    pub fn head_oid(&self) -> Option<Oid> {
        self.repo
            .head()
            .ok()
            .and_then(|h| h.peel_to_commit().ok())
            .map(|c| c.id())
    }

    /// Get working tree status (staged + unstaged + untracked changes)
    /// Returns None if there are no changes
    pub fn get_working_tree_status(&self) -> Result<Option<WorkingTreeStatus>> {
        if self.repo.is_bare() {
            return Ok(None);
        }

        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false);

        let statuses = self.repo.statuses(Some(&mut opts))?;
        let workdir = self.repo.workdir().unwrap_or_else(|| self.repo.path());
        let mut file_paths = Vec::new();

        for entry in statuses.iter() {
            let status = entry.status();

            // Staged changes (INDEX_*)
            let is_staged = status.intersects(
                git2::Status::INDEX_NEW
                    | git2::Status::INDEX_MODIFIED
                    | git2::Status::INDEX_DELETED
                    | git2::Status::INDEX_RENAMED
                    | git2::Status::INDEX_TYPECHANGE,
            );

            // Worktree changes: unstaged + untracked (WT_*)
            let has_worktree_changes = status.intersects(
                git2::Status::WT_NEW
                    | git2::Status::WT_MODIFIED
                    | git2::Status::WT_DELETED
                    | git2::Status::WT_RENAMED
                    | git2::Status::WT_TYPECHANGE,
            );

            if is_staged || has_worktree_changes {
                if let Some(path) = entry.path() {
                    if status.intersects(Status::WT_NEW) {
                        let full_path = workdir.join(path);
                        if matches!(
                            std::fs::symlink_metadata(&full_path),
                            Ok(meta) if meta.file_type().is_dir()
                        ) {
                            continue;
                        }
                    }
                    file_paths.push(path.to_string());
                }
            }
        }

        if file_paths.is_empty() {
            Ok(None)
        } else {
            file_paths.sort();

            // Compute mtime hash from all changed files
            let mtime_hash: u128 = file_paths
                .iter()
                .filter_map(|path| {
                    let full_path = workdir.join(path);
                    std::fs::symlink_metadata(&full_path)
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                        .map(|d| d.as_millis())
                })
                .sum();

            Ok(Some(WorkingTreeStatus {
                file_paths,
                mtime_hash,
            }))
        }
    }
}

/// Working tree status
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkingTreeStatus {
    /// Sorted list of file paths with changes (used as cache key)
    pub file_paths: Vec<String>,
    /// Sum of file mtimes in milliseconds (used as cache key for content changes)
    pub mtime_hash: u128,
}

impl WorkingTreeStatus {
    pub fn file_count(&self) -> usize {
        self.file_paths.len()
    }
}
