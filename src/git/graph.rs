//! コミットグラフの構築

use std::collections::HashMap;

use git2::Oid;

use super::CommitInfo;

/// コミット間の接続タイプ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionType {
    /// 同じレーンで直線
    Direct,
    /// 右から左へのマージ
    MergeIn,
    /// 左から右への分岐
    BranchOut,
}

/// 接続情報
#[derive(Debug, Clone)]
pub struct Connection {
    pub target_oid: Oid,
    pub source_lane: usize,
    pub target_lane: usize,
    pub connection_type: ConnectionType,
}

/// グラフノード
#[derive(Debug, Clone)]
pub struct GraphNode {
    pub commit: CommitInfo,
    pub lane: usize,
    pub row: usize,
    pub connections: Vec<Connection>,
}

/// グラフレイアウト
#[derive(Debug, Clone)]
pub struct GraphLayout {
    pub nodes: Vec<GraphNode>,
    pub max_lane: usize,
}

/// コミット一覧からグラフを構築
pub fn build_graph(commits: &[CommitInfo]) -> GraphLayout {
    if commits.is_empty() {
        return GraphLayout {
            nodes: Vec::new(),
            max_lane: 0,
        };
    }

    // OID -> 行番号のマッピング
    let oid_to_row: HashMap<Oid, usize> = commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.oid, i))
        .collect();

    // 各行でアクティブなレーンを追跡
    let mut active_lanes: Vec<Option<Oid>> = Vec::new();
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut max_lane = 0;

    for (row, commit) in commits.iter().enumerate() {
        // このコミットが既存のレーンにあるか確認
        let existing_lane = active_lanes
            .iter()
            .position(|lane| lane.map(|oid| oid == commit.oid).unwrap_or(false));

        let lane = if let Some(lane_idx) = existing_lane {
            // 既存のレーンを使用
            lane_idx
        } else {
            // 新しいレーンを割り当て（空きレーンを探すか、末尾に追加）
            let empty_lane = active_lanes.iter().position(|lane| lane.is_none());
            if let Some(lane_idx) = empty_lane {
                lane_idx
            } else {
                active_lanes.push(None);
                active_lanes.len() - 1
            }
        };

        // 現在のレーンを更新
        active_lanes[lane] = None;

        // 接続を計算
        let mut connections = Vec::new();
        for (parent_idx, parent_oid) in commit.parent_oids.iter().enumerate() {
            if let Some(&target_row) = oid_to_row.get(parent_oid) {
                // 親がすでにレーンにあるか確認
                let parent_lane = active_lanes
                    .iter()
                    .position(|l| l.map(|oid| oid == *parent_oid).unwrap_or(false));

                let target_lane = if let Some(pl) = parent_lane {
                    pl
                } else if parent_idx == 0 {
                    // 最初の親は同じレーンを継続
                    active_lanes[lane] = Some(*parent_oid);
                    lane
                } else {
                    // 2番目以降の親は新しいレーンまたは空きレーンを使用
                    let empty = active_lanes.iter().position(|l| l.is_none());
                    let new_lane = if let Some(l) = empty {
                        l
                    } else {
                        active_lanes.push(None);
                        active_lanes.len() - 1
                    };
                    active_lanes[new_lane] = Some(*parent_oid);
                    new_lane
                };

                let connection_type = if lane == target_lane {
                    ConnectionType::Direct
                } else if target_lane > lane {
                    ConnectionType::BranchOut
                } else {
                    ConnectionType::MergeIn
                };

                connections.push(Connection {
                    target_oid: *parent_oid,
                    source_lane: lane,
                    target_lane,
                    connection_type,
                });

                max_lane = max_lane.max(target_lane);
            }
        }

        max_lane = max_lane.max(lane);

        nodes.push(GraphNode {
            commit: commit.clone(),
            lane,
            row,
            connections,
        });
    }

    GraphLayout { nodes, max_lane }
}
