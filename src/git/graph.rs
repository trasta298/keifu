//! コミットグラフの構築

use std::collections::HashMap;

use git2::Oid;

use super::{BranchInfo, CommitInfo};
use crate::graph::colors::ColorAssigner;

/// グラフノード
#[derive(Debug, Clone)]
pub struct GraphNode {
    /// コミット情報（None の場合は接続行のみ）
    pub commit: Option<CommitInfo>,
    /// このコミットのレーン位置
    pub lane: usize,
    /// このノードのカラーインデックス
    pub color_index: usize,
    /// このコミットを指すブランチ名のリスト
    pub branch_names: Vec<String>,
    /// HEADがこのコミットを指しているか
    pub is_head: bool,
    /// この行の描画情報
    pub cells: Vec<CellType>,
}

/// セルの種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellType {
    /// 空
    Empty,
    /// 縦線（継続中のレーン）
    Pipe(usize),
    /// コミットノード
    Commit(usize),
    /// 右への分岐開始 ╭ (ブランチが右上へ分岐)
    BranchRight(usize),
    /// 左への分岐開始 ╮ (ブランチが左上へ分岐)
    BranchLeft(usize),
    /// 右へのマージ ╰ (ブランチが右下から合流)
    MergeRight(usize),
    /// 左へのマージ ╯ (ブランチが左下から合流)
    MergeLeft(usize),
    /// 横線
    Horizontal(usize),
    /// 横線（レーン通過）
    HorizontalPipe(usize, usize), // (horizontal_lane, pipe_lane)
    /// T字分岐（右へ）├
    TeeRight(usize),
    /// T字分岐（左へ）┤
    TeeLeft(usize),
}

/// グラフレイアウト
#[derive(Debug, Clone)]
pub struct GraphLayout {
    pub nodes: Vec<GraphNode>,
    pub max_lane: usize,
}

/// コミット一覧からグラフを構築
pub fn build_graph(commits: &[CommitInfo], branches: &[BranchInfo]) -> GraphLayout {
    if commits.is_empty() {
        return GraphLayout {
            nodes: Vec::new(),
            max_lane: 0,
        };
    }

    // OID -> ブランチ名のマッピング
    let mut oid_to_branches: HashMap<Oid, Vec<String>> = HashMap::new();
    let mut head_oid: Option<Oid> = None;
    for branch in branches {
        oid_to_branches
            .entry(branch.tip_oid)
            .or_default()
            .push(branch.name.clone());
        if branch.is_head {
            head_oid = Some(branch.tip_oid);
        }
    }

    // OID -> 行番号のマッピング
    let oid_to_row: HashMap<Oid, usize> = commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.oid, i))
        .collect();

    // レーン管理: 各レーンが追跡中のOID
    let mut lanes: Vec<Option<Oid>> = Vec::new();
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut max_lane: usize = 0;

    // 色管理
    let mut color_assigner = ColorAssigner::new();
    // OID -> カラーインデックスのマッピング
    let mut oid_color_index: HashMap<Oid, usize> = HashMap::new();

    for commit in commits {
        // このコミットのOIDを追跡中のレーンを探す
        let commit_lane_opt = lanes
            .iter()
            .position(|l| l.map(|oid| oid == commit.oid).unwrap_or(false));

        // レーンを決定
        let lane = if let Some(l) = commit_lane_opt {
            l
        } else {
            // 空きレーンを探すか、新規作成
            let empty = lanes.iter().position(|l| l.is_none());
            if let Some(l) = empty {
                l
            } else {
                lanes.push(None);
                lanes.len() - 1
            }
        };

        // カラーインデックスを決定
        let commit_color_index = if commit_lane_opt.is_some() {
            // 既存のブランチを継続
            color_assigner.continue_lane(lane)
        } else {
            // 新しいブランチ開始 - 新しい色を割り当て
            color_assigner.assign_color(lane)
        };
        oid_color_index.insert(commit.oid, commit_color_index);

        // このコミットのレーンをクリア
        if lane < lanes.len() {
            lanes[lane] = None;
        }

        // 親コミットの処理
        // (OID, レーン, 既存追跡か否か, カラーインデックス)
        let mut parent_lanes: Vec<(Oid, usize, bool, usize)> = Vec::new();
        let valid_parents: Vec<Oid> = commit
            .parent_oids
            .iter()
            .filter(|oid| oid_to_row.contains_key(oid))
            .copied()
            .collect();

        for (parent_idx, parent_oid) in valid_parents.iter().enumerate() {
            // 親がすでにレーンにあるか確認
            let existing_parent_lane = lanes
                .iter()
                .position(|l| l.map(|oid| oid == *parent_oid).unwrap_or(false));

            let (parent_lane, was_existing, parent_color) = if let Some(pl) = existing_parent_lane {
                // 既存レーン - 既存の色を使用
                let color = oid_color_index.get(parent_oid).copied().unwrap_or(pl);
                (pl, true, color)
            } else if parent_idx == 0 {
                // 最初の親は同じレーンを使用 - 同じ色を継承
                lanes[lane] = Some(*parent_oid);
                oid_color_index.insert(*parent_oid, commit_color_index);
                (lane, false, commit_color_index)
            } else {
                // 2番目以降の親は別レーンを使用 - 新しい色を割り当て
                let empty = lanes.iter().position(|l| l.is_none());
                let new_lane = if let Some(l) = empty {
                    l
                } else {
                    lanes.push(None);
                    lanes.len() - 1
                };
                lanes[new_lane] = Some(*parent_oid);
                let new_color = color_assigner.assign_color(new_lane);
                oid_color_index.insert(*parent_oid, new_color);
                (new_lane, false, new_color)
            };

            parent_lanes.push((*parent_oid, parent_lane, was_existing, parent_color));
        }

        // max_laneを更新
        max_lane = max_lane.max(lane);
        for &(_, pl, _, _) in &parent_lanes {
            max_lane = max_lane.max(pl);
        }

        // レーン統合が必要かチェック
        // コミットのレーンと親のレーンが異なり、親が既に追跡されている場合
        // → 高いレーンが終了して低いレーンに合流する
        let lane_merge: Option<(usize, usize)> = parent_lanes
            .iter()
            .find(|(_, pl, was_existing, _)| *was_existing && *pl != lane)
            .map(|(_, pl, _, color)| (*pl, *color));

        // この行のセルを生成（was_existing の親への接続線は除外 - 接続行で描画する）
        let non_merging_parents: Vec<(Oid, usize, bool, usize)> = parent_lanes
            .iter()
            .filter(|(_, pl, was_existing, _)| !(*was_existing && *pl != lane))
            .copied()
            .collect();
        let cells = build_row_cells_with_colors(
            lane,
            commit_color_index,
            &non_merging_parents,
            &lanes,
            &oid_color_index,
            max_lane,
        );

        // ブランチ名を取得
        let branch_names = oid_to_branches
            .get(&commit.oid)
            .cloned()
            .unwrap_or_default();

        let is_head = head_oid.map(|h| h == commit.oid).unwrap_or(false);

        // コミット行を追加
        nodes.push(GraphNode {
            commit: Some(commit.clone()),
            lane,
            color_index: commit_color_index,
            branch_names,
            is_head,
            cells,
        });

        // 接続行をコミット行の後に追加（レーン統合がある場合）
        // 接続行は終了するレーンの最後のコミットの後に来る
        if let Some((parent_lane, _)) = lane_merge {
            // 常に低いレーンがメイン（├）、高いレーンがマージ終了（╯）
            let (main_lane, ending_lane) = if parent_lane < lane {
                (parent_lane, lane)
            } else {
                (lane, parent_lane)
            };

            let main_color = lanes
                .get(main_lane)
                .and_then(|o| *o)
                .and_then(|oid| oid_color_index.get(&oid).copied())
                .unwrap_or(main_lane);
            let ending_color = oid_color_index
                .get(&commit.oid)
                .copied()
                .unwrap_or(ending_lane);

            let connector_cells = build_connector_cells_with_colors(
                main_lane,
                main_color,
                &[(ending_lane, ending_color)],
                &lanes,
                &oid_color_index,
                max_lane,
            );
            nodes.push(GraphNode {
                commit: None,
                lane: main_lane,
                color_index: main_color,
                branch_names: Vec::new(),
                is_head: false,
                cells: connector_cells,
            });

            // 終了するレーンを解放
            if ending_lane < lanes.len() {
                // 終了レーンのOIDをメインレーンに統合
                if let Some(oid) = lanes[ending_lane] {
                    if lanes.get(main_lane).map(|l| l.is_none()).unwrap_or(false) {
                        lanes[main_lane] = Some(oid);
                    }
                }
                lanes[ending_lane] = None;
                color_assigner.release_lane(ending_lane);
            }
        }
    }

    GraphLayout { nodes, max_lane }
}

/// 接続行のセルを構築（ブランチがマージする行）- 色インデックス版
fn build_connector_cells_with_colors(
    main_lane: usize,
    main_color: usize,
    merging_lanes: &[(usize, usize)], // (lane, color_index)
    active_lanes: &[Option<Oid>],
    oid_color_index: &HashMap<Oid, usize>,
    max_lane: usize,
) -> Vec<CellType> {
    let mut cells = vec![CellType::Empty; (max_lane + 1) * 2];

    // メインレーンにT字分岐を描画
    let main_cell_idx = main_lane * 2;
    if main_cell_idx < cells.len() {
        cells[main_cell_idx] = CellType::TeeRight(main_color);
    }

    // マージするレーン番号のリスト
    let merging_lane_nums: Vec<usize> = merging_lanes.iter().map(|(l, _)| *l).collect();

    // アクティブなレーンに縦線を描画（メインレーンとマージレーン以外）
    for (lane_idx, lane_oid) in active_lanes.iter().enumerate() {
        if let Some(oid) = lane_oid {
            if lane_idx != main_lane && !merging_lane_nums.contains(&lane_idx) {
                let cell_idx = lane_idx * 2;
                if cell_idx < cells.len() {
                    let color = oid_color_index.get(oid).copied().unwrap_or(lane_idx);
                    cells[cell_idx] = CellType::Pipe(color);
                }
            }
        }
    }

    // マージするレーンへの接続線を描画
    for &(merge_lane, merge_color) in merging_lanes {
        // メインレーンからマージレーンへの横線
        for col in (main_lane * 2 + 1)..(merge_lane * 2) {
            if col < cells.len() {
                let existing = cells[col];
                if let CellType::Pipe(pl) = existing {
                    cells[col] = CellType::HorizontalPipe(merge_color, pl);
                } else if existing == CellType::Empty {
                    cells[col] = CellType::Horizontal(merge_color);
                }
            }
        }
        // マージレーンの終点
        let end_idx = merge_lane * 2;
        if end_idx < cells.len() {
            cells[end_idx] = CellType::MergeLeft(merge_color);
        }
    }

    cells
}

/// 1行分のセルを構築 - 色インデックス版
/// parent_lanes: (親OID, レーン番号, 既存追跡フラグ, カラーインデックス)
fn build_row_cells_with_colors(
    commit_lane: usize,
    commit_color: usize,
    parent_lanes: &[(Oid, usize, bool, usize)],
    active_lanes: &[Option<Oid>],
    oid_color_index: &HashMap<Oid, usize>,
    max_lane: usize,
) -> Vec<CellType> {
    let mut cells = vec![CellType::Empty; (max_lane + 1) * 2];

    // アクティブなレーンに縦線を描画
    for (lane_idx, lane_oid) in active_lanes.iter().enumerate() {
        if let Some(oid) = lane_oid {
            if lane_idx != commit_lane {
                let cell_idx = lane_idx * 2;
                if cell_idx < cells.len() {
                    let color = oid_color_index.get(oid).copied().unwrap_or(lane_idx);
                    cells[cell_idx] = CellType::Pipe(color);
                }
            }
        }
    }

    // コミットノードを描画
    let commit_cell_idx = commit_lane * 2;
    if commit_cell_idx < cells.len() {
        cells[commit_cell_idx] = CellType::Commit(commit_color);
    }

    // 親への接続線を描画
    for &(_parent_oid, parent_lane, was_existing, parent_color) in parent_lanes.iter() {
        if parent_lane == commit_lane {
            // 同じレーン - 縦線のみ（次の行で描画）
            continue;
        }

        // 異なるレーンへの接続
        if parent_lane > commit_lane {
            // 右のレーンへの接続
            // コミット位置から右へ横線
            for col in (commit_lane * 2 + 1)..(parent_lane * 2) {
                if col < cells.len() {
                    let existing = cells[col];
                    if let CellType::Pipe(pl) = existing {
                        cells[col] = CellType::HorizontalPipe(parent_color, pl);
                    } else if existing == CellType::Empty {
                        cells[col] = CellType::Horizontal(parent_color);
                    }
                }
            }
            // 終点のマーク
            let end_idx = parent_lane * 2;
            if end_idx < cells.len() {
                if was_existing {
                    // 既存追跡: レーンが終了して合流 ╯（上へ接続）
                    cells[end_idx] = CellType::MergeLeft(parent_color);
                } else {
                    // 新規割り当て: レーンが継続 ╮（下へ継続）
                    cells[end_idx] = CellType::BranchLeft(parent_color);
                }
            }
        } else {
            // ブランチ終端: 左のレーン（メインライン）へ接続
            // コミット位置から左へ横線
            for col in (parent_lane * 2 + 1)..(commit_lane * 2) {
                if col < cells.len() {
                    let existing = cells[col];
                    if let CellType::Pipe(pl) = existing {
                        cells[col] = CellType::HorizontalPipe(commit_color, pl);
                    } else if existing == CellType::Empty {
                        cells[col] = CellType::Horizontal(commit_color);
                    }
                }
            }
            // 始点のマーク
            let start_idx = parent_lane * 2;
            if start_idx < cells.len() {
                let existing = cells[start_idx];
                if let CellType::Pipe(pl) = existing {
                    // パイプがある場合はT字分岐（├）に変更
                    cells[start_idx] = CellType::TeeRight(pl);
                } else if existing == CellType::Empty {
                    // 空の場合はマージマーク（╰）
                    cells[start_idx] = CellType::MergeRight(commit_color);
                }
            }
        }
    }

    cells
}
