//! Commit diff information

use std::path::PathBuf;

use anyhow::Result;
use git2::{Delta, Diff, DiffOptions, Oid, Repository};

/// Maximum number of files to display
const MAX_FILES_TO_DISPLAY: usize = 50;

/// File change kind
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
}

/// Per-file diff info
#[derive(Debug, Clone)]
pub struct FileDiffInfo {
    /// File path
    pub path: PathBuf,
    /// Change kind
    pub kind: FileChangeKind,
    /// Insertions
    pub insertions: usize,
    /// Deletions
    pub deletions: usize,
}

/// Commit diff info
#[derive(Debug, Clone, Default)]
pub struct CommitDiffInfo {
    /// Changed files list (up to MAX_FILES_TO_DISPLAY)
    pub files: Vec<FileDiffInfo>,
    /// Total insertions
    pub total_insertions: usize,
    /// Total deletions
    pub total_deletions: usize,
    /// Total files
    pub total_files: usize,
    /// Whether truncated
    pub truncated: bool,
}

impl CommitDiffInfo {
    /// Get diff info for working tree (staged + unstaged changes)
    pub fn from_working_tree(repo: &Repository) -> Result<Self> {
        let head_tree = repo.head()?.peel_to_tree().ok();

        let mut opts = DiffOptions::new();
        opts.include_untracked(false);
        opts.ignore_submodules(true);
        opts.context_lines(0);

        // Staged changes: HEAD -> index
        let staged_diff = repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))?;

        // Unstaged changes: index -> workdir
        let unstaged_diff = repo.diff_index_to_workdir(None, Some(&mut opts))?;

        // Merge both diffs
        let mut result = Self::from_diff(&staged_diff)?;
        let unstaged_result = Self::from_diff(&unstaged_diff)?;

        // Merge unstaged files into result
        for file in unstaged_result.files {
            if !result.files.iter().any(|f| f.path == file.path) {
                if result.files.len() < MAX_FILES_TO_DISPLAY {
                    result.files.push(file);
                }
            }
        }

        result.total_insertions += unstaged_result.total_insertions;
        result.total_deletions += unstaged_result.total_deletions;
        result.total_files = result.files.len();

        Ok(result)
    }

    /// Get diff info for a commit
    /// - Normal commit: diff vs parent
    /// - Merge commit: diff vs first parent
    /// - Initial commit: diff vs empty tree
    pub fn from_commit(repo: &Repository, commit_oid: Oid) -> Result<Self> {
        let commit = repo.find_commit(commit_oid)?;
        let new_tree = commit.tree()?;

        // Get parent tree (None for initial commit)
        let old_tree = if commit.parent_count() > 0 {
            Some(commit.parent(0)?.tree()?)
        } else {
            None
        };

        // Generate diff (performance options)
        let mut opts = DiffOptions::new();
        opts.minimal(false); // Skip minimal diff calculation
        opts.ignore_submodules(true); // Skip submodules
        opts.context_lines(0); // Set context lines to 0

        let diff = repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut opts))?;

        Self::from_diff(&diff)
    }

    fn from_diff(diff: &Diff) -> Result<Self> {
        let total_files = diff.deltas().len();
        let truncated = total_files > MAX_FILES_TO_DISPLAY;

        // Collect file info (up to limit)
        let mut files: Vec<FileDiffInfo> =
            Vec::with_capacity(MAX_FILES_TO_DISPLAY.min(total_files));

        for delta_idx in 0..total_files.min(MAX_FILES_TO_DISPLAY) {
            let delta = diff.get_delta(delta_idx).unwrap();

            // Skip binary files
            if delta.flags().is_binary() {
                continue;
            }

            let kind = match delta.status() {
                Delta::Added => FileChangeKind::Added,
                Delta::Deleted => FileChangeKind::Deleted,
                Delta::Modified => FileChangeKind::Modified,
                Delta::Renamed => FileChangeKind::Renamed,
                Delta::Copied => FileChangeKind::Copied,
                _ => continue,
            };

            let path = if kind == FileChangeKind::Deleted {
                delta.old_file().path()
            } else {
                delta.new_file().path()
            };

            if let Some(p) = path {
                files.push(FileDiffInfo {
                    path: p.to_path_buf(),
                    kind,
                    insertions: 0,
                    deletions: 0,
                });
            }
        }

        // Count lines (binaries already skipped)
        let mut total_insertions = 0;
        let mut total_deletions = 0;

        diff.foreach(
            &mut |_delta, _progress| true,
            None,
            None,
            Some(&mut |delta, _hunk, line| {
                // Skip binaries
                if delta.flags().is_binary() {
                    return true;
                }

                let file_path = delta.new_file().path().or_else(|| delta.old_file().path());

                if let Some(p) = file_path {
                    if let Some(file_info) = files.iter_mut().find(|f| f.path == p) {
                        match line.origin() {
                            '+' => {
                                file_info.insertions += 1;
                                total_insertions += 1;
                            }
                            '-' => {
                                file_info.deletions += 1;
                                total_deletions += 1;
                            }
                            _ => {}
                        }
                    }
                }
                true
            }),
        )?;

        Ok(Self {
            files,
            total_insertions,
            total_deletions,
            total_files,
            truncated,
        })
    }
}
