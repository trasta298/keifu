use std::fs;
use std::path::Path;

use git2::{Oid, Repository, Signature};
use keifu::git::{collect_triage, default_base_branch, CommitDiffInfo, WorktreeInfo};
use tempfile::TempDir;

fn init_repo_with_main() -> (TempDir, Repository) {
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

    // Normalize the default branch name to "main" regardless of git config
    let head_name = repo.head().unwrap().shorthand().unwrap().to_string();
    if head_name != "main" {
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("main", &head_commit, false).unwrap();
        drop(head_commit);
        repo.set_head("refs/heads/main").unwrap();
        repo.find_branch(&head_name, git2::BranchType::Local)
            .unwrap()
            .delete()
            .unwrap();
    }

    (tempdir, repo)
}

fn commit_file(repo: &Repository, path: &str, contents: &str, message: &str) -> Oid {
    let workdir = repo.workdir().unwrap();
    fs::write(workdir.join(path), contents).unwrap();

    let mut index = repo.index().unwrap();
    index.add_path(Path::new(path)).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = Signature::now("Test User", "test@example.com").unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &[&parent],
    )
    .unwrap()
}

#[test]
fn default_base_branch_prefers_local_main() {
    let (_tempdir, repo) = init_repo_with_main();

    let (name, oid) = default_base_branch(&repo).unwrap();
    assert_eq!(name, "main");
    assert_eq!(oid, repo.head().unwrap().target().unwrap());
}

#[test]
fn triage_marks_merged_and_ahead_branches() {
    let (_tempdir, repo) = init_repo_with_main();
    let base_commit = repo.head().unwrap().peel_to_commit().unwrap();

    // Branch fully contained in main
    repo.branch("merged-work", &base_commit, false).unwrap();

    // Branch with an extra commit
    repo.branch("feature", &base_commit, false).unwrap();
    repo.set_head("refs/heads/feature").unwrap();
    commit_file(&repo, "feature.txt", "feature\n", "feature work");

    // Advance main by one commit so feature is also behind
    repo.set_head("refs/heads/main").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    commit_file(&repo, "main.txt", "main\n", "main work");

    let (base_name, rows) = collect_triage(&repo, &[]).unwrap();
    assert_eq!(base_name.as_deref(), Some("main"));

    let merged = rows.iter().find(|r| r.name == "merged-work").unwrap();
    assert!(merged.merged);
    assert_eq!(merged.ahead, 0);
    assert_eq!(merged.behind, 1);

    let feature = rows.iter().find(|r| r.name == "feature").unwrap();
    assert!(!feature.merged);
    assert_eq!(feature.ahead, 1);
    assert_eq!(feature.behind, 1);

    let main = rows.iter().find(|r| r.name == "main").unwrap();
    assert!(main.is_base);
    assert!(main.is_head);
    assert!(!main.merged);

    // HEAD is pinned first
    assert_eq!(rows.first().unwrap().name, "main");
}

#[test]
fn worktree_listing_reports_branch_and_path() {
    let (tempdir, repo) = init_repo_with_main();

    let wt_path = tempdir.path().join("wt-feature");
    repo.worktree("wt-feature", &wt_path, None).unwrap();

    let worktrees = WorktreeInfo::list_all(&repo).unwrap();
    assert_eq!(worktrees.len(), 1);
    assert_eq!(worktrees[0].name, "wt-feature");
    assert_eq!(worktrees[0].branch.as_deref(), Some("wt-feature"));
    assert_eq!(
        worktrees[0].path.canonicalize().unwrap(),
        wt_path.canonicalize().unwrap()
    );

    // Triage rows pick up the worktree path
    let (_, rows) = collect_triage(&repo, &worktrees).unwrap();
    let row = rows.iter().find(|r| r.name == "wt-feature").unwrap();
    assert!(row.worktree_path.is_some());
}

#[test]
fn from_range_shows_branch_changes_only() {
    let (_tempdir, repo) = init_repo_with_main();
    let base_commit = repo.head().unwrap().peel_to_commit().unwrap();
    let base_oid = base_commit.id();

    // feature: two commits touching feature.txt
    repo.branch("feature", &base_commit, false).unwrap();
    repo.set_head("refs/heads/feature").unwrap();
    commit_file(&repo, "feature.txt", "one\n", "feat 1");
    let feature_tip = commit_file(&repo, "feature.txt", "one\ntwo\n", "feat 2");

    // main advances independently
    repo.set_head("refs/heads/main").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    commit_file(&repo, "main.txt", "main\n", "main work");

    let merge_base = repo
        .merge_base(repo.head().unwrap().target().unwrap(), feature_tip)
        .unwrap();
    assert_eq!(merge_base, base_oid);

    let diff = CommitDiffInfo::from_range(&repo, merge_base, feature_tip).unwrap();
    assert_eq!(diff.total_files, 1);
    assert_eq!(diff.files[0].path, Path::new("feature.txt"));
    assert_eq!(diff.files[0].insertions, 2);
    // main.txt (added on main after the merge base) must not appear
    assert!(diff.files.iter().all(|f| f.path != Path::new("main.txt")));
}
