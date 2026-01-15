//! Search dropdown widget with fuzzy matching

use crate::search::FuzzySearchResult;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Widget},
};

const MAX_VISIBLE_RESULTS: usize = 7;

/// Search dropdown widget showing input field and fuzzy search results
pub struct SearchDropdown<'a> {
    input: &'a str,
    results: &'a [FuzzySearchResult],
    branch_names: &'a [(usize, String)],
    selected_index: Option<usize>,
}

impl<'a> SearchDropdown<'a> {
    pub fn new(
        input: &'a str,
        results: &'a [FuzzySearchResult],
        branch_names: &'a [(usize, String)],
        selected_index: Option<usize>,
    ) -> Self {
        Self {
            input,
            results,
            branch_names,
            selected_index,
        }
    }

    /// Get the branch name for a search result
    fn get_branch_name(&self, result: &FuzzySearchResult) -> &str {
        self.branch_names
            .get(result.branch_idx)
            .map(|(_, name)| name.as_str())
            .unwrap_or("")
    }

    /// Render a branch name with matched characters highlighted
    fn render_highlighted_name(
        &self,
        result: &FuzzySearchResult,
        max_width: usize,
    ) -> Vec<Span<'a>> {
        let name = self.get_branch_name(result);
        let matched_set: std::collections::HashSet<usize> =
            result.matched_indices.iter().copied().collect();

        let mut spans = Vec::new();
        let mut current_segment = String::new();
        let mut current_is_matched = false;

        for (char_idx, ch) in name.chars().enumerate() {
            if char_idx >= max_width.saturating_sub(3) {
                current_segment.push_str("...");
                break;
            }

            let is_matched = matched_set.contains(&char_idx);

            if is_matched != current_is_matched && !current_segment.is_empty() {
                spans.push(Span::styled(
                    std::mem::take(&mut current_segment),
                    highlight_style(current_is_matched),
                ));
            }

            current_segment.push(ch);
            current_is_matched = is_matched;
        }

        if !current_segment.is_empty() {
            spans.push(Span::styled(
                current_segment,
                highlight_style(current_is_matched),
            ));
        }

        spans
    }
}

/// Get style for highlighted/non-highlighted text
fn highlight_style(is_matched: bool) -> Style {
    if is_matched {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    }
}

impl<'a> Widget for SearchDropdown<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);

        // Calculate dynamic height based on results
        let has_results = !self.results.is_empty();
        let visible_count = self.results.len().min(MAX_VISIBLE_RESULTS);

        // Build block with cyan border (matching InputDialog style)
        let block = Block::default()
            .title(" Search branches ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .style(Style::default().bg(Color::Black));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 2 || inner.width < 4 {
            return;
        }

        let mut y = inner.y;

        // Render input line
        let input_style = Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::UNDERLINED);
        let cursor_style = Style::default().fg(Color::Cyan);

        let input_line = Line::from(vec![
            Span::raw("  "),
            Span::styled(self.input, input_style),
            Span::styled("_", cursor_style),
        ]);
        buf.set_line(inner.x, y, &input_line, inner.width);
        y += 1;

        // If we have results and space, show separator and results
        if has_results && y < inner.y + inner.height {
            // Draw separator line
            let separator = "─".repeat(inner.width as usize);
            buf.set_string(inner.x, y, &separator, Style::default().fg(Color::DarkGray));
            y += 1;

            // Calculate scroll offset to keep selected item visible
            let selected = self.selected_index.unwrap_or(0);
            let scroll_offset = if selected >= visible_count {
                selected - visible_count + 1
            } else {
                0
            };

            let has_more_above = scroll_offset > 0;
            let has_more_below = scroll_offset + visible_count < self.results.len();

            // Render results with scroll
            let max_name_width = inner.width.saturating_sub(4) as usize;

            for (display_idx, (i, result)) in self
                .results
                .iter()
                .enumerate()
                .skip(scroll_offset)
                .take(visible_count)
                .enumerate()
            {
                if y >= inner.y + inner.height {
                    break;
                }

                let is_selected = self.selected_index == Some(i);

                // Show scroll indicators on first/last visible items
                let prefix = if display_idx == 0 && has_more_above {
                    if is_selected {
                        "▲ "
                    } else {
                        "↑ "
                    }
                } else if display_idx == visible_count - 1 && has_more_below {
                    if is_selected {
                        "▼ "
                    } else {
                        "↓ "
                    }
                } else if is_selected {
                    "▶ "
                } else {
                    "  "
                };

                // Build the line with highlighting
                let mut spans = vec![Span::styled(
                    prefix,
                    if is_selected {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                )];

                if is_selected {
                    // For selected item, use inverted colors without per-char highlighting
                    let name = self.get_branch_name(result);
                    let display_name: String = name.chars().take(max_name_width).collect();
                    spans.push(Span::styled(
                        display_name,
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ));
                } else {
                    // For non-selected items, show match highlighting
                    spans.extend(self.render_highlighted_name(result, max_name_width));
                }

                let line = Line::from(spans);
                buf.set_line(inner.x, y, &line, inner.width);
                y += 1;
            }
        }

        // Show hint at bottom if there's space
        if y < inner.y + inner.height {
            let hint = select_hint_text(inner.width as usize, has_results, self.input.is_empty());
            if !hint.is_empty() {
                let hint_y = inner.y + inner.height - 1;
                buf.set_string(inner.x, hint_y, hint, Style::default().fg(Color::DarkGray));
            }
        }
    }
}

/// Select appropriate hint text based on width and state
fn select_hint_text(width: usize, has_results: bool, input_empty: bool) -> &'static str {
    if has_results {
        match width {
            40.. => "  ↑↓: select  Enter: jump  Esc: cancel",
            28..=39 => "  ↑↓/Tab  Enter  Esc",
            16..=27 => "  ↑↓ Enter Esc",
            _ => "",
        }
    } else if input_empty {
        match width {
            28.. => "  Enter: confirm  Esc: cancel",
            16..=27 => "  Enter  Esc",
            _ => "",
        }
    } else if width >= 12 {
        "  No matches"
    } else {
        ""
    }
}

/// Calculate the required height for the search dropdown
pub fn calculate_dropdown_height(result_count: usize) -> u16 {
    // Input line (1) + separator (1 if results) + results (up to MAX) + hint (1) + borders (2)
    let base_height = 4; // borders + input + hint
    let results_height = if result_count > 0 {
        1 + result_count.min(MAX_VISIBLE_RESULTS) // separator + results
    } else {
        0
    };
    (base_height + results_height) as u16
}
