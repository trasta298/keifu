//! Commit diff information

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::Result;
use git2::{
    AttrCheckFlags, AttrValue, Delta, Diff, DiffDelta, DiffOptions, ErrorCode, Oid, Patch,
    Repository, Status, StatusOptions, Tree,
};

/// Maximum number of files to display
const MAX_FILES_TO_DISPLAY: usize = 50;

/// Maximum file size (bytes) to read for line counting; larger files are treated as binary
const MAX_TEXT_FILE_SIZE: u64 = 10 * 1024 * 1024;

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
    deferred_paths: HashSet<PathBuf>,
}

impl DiffScan {
    fn line_totals(&self) -> (usize, usize) {
        let insertions = self.files.iter().map(|file| file.insertions).sum();
        let deletions = self.files.iter().map(|file| file.deletions).sum();
        (insertions, deletions)
    }
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
        let refresh_paths: HashSet<PathBuf> = staged_result
            .all_paths
            .intersection(&unstaged_result.all_paths)
            .cloned()
            .collect();
        let untracked_display_limit = MAX_FILES_TO_DISPLAY;
        let untracked_result = Self::scan_untracked_worktree(repo, untracked_display_limit)?;
        let mut worktree_refresh_paths = HashSet::new();
        let mut scan = Self::merge_scans(
            [staged_result, unstaged_result, untracked_result],
            workdir,
            &mut worktree_refresh_paths,
        )?;
        let (total_insertions, total_deletions) = Self::refresh_worktree_stats(
            repo,
            head_tree.as_ref(),
            &mut scan,
            workdir,
            &refresh_paths,
            &worktree_refresh_paths,
            &staged_diff,
        )?;
        Self::build_info(scan, Some((total_insertions, total_deletions)))
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

        Self::build_info(Self::scan_diff(&diff)?, None)
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

        Ok(DiffScan {
            files,
            all_paths,
            deferred_paths: HashSet::new(),
        })
    }

    fn scan_untracked_worktree(repo: &Repository, display_limit: usize) -> Result<DiffScan> {
        let mut opts = StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false);

        let statuses = repo.statuses(Some(&mut opts))?;
        let workdir = repo.workdir().unwrap_or_else(|| repo.path());
        let mut files = Vec::new();
        let mut all_paths = HashSet::new();
        let mut deferred_paths = HashSet::new();

        for entry in statuses.iter() {
            let status = entry.status();
            if !status.intersects(Status::WT_NEW) {
                continue;
            }

            let path_buf = Self::path_from_bytes(entry.path_bytes());
            let full_path = workdir.join(&path_buf);
            if Self::is_plain_directory(&full_path) {
                continue;
            }

            all_paths.insert(path_buf.clone());
            if files.len() >= display_limit {
                continue;
            }
            deferred_paths.insert(path_buf.clone());

            files.push(FileDiffInfo {
                path: path_buf,
                kind: FileChangeKind::Added,
                is_binary: false,
                insertions: 0,
                deletions: 0,
            });
        }

        Ok(DiffScan {
            files,
            all_paths,
            deferred_paths,
        })
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

    fn merge_scans(
        scans: [DiffScan; 3],
        workdir: &Path,
        worktree_refresh_paths: &mut HashSet<PathBuf>,
    ) -> Result<DiffScan> {
        let mut files: Vec<FileDiffInfo> = Vec::new();
        let mut file_indexes: HashMap<PathBuf, usize> = HashMap::new();
        let mut all_paths = HashSet::new();
        let mut deferred_paths = HashSet::new();

        for scan in scans {
            all_paths.extend(scan.all_paths);
            deferred_paths.extend(scan.deferred_paths);

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
                        worktree_refresh_paths.insert(file.path.clone());
                    } else if file.kind != FileChangeKind::Deleted {
                        // Prefer the worktree-side classification when the final path still
                        // exists, so a later text rewrite can override an earlier binary delta.
                        existing.is_binary = file.is_binary;
                    } else {
                        // file.kind == Deleted — the path was removed from the
                        // worktree after being staged (e.g. MD status).
                        // Skip Added so AD (added then deleted) doesn't become
                        // an invalid Deleted-from-HEAD entry.
                        if existing.kind != FileChangeKind::Added {
                            existing.kind = FileChangeKind::Deleted;
                        }
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

        // Recount lines for Added files that have no stats yet (e.g. staged adds
        // where Patch::from_diff returned None). Files already counted by
        // scan_untracked_worktree or scan_diff are skipped to avoid redundant I/O.
        for file in &mut files {
            if file.is_binary || file.kind != FileChangeKind::Added {
                continue;
            }
            if file.insertions > 0 {
                continue;
            }
            if deferred_paths.contains(&file.path) {
                continue;
            }

            let full_path = workdir.join(&file.path);
            if let Some(line_count) = Self::count_text_file_lines(&full_path)? {
                file.insertions = line_count;
            }
        }

        Ok(DiffScan {
            files,
            all_paths,
            deferred_paths,
        })
    }

    fn refresh_worktree_stats(
        repo: &Repository,
        head_tree: Option<&Tree<'_>>,
        scan: &mut DiffScan,
        workdir: &Path,
        refresh_paths: &HashSet<PathBuf>,
        worktree_refresh_paths: &HashSet<PathBuf>,
        staged_diff: &Diff,
    ) -> Result<(usize, usize)> {
        let mut worktree_opts = DiffOptions::new();
        worktree_opts.ignore_submodules(true);
        worktree_opts.context_lines(0);
        worktree_opts.include_untracked(true);
        worktree_opts.recurse_untracked_dirs(true);
        worktree_opts.show_untracked_content(true);
        let worktree_diff = repo.diff_tree_to_workdir(head_tree, Some(&mut worktree_opts))?;
        let worktree_stats = worktree_diff.stats()?;
        let mut total_insertions = worktree_stats.insertions();
        let mut total_deletions = worktree_stats.deletions();
        // Build path→delta indexes once to avoid O(n²) repeated scanning
        let worktree_index = Self::build_delta_index(&worktree_diff);
        let staged_index = Self::build_delta_index(staged_diff);
        for file in &mut scan.files {
            let use_worktree_diff = worktree_refresh_paths.contains(&file.path);
            let worktree_path_stats =
                Self::line_stats_from_index(&worktree_diff, &worktree_index, &file.path)?;
            // NOTE: when .gitattributes is modified in the same uncommitted
            // changes (e.g. switching a file from binary to text), file.is_binary
            // still reflects the old attribute state.  This means a file that was
            // binary will skip refresh here and keep showing +0/-0.  Fixing this
            // would require comparing HEAD vs worktree .gitattributes per path,
            // which libgit2 does not directly support.
            let needs_refresh = use_worktree_diff
                || scan.deferred_paths.contains(&file.path)
                || refresh_paths.contains(&file.path)
                || (!file.is_binary && file.insertions == 0 && file.deletions == 0);
            if matches!(file.kind, FileChangeKind::Deleted) || !needs_refresh {
                continue;
            }

            let Some((worktree_is_binary, worktree_insertions, worktree_deletions)) =
                worktree_path_stats
            else {
                // Path has no HEAD→workdir diff (e.g. MM/AD where workdir
                // matches HEAD).  Use the staged (HEAD→index) stats instead
                // of merge_scans totals to avoid double-counting.
                let staged = Self::line_stats_from_index(staged_diff, &staged_index, &file.path)?;
                let (is_binary, insertions, deletions) = staged.unwrap_or((false, 0, 0));
                file.is_binary = is_binary;
                file.insertions = insertions;
                file.deletions = deletions;
                total_insertions += insertions;
                total_deletions += deletions;
                continue;
            };
            let worktree_stats = (worktree_is_binary, worktree_insertions, worktree_deletions);
            let (is_binary, insertions, deletions) = if use_worktree_diff {
                Self::fallback_recreated_path_stats(
                    repo,
                    head_tree,
                    workdir,
                    &file.path,
                    worktree_stats,
                )?
                .unwrap_or(worktree_stats)
            } else {
                worktree_stats
            };

            if insertions != worktree_insertions || deletions != worktree_deletions {
                total_insertions = total_insertions + insertions - worktree_insertions;
                total_deletions = total_deletions + deletions - worktree_deletions;
            }

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

        Ok((total_insertions, total_deletions))
    }

    fn build_info(scan: DiffScan, totals: Option<(usize, usize)>) -> Result<Self> {
        let total_files = scan.all_paths.len();
        let (total_insertions, total_deletions) = totals.unwrap_or_else(|| scan.line_totals());
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

    /// Build a map from path → delta index for O(1) lookups.
    fn build_delta_index(diff: &Diff) -> HashMap<PathBuf, (usize, bool)> {
        let mut index = HashMap::new();
        for (delta_idx, delta) in diff.deltas().enumerate() {
            let Some((_, delta_path, is_binary)) = Self::diff_entry(delta) else {
                continue;
            };
            index.insert(delta_path.to_path_buf(), (delta_idx, is_binary));
        }
        index
    }

    fn line_stats_from_index(
        diff: &Diff,
        delta_index: &HashMap<PathBuf, (usize, bool)>,
        path: &Path,
    ) -> Result<Option<(bool, usize, usize)>> {
        let Some(&(delta_idx, is_binary)) = delta_index.get(path) else {
            return Ok(None);
        };
        let (insertions, deletions) = if is_binary {
            (0, 0)
        } else {
            Self::line_stats_for_delta(diff, delta_idx)?
        };
        Ok(Some((is_binary, insertions, deletions)))
    }

    fn line_stats_for_delta(diff: &Diff, delta_idx: usize) -> Result<(usize, usize)> {
        let Some(patch) = Patch::from_diff(diff, delta_idx)? else {
            return Ok((0, 0));
        };
        let (_, insertions, deletions) = patch.line_stats()?;
        Ok((insertions, deletions))
    }

    fn fallback_recreated_path_stats(
        repo: &Repository,
        head_tree: Option<&Tree<'_>>,
        workdir: &Path,
        path: &Path,
        stats: (bool, usize, usize),
    ) -> Result<Option<(bool, usize, usize)>> {
        let Some((old_is_binary, _old_lines)) = Self::head_path_line_info(repo, head_tree, path)?
        else {
            return Ok(Some(stats));
        };
        let (new_is_binary, new_lines) = Self::worktree_path_line_info(repo, workdir, path)?;

        let fallback = match (old_is_binary, new_is_binary) {
            (true, false) => Some((false, new_lines, 0)),
            (false, true) => Some((true, 0, 0)),
            (true, true) => Some((true, 0, 0)),
            (false, false) => None,
        };

        Ok(Some(fallback.unwrap_or(stats)))
    }

    /// Check whether the path existed in HEAD and return (is_binary, line_count).
    ///
    /// Binary detection uses NUL-byte heuristics on blob content only — it does
    /// NOT consult HEAD's `.gitattributes`.  If a file was marked binary via
    /// attributes in HEAD and those attributes are removed in the same uncommitted
    /// change, the old side may be misclassified as text (or vice-versa).
    fn head_path_line_info(
        repo: &Repository,
        head_tree: Option<&Tree<'_>>,
        path: &Path,
    ) -> Result<Option<(bool, usize)>> {
        let Some(head_tree) = head_tree else {
            return Ok(None);
        };
        let entry = match head_tree.get_path(path) {
            Ok(entry) => entry,
            Err(err) if err.code() == ErrorCode::NotFound => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        let blob = repo.find_blob(entry.id())?;
        let Some(lines) = Self::count_text_lines(blob.content()) else {
            return Ok(Some((true, 0)));
        };

        Ok(Some((false, lines)))
    }

    fn worktree_path_line_info(
        repo: &Repository,
        workdir: &Path,
        path: &Path,
    ) -> Result<(bool, usize)> {
        if Self::path_is_binary_by_attributes(repo, path)? {
            return Ok((true, 0));
        }

        let full_path = workdir.join(path);
        let Some(lines) = Self::count_text_file_lines(&full_path)? else {
            return Ok((true, 0));
        };

        Ok((false, lines))
    }

    /// Count lines in a text file. Returns `None` if the file appears to be binary
    /// (contains null bytes). Returns `Some(0)` if the file cannot be found
    /// (e.g. deleted between listing and reading).
    fn count_text_file_lines(path: &Path) -> Result<Option<usize>> {
        match std::fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_dir() => return Ok(None),
            Ok(meta) if meta.file_type().is_symlink() => return Self::count_symlink_lines(path),
            Ok(meta) if !meta.is_file() => return Ok(None),
            // Treat very large files as binary to avoid excessive memory usage
            Ok(meta) if meta.len() > MAX_TEXT_FILE_SIZE => return Ok(None),
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

        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        Ok(Self::count_text_lines(&buf))
    }

    fn count_text_lines(content: &[u8]) -> Option<usize> {
        if content.contains(&0) {
            return None;
        }

        let mut line_count = content.iter().filter(|&&byte| byte == b'\n').count();
        if !content.is_empty() && !content.ends_with(b"\n") {
            line_count += 1;
        }

        Some(line_count)
    }

    fn count_symlink_lines(path: &Path) -> Result<Option<usize>> {
        let target = match std::fs::read_link(path) {
            Ok(target) => target,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Some(0)),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        #[cfg(unix)]
        let bytes = {
            use std::os::unix::ffi::OsStrExt;
            target.as_os_str().as_bytes()
        };
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

    #[cfg(unix)]
    fn path_from_bytes(bytes: &[u8]) -> PathBuf {
        use std::os::unix::ffi::OsStrExt;
        PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
    }

    #[cfg(not(unix))]
    fn path_from_bytes(bytes: &[u8]) -> PathBuf {
        PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
    }

    pub(crate) fn is_plain_directory(path: &Path) -> bool {
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
