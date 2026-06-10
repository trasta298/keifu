//! Git operations (checkout, merge, rebase, branch operations)

use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use git2::{BranchType, IndexAddOption, Oid, Repository};

/// Checkout a branch
pub fn checkout_branch(repo: &Repository, branch_name: &str) -> Result<()> {
    let branch = repo
        .find_branch(branch_name, BranchType::Local)
        .context(format!("Branch '{}' not found", branch_name))?;

    let reference = branch.get();
    let commit = reference.peel_to_commit()?;
    let tree = commit.tree()?;

    repo.checkout_tree(tree.as_object(), None)?;
    repo.set_head(reference.name().unwrap())?;

    Ok(())
}

/// Checkout a commit (detached HEAD)
pub fn checkout_commit(repo: &Repository, oid: Oid) -> Result<()> {
    let commit = repo.find_commit(oid).context("Commit not found")?;
    let tree = commit.tree()?;

    repo.checkout_tree(tree.as_object(), None)?;
    repo.set_head_detached(oid)?;

    Ok(())
}

/// Checkout a remote branch (create and track a local branch)
pub fn checkout_remote_branch(repo: &Repository, remote_branch: &str) -> Result<()> {
    // Extract "branch-name" from "origin/branch-name"
    let local_name = remote_branch
        .strip_prefix("origin/")
        .context("Invalid remote branch format")?;

    // Look up the remote branch
    let remote_ref = repo
        .find_branch(remote_branch, BranchType::Remote)
        .context(format!("Remote branch '{}' not found", remote_branch))?;

    let remote_commit = remote_ref.get().peel_to_commit()?;
    let remote_oid = remote_commit.id();
    let tree = remote_commit.tree()?;

    // Check if a local branch with the same name exists
    if let Ok(local_branch) = repo.find_branch(local_name, BranchType::Local) {
        // Get OIDs via peel_to_commit() for a reliable comparison
        let local_commit = local_branch.get().peel_to_commit()?;
        let local_oid = local_commit.id();
        if local_oid == remote_oid {
            // Local and remote point to the same commit -> checkout local branch
            return checkout_branch(repo, local_name);
        } else {
            // Pointing to different commits -> update local branch and checkout
            // Equivalent to: git checkout -B local_name origin/xxx
            let is_current_branch = local_branch.is_head();
            drop(local_branch); // Release the branch reference

            let refname = format!("refs/heads/{}", local_name);
            if is_current_branch {
                // Cannot force update current branch with repo.branch()
                // Update the reference directly after checkout
                repo.checkout_tree(tree.as_object(), None)?;
                repo.reference(&refname, remote_oid, true, "Update to remote")?;
            } else {
                repo.branch(local_name, &remote_commit, true)?; // Overwrite with force=true
                repo.checkout_tree(tree.as_object(), None)?;
                repo.set_head(&refname)?;
            }
            return Ok(());
        }
    }

    // No local branch -> create and track
    let mut local_branch = repo
        .branch(local_name, &remote_commit, false)
        .context(format!("Failed to create local branch '{}'", local_name))?;

    // Set upstream
    local_branch.set_upstream(Some(remote_branch))?;

    // Checkout
    repo.checkout_tree(tree.as_object(), None)?;
    repo.set_head(&format!("refs/heads/{}", local_name))?;

    Ok(())
}

/// Create a new branch
pub fn create_branch(repo: &Repository, branch_name: &str, from_oid: Oid) -> Result<()> {
    let commit = repo.find_commit(from_oid).context("Commit not found")?;

    repo.branch(branch_name, &commit, false)
        .context(format!("Failed to create branch '{}'", branch_name))?;

    Ok(())
}

/// Delete a branch
pub fn delete_branch(repo: &Repository, branch_name: &str) -> Result<()> {
    let mut branch = repo
        .find_branch(branch_name, BranchType::Local)
        .context(format!("Branch '{}' not found", branch_name))?;

    if branch.is_head() {
        bail!("Cannot delete current branch");
    }

    branch.delete()?;
    Ok(())
}

/// Perform a merge
pub fn merge_branch(repo: &Repository, branch_name: &str) -> Result<()> {
    let branch = repo
        .find_branch(branch_name, BranchType::Local)
        .context(format!("Branch '{}' not found", branch_name))?;

    let reference = branch.get();
    let annotated_commit = repo.reference_to_annotated_commit(reference)?;

    let (analysis, _) = repo.merge_analysis(&[&annotated_commit])?;

    if analysis.is_up_to_date() {
        return Ok(());
    }

    if analysis.is_fast_forward() {
        // Fast-forward merge
        let target_oid = reference.target().unwrap();
        let target_commit = repo.find_commit(target_oid)?;
        let tree = target_commit.tree()?;

        repo.checkout_tree(tree.as_object(), None)?;

        let mut head_ref = repo.head()?;
        head_ref.set_target(target_oid, &format!("Fast-forward merge: {}", branch_name))?;

        return Ok(());
    }

    if analysis.is_normal() {
        // Normal merge
        repo.merge(&[&annotated_commit], None, None)?;

        if repo.index()?.has_conflicts() {
            bail!("Merge conflict occurred. Please resolve manually.");
        }

        // Create a merge commit
        let signature = repo.signature()?;
        let head = repo.head()?;
        let head_commit = head.peel_to_commit()?;
        let merge_commit = repo.find_commit(annotated_commit.id())?;
        let tree_oid = repo.index()?.write_tree()?;
        let tree = repo.find_tree(tree_oid)?;

        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            &format!("Merge branch '{}'", branch_name),
            &tree,
            &[&head_commit, &merge_commit],
        )?;

        repo.cleanup_state()?;
    }

    Ok(())
}

/// Perform a rebase (simple implementation)
pub fn rebase_branch(repo: &Repository, onto_branch: &str) -> Result<()> {
    let onto = repo
        .find_branch(onto_branch, BranchType::Local)
        .context(format!("Branch '{}' not found", onto_branch))?;

    let onto_annotated = repo.reference_to_annotated_commit(onto.get())?;

    let mut rebase = repo.rebase(None, Some(&onto_annotated), None, None)?;

    while let Some(op) = rebase.next() {
        let _operation = op?;
        let signature = repo.signature()?;
        rebase.commit(None, &signature, None)?;
    }

    rebase.finish(None)?;

    Ok(())
}

/// Stage a single path (add to the index, or remove for deleted files)
pub fn stage_path(repo: &Repository, path: &Path) -> Result<()> {
    let workdir = repo
        .workdir()
        .context("Repository has no working directory")?;
    let mut index = repo.index()?;
    if workdir.join(path).exists() {
        index.add_path(path)?;
    } else {
        index.remove_path(path)?;
    }
    index.write()?;
    Ok(())
}

/// Unstage a single path (reset the index entry to HEAD)
pub fn unstage_path(repo: &Repository, path: &Path) -> Result<()> {
    match repo.head() {
        Ok(head) => {
            let commit = head.peel(git2::ObjectType::Commit)?;
            repo.reset_default(Some(&commit), [path])?;
        }
        Err(_) => {
            // Unborn HEAD: unstaging means removing the entry from the index
            let mut index = repo.index()?;
            index.remove_path(path)?;
            index.write()?;
        }
    }
    Ok(())
}

/// Stage all changes (tracked modifications, deletions, and untracked files)
pub fn stage_all(repo: &Repository) -> Result<()> {
    let mut index = repo.index()?;
    index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
    index.update_all(["*"].iter(), None)?;
    index.write()?;
    Ok(())
}

/// Unstage all changes (reset the index to HEAD)
pub fn unstage_all(repo: &Repository) -> Result<()> {
    match repo.head() {
        Ok(head) => {
            let commit = head.peel(git2::ObjectType::Commit)?;
            repo.reset_default(Some(&commit), ["*"])?;
        }
        Err(_) => {
            let mut index = repo.index()?;
            index.clear()?;
            index.write()?;
        }
    }
    Ok(())
}

/// Create a commit from the current index
pub fn create_commit(repo: &Repository, message: &str) -> Result<Oid> {
    let signature = repo
        .signature()
        .context("Cannot determine author (set user.name and user.email)")?;

    let mut index = repo.index()?;
    let tree_oid = index.write_tree()?;
    let tree = repo.find_tree(tree_oid)?;

    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    match &parent {
        Some(parent_commit) if parent_commit.tree_id() == tree_oid => {
            bail!("No staged changes to commit")
        }
        None if index.is_empty() => bail!("No staged changes to commit"),
        _ => {}
    }

    let parents: Vec<&git2::Commit> = parent.iter().collect();
    let oid = repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &parents,
    )?;
    Ok(oid)
}

/// Push the given branch to origin using git command (sets upstream)
pub fn push_branch(repo_path: &str, branch: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["push", "--set-upstream", "origin", branch])
        .current_dir(repo_path)
        .output()
        .context("Failed to execute git push")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git push failed: {}", stderr.trim());
    }

    Ok(())
}

/// Fetch from origin remote using git command
pub fn fetch_origin(repo_path: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["fetch", "origin"])
        .current_dir(repo_path)
        .output()
        .context("Failed to execute git fetch")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git fetch failed: {}", stderr.trim());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use git2::Signature;
    use tempfile::TempDir;

    use super::*;
    use crate::git::{GitRepository, StageState};

    fn init_repo_with_commit() -> (TempDir, Repository) {
        let tempdir = tempfile::tempdir().unwrap();
        let repo = Repository::init(tempdir.path()).unwrap();
        fs::write(tempdir.path().join("base.txt"), "base\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("base.txt")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = Signature::now("Test", "test@example.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        drop(tree);
        (tempdir, repo)
    }

    fn state_of(tempdir: &TempDir, path: &str) -> Option<StageState> {
        let repo = GitRepository::open(tempdir.path()).unwrap();
        repo.stage_states().unwrap().get(Path::new(path)).copied()
    }

    #[test]
    fn stage_and_unstage_untracked_file() {
        let (tempdir, repo) = init_repo_with_commit();
        fs::write(tempdir.path().join("new.txt"), "hello\n").unwrap();
        assert_eq!(state_of(&tempdir, "new.txt"), Some(StageState::Unstaged));

        stage_path(&repo, Path::new("new.txt")).unwrap();
        assert_eq!(state_of(&tempdir, "new.txt"), Some(StageState::Staged));

        unstage_path(&repo, Path::new("new.txt")).unwrap();
        assert_eq!(state_of(&tempdir, "new.txt"), Some(StageState::Unstaged));
    }

    #[test]
    fn stage_deleted_file() {
        let (tempdir, repo) = init_repo_with_commit();
        fs::remove_file(tempdir.path().join("base.txt")).unwrap();
        assert_eq!(state_of(&tempdir, "base.txt"), Some(StageState::Unstaged));

        stage_path(&repo, Path::new("base.txt")).unwrap();
        assert_eq!(state_of(&tempdir, "base.txt"), Some(StageState::Staged));
    }

    #[test]
    fn stage_all_and_unstage_all() {
        let (tempdir, repo) = init_repo_with_commit();
        fs::write(tempdir.path().join("a.txt"), "a\n").unwrap();
        fs::write(tempdir.path().join("base.txt"), "modified\n").unwrap();

        stage_all(&repo).unwrap();
        assert_eq!(state_of(&tempdir, "a.txt"), Some(StageState::Staged));
        assert_eq!(state_of(&tempdir, "base.txt"), Some(StageState::Staged));

        unstage_all(&repo).unwrap();
        assert_eq!(state_of(&tempdir, "a.txt"), Some(StageState::Unstaged));
        assert_eq!(state_of(&tempdir, "base.txt"), Some(StageState::Unstaged));
    }

    #[test]
    fn partially_staged_file_is_reported_as_partial() {
        let (tempdir, repo) = init_repo_with_commit();
        fs::write(tempdir.path().join("base.txt"), "staged change\n").unwrap();
        stage_path(&repo, Path::new("base.txt")).unwrap();
        fs::write(tempdir.path().join("base.txt"), "staged + unstaged\n").unwrap();

        assert_eq!(state_of(&tempdir, "base.txt"), Some(StageState::Partial));
    }

    #[test]
    fn create_commit_commits_staged_changes_only() {
        let (tempdir, repo) = init_repo_with_commit();
        repo.config().unwrap().set_str("user.name", "Test").unwrap();
        repo.config()
            .unwrap()
            .set_str("user.email", "test@example.com")
            .unwrap();

        fs::write(tempdir.path().join("staged.txt"), "s\n").unwrap();
        fs::write(tempdir.path().join("unstaged.txt"), "u\n").unwrap();
        stage_path(&repo, Path::new("staged.txt")).unwrap();

        let oid = create_commit(&repo, "add staged.txt").unwrap();
        let commit = repo.find_commit(oid).unwrap();
        assert_eq!(commit.message(), Some("add staged.txt"));
        let tree = commit.tree().unwrap();
        assert!(tree.get_name("staged.txt").is_some());
        assert!(tree.get_name("unstaged.txt").is_none());

        // Unstaged file remains in the working tree
        assert_eq!(
            state_of(&tempdir, "unstaged.txt"),
            Some(StageState::Unstaged)
        );
    }

    #[test]
    fn create_commit_rejects_empty_index() {
        let (_tempdir, repo) = init_repo_with_commit();
        repo.config().unwrap().set_str("user.name", "Test").unwrap();
        repo.config()
            .unwrap()
            .set_str("user.email", "test@example.com")
            .unwrap();

        let err = create_commit(&repo, "empty").unwrap_err();
        assert!(err.to_string().contains("No staged changes"));
    }
}
