//! Unicode文字でのグラフ描画

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

use crate::git::graph::{ConnectionType, GraphNode};

use super::colors::get_lane_color;

/// グラフ描画用の文字セット
pub struct GraphChars {
    pub vertical: char,
    pub horizontal: char,
    pub commit: char,
    pub commit_selected: char,
    pub merge_left: char,
    pub merge_right: char,
    pub branch_left: char,
    pub branch_right: char,
    pub tee_right: char,
    pub tee_left: char,
    pub cross: char,
}

impl Default for GraphChars {
    fn default() -> Self {
        Self {
            vertical: '│',
            horizontal: '─',
            commit: '●',
            commit_selected: '◉',
            merge_left: '╭',
            merge_right: '╮',
            branch_left: '╰',
            branch_right: '╯',
            tee_right: '├',
            tee_left: '┤',
            cross: '┼',
        }
    }
}

/// 1行分のグラフを描画
pub fn render_graph_line<'a>(
    node: &GraphNode,
    max_lane: usize,
    is_selected: bool,
    active_lanes: &[bool],
    chars: &GraphChars,
) -> Line<'a> {
    let mut spans: Vec<Span> = Vec::new();
    let lane = node.lane;
    let color = get_lane_color(lane);

    // 各レーン位置の描画
    for col in 0..=max_lane {
        if col == lane {
            // コミットノード
            let commit_char = if is_selected {
                chars.commit_selected
            } else {
                chars.commit
            };
            let style = if is_selected {
                Style::default().fg(color).bg(Color::DarkGray)
            } else {
                Style::default().fg(color)
            };
            spans.push(Span::styled(commit_char.to_string(), style));
        } else if active_lanes.get(col).copied().unwrap_or(false) {
            // アクティブなレーンの継続線
            let col_color = get_lane_color(col);
            spans.push(Span::styled(
                chars.vertical.to_string(),
                Style::default().fg(col_color),
            ));
        } else {
            // 空きスペース
            spans.push(Span::raw(" "));
        }

        // レーン間のスペース
        if col < max_lane {
            // 接続線があるか確認
            let has_connection = node.connections.iter().any(|conn| {
                let min_lane = conn.source_lane.min(conn.target_lane);
                let max_lane_conn = conn.source_lane.max(conn.target_lane);
                col >= min_lane && col < max_lane_conn && conn.connection_type != ConnectionType::Direct
            });

            if has_connection && col >= lane {
                // 水平接続線
                spans.push(Span::styled(
                    chars.horizontal.to_string(),
                    Style::default().fg(color),
                ));
            } else {
                spans.push(Span::raw(" "));
            }
        }
    }

    Line::from(spans)
}

/// アクティブなレーンの状態を更新
pub fn update_active_lanes(node: &GraphNode, active_lanes: &mut Vec<bool>) {
    let lane = node.lane;

    // 現在のレーンをアクティブに
    while active_lanes.len() <= lane {
        active_lanes.push(false);
    }

    // このノードでレーンが終了する場合は非アクティブに
    // 親への接続がある場合はアクティブを維持
    let has_parent_on_same_lane = node
        .connections
        .iter()
        .any(|conn| conn.target_lane == lane);

    active_lanes[lane] = has_parent_on_same_lane;

    // 新しいレーンへの分岐があればアクティブに
    for conn in &node.connections {
        while active_lanes.len() <= conn.target_lane {
            active_lanes.push(false);
        }
        active_lanes[conn.target_lane] = true;
    }
}
