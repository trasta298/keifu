//! Branch info structure and operations

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use git2::{BranchType, Oid, Repository};

#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub name: String,
    pub is_head: bool,
    pub is_remote: bool,
    pub remote_name: Option<String>,
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
                        remote_name: None,
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
                            remote_name: reference
                                .name()
                                .and_then(|name| repo.branch_remote_name(name).ok())
                                .and_then(|name| name.as_str().map(str::to_string))
                                // Remote-tracking refs can outlive their remote configuration.
                                .or_else(|| {
                                    name.split_once('/').map(|(remote, _)| remote.to_string())
                                }),
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

/// A selectable branch and the remotes whose same-named branch shares its commit.
#[derive(Debug, Clone)]
pub struct BranchLabel {
    pub name: String,
    pub remote_suffix: String,
}

/// Group refs on one commit, keeping unmatched remote refs in their original order.
pub(crate) fn group_branch_labels(branches: &[&BranchInfo]) -> Vec<BranchLabel> {
    let locals: HashSet<_> = branches
        .iter()
        .filter(|branch| !branch.is_remote)
        .map(|branch| branch.name.as_str())
        .collect();
    let mut remotes: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut grouped = HashSet::new();
    for branch in branches.iter().filter(|branch| branch.is_remote) {
        if let Some(remote) = branch.remote_name.as_deref() {
            if let Some(local) = branch.name.strip_prefix(&format!("{remote}/")) {
                if locals.contains(local) {
                    remotes.entry(local).or_default().push(remote);
                    grouped.insert(branch.name.as_str());
                }
            }
        }
    }
    branches
        .iter()
        .filter(|branch| !branch.is_remote || !grouped.contains(branch.name.as_str()))
        .map(|branch| {
            let mut matching = remotes.remove(branch.name.as_str()).unwrap_or_default();
            matching.sort_unstable();
            matching.dedup();
            BranchLabel {
                name: branch.name.clone(),
                remote_suffix: if matching.is_empty() {
                    String::new()
                } else {
                    format!(" ↔ {}", matching.join(", "))
                },
            }
        })
        .collect()
}
