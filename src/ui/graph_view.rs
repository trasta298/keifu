//! グラフ表示Widget

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, StatefulWidget},
};

use crate::{
    app::{App, PaneFocus},
    git::graph::GraphNode,
    graph::colors::get_lane_color,
};

pub struct GraphViewWidget<'a> {
    items: Vec<ListItem<'a>>,
    is_focused: bool,
}

impl<'a> GraphViewWidget<'a> {
    pub fn new(app: &App) -> Self {
        let max_lane = app.graph_layout.max_lane;
        let graph_width = (max_lane + 1) * 2 + 1;

        let items: Vec<ListItem> = app
            .graph_layout
            .nodes
            .iter()
            .enumerate()
            .map(|(idx, node)| {
                let is_selected = app.graph_list_state.selected() == Some(idx);
                let line = render_graph_line_with_commit(node, max_lane, is_selected, graph_width);
                ListItem::new(line)
            })
            .collect();

        Self {
            items,
            is_focused: app.focus == PaneFocus::Graph,
        }
    }
}

fn render_graph_line_with_commit<'a>(
    node: &GraphNode,
    max_lane: usize,
    is_selected: bool,
    graph_width: usize,
) -> Line<'a> {
    let mut spans: Vec<Span> = Vec::new();
    let lane = node.lane;
    let color = get_lane_color(lane);

    // グラフ部分を描画
    for col in 0..=max_lane {
        if col == lane {
            // コミットノード
            let commit_char = if node.is_head {
                '◉'  // HEAD
            } else if is_selected {
                '●'
            } else {
                '○'
            };
            let style = if node.is_head {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(color)
            };
            spans.push(Span::styled(commit_char.to_string(), style));
        } else {
            // アクティブなレーンのみ縦線を描画
            let is_active = node.active_lanes.get(col).copied().unwrap_or(false);
            if is_active {
                let col_color = get_lane_color(col);
                spans.push(Span::styled("│", Style::default().fg(col_color)));
            } else {
                spans.push(Span::raw(" "));
            }
        }

        // レーン間のスペースと接続線
        if col < max_lane {
            // このノードから他レーンへの接続があるか確認
            let has_branch_out = node.connections.iter().any(|conn| {
                conn.source_lane == lane && conn.target_lane > lane && col >= lane && col < conn.target_lane
            });
            let has_merge_in = node.connections.iter().any(|conn| {
                conn.source_lane == lane && conn.target_lane < lane && col >= conn.target_lane && col < lane
            });

            if has_branch_out || has_merge_in {
                spans.push(Span::styled("─", Style::default().fg(color)));
            } else {
                spans.push(Span::raw(" "));
            }
        }
    }

    // グラフ幅を揃えるためのパディング
    let current_width: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    if current_width < graph_width {
        spans.push(Span::raw(" ".repeat(graph_width - current_width)));
    }

    // セパレータ
    spans.push(Span::raw(" "));

    // ブランチ名を表示（あれば）
    if !node.branch_names.is_empty() {
        for (i, name) in node.branch_names.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" "));
            }
            let branch_style = if node.is_head {
                Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Black).bg(Color::Yellow)
            };
            spans.push(Span::styled(format!(" {} ", name), branch_style));
        }
        spans.push(Span::raw(" "));
    }

    // コミット情報
    let commit = &node.commit;
    let hash_style = Style::default().fg(Color::Yellow);
    let author_style = Style::default().fg(Color::Blue);
    let date_style = Style::default().fg(Color::DarkGray);
    let msg_style = if is_selected {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    spans.push(Span::styled(commit.short_id.clone(), hash_style));
    spans.push(Span::raw(" "));

    // 著者名（最大8文字）
    let author: String = commit.author_name.chars().take(8).collect();
    spans.push(Span::styled(format!("{:<8}", author), author_style));
    spans.push(Span::raw(" "));

    // 日時
    let date = commit.timestamp.format("%m-%d").to_string();
    spans.push(Span::styled(date, date_style));
    spans.push(Span::raw(" "));

    // メッセージ
    let message: String = commit.message.chars().take(40).collect();
    spans.push(Span::styled(message, msg_style));

    Line::from(spans)
}

impl<'a> StatefulWidget for GraphViewWidget<'a> {
    type State = ListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let border_style = if self.is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title(" Commits ")
            .borders(Borders::ALL)
            .border_style(border_style);

        let highlight_style = Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD);

        let list = List::new(self.items)
            .block(block)
            .highlight_style(highlight_style);

        StatefulWidget::render(list, area, buf, state);
    }
}
