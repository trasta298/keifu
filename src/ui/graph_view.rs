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
    graph::{colors::get_lane_color, GraphChars},
};

pub struct GraphViewWidget<'a> {
    items: Vec<ListItem<'a>>,
    is_focused: bool,
}

impl<'a> GraphViewWidget<'a> {
    pub fn new(app: &App) -> Self {
        let chars = GraphChars::default();
        let max_lane = app.graph_layout.max_lane;
        let graph_width = (max_lane + 1) * 2;

        let items: Vec<ListItem> = app
            .graph_layout
            .nodes
            .iter()
            .enumerate()
            .map(|(idx, node)| {
                let is_selected = app.graph_list_state.selected() == Some(idx);
                let line = render_graph_line_with_commit(node, max_lane, is_selected, &chars, graph_width);
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
    chars: &GraphChars,
    graph_width: usize,
) -> Line<'a> {
    let mut spans: Vec<Span> = Vec::new();
    let lane = node.lane;
    let color = get_lane_color(lane);

    // グラフ部分を描画
    for col in 0..=max_lane {
        if col == lane {
            // コミットノード
            let commit_char = if is_selected {
                chars.commit_selected
            } else {
                chars.commit
            };
            let style = Style::default().fg(color);
            spans.push(Span::styled(commit_char.to_string(), style));
        } else {
            // 他のレーンは縦線またはスペース
            // TODO: より正確なアクティブレーン追跡
            spans.push(Span::styled("│", Style::default().fg(Color::DarkGray)));
        }

        // レーン間のスペース
        if col < max_lane {
            // 接続線があれば横線を描画
            let has_horizontal = node.connections.iter().any(|conn| {
                let min = conn.source_lane.min(conn.target_lane);
                let max = conn.source_lane.max(conn.target_lane);
                col >= min && col < max
            });

            if has_horizontal {
                spans.push(Span::styled("─", Style::default().fg(color)));
            } else {
                spans.push(Span::raw(" "));
            }
        }
    }

    // グラフ幅を揃えるためのパディング
    let current_width: usize = spans.iter().map(|s| s.content.len()).sum();
    if current_width < graph_width {
        spans.push(Span::raw(" ".repeat(graph_width - current_width)));
    }

    // セパレータ
    spans.push(Span::raw(" "));

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

    // 著者名（最大10文字）
    let author: String = commit.author_name.chars().take(10).collect();
    spans.push(Span::styled(format!("{:<10}", author), author_style));
    spans.push(Span::raw(" "));

    // 日時
    let date = commit.timestamp.format("%m-%d %H:%M").to_string();
    spans.push(Span::styled(date, date_style));
    spans.push(Span::raw(" "));

    // メッセージ（残りの幅に収まるように）
    let message: String = commit.message.chars().take(50).collect();
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
