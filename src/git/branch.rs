//! ブランチ情報の構造体と操作

use anyhow::Result;
use git2::{BranchType, Oid, Repository};

#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub name: String,
    pub is_head: bool,
    pub is_remote: bool,
    pub upstream: Option<String>,
    pub tip_oid: Oid,
}

impl BranchInfo {
    pub fn list_all(repo: &Repository) -> Result<Vec<Self>> {
        let mut branches = Vec::new();

        // HEADの取得
        let head_oid = repo.head().ok().and_then(|r| r.target());

        // ローカルブランチ
        for branch_result in repo.branches(Some(BranchType::Local))? {
            let (branch, _) = branch_result?;
            if let Some(name) = branch.name()? {
                let reference = branch.get();
                if let Some(oid) = reference.target() {
                    let is_head = head_oid.map(|h| h == oid).unwrap_or(false)
                        && repo.head().ok().and_then(|h| h.shorthand().map(|s| s == name)).unwrap_or(false);

                    let upstream = branch
                        .upstream()
                        .ok()
                        .and_then(|u| u.name().ok().flatten().map(|s| s.to_string()));

                    branches.push(BranchInfo {
                        name: name.to_string(),
                        is_head,
                        is_remote: false,
                        upstream,
                        tip_oid: oid,
                    });
                }
            }
        }

        // リモートブランチ
        for branch_result in repo.branches(Some(BranchType::Remote))? {
            let (branch, _) = branch_result?;
            if let Some(name) = branch.name()? {
                let reference = branch.get();
                if let Some(oid) = reference.target() {
                    branches.push(BranchInfo {
                        name: name.to_string(),
                        is_head: false,
                        is_remote: true,
                        upstream: None,
                        tip_oid: oid,
                    });
                }
            }
        }

        // HEADのブランチを先頭に
        branches.sort_by(|a, b| b.is_head.cmp(&a.is_head).then(a.name.cmp(&b.name)));

        Ok(branches)
    }
}
