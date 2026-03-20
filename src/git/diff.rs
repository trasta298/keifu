//! Commit diff information

use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use anyhow::Result;
use git2::{
    AttrCheckFlags, AttrValue, Delta, Diff, DiffDelta, DiffOptions, ErrorCode, Oid, Patch,
    Repository, Status, StatusOptions,
};

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
    /// Whether the file is binary
    pub is_binary: bool,
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

/// Intermediate scan result carrying both display info and the full set of
/// changed paths (used by `merge_scans` for accurate `total_files` counting).
struct DiffScan {
    files: Vec<FileDiffInfo>,
    all_paths: HashSet<PathBuf>,
}

impl CommitDiffInfo {
    /// Get diff info for working tree (staged + unstaged + untracked changes)
    pub fn from_working_tree(repo: &Repository) -> Result<Self> {
        let head_tree = match repo.head() {
            Ok(head) => Some(head.peel_to_tree()?),
            Err(err)
                if err.code() == ErrorCode::UnbornBranch || err.code() == ErrorCode::NotFound =>
            {
                None
            }
            Err(err) => return Err(err.into()),
        };

        let mut opts = DiffOptions::new();
        opts.ignore_submodules(true);
        opts.context_lines(0);

        // Staged changes: HEAD -> index
        let staged_diff = repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))?;

        // Unstaged tracked changes: index -> workdir
        let unstaged_diff = repo.diff_index_to_workdir(None, Some(&mut opts))?;
        let workdir = repo.workdir().unwrap_or_else(|| repo.path());
        let staged_result = Self::scan_diff(&staged_diff)?;
        let unstaged_result = Self::scan_diff(&unstaged_diff)?;
        let refresh_paths = staged_result
            .all_paths
            .intersection(&unstaged_result.all_paths)
            .cloned()
            .collect();
        let untracked_result = Self::scan_untracked_worktree(repo)?;
        let mut scan =
            Self::merge_scans([staged_result, unstaged_result, untracked_result], workdir)?;
        Self::refresh_worktree_stats(repo, head_tree.as_ref(), &mut scan, &refresh_paths)?;
        Self::build_info(scan)
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

        Self::build_info(Self::scan_diff(&diff)?)
    }

    fn scan_diff(diff: &Diff) -> Result<DiffScan> {
        let _ = diff.stats()?;
        let mut files = Vec::with_capacity(diff.deltas().len());
        let mut all_paths = HashSet::new();

        for (delta_idx, delta) in diff.deltas().enumerate() {
            let Some((kind, path, is_binary)) = Self::diff_entry(delta) else {
                continue;
            };

            let path_buf = path.to_path_buf();
            all_paths.insert(path_buf.clone());

            let (insertions, deletions) = if is_binary {
                (0, 0)
            } else {
                Self::line_stats_for_delta(diff, delta_idx)?
            };
            files.push(FileDiffInfo {
                path: path_buf,
                kind,
                is_binary,
                insertions,
                deletions,
            });
        }

        Ok(DiffScan { files, all_paths })
    }

    fn scan_untracked_worktree(repo: &Repository) -> Result<DiffScan> {
        let mut opts = StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false);

        let statuses = repo.statuses(Some(&mut opts))?;
        let workdir = repo.workdir().unwrap_or_else(|| repo.path());
        let mut files = Vec::new();
        let mut all_paths = HashSet::new();

        for entry in statuses.iter() {
            let status = entry.status();
            if !status.intersects(Status::WT_NEW) {
                continue;
            }

            let Some(path) = entry.path() else {
                continue;
            };

            let path_buf = PathBuf::from(path);
            let full_path = workdir.join(&path_buf);
            if Self::is_plain_directory(&full_path) {
                continue;
            }

            let line_count = if Self::path_is_binary_by_attributes(repo, &path_buf)? {
                None
            } else {
                Self::count_text_file_lines(&full_path)?
            };
            let (is_binary, insertions) = match line_count {
                Some(insertions) => (false, insertions),
                None => (true, 0),
            };
            all_paths.insert(path_buf.clone());

            files.push(FileDiffInfo {
                path: path_buf,
                kind: FileChangeKind::Added,
                is_binary,
                insertions,
                deletions: 0,
            });
        }

        Ok(DiffScan { files, all_paths })
    }

    fn path_is_binary_by_attributes(repo: &Repository, path: &Path) -> Result<bool> {
        let flags = AttrCheckFlags::FILE_THEN_INDEX;
        let binary_attr = AttrValue::from_string(repo.get_attr(path, "binary", flags)?);
        if matches!(binary_attr, AttrValue::True) {
            return Ok(true);
        }

        let diff_attr = AttrValue::from_string(repo.get_attr(path, "diff", flags)?);
        Ok(matches!(diff_attr, AttrValue::False))
    }

    fn merge_scans(scans: [DiffScan; 3], workdir: &Path) -> Result<DiffScan> {
        let mut files: Vec<FileDiffInfo> = Vec::new();
        let mut file_indexes: std::collections::HashMap<PathBuf, usize> =
            std::collections::HashMap::new();
        let mut all_paths = HashSet::new();

        for scan in scans {
            all_paths.extend(scan.all_paths);

            for file in scan.files {
                if let Some(&idx) = file_indexes.get(&file.path) {
                    let existing = &mut files[idx];
                    // e.g. git rm foo && create new foo → INDEX_DELETED + WT_NEW
                    // The file still exists on disk, so treat as Modified rather than Deleted.
                    if existing.kind == FileChangeKind::Deleted
                        && file.kind == FileChangeKind::Added
                    {
                        existing.kind = FileChangeKind::Modified;
                        existing.is_binary = file.is_binary;
                    } else if file.kind != FileChangeKind::Deleted {
                        // Prefer the worktree-side classification when the final path still
                        // exists, so a later text rewrite can override an earlier binary delta.
                        existing.is_binary = file.is_binary;
                    } else {
                        existing.is_binary |= file.is_binary;
                    }
                    existing.insertions += file.insertions;
                    existing.deletions += file.deletions;
                } else {
                    file_indexes.insert(file.path.clone(), files.len());
                    files.push(file);
                }
            }
        }

        for file in &mut files {
            if file.is_binary {
                continue;
            }

            let full_path = workdir.join(&file.path);
            if std::fs::symlink_metadata(&full_path).is_err() {
                continue;
            }

            let Some(line_count) = Self::count_text_file_lines(&full_path)? else {
                continue;
            };
            if file.kind == FileChangeKind::Added {
                file.insertions = line_count;
                file.deletions = 0;
            }
        }

        Ok(DiffScan { files, all_paths })
    }

    fn refresh_worktree_stats(
        repo: &Repository,
        head_tree: Option<&git2::Tree<'_>>,
        scan: &mut DiffScan,
        refresh_paths: &HashSet<PathBuf>,
    ) -> Result<()> {
        let mut opts = DiffOptions::new();
        opts.ignore_submodules(true);
        opts.context_lines(0);
        opts.include_untracked(true);
        opts.recurse_untracked_dirs(true);

        let diff = repo.diff_tree_to_workdir_with_index(head_tree, Some(&mut opts))?;
        for file in &mut scan.files {
            let needs_refresh = refresh_paths.contains(&file.path)
                || (!file.is_binary && file.insertions == 0 && file.deletions == 0);
            if matches!(file.kind, FileChangeKind::Deleted) || !needs_refresh {
                continue;
            }

            let Some((is_binary, insertions, deletions)) =
                Self::line_stats_for_path(&diff, &file.path)?
            else {
                continue;
            };

            if file.is_binary == is_binary
                && file.insertions == insertions
                && file.deletions == deletions
            {
                continue;
            }

            file.is_binary = is_binary;
            file.insertions = insertions;
            file.deletions = deletions;
        }

        Ok(())
    }

    fn build_info(scan: DiffScan) -> Result<Self> {
        let total_files = scan.all_paths.len();
        let total_insertions = scan.files.iter().map(|file| file.insertions).sum();
        let total_deletions = scan.files.iter().map(|file| file.deletions).sum();
        let truncated = total_files > MAX_FILES_TO_DISPLAY;
        let files = scan.files.into_iter().take(MAX_FILES_TO_DISPLAY).collect();

        Ok(Self {
            files,
            total_insertions,
            total_deletions,
            total_files,
            truncated,
        })
    }

    fn line_stats_for_path(diff: &Diff, path: &Path) -> Result<Option<(bool, usize, usize)>> {
        for (delta_idx, delta) in diff.deltas().enumerate() {
            let Some((_, delta_path, is_binary)) = Self::diff_entry(delta) else {
                continue;
            };

            if delta_path != path {
                continue;
            }

            let (insertions, deletions) = if is_binary {
                (0, 0)
            } else {
                Self::line_stats_for_delta(diff, delta_idx)?
            };

            return Ok(Some((is_binary, insertions, deletions)));
        }

        Ok(None)
    }

    fn line_stats_for_delta(diff: &Diff, delta_idx: usize) -> Result<(usize, usize)> {
        let Some(patch) = Patch::from_diff(diff, delta_idx)? else {
            return Ok((0, 0));
        };
        let (_, insertions, deletions) = patch.line_stats()?;
        Ok((insertions, deletions))
    }

    /// Count lines in a text file. Returns `None` if the file appears to be binary
    /// (contains null bytes). Returns `Some(0)` if the file cannot be found
    /// (e.g. deleted between listing and reading).
    fn count_text_file_lines(path: &Path) -> Result<Option<usize>> {
        match std::fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_dir() => return Ok(None),
            Ok(meta) if meta.file_type().is_symlink() => return Self::count_symlink_lines(path),
            Ok(meta) if !meta.is_file() => return Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Some(0)),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return Ok(None),
            Err(_) | Ok(_) => {}
        }

        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Some(0)),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        let mut buf = [0_u8; 8192];
        let mut line_count = 0;
        let mut has_content = false;
        let mut last_byte = None;

        loop {
            let read = match file.read(&mut buf) {
                Ok(n) => n,
                Err(_) => return Ok(None),
            };
            if read == 0 {
                break;
            }

            let chunk = &buf[..read];
            if chunk.contains(&0) {
                return Ok(None);
            }

            has_content = true;
            line_count += chunk.iter().filter(|&&byte| byte == b'\n').count();
            last_byte = chunk.last().copied();
        }

        if has_content && last_byte != Some(b'\n') {
            line_count += 1;
        }

        Ok(Some(line_count))
    }

    fn count_symlink_lines(path: &Path) -> Result<Option<usize>> {
        let target = match std::fs::read_link(path) {
            Ok(target) => target,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Some(0)),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        #[cfg(unix)]
        let bytes = target.as_os_str().as_bytes();
        #[cfg(not(unix))]
        let owned = target.to_string_lossy().into_owned().into_bytes();
        #[cfg(not(unix))]
        let bytes = owned.as_slice();

        Ok(Some(Self::count_lines_in_bytes(bytes)))
    }

    fn count_lines_in_bytes(bytes: &[u8]) -> usize {
        if bytes.is_empty() {
            return 0;
        }

        let mut line_count = bytes.iter().filter(|&&byte| byte == b'\n').count();
        if bytes.last().copied() != Some(b'\n') {
            line_count += 1;
        }
        line_count
    }

    fn is_plain_directory(path: &Path) -> bool {
        matches!(
            std::fs::symlink_metadata(path),
            Ok(meta) if meta.file_type().is_dir()
        )
    }

    fn diff_entry(delta: DiffDelta<'_>) -> Option<(FileChangeKind, &Path, bool)> {
        let kind = match delta.status() {
            Delta::Added => FileChangeKind::Added,
            Delta::Deleted => FileChangeKind::Deleted,
            Delta::Modified | Delta::Typechange | Delta::Conflicted => FileChangeKind::Modified,
            Delta::Renamed => FileChangeKind::Renamed,
            Delta::Copied => FileChangeKind::Copied,
            // Untracked files are shown as Added (no separate UI distinction needed)
            Delta::Untracked => FileChangeKind::Added,
            Delta::Unmodified | Delta::Ignored | Delta::Unreadable => return None,
        };

        let path = if kind == FileChangeKind::Deleted {
            delta.old_file().path()
        } else {
            delta.new_file().path()
        }?;

        Some((kind, path, delta.flags().is_binary()))
    }
}
