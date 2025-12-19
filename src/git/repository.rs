//! Repository操作ラッパー

use std::path::Path;

use anyhow::{Context, Result};
use git2::Repository;

use super::{BranchInfo, CommitInfo};

pub struct GitRepository {
    pub repo: Repository,
    pub path: String,
}

impl GitRepository {
    /// カレントディレクトリからリポジトリを検出
    pub fn discover() -> Result<Self> {
        let repo = Repository::discover(".")
            .context("Gitリポジトリが見つかりません。Gitリポジトリ内で実行してください。")?;
        let path = repo
            .workdir()
            .unwrap_or_else(|| repo.path())
            .to_string_lossy()
            .to_string();
        Ok(Self { repo, path })
    }

    /// 指定パスからリポジトリを開く
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let repo = Repository::open(path.as_ref())
            .context("指定されたパスにGitリポジトリが見つかりません。")?;
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

    /// コミット履歴を取得（新しい順）
    pub fn get_commits(&self, max_count: usize) -> Result<Vec<CommitInfo>> {
        let mut revwalk = self.repo.revwalk()?;
        revwalk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)?;

        // すべてのブランチを対象にする
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

    /// ブランチ一覧を取得
    pub fn get_branches(&self) -> Result<Vec<BranchInfo>> {
        BranchInfo::list_all(&self.repo)
    }

    /// 現在のHEADの名前を取得
    pub fn head_name(&self) -> Option<String> {
        self.repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(|s| s.to_string()))
    }
}
