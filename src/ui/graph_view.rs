//! グラフ表示Widget

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, StatefulWidget},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::App,
    git::graph::{CellType, GraphNode},
    graph::colors::get_color_by_index,
};

/// 文字列の表示幅を計算
fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

pub struct GraphViewWidget<'a> {
    items: Vec<ListItem<'a>>,
}

impl<'a> GraphViewWidget<'a> {
    pub fn new(app: &App, width: u16) -> Self {
        let max_lane = app.graph_layout.max_lane;
        // ボーダー分を引いた実際の幅
        let inner_width = width.saturating_sub(2) as usize;

        let items: Vec<ListItem> = app
            .graph_layout
            .nodes
            .iter()
            .enumerate()
            .map(|(idx, node)| {
                let is_selected = app.graph_list_state.selected() == Some(idx);
                let line = render_graph_line(node, max_lane, is_selected, inner_width);
                ListItem::new(line)
            })
            .collect();

        Self { items }
    }
}

/// ブランチ名の表示を最適化
/// - ローカルブランチと対応するorigin/xxxが一致している場合は「xxx ↔ origin」と表示
/// - それ以外は従来通り個別に表示
/// - グラフのカラーインデックスで太文字表示、カッコで囲む
fn optimize_branch_display(branch_names: &[String], is_head: bool, color_index: usize) -> Vec<(String, Style)> {
    use std::collections::HashSet;

    if branch_names.is_empty() {
        return Vec::new();
    }

    let mut result: Vec<(String, Style)> = Vec::new();
    let mut processed_remotes: HashSet<String> = HashSet::new();

    // ローカルブランチとリモートブランチを分離
    let local_branches: Vec<&str> = branch_names
        .iter()
        .filter(|n| !n.starts_with("origin/"))
        .map(|s| s.as_str())
        .collect();
    let remote_branches: HashSet<&str> = branch_names
        .iter()
        .filter(|n| n.starts_with("origin/"))
        .map(|s| s.as_str())
        .collect();

    // スタイル: グラフのカラーインデックスで太文字
    // メインブランチ（青）は常に青、それ以外のHEADは緑
    let base_color = if color_index == crate::graph::colors::MAIN_BRANCH_COLOR {
        get_color_by_index(color_index) // メインブランチは常に青
    } else if is_head {
        Color::Green
    } else {
        get_color_by_index(color_index)
    };
    let style = Style::default().fg(base_color).add_modifier(Modifier::BOLD);

    // ローカルブランチを処理
    for local in local_branches.iter() {
        let remote_name = format!("origin/{}", local);
        let has_matching_remote = remote_branches.contains(remote_name.as_str());

        if has_matching_remote {
            // ローカルとリモートが一致 → 簡潔表示
            result.push((format!("[{} ↔ origin]", local), style));
            processed_remotes.insert(remote_name);
        } else {
            // ローカルのみ
            result.push((format!("[{}]", local), style));
        }
    }

    // 対応するローカルがないリモートブランチを追加（グラフと同じ色で表示）
    for remote in branch_names.iter().filter(|n| n.starts_with("origin/")) {
        if !processed_remotes.contains(remote) {
            result.push((format!("[{}]", remote), style));
        }
    }

    result
}

/// 文字列を指定した表示幅で切り詰める
fn truncate_to_width(s: &str, max_width: usize) -> String {
    let mut result = String::new();
    let mut current_width = 0;
    for ch in s.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if current_width + ch_width > max_width {
            break;
        }
        result.push(ch);
        current_width += ch_width;
    }
    result
}

fn render_graph_line<'a>(
    node: &GraphNode,
    max_lane: usize,
    is_selected: bool,
    total_width: usize,
) -> Line<'a> {
    let mut spans: Vec<Span> = Vec::new();

    // グラフ開始マーカー（境界線と区別するため）
    spans.push(Span::raw(" "));
    let mut left_width: usize = 1;

    // セルを描画
    for cell in &node.cells {
        let (ch, color) = match cell {
            CellType::Empty => (' ', Color::Reset),
            CellType::Pipe(color_idx) => ('│', get_color_by_index(*color_idx)),
            CellType::Commit(color_idx) => {
                // HEADは二重丸、それ以外は塗りつぶし丸
                let ch = if node.is_head { '◉' } else { '●' };
                // メインブランチ（青）は常に青、それ以外のHEADは緑
                let color = if *color_idx == crate::graph::colors::MAIN_BRANCH_COLOR {
                    get_color_by_index(*color_idx)
                } else if node.is_head {
                    Color::Green
                } else {
                    get_color_by_index(*color_idx)
                };
                (ch, color)
            }
            CellType::BranchRight(color_idx) => ('╭', get_color_by_index(*color_idx)),
            CellType::BranchLeft(color_idx) => ('╮', get_color_by_index(*color_idx)),
            CellType::MergeRight(color_idx) => ('╰', get_color_by_index(*color_idx)),
            CellType::MergeLeft(color_idx) => ('╯', get_color_by_index(*color_idx)),
            CellType::Horizontal(color_idx) => ('─', get_color_by_index(*color_idx)),
            CellType::HorizontalPipe(_h_color_idx, p_color_idx) => {
                // 縦線と横線が交差（縦線の色を優先）
                ('┼', get_color_by_index(*p_color_idx))
            }
            CellType::TeeRight(color_idx) => ('├', get_color_by_index(*color_idx)),
            CellType::TeeLeft(color_idx) => ('┤', get_color_by_index(*color_idx)),
            CellType::TeeUp(color_idx) => ('┴', get_color_by_index(*color_idx)),
        };

        // 罫線はすべてBOLDで太く表示
        let style = Style::default().fg(color).add_modifier(Modifier::BOLD);

        let ch_str = ch.to_string();
        let ch_width = display_width(&ch_str);
        spans.push(Span::styled(ch_str, style));
        left_width += ch_width;
    }

    // グラフ幅を揃えるためのパディング（表示幅ベース）
    let graph_display_width = (max_lane + 1) * 2;
    if left_width < graph_display_width + 1 {  // +1 は開始マーカー分
        let padding = graph_display_width + 1 - left_width;
        spans.push(Span::raw(" ".repeat(padding)));
        left_width += padding;
    }

    // セパレータ（グラフとコミット情報の間）
    spans.push(Span::raw(" "));
    left_width += 1;

    // コミットがない場合（接続行のみ）は早期リターン
    let commit = match &node.commit {
        Some(c) => c,
        None => return Line::from(spans),
    };

    // スタイル定義
    let hash_style = Style::default().fg(Color::Yellow);
    let author_style = Style::default().fg(Color::Cyan);
    let date_style = Style::default().fg(Color::DarkGray);
    let msg_style = if is_selected {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    // === 左寄せ部分: ブランチ名 + メッセージ ===

    // ブランチ名を最適化（local と origin/local が一致している場合は簡潔に表示）
    let branch_display = optimize_branch_display(&node.branch_names, node.is_head, node.color_index);

    // === 右寄せ部分: 日時 author hash（固定幅） ===
    let date = commit.timestamp.format("%Y-%m-%d").to_string();  // 10文字
    let author = truncate_to_width(&commit.author_name, 8);
    let author_formatted = format!("{:<8}", author);  // 8文字固定
    let hash = truncate_to_width(&commit.short_id, 7);
    let hash_formatted = format!("{:<7}", hash);  // 7文字固定

    // 右寄せ部分の固定幅: " YYYY-MM-DD  author    hash   "
    // スペース1 + 日付10 + スペース2 + author8 + スペース2 + hash7 + スペース1 = 31
    const RIGHT_FIXED_WIDTH: usize = 31;

    // ブランチ名を表示（カッコ付きで太文字、グラフと同じ色）
    for (i, (label, style)) in branch_display.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
            left_width += 1;
        }
        left_width += display_width(label);
        spans.push(Span::styled(label.clone(), *style));
    }
    if !branch_display.is_empty() {
        spans.push(Span::raw(" "));
        left_width += 1;
    }

    // メッセージの最大幅を計算（利用可能な幅いっぱいまで使用）
    let available_for_message = total_width
        .saturating_sub(left_width)
        .saturating_sub(RIGHT_FIXED_WIDTH);
    let message = truncate_to_width(&commit.message, available_for_message);
    let message_width = display_width(&message);
    spans.push(Span::styled(message, msg_style));
    left_width += message_width;

    // 右寄せのためのパディング（右寄せ部分が常に同じ位置から始まるように）
    let padding = total_width.saturating_sub(left_width).saturating_sub(RIGHT_FIXED_WIDTH);
    if padding > 0 {
        spans.push(Span::raw(" ".repeat(padding)));
    }

    // === 右寄せ部分を追加（固定幅フォーマット） ===
    spans.push(Span::raw(" "));
    spans.push(Span::styled(date, date_style));
    spans.push(Span::raw("  "));
    spans.push(Span::styled(author_formatted, author_style));
    spans.push(Span::raw("  "));
    spans.push(Span::styled(hash_formatted, hash_style));
    spans.push(Span::raw(" "));

    Line::from(spans)
}

impl<'a> StatefulWidget for GraphViewWidget<'a> {
    type State = ListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let block = Block::default()
            .title(" Commits ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let highlight_style = Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD);

        let list = List::new(self.items)
            .block(block)
            .highlight_style(highlight_style);

        StatefulWidget::render(list, area, buf, state);
    }
}
