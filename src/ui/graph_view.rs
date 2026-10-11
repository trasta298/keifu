//! Graph view widget

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, StatefulWidget},
};
use unicode_width::UnicodeWidthChar;

use crate::{
    app::App,
    git::branch::BranchLabel,
    git::graph::{CellType, GraphNode},
    graph::colors::get_color_by_index,
};

use super::{render_placeholder_block, MIN_WIDGET_HEIGHT, MIN_WIDGET_WIDTH};

const ROW_SELECTION_BG: Color = Color::Rgb(40, 44, 62);
const MAX_REF_LABEL_WIDTH: usize = 40;
const MIN_COMMIT_MESSAGE_WIDTH: usize = 20;
const ABBREVIATED_LABEL_OVERHEAD: usize = 5;

fn with_row_selection(style: Style, is_row_selected: bool) -> Style {
    if is_row_selected {
        style.bg(ROW_SELECTION_BG)
    } else {
        style
    }
}

fn row_space(count: usize, is_row_selected: bool) -> Span<'static> {
    if count == 0 {
        return Span::raw(String::new());
    }
    let text = " ".repeat(count);
    if is_row_selected {
        Span::styled(text, Style::default().bg(ROW_SELECTION_BG))
    } else {
        Span::raw(text)
    }
}

/// VS16 (U+FE0F) variation selector for emoji presentation
const VS16: char = '\u{FE0F}';

/// Calculate character width considering VS16 emoji presentation sequence.
/// If `next_char` is VS16, the character has emoji presentation width (2).
/// VS16 itself has no width.
fn char_width_with_vs16(c: char, next_char: Option<char>) -> usize {
    if next_char == Some(VS16) {
        2
    } else if c == VS16 {
        0
    } else {
        UnicodeWidthChar::width(c).unwrap_or(0)
    }
}

/// Calculate display width of a string.
/// Handles VS16 which changes preceding character to emoji presentation (width 2).
pub(super) fn display_width(s: &str) -> usize {
    let chars: Vec<char> = s.chars().collect();
    let mut width = 0;
    let mut i = 0;
    while i < chars.len() {
        let next_char = chars.get(i + 1).copied();
        let ch_width = char_width_with_vs16(chars[i], next_char);
        width += ch_width;
        // Skip next char if it was VS16 (already accounted for)
        if next_char == Some(VS16) {
            i += 2;
        } else {
            i += 1;
        }
    }
    width
}

pub struct GraphViewWidget<'a> {
    items: Vec<ListItem<'a>>,
    focused: bool,
    /// (selected position, total rows) for the pane title
    position: (usize, usize),
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

        let focused = matches!(app.mode, crate::app::AppMode::Normal)
            && app.focused_pane == crate::app::FocusedPane::Graph;

        let position = (
            app.graph_list_state.selected().map_or(0, |idx| idx + 1),
            app.graph_layout.nodes.len(),
        );

        Self {
            items,
            focused,
            position,
        }
    }
}

/// Optimize branch name display
/// - Matching remotes are listed after the local branch, e.g. "main ↔ origin, upstream"
/// - Otherwise, show each name separately
/// - Render in bold with the graph color, wrapped in brackets
/// - Selected branch is shown with inverted colors
fn optimize_branch_display(
    branches: &[BranchLabel],
    max_width: usize,
    is_head: bool,
    color_index: usize,
    selected_branch_name: Option<&str>,
    is_row_selected: bool,
) -> Vec<(String, Style)> {
    if branches.is_empty() {
        return Vec::new();
    }

    // Determine base color: main branch stays blue; other HEADs are green
    let is_main_branch = color_index == crate::graph::colors::MAIN_BRANCH_COLOR;
    let base_color = if is_head && !is_main_branch {
        Color::Green
    } else {
        get_color_by_index(color_index)
    };

    // Helper to create style based on selection state
    let make_style = |branch_name: &str| -> Style {
        let style = Style::default().fg(base_color).add_modifier(Modifier::BOLD);
        if selected_branch_name == Some(branch_name) {
            style.fg(Color::Black).bg(base_color)
        } else if is_row_selected {
            with_row_selection(style, true)
        } else {
            style
        }
    };

    // Select from the same deduplicated branches used for navigation and counting.
    let branch = branches
        .iter()
        .find(|branch| Some(branch.name.as_str()) == selected_branch_name)
        .unwrap_or(&branches[0]);
    let count = if branches.len() > 1 {
        format!(" +{}", branches.len() - 1)
    } else {
        String::new()
    };
    let max_width = max_width.min(MAX_REF_LABEL_WIDTH);
    if max_width < ABBREVIATED_LABEL_OVERHEAD + display_width(&count) {
        return Vec::new();
    }
    let mut suffix = truncate_with_ellipsis(
        &branch.remote_suffix,
        max_width.saturating_sub(2 + display_width(&count) + display_width(&branch.name).min(4)),
    );
    suffix.push_str(&count);

    // Reserve space for the fitted remote list and count before abbreviating the name.
    let mut label = abbreviate_ref_label(
        &branch.name,
        max_width.saturating_sub(display_width(&suffix)),
        0,
        '[',
        ']',
    );
    label.insert_str(label.len() - 1, &suffix);

    vec![(label, make_style(&branch.name))]
}

/// Truncate a string to the specified display width.
/// Handles VS16 which changes preceding character to emoji presentation (width 2).
fn truncate_to_width(s: &str, max_width: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut result = String::new();
    let mut current_width = 0;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next_char = chars.get(i + 1).copied();
        let ch_width = char_width_with_vs16(c, next_char);
        if current_width + ch_width > max_width {
            break;
        }
        result.push(c);
        current_width += ch_width;
        if next_char == Some(VS16) {
            result.push(VS16);
            i += 2;
        } else {
            i += 1;
        }
    }
    result
}

/// Truncate by terminal columns without splitting a Unicode character.
pub(super) fn truncate_with_ellipsis(s: &str, max_width: usize) -> String {
    if display_width(s) <= max_width {
        s.to_string()
    } else {
        format!(
            "{}{}",
            truncate_to_width(s, max_width.saturating_sub(3)),
            ".".repeat(max_width.min(3))
        )
    }
}

/// Determine which right-side elements (date, author, hash) to display based on available width.
/// Returns (show_date, show_author, show_hash, total_right_width).
/// Priority: author > date > hash (hash disappears first, then date, then author)
fn compute_right_side_visibility(remaining_for_content: usize) -> (bool, bool, bool, usize) {
    // Widths for each display level (right-aligned block)
    const WIDTH_DATE_AUTHOR_HASH: usize = 31; // " YYYY-MM-DD  author    hash   "
    const WIDTH_DATE_AUTHOR: usize = 22; // " YYYY-MM-DD  author   "
    const WIDTH_AUTHOR_ONLY: usize = 11; // "  author   "

    // Ensure minimum space for branch + commit message before showing right-side info
    const CONTENT_MIN_WIDTH: usize = 50;
    let available = remaining_for_content.saturating_sub(CONTENT_MIN_WIDTH);

    if available >= WIDTH_DATE_AUTHOR_HASH {
        (true, true, true, WIDTH_DATE_AUTHOR_HASH)
    } else if available >= WIDTH_DATE_AUTHOR {
        (true, true, false, WIDTH_DATE_AUTHOR)
    } else if available >= WIDTH_AUTHOR_ONLY {
        (false, true, false, WIDTH_AUTHOR_ONLY)
    } else {
        (false, false, false, 0)
    }
}

fn abbreviate_ref_label(
    name: &str,
    max_width: usize,
    extra_count: usize,
    open: char,
    close: char,
) -> String {
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
        return format!("{}{}{}{}", open, name, suffix, close);
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
        return format!("{}{}...{}{}", open, truncated, suffix, close);
    }

    let head = truncate_to_width(rest, head_available);

    format!(
        "{}{}{}{}{}{}{}",
        open, prefix, head, ELLIPSIS, tail, suffix, close
    )
}

fn optimize_tag_display(
    tag_names: &[String],
    max_width: usize,
    is_row_selected: bool,
) -> Vec<(String, Style)> {
    if tag_names.is_empty() {
        return Vec::new();
    }

    const TAG_COLOR: Color = Color::Yellow;
    let max_width = max_width.min(MAX_REF_LABEL_WIDTH);
    let extra_count = tag_names.len().saturating_sub(1);
    let suffix_width = if extra_count > 0 {
        display_width(&format!(" +{extra_count}"))
    } else {
        0
    };
    if max_width < ABBREVIATED_LABEL_OVERHEAD + suffix_width {
        return Vec::new();
    }

    let style = with_row_selection(
        Style::default().fg(TAG_COLOR).add_modifier(Modifier::BOLD),
        is_row_selected,
    );

    if extra_count > 0 {
        let label = abbreviate_ref_label(&tag_names[0], max_width, extra_count, '(', ')');
        return vec![(label, style)];
    }

    let name = &tag_names[0];
    let label = format!("({})", name);
    if display_width(&label) <= max_width {
        return vec![(label, style)];
    }

    vec![(abbreviate_ref_label(name, max_width, 0, '(', ')'), style)]
}

fn render_graph_line<'a>(
    node: &GraphNode,
    max_lane: usize,
    is_selected: bool,
    total_width: usize,
    selected_branch_name: Option<&str>,
) -> Line<'a> {
    let mut spans: Vec<Span> = Vec::new();

    // Graph start marker; accent bar makes the selected row easy to spot
    if is_selected {
        spans.push(Span::styled(
            "▌",
            with_row_selection(Style::default().fg(Color::Cyan), true),
        ));
    } else {
        spans.push(Span::raw(" "));
    }
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
        let style = with_row_selection(
            Style::default().fg(color).add_modifier(Modifier::BOLD),
            is_selected,
        );

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
        spans.push(row_space(padding, is_selected));
        left_width += padding;
    }

    // Separator between graph and commit info
    spans.push(row_space(1, is_selected));
    left_width += 1;

    // Handle uncommitted changes row
    if node.is_uncommitted {
        let text = match node.uncommitted_count {
            Some(count) => format!("uncommitted changes ({})", count),
            None => "uncommitted changes".to_string(),
        };
        let style = with_row_selection(Style::default().fg(Color::Rgb(192, 202, 245)), is_selected);
        spans.push(Span::styled(text, style));
        return Line::from(spans);
    }

    // Early return for connector-only rows
    let commit = match &node.commit {
        Some(c) => c,
        None => return Line::from(spans),
    };

    // Style definitions
    let hash_style = with_row_selection(
        Style::default().fg(if is_selected {
            Color::Rgb(224, 175, 104)
        } else {
            Color::Yellow
        }),
        is_selected,
    );
    let author_style = with_row_selection(
        Style::default().fg(if is_selected {
            Color::Rgb(125, 207, 255)
        } else {
            Color::Cyan
        }),
        is_selected,
    );
    let date_style = with_row_selection(
        Style::default().fg(if is_selected {
            Color::Rgb(154, 162, 180)
        } else {
            Color::DarkGray
        }),
        is_selected,
    );
    let msg_style = if is_selected {
        with_row_selection(
            Style::default()
                .fg(Color::Rgb(192, 202, 245))
                .add_modifier(Modifier::BOLD),
            true,
        )
    } else {
        Style::default()
    };

    // === Left-aligned: branch names + message ===

    let remaining_for_content = total_width.saturating_sub(left_width);
    // Render the grouped labels used for navigation and the branch popup.
    let branch_display = optimize_branch_display(
        &node.branch_labels,
        remaining_for_content.saturating_sub(1),
        node.is_head,
        node.color_index,
        selected_branch_name,
        is_selected,
    );

    // === Right-aligned: date author hash (fixed width) ===
    let date = commit.timestamp.format("%Y-%m-%d").to_string(); // 10 chars
    let author = truncate_to_width(&commit.author_name, 8);
    let author_formatted = format!("{:<8}", author); // fixed 8 chars
    let hash = truncate_to_width(&commit.short_id, 7);
    let hash_formatted = format!("{:<7}", hash); // fixed 7 chars

    let branch_width: usize = branch_display
        .iter()
        .enumerate()
        .map(|(i, (label, _))| display_width(label) + if i > 0 { 1 } else { 0 })
        .sum::<usize>()
        + if !branch_display.is_empty() { 1 } else { 0 };

    let max_tag_width = remaining_for_content
        .saturating_sub(branch_width)
        .saturating_sub(MIN_COMMIT_MESSAGE_WIDTH)
        .saturating_sub(1)
        .min(MAX_REF_LABEL_WIDTH);
    let tag_display = optimize_tag_display(&node.tag_names, max_tag_width, is_selected);
    let tag_width: usize = tag_display
        .iter()
        .map(|(label, _)| display_width(label))
        .sum::<usize>()
        + if !tag_display.is_empty() { 1 } else { 0 };

    let remaining_after_refs = remaining_for_content
        .saturating_sub(branch_width)
        .saturating_sub(tag_width);
    let (show_date, show_author, show_hash, right_width) =
        compute_right_side_visibility(remaining_after_refs);

    // Render branch labels
    for (i, (label, style)) in branch_display.iter().enumerate() {
        if i > 0 {
            spans.push(row_space(1, is_selected));
            left_width += 1;
        }
        left_width += display_width(label);
        spans.push(Span::styled(label.clone(), *style));
    }
    if !branch_display.is_empty() {
        spans.push(row_space(1, is_selected));
        left_width += 1;
    }

    for (label, style) in &tag_display {
        left_width += display_width(label);
        spans.push(Span::styled(label.clone(), *style));
    }
    if !tag_display.is_empty() {
        spans.push(row_space(1, is_selected));
        left_width += 1;
    }

    let available_for_message = remaining_after_refs.saturating_sub(right_width);
    let message = truncate_to_width(&commit.message, available_for_message);
    let message_width = display_width(&message);
    spans.push(Span::styled(message, msg_style));
    left_width += message_width;

    // Padding so the right-aligned block starts at a fixed column
    let padding = total_width
        .saturating_sub(left_width)
        .saturating_sub(right_width);
    if padding > 0 {
        spans.push(row_space(padding, is_selected));
    }

    // === Append right-aligned block (display: date, author, hash) ===
    if show_date {
        spans.push(row_space(1, is_selected));
        spans.push(Span::styled(date, date_style));
    }
    if show_author {
        spans.push(row_space(2, is_selected));
        spans.push(Span::styled(author_formatted, author_style));
    }
    if show_hash {
        spans.push(row_space(2, is_selected));
        spans.push(Span::styled(hash_formatted, hash_style));
    }
    if show_date || show_author || show_hash {
        spans.push(row_space(1, is_selected));
    }

    Line::from(spans)
}

impl<'a> StatefulWidget for GraphViewWidget<'a> {
    type State = ListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        if area.width < MIN_WIDGET_WIDTH || area.height < MIN_WIDGET_HEIGHT {
            render_placeholder_block(area, buf);
            return;
        }

        let title = format!("Commits {}/{}", self.position.0, self.position.1);
        let block = super::pane_block(&title, self.focused);

        let list = List::new(self.items).block(block);

        StatefulWidget::render(list, area, buf, state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(names: &[String]) -> Vec<BranchLabel> {
        use crate::git::{branch::group_branch_labels, BranchInfo};

        let branches: Vec<_> = names
            .iter()
            .map(|name| BranchInfo {
                name: name.clone(),
                is_head: false,
                is_remote: name.starts_with("origin/"),
                remote_name: name.starts_with("origin/").then(|| "origin".to_string()),
                upstream: None,
                tip_oid: git2::Oid::zero(),
            })
            .collect();
        group_branch_labels(&branches.iter().collect::<Vec<_>>())
    }

    #[test]
    fn collapsed_branches_preserve_origin_marker() {
        let names = ["main", "dev", "origin/dev", "origin/main"].map(String::from);

        for (selected, expected) in [
            (None, "[main ↔ origin +1]"),
            (Some("main"), "[main ↔ origin +1]"),
            (Some("dev"), "[dev ↔ origin +1]"),
        ] {
            let result = optimize_branch_display(
                &labels(&names),
                MAX_REF_LABEL_WIDTH,
                true,
                0,
                selected,
                true,
            );

            assert_eq!(result.len(), 1);
            assert_eq!(result[0].0, expected);
            if selected.is_some() {
                assert_eq!(result[0].1.fg, Some(Color::Black));
            }
        }
    }

    #[test]
    fn collapsed_branches_select_from_deduplicated_names() {
        let names = ["main", "origin/main", "origin/zzz", "zzz", "zzz2"].map(String::from);
        let result = optimize_branch_display(
            &labels(&names),
            MAX_REF_LABEL_WIDTH,
            true,
            0,
            Some("zzz"),
            true,
        );

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "[zzz ↔ origin +2]");
        assert_eq!(result[0].1.fg, Some(Color::Black));
    }

    #[test]
    fn collapsed_synced_branches_abbreviate_only_the_name() {
        for name in [
            "feature/this-is-a-very-long-branch-name-12345",
            "feature/とても長いブランチ名のテストです-12345",
        ] {
            let names = vec![
                name.to_string(),
                format!("origin/{name}"),
                "main".to_string(),
            ];
            let result = optimize_branch_display(
                &labels(&names),
                MAX_REF_LABEL_WIDTH,
                false,
                0,
                Some(name),
                false,
            );
            let label = &result[0].0;

            assert!(label.starts_with("[feature/"), "{label}");
            assert!(label.contains("..."), "{label}");
            assert!(label.ends_with("12345 ↔ origin +1]"), "{label}");
            assert!(display_width(label) <= MAX_REF_LABEL_WIDTH, "{label}");
        }
    }

    #[test]
    fn many_remotes_keep_branch_name_and_count_within_label_width() {
        let branches = [
            BranchLabel {
                name: "main".to_string(),
                remote_suffix: " ↔ origin, origin_2, origin_3, origin_4, origin_5".to_string(),
            },
            BranchLabel {
                name: "dev".to_string(),
                remote_suffix: String::new(),
            },
        ];
        let rendered =
            optimize_branch_display(&branches, MAX_REF_LABEL_WIDTH, true, 0, Some("main"), true);
        let label = &rendered[0].0;
        assert!(label.starts_with("[main ↔ origin"), "{label}");
        assert!(label.ends_with("... +1]"), "{label}");
        assert!(display_width(label) <= MAX_REF_LABEL_WIDTH, "{label}");
    }

    #[test]
    fn optimize_tag_display_returns_empty_for_no_tags() {
        assert!(optimize_tag_display(&[], MAX_REF_LABEL_WIDTH, false).is_empty());
    }

    #[test]
    fn optimize_tag_display_wraps_single_tag_in_parens() {
        let result = optimize_tag_display(&["v1.0.0".to_string()], MAX_REF_LABEL_WIDTH, false);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "(v1.0.0)");
    }

    #[test]
    fn optimize_tag_display_collapses_multiple_tags_with_count() {
        let names = vec!["v1.0.0".to_string(), "v1.1.0".to_string()];
        let result = optimize_tag_display(&names, MAX_REF_LABEL_WIDTH, false);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "(v1.0.0 +1)");
    }

    #[test]
    fn optimize_tag_display_abbreviates_long_tag_name() {
        let long_name = "a-very-very-very-long-release-tag-name-2024".to_string();
        let result = optimize_tag_display(&[long_name], MAX_REF_LABEL_WIDTH, false);
        assert_eq!(result.len(), 1);
        assert!(result[0].0.len() <= 40);
        assert!(result[0].0.starts_with('('));
        assert!(result[0].0.ends_with(')'));
    }

    #[test]
    fn abbreviate_ref_label_places_suffix_inside_brackets() {
        assert_eq!(abbreviate_ref_label("main", 40, 2, '[', ']'), "[main +2]");
    }

    #[test]
    fn long_branch_and_tags_preserve_commit_message_within_viewport() {
        use chrono::Local;
        use git2::Oid;

        use crate::git::CommitInfo;

        let oid = Oid::from_str("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        let node = GraphNode {
            commit: Some(CommitInfo {
                oid,
                short_id: "aaaaaaa".to_string(),
                author_name: "Reviewer".to_string(),
                author_email: "review@example.com".to_string(),
                timestamp: Local::now(),
                message: "release commit".to_string(),
                full_message: "release commit".to_string(),
                parent_oids: Vec::new(),
            }),
            lane: 0,
            color_index: 0,
            branch_names: vec!["feature/this-is-a-very-long-branch-name-123456789".to_string()],
            branch_labels: labels(&[
                "feature/this-is-a-very-long-branch-name-123456789".to_string()
            ]),
            tag_names: vec![
                "release/this-is-a-very-long-tag-name-123456789".to_string(),
                "release/zzz-second-tag".to_string(),
            ],
            is_head: false,
            is_uncommitted: false,
            uncommitted_count: None,
            cells: vec![CellType::Commit(0)],
        };
        let width = 78;
        let line = render_graph_line(&node, 0, false, width, None);
        let rendered = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(rendered.contains("+1)"));
        assert!(rendered.contains("release commit"));
        assert!(display_width(&rendered) <= width);
    }
}
