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
/// Accounts for emoji variation selectors (U+FE0F) which cause preceding
/// characters to display as 2-width emoji in terminals
fn display_width(s: &str) -> usize {
    let base_width = UnicodeWidthStr::width(s);
    // Count variation selectors - each one adds 1 to width
    // because the preceding character becomes a 2-width emoji
    let variation_selectors = s.chars().filter(|&c| c == '\u{FE0F}').count();
    base_width + variation_selectors
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

    // Max width for a single branch label (e.g., "[fix/feature-name]")
    const MAX_LABEL_WIDTH: usize = 40;

    // Split local and remote branches (HashSet for O(1) lookup)
    let local_branches: HashSet<&str> = branch_names
        .iter()
        .filter(|n| !n.starts_with("origin/"))
        .map(|s| s.as_str())
        .collect();
    let remote_branches: HashSet<&str> = branch_names
        .iter()
        .filter(|n| n.starts_with("origin/"))
        .map(|s| s.as_str())
        .collect();

    // Determine base color: main branch stays blue; other HEADs are green
    let is_main_branch = color_index == crate::graph::colors::MAIN_BRANCH_COLOR;
    let base_color = if is_head && !is_main_branch {
        Color::Green
    } else {
        get_color_by_index(color_index)
    };

    // Helper to create style based on selection state
    let make_style = |branch_name: &str| -> Style {
        if selected_branch_name == Some(branch_name) {
            Style::default()
                .fg(Color::Black)
                .bg(base_color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(base_color).add_modifier(Modifier::BOLD)
        }
    };

    // Helper to create label with optional abbreviation
    let make_label = |name: &str, suffix: Option<&str>| -> String {
        let (label, abbrev_width) = match suffix {
            Some(s) => (format!("[{} {}]", name, s), MAX_LABEL_WIDTH - s.len() - 3),
            None => (format!("[{}]", name), MAX_LABEL_WIDTH),
        };
        if display_width(&label) > MAX_LABEL_WIDTH {
            let abbrev = abbreviate_branch_label(name, abbrev_width, 0);
            match suffix {
                Some(s) => abbrev.replace(']', &format!(" {}]", s)),
                None => abbrev,
            }
        } else {
            label
        }
    };

    // Process branches in original order (matches tab order from filter_remote_duplicates)
    let mut result: Vec<(String, Style)> = Vec::new();
    for name in branch_names {
        if let Some(local_name) = name.strip_prefix("origin/") {
            // Remote branch: skip if matching local exists
            if local_branches.contains(local_name) {
                continue;
            }
            result.push((make_label(name, None), make_style(name)));
        } else {
            // Local branch: check for matching remote
            let remote_name = format!("origin/{}", name);
            let suffix = if remote_branches.contains(remote_name.as_str()) {
                Some("↔ origin")
            } else {
                None
            };
            result.push((make_label(name, suffix), make_style(name)));
        }
    }

    // Collapse multiple branches to single + count
    if result.len() > 1 {
        // Find selected index directly from branch_names
        let selected_idx = selected_branch_name
            .and_then(|sel| branch_names.iter().position(|n| n == sel || n.ends_with(&format!("/{}", sel))))
            .unwrap_or(0);

        let (label, style) = &result[selected_idx];
        let clean_name = label
            .trim_start_matches('[')
            .split([']', ' '])
            .next()
            .unwrap_or(label);
        let abbreviated = abbreviate_branch_label(clean_name, MAX_LABEL_WIDTH, result.len() - 1);

        return vec![(abbreviated, *style)];
    }

    result
}

/// Truncate a string to the specified display width
/// Accounts for emoji variation selectors (U+FE0F)
fn truncate_to_width(s: &str, max_width: usize) -> String {
    let mut result = String::new();
    let mut current_width = 0;
    for ch in s.chars() {
        // Variation selector adds 1 to width (makes preceding char 2-width emoji)
        let ch_width = if ch == '\u{FE0F}' {
            1
        } else {
            unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0)
        };
        if current_width + ch_width > max_width {
            break;
        }
        result.push(ch);
        current_width += ch_width;
    }
    result
}

/// Abbreviate branch name to max_width, showing "+N" if more branches exist
/// Uses format: prefix/head...tail (preserving last 5 chars)
fn abbreviate_branch_label(name: &str, max_width: usize, extra_count: usize) -> String {
    const TAIL_LEN: usize = 5;
    const ELLIPSIS: &str = "...";

    let suffix = if extra_count > 0 {
        format!(" +{}", extra_count)
    } else {
        String::new()
    };

    let suffix_len = display_width(&suffix);
    let available = max_width.saturating_sub(suffix_len).saturating_sub(2); // -2 for brackets

    // If name fits, return as-is
    if display_width(name) <= available {
        return format!("[{}]{}", name, suffix);
    }

    // Find "/" position to preserve prefix
    let slash_pos = name.find('/');

    // Split into prefix and rest
    let (prefix, rest) = match slash_pos {
        Some(pos) => (&name[..=pos], &name[pos + 1..]),
        None => ("", name),
    };

    let prefix_width = display_width(prefix);
    let ellipsis_width = display_width(ELLIPSIS);

    // Get last TAIL_LEN characters from rest
    let rest_chars: Vec<char> = rest.chars().collect();
    let tail: String = if rest_chars.len() > TAIL_LEN {
        rest_chars[rest_chars.len() - TAIL_LEN..].iter().collect()
    } else {
        rest.to_string()
    };
    let tail_width = display_width(&tail);

    // Calculate available width for head portion
    let head_available = available.saturating_sub(prefix_width + ellipsis_width + tail_width);

    if head_available == 0 {
        // Not enough space for head, just show truncated name
        let truncated = truncate_to_width(name, available.saturating_sub(3));
        return format!("[{}...]{}", truncated, suffix);
    }

    let head = truncate_to_width(rest, head_available);

    format!("[{}{}{}{}]{}", prefix, head, ELLIPSIS, tail, suffix)
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
                let is_main = *color_idx == crate::graph::colors::MAIN_BRANCH_COLOR;
                let color = if node.is_head && !is_main {
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

    // Handle uncommitted changes row
    if node.is_uncommitted {
        let text = format!("uncommitted changes ({})", node.uncommitted_count);
        let style = Style::default().fg(Color::White);
        spans.push(Span::styled(text, style));
        return Line::from(spans);
    }

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

    // Widths for each display level (right-aligned block)
    // Display order: date, author, hash
    // Priority: author > date > hash (hash disappears first, then date, then author)
    const WIDTH_DATE_AUTHOR_HASH: usize = 31; // " YYYY-MM-DD  author    hash   "
    const WIDTH_DATE_AUTHOR: usize = 22; // " YYYY-MM-DD  author   "
    const WIDTH_AUTHOR_ONLY: usize = 11; // "  author   "

    // Calculate branch width first (before rendering)
    let branch_width: usize = branch_display
        .iter()
        .enumerate()
        .map(|(i, (label, _))| display_width(label) + if i > 0 { 1 } else { 0 })
        .sum::<usize>()
        + if !branch_display.is_empty() { 1 } else { 0 };

    // Calculate remaining space for branch + message + right info
    let graph_width = left_width;
    let remaining_for_content = total_width.saturating_sub(graph_width);

    // Ensure minimum space for branch + commit message before showing right-side info
    // This keeps right-side alignment consistent across all rows
    const CONTENT_MIN_WIDTH: usize = 50;
    let available_for_right = remaining_for_content.saturating_sub(CONTENT_MIN_WIDTH);
    let (show_date, show_author, show_hash, right_width) = match available_for_right {
        w if w >= WIDTH_DATE_AUTHOR_HASH => (true, true, true, WIDTH_DATE_AUTHOR_HASH),
        w if w >= WIDTH_DATE_AUTHOR => (true, true, false, WIDTH_DATE_AUTHOR),
        w if w >= WIDTH_AUTHOR_ONLY => (false, true, false, WIDTH_AUTHOR_ONLY),
        _ => (false, false, false, 0),
    };

    // Render branch labels
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

    // Compute max message width (remaining space after branch and right side)
    let available_for_message = remaining_for_content
        .saturating_sub(branch_width)
        .saturating_sub(right_width);
    let message = truncate_to_width(&commit.message, available_for_message);
    let message_width = display_width(&message);
    spans.push(Span::styled(message, msg_style));
    left_width += message_width;

    // Padding so the right-aligned block starts at a fixed column
    let padding = total_width
        .saturating_sub(left_width)
        .saturating_sub(right_width);
    if padding > 0 {
        spans.push(Span::raw(" ".repeat(padding)));
    }

    // === Append right-aligned block (display: date, author, hash) ===
    if show_date {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(date, date_style));
    }
    if show_author {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(author_formatted, author_style));
    }
    if show_hash {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(hash_formatted, hash_style));
    }
    if show_date || show_author || show_hash {
        spans.push(Span::raw(" "));
    }

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
