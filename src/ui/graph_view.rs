//! Graph view widget

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

/// Calculate display width of a string
fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

pub struct GraphViewWidget<'a> {
    items: Vec<ListItem<'a>>,
}

impl<'a> GraphViewWidget<'a> {
    pub fn new(app: &App, width: u16) -> Self {
        let max_lane = app.graph_layout.max_lane;
        // Actual width minus borders
        let inner_width = width.saturating_sub(2) as usize;

        // Get the currently selected branch name
        let selected_branch_name = app.selected_branch_name();

        let items: Vec<ListItem> = app
            .graph_layout
            .nodes
            .iter()
            .enumerate()
            .map(|(idx, node)| {
                let is_selected = app.graph_list_state.selected() == Some(idx);
                let line = render_graph_line(
                    node,
                    max_lane,
                    is_selected,
                    inner_width,
                    selected_branch_name,
                );
                ListItem::new(line)
            })
            .collect();

        Self { items }
    }
}

/// Optimize branch name display
/// - If a local branch matches its origin/xxx, show "xxx <-> origin"
/// - Otherwise, show each name separately
/// - Render in bold with the graph color, wrapped in brackets
/// - Selected branch is shown with inverted colors
fn optimize_branch_display(
    branch_names: &[String],
    is_head: bool,
    color_index: usize,
    selected_branch_name: Option<&str>,
) -> Vec<(String, Style)> {
    use std::collections::HashSet;

    if branch_names.is_empty() {
        return Vec::new();
    }

    let mut result: Vec<(String, Style)> = Vec::new();
    let mut processed_remotes: HashSet<String> = HashSet::new();

    // Split local and remote branches
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

    // Style: bold with the graph color index
    // Main branch (blue) stays blue; other HEADs are green
    let base_color = if color_index == crate::graph::colors::MAIN_BRANCH_COLOR {
        get_color_by_index(color_index) // Main branch is always blue
    } else if is_head {
        Color::Green
    } else {
        get_color_by_index(color_index)
    };

    // Helper to determine if a branch is selected and create appropriate style
    let make_style = |branch_name: &str| -> Style {
        let is_selected = selected_branch_name == Some(branch_name);
        if is_selected {
            // Inverted colors for selected branch
            Style::default()
                .fg(Color::Black)
                .bg(base_color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(base_color).add_modifier(Modifier::BOLD)
        }
    };

    // Handle local branches
    for local in local_branches.iter() {
        let remote_name = format!("origin/{}", local);
        let has_matching_remote = remote_branches.contains(remote_name.as_str());
        let style = make_style(local);

        if has_matching_remote {
            // Local and remote match -> compact display
            result.push((format!("[{} ↔ origin]", local), style));
            processed_remotes.insert(remote_name);
        } else {
            // Local only
            result.push((format!("[{}]", local), style));
        }
    }

    // Add remote branches without a local counterpart (same color as graph)
    for remote in branch_names.iter().filter(|n| n.starts_with("origin/")) {
        if !processed_remotes.contains(remote) {
            let style = make_style(remote);
            result.push((format!("[{}]", remote), style));
        }
    }

    result
}

/// Truncate a string to the specified display width
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
    selected_branch_name: Option<&str>,
) -> Line<'a> {
    let mut spans: Vec<Span> = Vec::new();

    // Graph start marker (to distinguish from borders)
    spans.push(Span::raw(" "));
    let mut left_width: usize = 1;

    // Render cells
    for cell in &node.cells {
        let (ch, color) = match cell {
            CellType::Empty => (' ', Color::Reset),
            CellType::Pipe(color_idx) => ('│', get_color_by_index(*color_idx)),
            CellType::Commit(color_idx) => {
                // HEAD uses a double circle, others use a filled circle
                let ch = if node.is_head { '◉' } else { '●' };
                // Main branch (blue) stays blue; other HEADs are green
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
                // Vertical and horizontal lines cross (use pipe color)
                ('┼', get_color_by_index(*p_color_idx))
            }
            CellType::TeeRight(color_idx) => ('├', get_color_by_index(*color_idx)),
            CellType::TeeLeft(color_idx) => ('┤', get_color_by_index(*color_idx)),
            CellType::TeeUp(color_idx) => ('┴', get_color_by_index(*color_idx)),
        };

        // Draw all line glyphs in bold
        let style = Style::default().fg(color).add_modifier(Modifier::BOLD);

        let ch_str = ch.to_string();
        let ch_width = display_width(&ch_str);
        spans.push(Span::styled(ch_str, style));
        left_width += ch_width;
    }

    // Padding to align graph width (display width based)
    let graph_display_width = (max_lane + 1) * 2;
    if left_width < graph_display_width + 1 {
        // +1 accounts for the start marker
        let padding = graph_display_width + 1 - left_width;
        spans.push(Span::raw(" ".repeat(padding)));
        left_width += padding;
    }

    // Separator between graph and commit info
    spans.push(Span::raw(" "));
    left_width += 1;

    // Early return for connector-only rows
    let commit = match &node.commit {
        Some(c) => c,
        None => return Line::from(spans),
    };

    // Style definitions
    let hash_style = Style::default().fg(Color::Yellow);
    let author_style = Style::default().fg(Color::Cyan);
    let date_style = Style::default().fg(Color::DarkGray);
    let msg_style = if is_selected {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    // === Left-aligned: branch names + message ===

    // Optimize branch names (compact when local matches origin/local)
    let branch_display = optimize_branch_display(
        &node.branch_names,
        node.is_head,
        node.color_index,
        selected_branch_name,
    );

    // === Right-aligned: date author hash (fixed width) ===
    let date = commit.timestamp.format("%Y-%m-%d").to_string(); // 10 chars
    let author = truncate_to_width(&commit.author_name, 8);
    let author_formatted = format!("{:<8}", author); // fixed 8 chars
    let hash = truncate_to_width(&commit.short_id, 7);
    let hash_formatted = format!("{:<7}", hash); // fixed 7 chars

    // Fixed width for right-aligned part: " YYYY-MM-DD  author    hash   "
    // Space1 + date10 + space2 + author8 + space2 + hash7 + space1 = 31
    const RIGHT_FIXED_WIDTH: usize = 31;

    // Render branch labels (bold, bracketed, graph color)
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

    // Compute max message width (use remaining space)
    let available_for_message = total_width
        .saturating_sub(left_width)
        .saturating_sub(RIGHT_FIXED_WIDTH);
    let message = truncate_to_width(&commit.message, available_for_message);
    let message_width = display_width(&message);
    spans.push(Span::styled(message, msg_style));
    left_width += message_width;

    // Padding so the right-aligned block starts at a fixed column
    let padding = total_width
        .saturating_sub(left_width)
        .saturating_sub(RIGHT_FIXED_WIDTH);
    if padding > 0 {
        spans.push(Span::raw(" ".repeat(padding)));
    }

    // === Append right-aligned block (fixed width) ===
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
