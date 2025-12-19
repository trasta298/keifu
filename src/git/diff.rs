//! コミット差分情報

use std::path::PathBuf;

use anyhow::Result;
use git2::{Delta, Diff, DiffOptions, Oid, Repository};

/// 表示するファイルの最大数
const MAX_FILES_TO_DISPLAY: usize = 50;

/// ファイルの変更種別
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
}

/// 個別ファイルの差分情報
#[derive(Debug, Clone)]
pub struct FileDiffInfo {
    /// ファイルパス
    pub path: PathBuf,
    /// 変更種別
    pub kind: FileChangeKind,
    /// 追加行数
    pub insertions: usize,
    /// 削除行数
    pub deletions: usize,
}

/// コミットの差分情報
#[derive(Debug, Clone, Default)]
pub struct CommitDiffInfo {
    /// 変更ファイル一覧（最大MAX_FILES_TO_DISPLAY件）
    pub files: Vec<FileDiffInfo>,
    /// 合計追加行数
    pub total_insertions: usize,
    /// 合計削除行数
    pub total_deletions: usize,
    /// 総ファイル数
    pub total_files: usize,
    /// 切り捨てられたかどうか
    pub truncated: bool,
}

impl CommitDiffInfo {
    /// コミットの差分情報を取得
    /// - 通常コミット: 親との差分
    /// - マージコミット: 最初の親との差分
    /// - 初期コミット: 空treeとの差分
    pub fn from_commit(repo: &Repository, commit_oid: Oid) -> Result<Self> {
        let commit = repo.find_commit(commit_oid)?;
        let new_tree = commit.tree()?;

        // 親treeを取得（初期コミットの場合はNone）
        let old_tree = if commit.parent_count() > 0 {
            Some(commit.parent(0)?.tree()?)
        } else {
            None
        };

        // diff生成（高速化オプション）
        let mut opts = DiffOptions::new();
        opts.minimal(false);           // 最小差分計算をスキップ
        opts.ignore_submodules(true);  // サブモジュールをスキップ
        opts.context_lines(0);         // コンテキスト行を0に

        let diff = repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut opts))?;

        Self::from_diff(&diff)
    }

    fn from_diff(diff: &Diff) -> Result<Self> {
        let total_files = diff.deltas().len();
        let truncated = total_files > MAX_FILES_TO_DISPLAY;

        // ファイル情報を収集（上限まで）
        let mut files: Vec<FileDiffInfo> = Vec::with_capacity(MAX_FILES_TO_DISPLAY.min(total_files));

        for delta_idx in 0..total_files.min(MAX_FILES_TO_DISPLAY) {
            let delta = diff.get_delta(delta_idx).unwrap();

            // バイナリファイルはスキップ
            if delta.flags().is_binary() {
                continue;
            }

            let kind = match delta.status() {
                Delta::Added => FileChangeKind::Added,
                Delta::Deleted => FileChangeKind::Deleted,
                Delta::Modified => FileChangeKind::Modified,
                Delta::Renamed => FileChangeKind::Renamed,
                Delta::Copied => FileChangeKind::Copied,
                _ => continue,
            };

            let path = if kind == FileChangeKind::Deleted {
                delta.old_file().path()
            } else {
                delta.new_file().path()
            };

            if let Some(p) = path {
                files.push(FileDiffInfo {
                    path: p.to_path_buf(),
                    kind,
                    insertions: 0,
                    deletions: 0,
                });
            }
        }

        // 行数をカウント（バイナリはスキップ済み）
        let mut total_insertions = 0;
        let mut total_deletions = 0;

        diff.foreach(
            &mut |_delta, _progress| true,
            None,
            None,
            Some(&mut |delta, _hunk, line| {
                // バイナリはスキップ
                if delta.flags().is_binary() {
                    return true;
                }

                let file_path = delta.new_file().path().or_else(|| delta.old_file().path());

                if let Some(p) = file_path {
                    if let Some(file_info) = files.iter_mut().find(|f| f.path == p) {
                        match line.origin() {
                            '+' => {
                                file_info.insertions += 1;
                                total_insertions += 1;
                            }
                            '-' => {
                                file_info.deletions += 1;
                                total_deletions += 1;
                            }
                            _ => {}
                        }
                    }
                }
                true
            }),
        )?;

        Ok(Self {
            files,
            total_insertions,
            total_deletions,
            total_files,
            truncated,
        })
    }
}
