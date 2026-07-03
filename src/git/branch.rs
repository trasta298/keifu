//! Branch info structure and operations

use std::path::PathBuf;

use anyhow::Result;
use chrono::{DateTime, Local, TimeZone};
use git2::{BranchType, Oid, Repository};

use super::worktree::WorktreeInfo;

#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub name: String,
    pub is_head: bool,
    pub is_remote: bool,
    pub upstream: Option<String>,
    pub tip_oid: Oid,
}

impl BranchInfo {
    pub fn list_all(repo: &Repository, include_remotes: bool) -> Result<Vec<Self>> {
        let mut branches = Vec::new();

        // Get HEAD
        let head_oid = repo.head().ok().and_then(|r| r.target());

        // Local branches
        for branch_result in repo.branches(Some(BranchType::Local))? {
            let (branch, _) = branch_result?;
            if let Some(name) = branch.name()? {
                let reference = branch.get();
                if let Some(oid) = reference.target() {
                    let is_head = head_oid.map(|h| h == oid).unwrap_or(false)
                        && repo
                            .head()
                            .ok()
                            .and_then(|h| h.shorthand().map(|s| s == name))
                            .unwrap_or(false);

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

        if include_remotes {
            // Remote branches
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
        }

        // Put the HEAD branch first
        branches.sort_by(|a, b| b.is_head.cmp(&a.is_head).then(a.name.cmp(&b.name)));

        Ok(branches)
    }
}

/// Detect the base branch to compare other branches against.
/// Prefers origin/HEAD's target, then "main", then "master"; a local branch
/// wins over its remote counterpart. Returns (display name, tip oid).
pub fn default_base_branch(repo: &Repository) -> Option<(String, Oid)> {
    let from_origin_head = repo
        .find_reference("refs/remotes/origin/HEAD")
        .ok()
        .and_then(|r| r.symbolic_target().map(String::from))
        .and_then(|t| t.strip_prefix("refs/remotes/origin/").map(String::from));

    let candidates = from_origin_head
        .into_iter()
        .chain(["main".to_string(), "master".to_string()]);

    for name in candidates {
        if let Ok(branch) = repo.find_branch(&name, BranchType::Local) {
            if let Some(oid) = branch.get().target() {
                return Some((name, oid));
            }
        }
        let remote_name = format!("origin/{name}");
        if let Ok(branch) = repo.find_branch(&remote_name, BranchType::Remote) {
            if let Some(oid) = branch.get().target() {
                return Some((remote_name, oid));
            }
        }
    }
    None
}

/// One row of the branch triage list
#[derive(Debug, Clone)]
pub struct BranchTriageRow {
    pub name: String,
    pub tip_oid: Oid,
    pub is_head: bool,
    pub is_base: bool,
    pub ahead: usize,
    pub behind: usize,
    /// Fully contained in the base branch (safe to delete)
    pub merged: bool,
    pub last_commit: DateTime<Local>,
    pub subject: String,
    pub worktree_path: Option<PathBuf>,
}

/// Collect triage info for all local branches, newest activity first
/// (HEAD pinned to the top). Returns (base branch name, rows).
pub fn collect_triage(
    repo: &Repository,
    worktrees: &[WorktreeInfo],
) -> Result<(Option<String>, Vec<BranchTriageRow>)> {
    let base = default_base_branch(repo);
    let mut rows = Vec::new();

    for branch_result in repo.branches(Some(BranchType::Local))? {
        let (branch, _) = branch_result?;
        let Some(name) = branch.name()? else {
            continue;
        };
        let Some(tip_oid) = branch.get().target() else {
            continue;
        };
        let commit = repo.find_commit(tip_oid)?;
        let last_commit = Local
            .timestamp_opt(commit.time().seconds(), 0)
            .single()
            .unwrap_or_else(Local::now);
        let subject = commit.summary().unwrap_or("").to_string();

        let is_base = base
            .as_ref()
            .is_some_and(|(base_name, _)| base_name == name);
        let (ahead, behind) = match &base {
            Some((_, base_oid)) if !is_base => repo.graph_ahead_behind(tip_oid, *base_oid)?,
            _ => (0, 0),
        };

        rows.push(BranchTriageRow {
            name: name.to_string(),
            tip_oid,
            is_head: branch.is_head(),
            is_base,
            ahead,
            behind,
            merged: base.is_some() && !is_base && ahead == 0,
            last_commit,
            subject,
            worktree_path: worktrees
                .iter()
                .find(|w| w.branch.as_deref() == Some(name))
                .map(|w| w.path.clone()),
        });
    }

    rows.sort_by(|a, b| {
        b.is_head
            .cmp(&a.is_head)
            .then(b.last_commit.cmp(&a.last_commit))
            .then(a.name.cmp(&b.name))
    });

    Ok((base.map(|(name, _)| name), rows))
}
