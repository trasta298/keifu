//! Git操作（checkout, merge, rebase, ブランチ操作）

use anyhow::{bail, Context, Result};
use git2::{BranchType, Oid, Repository};

/// ブランチをチェックアウト
pub fn checkout_branch(repo: &Repository, branch_name: &str) -> Result<()> {
    let branch = repo
        .find_branch(branch_name, BranchType::Local)
        .context(format!("ブランチ '{}' が見つかりません", branch_name))?;

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
        .context("コミットが見つかりません")?;
    let tree = commit.tree()?;

    repo.checkout_tree(tree.as_object(), None)?;
    repo.set_head_detached(oid)?;

    Ok(())
}

/// 新しいブランチを作成
pub fn create_branch(repo: &Repository, branch_name: &str, from_oid: Oid) -> Result<()> {
    let commit = repo
        .find_commit(from_oid)
        .context("コミットが見つかりません")?;

    repo.branch(branch_name, &commit, false)
        .context(format!("ブランチ '{}' の作成に失敗しました", branch_name))?;

    Ok(())
}

/// ブランチを削除
pub fn delete_branch(repo: &Repository, branch_name: &str) -> Result<()> {
    let mut branch = repo
        .find_branch(branch_name, BranchType::Local)
        .context(format!("ブランチ '{}' が見つかりません", branch_name))?;

    if branch.is_head() {
        bail!("現在のブランチは削除できません");
    }

    branch.delete()?;
    Ok(())
}

/// マージを実行
pub fn merge_branch(repo: &Repository, branch_name: &str) -> Result<()> {
    let branch = repo
        .find_branch(branch_name, BranchType::Local)
        .context(format!("ブランチ '{}' が見つかりません", branch_name))?;

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
            bail!(
                "マージコンフリクトが発生しました。手動で解決してください。"
            );
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
        .context(format!("ブランチ '{}' が見つかりません", onto_branch))?;

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
