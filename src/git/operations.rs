//! Git操作（checkout, merge, rebase, ブランチ操作）

use anyhow::{bail, Context, Result};
use git2::{BranchType, Oid, Repository};

/// ブランチをチェックアウト
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

/// コミットをチェックアウト（detached HEAD）
pub fn checkout_commit(repo: &Repository, oid: Oid) -> Result<()> {
    let commit = repo
        .find_commit(oid)
        .context("Commit not found")?;
    let tree = commit.tree()?;

    repo.checkout_tree(tree.as_object(), None)?;
    repo.set_head_detached(oid)?;

    Ok(())
}

/// リモートブランチをチェックアウト（ローカルブランチを作成して追跡）
pub fn checkout_remote_branch(repo: &Repository, remote_branch: &str) -> Result<()> {
    // "origin/branch-name" から "branch-name" を抽出
    let local_name = remote_branch
        .strip_prefix("origin/")
        .context("Invalid remote branch format")?;

    // リモートブランチを取得
    let remote_ref = repo
        .find_branch(remote_branch, BranchType::Remote)
        .context(format!("Remote branch '{}' not found", remote_branch))?;

    let remote_commit = remote_ref.get().peel_to_commit()?;
    let remote_oid = remote_commit.id();
    let tree = remote_commit.tree()?;

    // 同名のローカルブランチが既に存在するか確認
    if let Ok(local_branch) = repo.find_branch(local_name, BranchType::Local) {
        // 両方とも peel_to_commit() を使ってOIDを取得（確実な比較のため）
        let local_commit = local_branch.get().peel_to_commit()?;
        let local_oid = local_commit.id();
        if local_oid == remote_oid {
            // ローカルとリモートが同じコミットを指している → ローカルブランチをチェックアウト
            return checkout_branch(repo, local_name);
        } else {
            // 異なるコミットを指している → ローカルブランチを更新してチェックアウト
            // git checkout -B local_name origin/xxx と同等
            drop(local_branch); // ブランチへの参照を解放
            repo.branch(local_name, &remote_commit, true)?; // force=true で上書き
            repo.checkout_tree(tree.as_object(), None)?;
            repo.set_head(&format!("refs/heads/{}", local_name))?;
            return Ok(());
        }
    }

    // ローカルブランチが存在しない → 新規作成して追跡
    let mut local_branch = repo
        .branch(local_name, &remote_commit, false)
        .context(format!("Failed to create local branch '{}'", local_name))?;

    // アップストリームを設定
    local_branch.set_upstream(Some(remote_branch))?;

    // チェックアウト
    repo.checkout_tree(tree.as_object(), None)?;
    repo.set_head(&format!("refs/heads/{}", local_name))?;

    Ok(())
}

/// 新しいブランチを作成
pub fn create_branch(repo: &Repository, branch_name: &str, from_oid: Oid) -> Result<()> {
    let commit = repo
        .find_commit(from_oid)
        .context("Commit not found")?;

    repo.branch(branch_name, &commit, false)
        .context(format!("Failed to create branch '{}'", branch_name))?;

    Ok(())
}

/// ブランチを削除
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

/// マージを実行
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
        // Fast-forward マージ
        let target_oid = reference.target().unwrap();
        let target_commit = repo.find_commit(target_oid)?;
        let tree = target_commit.tree()?;

        repo.checkout_tree(tree.as_object(), None)?;

        let mut head_ref = repo.head()?;
        head_ref.set_target(target_oid, &format!("Fast-forward merge: {}", branch_name))?;

        return Ok(());
    }

    if analysis.is_normal() {
        // 通常のマージ
        repo.merge(&[&annotated_commit], None, None)?;

        if repo.index()?.has_conflicts() {
            bail!("Merge conflict occurred. Please resolve manually.");
        }

        // マージコミットを作成
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

/// リベースを実行（シンプルな実装）
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
