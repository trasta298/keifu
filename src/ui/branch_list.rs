//! Branch triage list widget (full-screen, opened with B)

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use unicode_width::UnicodeWidthStr;

use crate::git::BranchTriageRow;

use super::commit_detail::relative_time;
use super::{render_placeholder_block, MIN_WIDGET_HEIGHT, MIN_WIDGET_WIDTH};

const NAME_WIDTH: usize = 28;
const STATUS_WIDTH: usize = 12;

/// Scroll offset that keeps the selected row visible
pub fn scroll_offset(selected: usize, row_count: usize, area: Rect) -> u16 {
    let visible_height = area.height.saturating_sub(2);
    if visible_height == 0 {
        return 0;
    }
    let selected = selected as u16;
    let max_scroll = (row_count as u16).saturating_sub(visible_height);
    if selected >= visible_height {
        (selected - visible_height / 2).min(max_scroll)
    } else {
        0
    }
}

pub struct BranchListWidget<'a> {
    rows: &'a [BranchTriageRow],
    selected: usize,
    base_name: Option<&'a str>,
    scroll: u16,
}

impl<'a> BranchListWidget<'a> {
    pub fn new(
        rows: &'a [BranchTriageRow],
        selected: usize,
        base_name: Option<&'a str>,
        scroll: u16,
    ) -> Self {
        Self {
            rows,
            selected,
            base_name,
            scroll,
        }
    }

    fn truncate(text: &str, max_width: usize) -> String {
        if text.width() <= max_width {
            return text.to_string();
        }
        let mut result = String::new();
        let mut width = 1; // for the trailing ellipsis
        for c in text.chars() {
            let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            if width + w > max_width {
                break;
            }
            result.push(c);
            width += w;
        }
        format!("{}…", result)
    }

    fn build_line(
        &self,
        row: &BranchTriageRow,
        is_selected: bool,
        inner_width: usize,
    ) -> Line<'static> {
        let mut spans: Vec<Span> = Vec::new();

        if is_selected {
            spans.push(Span::styled("▌", Style::default().fg(Color::Cyan)));
        } else {
            spans.push(Span::raw(" "));
        }

        // Flag column: HEAD or worktree
        if row.is_head {
            spans.push(Span::styled("◉ ", Style::default().fg(Color::Green)));
        } else if row.worktree_path.is_some() {
            spans.push(Span::styled("⌂ ", Style::default().fg(Color::Magenta)));
        } else {
            spans.push(Span::raw("  "));
        }

        // Branch name
        let name = Self::truncate(&row.name, NAME_WIDTH);
        let name_pad = NAME_WIDTH.saturating_sub(name.width()) + 1;
        let name_style = if row.is_head {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        };
        spans.push(Span::styled(name, name_style));
        spans.push(Span::raw(" ".repeat(name_pad)));

        // Status: base / merged / ahead-behind
        let (status_spans, status_width) = if row.is_base {
            (
                vec![Span::styled("base", Style::default().fg(Color::Blue))],
                4,
            )
        } else if row.merged {
            (
                vec![Span::styled("merged", Style::default().fg(Color::Magenta))],
                6,
            )
        } else {
            let ahead = format!("↑{}", row.ahead);
            let behind = format!("↓{}", row.behind);
            let width = ahead.width() + 1 + behind.width();
            (
                vec![
                    Span::styled(ahead, Style::default().fg(Color::Green)),
                    Span::raw(" "),
                    Span::styled(behind, Style::default().fg(Color::Red)),
                ],
                width,
            )
        };
        spans.extend(status_spans);
        spans.push(Span::raw(
            " ".repeat(STATUS_WIDTH.saturating_sub(status_width) + 1),
        ));

        // Right-aligned relative time; subject fills the middle
        let date = relative_time(row.last_commit);
        let used: usize = 1 + 2 + NAME_WIDTH + 1 + STATUS_WIDTH + 1;
        let subject_budget = inner_width
            .saturating_sub(used)
            .saturating_sub(date.width() + 2);
        if subject_budget > 4 {
            let subject = Self::truncate(&row.subject, subject_budget);
            let subject_width = subject.width();
            spans.push(Span::styled(subject, Style::default().fg(Color::DarkGray)));
            spans.push(Span::raw(
                " ".repeat(subject_budget.saturating_sub(subject_width) + 2),
            ));
            spans.push(Span::styled(date, Style::default().fg(Color::DarkGray)));
        }

        let mut line = Line::from(spans);
        if is_selected {
            line = line.style(Style::default().bg(Color::Rgb(40, 44, 62)));
        }
        line
    }
}

impl<'a> Widget for BranchListWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < MIN_WIDGET_WIDTH || area.height < MIN_WIDGET_HEIGHT {
            render_placeholder_block(area, buf);
            return;
        }

        let title = match self.base_name {
            Some(base) => format!("Branches ({}) · vs {}", self.rows.len(), base),
            None => format!("Branches ({})", self.rows.len()),
        };
        let block = super::pane_block(&title, true);

        let inner_width = area.width.saturating_sub(2) as usize;
        let lines: Vec<Line> = self
            .rows
            .iter()
            .enumerate()
            .map(|(idx, row)| self.build_line(row, idx == self.selected, inner_width))
            .collect();

        let paragraph = Paragraph::new(lines).block(block).scroll((self.scroll, 0));
        Widget::render(paragraph, area, buf);
    }
}
