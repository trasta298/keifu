use std::fs;
#[cfg(unix)]
use std::os::unix::fs::symlink;
use std::path::Path;
use std::thread;
use std::time::Duration;

use git2::{Repository, Signature};
use keifu::git::{CommitDiffInfo, FileChangeKind, GitRepository};
use tempfile::TempDir;

fn init_repo() -> (TempDir, Repository) {
    let tempdir = tempfile::tempdir().unwrap();
    let repo = Repository::init(tempdir.path()).unwrap();

    fs::write(tempdir.path().join("tracked.txt"), "tracked\n").unwrap();

    let mut index = repo.index().unwrap();
    index.add_path(Path::new("tracked.txt")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = Signature::now("Test User", "test@example.com").unwrap();

    repo.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
        .unwrap();

    drop(tree);

    (tempdir, repo)
}

fn init_unborn_repo() -> (TempDir, Repository) {
    let tempdir = tempfile::tempdir().unwrap();
    let repo = Repository::init(tempdir.path()).unwrap();
    (tempdir, repo)
}

#[test]
fn from_working_tree_lists_root_untracked_file() {
    let (tempdir, repo) = init_repo();

    fs::write(tempdir.path().join("new_file.txt"), "hello\n").unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();
    let file = diff
        .files
        .iter()
        .find(|file| file.path == Path::new("new_file.txt"))
        .unwrap();

    assert_eq!(diff.total_files, 1);
    assert_eq!(file.kind, FileChangeKind::Added);
    assert_eq!(file.insertions, 1);
    assert_eq!(file.deletions, 0);
}

#[test]
fn from_working_tree_lists_untracked_file_before_initial_commit() {
    let (tempdir, repo) = init_unborn_repo();

    fs::write(tempdir.path().join("new_file.txt"), "hello\n").unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();
    let file = diff
        .files
        .iter()
        .find(|file| file.path == Path::new("new_file.txt"))
        .unwrap();

    assert_eq!(diff.total_files, 1);
    assert_eq!(file.kind, FileChangeKind::Added);
    assert_eq!(file.insertions, 1);
    assert_eq!(file.deletions, 0);
}

#[test]
fn from_working_tree_lists_mixed_tracked_and_untracked() {
    let (tempdir, repo) = init_repo();

    // Modify tracked file (unstaged)
    fs::write(tempdir.path().join("tracked.txt"), "modified\n").unwrap();
    // Create untracked file
    fs::write(tempdir.path().join("untracked.txt"), "new\n").unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();

    assert_eq!(diff.total_files, 2);

    let tracked = diff
        .files
        .iter()
        .find(|f| f.path == Path::new("tracked.txt"))
        .unwrap();
    assert_eq!(tracked.kind, FileChangeKind::Modified);

    let untracked = diff
        .files
        .iter()
        .find(|f| f.path == Path::new("untracked.txt"))
        .unwrap();
    assert_eq!(untracked.kind, FileChangeKind::Added);
}

#[test]
fn working_tree_status_detects_untracked_only() {
    let (tempdir, _repo) = init_repo();

    fs::write(tempdir.path().join("new_file.txt"), "hello\n").unwrap();

    let git_repo = GitRepository::open(tempdir.path()).unwrap();
    let status = git_repo.get_working_tree_status().unwrap();

    assert!(status.is_some());
    let status = status.unwrap();
    assert_eq!(status.file_count(), 1);
    assert_eq!(status.file_paths, vec!["new_file.txt".to_string()]);
}

#[test]
fn from_working_tree_lists_nested_untracked_files() {
    let (tempdir, repo) = init_repo();
    let nested_path = tempdir.path().join("dir/sub/file.txt");

    fs::create_dir_all(nested_path.parent().unwrap()).unwrap();
    fs::write(&nested_path, "first line\nsecond line\n").unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();
    let file = diff
        .files
        .iter()
        .find(|file| file.path == Path::new("dir/sub/file.txt"))
        .unwrap();

    assert_eq!(diff.total_files, 1);
    assert_eq!(file.kind, FileChangeKind::Added);
    assert_eq!(file.insertions, 2);
    assert_eq!(file.deletions, 0);
}

#[cfg(unix)]
#[test]
fn from_working_tree_counts_untracked_symlink_as_link_itself() {
    let (tempdir, repo) = init_repo();

    fs::write(tempdir.path().join("target.txt"), "one\ntwo\nthree\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("target.txt")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = Signature::now("Test User", "test@example.com").unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "add target file",
        &tree,
        &[&parent],
    )
    .unwrap();
    drop(tree);

    symlink("target.txt", tempdir.path().join("link")).unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();
    let file = diff
        .files
        .iter()
        .find(|file| file.path == Path::new("link"))
        .unwrap();

    assert_eq!(diff.total_files, 1);
    assert_eq!(file.kind, FileChangeKind::Added);
    assert!(!file.is_binary);
    assert_eq!(file.insertions, 1);
    assert_eq!(file.deletions, 0);
}

#[cfg(unix)]
#[test]
fn from_working_tree_includes_untracked_symlink_to_directory() {
    let (tempdir, repo) = init_repo();

    fs::create_dir_all(tempdir.path().join("dir")).unwrap();
    fs::write(tempdir.path().join("dir/file.txt"), "nested\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("dir/file.txt")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = Signature::now("Test User", "test@example.com").unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "add tracked dir",
        &tree,
        &[&parent],
    )
    .unwrap();
    drop(tree);

    symlink("dir", tempdir.path().join("linkdir")).unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();
    let file = diff
        .files
        .iter()
        .find(|file| file.path == Path::new("linkdir"))
        .unwrap();

    assert_eq!(diff.total_files, 1);
    assert_eq!(file.kind, FileChangeKind::Added);
    assert!(!file.is_binary);
    assert_eq!(file.insertions, 1);
    assert_eq!(file.deletions, 0);
}

#[test]
fn working_tree_status_tracks_nested_untracked_file_changes() {
    let (tempdir, _repo) = init_repo();
    let nested_path = tempdir.path().join("dir/sub/file.txt");

    fs::create_dir_all(nested_path.parent().unwrap()).unwrap();
    fs::write(&nested_path, "first version\n").unwrap();

    let git_repo = GitRepository::open(tempdir.path()).unwrap();
    let initial_status = git_repo.get_working_tree_status().unwrap().unwrap();

    assert_eq!(initial_status.file_count(), 1);
    assert_eq!(initial_status.file_paths, vec!["dir/".to_string()]);
    assert!(initial_status.has_collapsed_untracked_dirs);
    assert!(!initial_status.is_precise_cache_key());

    thread::sleep(Duration::from_millis(1100));
    fs::write(&nested_path, "second version\nwith more content\n").unwrap();

    let updated_status = git_repo.get_working_tree_status().unwrap().unwrap();

    assert_eq!(updated_status.file_count(), 1);
    assert_eq!(updated_status.file_paths, vec!["dir/".to_string()]);
    assert!(updated_status.has_collapsed_untracked_dirs);
}

#[test]
fn from_working_tree_merges_staged_and_unstaged_for_same_file() {
    let (tempdir, repo) = init_repo();

    // Create a new file and stage it
    fs::write(tempdir.path().join("new.txt"), "line1\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("new.txt")).unwrap();
    index.write().unwrap();

    // Further edit the same file (unstaged change)
    fs::write(tempdir.path().join("new.txt"), "line1\nline2\n").unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();

    assert_eq!(diff.total_files, 1);
    assert_eq!(diff.files.len(), 1);

    let file = &diff.files[0];
    assert_eq!(file.path, Path::new("new.txt"));
    // staged: +1 (line1 added to index), unstaged: +1 (line2 appended in workdir)
    assert_eq!(file.insertions, 2);
    assert_eq!(file.deletions, 0);
    assert_eq!(diff.total_insertions, 2);
    assert_eq!(diff.total_deletions, 0);
}

#[test]
fn from_working_tree_recomputes_modified_stats_across_staged_and_unstaged() {
    let (tempdir, repo) = init_repo();

    fs::write(tempdir.path().join("tracked.txt"), "staged change\n").unwrap();

    let mut index = repo.index().unwrap();
    index.add_path(Path::new("tracked.txt")).unwrap();
    index.write().unwrap();

    fs::write(tempdir.path().join("tracked.txt"), "final change\n").unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();
    let file = diff
        .files
        .iter()
        .find(|f| f.path == Path::new("tracked.txt"))
        .expect("tracked.txt should appear in diff");

    assert_eq!(file.kind, FileChangeKind::Modified);
    assert_eq!(file.insertions, 1);
    assert_eq!(file.deletions, 1);
    assert_eq!(diff.total_insertions, 1);
    assert_eq!(diff.total_deletions, 1);
}

#[test]
fn from_working_tree_staged_binary_then_rewritten_as_text_uses_worktree_classification() {
    let (tempdir, repo) = init_repo();

    fs::write(
        tempdir.path().join("new.txt"),
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR",
    )
    .unwrap();

    let mut index = repo.index().unwrap();
    index.add_path(Path::new("new.txt")).unwrap();
    index.write().unwrap();

    fs::write(tempdir.path().join("new.txt"), "hello\nworld\n").unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();
    let file = diff
        .files
        .iter()
        .find(|f| f.path == Path::new("new.txt"))
        .expect("new.txt should appear in diff");

    assert_eq!(file.kind, FileChangeKind::Added);
    assert!(!file.is_binary);
    assert_eq!(file.insertions, 2);
    assert_eq!(file.deletions, 0);
    assert_eq!(diff.total_files, 1);
    assert_eq!(diff.total_insertions, 2);
    assert_eq!(diff.total_deletions, 0);
}

#[test]
fn from_working_tree_staged_text_then_rewritten_as_binary_recomputes_stats() {
    let (tempdir, repo) = init_repo();

    fs::write(tempdir.path().join("new.txt"), "hello\nworld\n").unwrap();

    let mut index = repo.index().unwrap();
    index.add_path(Path::new("new.txt")).unwrap();
    index.write().unwrap();

    fs::write(
        tempdir.path().join("new.txt"),
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR",
    )
    .unwrap();
    fs::create_dir_all(tempdir.path().join(".git/info")).unwrap();
    fs::write(
        tempdir.path().join(".git/info/attributes"),
        "new.txt binary\n",
    )
    .unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();
    let file = diff
        .files
        .iter()
        .find(|f| f.path == Path::new("new.txt"))
        .expect("new.txt should appear in diff");

    assert_eq!(file.kind, FileChangeKind::Added);
    assert_eq!(file.insertions, 0);
    assert_eq!(file.deletions, 0);
    assert_eq!(diff.total_files, 1);
    assert_eq!(diff.total_insertions, 0);
    assert_eq!(diff.total_deletions, 0);
}

#[test]
fn from_working_tree_staged_added_file_marked_binary_keeps_binary_classification() {
    let (tempdir, repo) = init_repo();

    fs::create_dir_all(tempdir.path().join(".git/info")).unwrap();
    fs::write(
        tempdir.path().join(".git/info/attributes"),
        "new.dat binary\n",
    )
    .unwrap();
    fs::write(tempdir.path().join("new.dat"), "hello\nworld\n").unwrap();

    let mut index = repo.index().unwrap();
    index.add_path(Path::new("new.dat")).unwrap();
    index.write().unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();
    let file = diff
        .files
        .iter()
        .find(|f| f.path == Path::new("new.dat"))
        .expect("new.dat should appear in diff");

    assert_eq!(file.kind, FileChangeKind::Added);
    assert!(file.is_binary);
    assert_eq!(file.insertions, 0);
    assert_eq!(file.deletions, 0);
    assert_eq!(diff.total_files, 1);
    assert_eq!(diff.total_insertions, 0);
    assert_eq!(diff.total_deletions, 0);
}

#[test]
fn from_working_tree_drops_staged_add_removed_in_worktree() {
    let (tempdir, repo) = init_repo();

    fs::write(tempdir.path().join("new.txt"), "line1\n").unwrap();

    let mut index = repo.index().unwrap();
    index.add_path(Path::new("new.txt")).unwrap();
    index.write().unwrap();

    fs::remove_file(tempdir.path().join("new.txt")).unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();

    assert_eq!(diff.total_files, 0);
    assert_eq!(diff.total_insertions, 0);
    assert_eq!(diff.total_deletions, 0);
    assert!(diff.files.is_empty());
}

#[test]
fn from_working_tree_drops_staged_change_reverted_in_worktree() {
    let (tempdir, repo) = init_repo();

    fs::write(tempdir.path().join("tracked.txt"), "staged change\n").unwrap();

    let mut index = repo.index().unwrap();
    index.add_path(Path::new("tracked.txt")).unwrap();
    index.write().unwrap();

    fs::write(tempdir.path().join("tracked.txt"), "tracked\n").unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();

    assert_eq!(diff.total_files, 0);
    assert_eq!(diff.total_insertions, 0);
    assert_eq!(diff.total_deletions, 0);
    assert!(diff.files.is_empty());
}

#[test]
fn from_working_tree_deleted_then_recreated_shows_modified() {
    let (tempdir, repo) = init_repo();

    // Stage deletion of tracked file (git rm)
    let mut index = repo.index().unwrap();
    index.remove(Path::new("tracked.txt"), 0).unwrap();
    index.write().unwrap();
    // Recreate the same file on disk (now untracked)
    fs::write(tempdir.path().join("tracked.txt"), "new content\n").unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();

    let file = diff
        .files
        .iter()
        .find(|f| f.path == Path::new("tracked.txt"))
        .expect("tracked.txt should appear in diff");

    // Deleted in index + Added as untracked → should show as Modified, not Deleted
    assert_eq!(file.kind, FileChangeKind::Modified);
    assert_eq!(diff.total_files, 1);
}

#[test]
fn from_working_tree_deleted_then_recreated_recomputes_overlapping_text_stats() {
    let (tempdir, repo) = init_repo();

    fs::write(tempdir.path().join("tracked.txt"), "a\nb\n").unwrap();

    let mut index = repo.index().unwrap();
    index.add_path(Path::new("tracked.txt")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = Signature::now("Test User", "test@example.com").unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "replace tracked.txt contents",
        &tree,
        &[&parent],
    )
    .unwrap();
    drop(tree);

    let mut index = repo.index().unwrap();
    index.remove(Path::new("tracked.txt"), 0).unwrap();
    index.write().unwrap();

    fs::write(tempdir.path().join("tracked.txt"), "a\nc\n").unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();
    let file = diff
        .files
        .iter()
        .find(|f| f.path == Path::new("tracked.txt"))
        .expect("tracked.txt should appear in diff");

    assert_eq!(file.kind, FileChangeKind::Modified);
    assert!(!file.is_binary);
    assert_eq!(file.insertions, 1);
    assert_eq!(file.deletions, 1);
    assert_eq!(diff.total_insertions, 1);
    assert_eq!(diff.total_deletions, 1);
}

#[test]
fn from_working_tree_deleted_binary_then_recreated_text_uses_text_stats() {
    let (tempdir, repo) = init_repo();

    fs::write(
        tempdir.path().join("tracked.txt"),
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR",
    )
    .unwrap();

    let mut index = repo.index().unwrap();
    index.add_path(Path::new("tracked.txt")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = Signature::now("Test User", "test@example.com").unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "replace tracked.txt with binary",
        &tree,
        &[&parent],
    )
    .unwrap();
    drop(tree);

    let mut index = repo.index().unwrap();
    index.remove(Path::new("tracked.txt"), 0).unwrap();
    index.write().unwrap();
    fs::write(
        tempdir.path().join("tracked.txt"),
        "new content\nsecond line\n",
    )
    .unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();
    let file = diff
        .files
        .iter()
        .find(|f| f.path == Path::new("tracked.txt"))
        .expect("tracked.txt should appear in diff");

    assert_eq!(file.kind, FileChangeKind::Modified);
    assert!(!file.is_binary);
    assert_eq!(file.insertions, 2);
    assert_eq!(file.deletions, 0);
}

#[test]
fn from_working_tree_binary_to_text_modified_recomputes_line_stats() {
    let (tempdir, repo) = init_repo();

    fs::write(
        tempdir.path().join("tracked.txt"),
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR",
    )
    .unwrap();

    let mut index = repo.index().unwrap();
    index.add_path(Path::new("tracked.txt")).unwrap();
    index.write().unwrap();

    fs::write(
        tempdir.path().join("tracked.txt"),
        "new content\nsecond line\n",
    )
    .unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();
    let file = diff
        .files
        .iter()
        .find(|f| f.path == Path::new("tracked.txt"))
        .expect("tracked.txt should appear in diff");

    assert_eq!(file.kind, FileChangeKind::Modified);
    assert!(!file.is_binary);
    assert_eq!(file.insertions, 2);
    assert_eq!(file.deletions, 1);
    assert_eq!(diff.total_insertions, 2);
    assert_eq!(diff.total_deletions, 1);
}

#[test]
fn from_working_tree_total_files_not_capped_by_display_limit() {
    let (tempdir, repo) = init_repo();

    // Create 55 untracked files (exceeds MAX_FILES_TO_DISPLAY of 50)
    for i in 0..55 {
        fs::write(
            tempdir.path().join(format!("untracked_{:03}.txt", i)),
            format!("content {}\n", i),
        )
        .unwrap();
    }

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();

    assert_eq!(diff.total_files, 55);
    assert_eq!(diff.total_insertions, 55);
    assert_eq!(diff.total_deletions, 0);
    assert!(diff.truncated);
    assert_eq!(diff.files.len(), 50);
    // "...and N more files" should show 5
    assert_eq!(diff.total_files - diff.files.len(), 5);
}

#[test]
fn from_working_tree_refreshes_totals_beyond_display_limit() {
    let (tempdir, repo) = init_repo();

    for i in 0..55 {
        fs::write(
            tempdir.path().join(format!("untracked_{:03}.txt", i)),
            format!("content {}\n", i),
        )
        .unwrap();
    }

    fs::write(tempdir.path().join("tracked.txt"), "staged change\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("tracked.txt")).unwrap();
    index.write().unwrap();

    fs::write(
        tempdir.path().join("tracked.txt"),
        "final change\nwith extra line\n",
    )
    .unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();

    assert_eq!(diff.total_files, 56);
    assert_eq!(diff.total_insertions, 57);
    assert_eq!(diff.total_deletions, 1);
    assert!(diff.truncated);
    assert_eq!(diff.files.len(), 50);
}

#[test]
fn from_working_tree_includes_untracked_binary_files_without_line_stats() {
    let (tempdir, repo) = init_repo();

    fs::write(
        tempdir.path().join("image.png"),
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR",
    )
    .unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();

    assert_eq!(diff.total_files, 1);
    assert_eq!(diff.total_insertions, 0);
    assert_eq!(diff.total_deletions, 0);
    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].path, Path::new("image.png"));
    assert_eq!(diff.files[0].kind, FileChangeKind::Added);
    assert!(diff.files[0].is_binary);
    assert!(!diff.truncated);
}

#[test]
fn from_working_tree_marks_untracked_file_binary_from_attributes() {
    let (tempdir, repo) = init_repo();

    fs::create_dir_all(tempdir.path().join(".git/info")).unwrap();
    fs::write(
        tempdir.path().join(".git/info/attributes"),
        "new.dat binary\n",
    )
    .unwrap();
    fs::write(tempdir.path().join("new.dat"), "hello\nworld\n").unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();
    let file = diff
        .files
        .iter()
        .find(|f| f.path == Path::new("new.dat"))
        .expect("new.dat should appear in diff");

    assert_eq!(file.kind, FileChangeKind::Added);
    assert!(file.is_binary);
    assert_eq!(file.insertions, 0);
    assert_eq!(file.deletions, 0);
    assert_eq!(diff.total_files, 1);
    assert_eq!(diff.total_insertions, 0);
    assert_eq!(diff.total_deletions, 0);
}

#[test]
fn from_working_tree_skips_untracked_directories() {
    let (tempdir, repo) = init_repo();

    Repository::init(tempdir.path().join("child")).unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();

    assert_eq!(diff.total_files, 0);
    assert_eq!(diff.total_insertions, 0);
    assert_eq!(diff.total_deletions, 0);
    assert!(diff.files.is_empty());
}

#[test]
fn from_commit_includes_binary_files_without_line_stats() {
    let (tempdir, repo) = init_repo();

    fs::write(
        tempdir.path().join("image.png"),
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR",
    )
    .unwrap();

    let mut index = repo.index().unwrap();
    index.add_path(Path::new("image.png")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = Signature::now("Test User", "test@example.com").unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();

    let oid = repo
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            "add binary",
            &tree,
            &[&parent],
        )
        .unwrap();

    let diff = CommitDiffInfo::from_commit(&repo, oid).unwrap();

    assert_eq!(diff.total_files, 1);
    assert_eq!(diff.total_insertions, 0);
    assert_eq!(diff.total_deletions, 0);
    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].path, Path::new("image.png"));
    assert_eq!(diff.files[0].kind, FileChangeKind::Added);
    assert!(diff.files[0].is_binary);
}

#[test]
fn from_working_tree_empty_untracked_file_has_zero_insertions() {
    let (tempdir, repo) = init_repo();

    fs::write(tempdir.path().join("empty.txt"), "").unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();

    assert_eq!(diff.total_files, 1);
    let file = diff
        .files
        .iter()
        .find(|f| f.path == Path::new("empty.txt"))
        .unwrap();
    assert_eq!(file.kind, FileChangeKind::Added);
    assert_eq!(file.insertions, 0);
    assert!(!file.is_binary);
}

#[test]
fn from_working_tree_no_trailing_newline_counts_correctly() {
    let (tempdir, repo) = init_repo();

    fs::write(
        tempdir.path().join("no_newline.txt"),
        "line without newline",
    )
    .unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();
    let file = diff
        .files
        .iter()
        .find(|f| f.path == Path::new("no_newline.txt"))
        .unwrap();
    assert_eq!(file.insertions, 1);
    assert!(!file.is_binary);
}

#[test]
fn working_tree_status_includes_untracked_binary_files() {
    let (tempdir, _repo) = init_repo();

    fs::write(
        tempdir.path().join("image.png"),
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR",
    )
    .unwrap();

    let git_repo = GitRepository::open(tempdir.path()).unwrap();
    let status = git_repo.get_working_tree_status().unwrap();

    assert!(status.is_some());
    let status = status.unwrap();
    assert_eq!(status.file_count(), 1);
    assert_eq!(status.file_paths, vec!["image.png".to_string()]);
}

#[test]
fn working_tree_status_skips_untracked_directories() {
    let (tempdir, _repo) = init_repo();

    Repository::init(tempdir.path().join("child")).unwrap();

    let git_repo = GitRepository::open(tempdir.path()).unwrap();
    let status = git_repo.get_working_tree_status().unwrap();

    assert!(status.is_none());
}

#[cfg(unix)]
#[test]
fn working_tree_status_includes_untracked_symlink_to_directory() {
    let (tempdir, repo) = init_repo();

    fs::create_dir_all(tempdir.path().join("dir")).unwrap();
    fs::write(tempdir.path().join("dir/file.txt"), "nested\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("dir/file.txt")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = Signature::now("Test User", "test@example.com").unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "add tracked dir",
        &tree,
        &[&parent],
    )
    .unwrap();
    drop(tree);

    symlink("dir", tempdir.path().join("linkdir")).unwrap();

    let git_repo = GitRepository::open(tempdir.path()).unwrap();
    let status = git_repo.get_working_tree_status().unwrap();

    assert!(status.is_some());
    let status = status.unwrap();
    assert_eq!(status.file_count(), 1);
    assert_eq!(status.file_paths, vec!["linkdir".to_string()]);
}

#[test]
fn from_working_tree_backfills_untracked_when_tracked_dropped() {
    let (tempdir, repo) = init_repo();

    // Stage additions that will be removed from the worktree → dropped by refresh
    for i in 0..3 {
        let name = format!("staged_{}.txt", i);
        fs::write(tempdir.path().join(&name), format!("line {}\n", i)).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new(&name)).unwrap();
        index.write().unwrap();
        fs::remove_file(tempdir.path().join(&name)).unwrap();
    }

    // Create untracked files that should fill the display
    for i in 0..3 {
        fs::write(
            tempdir.path().join(format!("untracked_{}.txt", i)),
            format!("content {}\n", i),
        )
        .unwrap();
    }

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();

    // The 3 staged-then-removed files should be dropped; only 3 untracked remain
    assert_eq!(diff.total_files, 3);
    assert_eq!(diff.files.len(), 3);
    assert!(!diff.truncated);
    for i in 0..3 {
        let name = format!("untracked_{}.txt", i);
        assert!(
            diff.files.iter().any(|f| f.path == Path::new(&name)),
            "untracked file {} should be in files list",
            name
        );
    }
}

#[test]
fn from_working_tree_deleted_text_then_recreated_binary_shows_zero_lines() {
    let (tempdir, repo) = init_repo();

    // tracked.txt already has "tracked\n" (1 line) from init_repo
    let mut index = repo.index().unwrap();
    index.remove(Path::new("tracked.txt"), 0).unwrap();
    index.write().unwrap();

    // Recreate as binary at the same path
    fs::write(
        tempdir.path().join("tracked.txt"),
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR",
    )
    .unwrap();

    let diff = CommitDiffInfo::from_working_tree(&repo).unwrap();
    let file = diff
        .files
        .iter()
        .find(|f| f.path == Path::new("tracked.txt"))
        .expect("tracked.txt should appear in diff");

    // Binary change → no line stats (consistent with git diff HEAD --numstat showing "- -")
    assert_eq!(file.kind, FileChangeKind::Modified);
    assert!(file.is_binary);
    assert_eq!(file.insertions, 0);
    assert_eq!(file.deletions, 0);
    assert_eq!(diff.total_insertions, 0);
    assert_eq!(diff.total_deletions, 0);
}
