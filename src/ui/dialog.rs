//! Input and confirmation dialog widgets

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

/// Truncate a string to fit within max_width, adding "..." if needed
fn truncate_with_ellipsis(s: &str, max_width: usize) -> String {
    if s.len() <= max_width {
        s.to_string()
    } else {
        format!("{}...", &s[..max_width.saturating_sub(3)])
    }
}

/// Input dialog
pub struct InputDialog<'a> {
    title: &'a str,
    input: &'a str,
}

impl<'a> InputDialog<'a> {
    pub fn new(title: &'a str, input: &'a str) -> Self {
        Self { title, input }
    }
}

impl<'a> Widget for InputDialog<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);

        let block = Block::default()
            .title(format!(" {} ", self.title))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .style(Style::default().bg(Color::Black));

        let input_style = Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::UNDERLINED);

        let hint_style = Style::default().fg(Color::DarkGray);
        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(self.input, input_style),
                Span::styled("_", Style::default().fg(Color::Cyan)),
            ]),
            Line::from(""),
            Line::from(Span::styled("  Enter: confirm  Esc: cancel", hint_style)),
        ];

        let paragraph = Paragraph::new(lines).block(block);
        Widget::render(paragraph, area, buf);
    }
}

/// Confirmation dialog
pub struct ConfirmDialog<'a> {
    message: &'a str,
}

impl<'a> ConfirmDialog<'a> {
    pub fn new(message: &'a str) -> Self {
        Self { message }
    }
}

impl<'a> Widget for ConfirmDialog<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);

        let block = Block::default()
            .title(" Confirm ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .style(Style::default().bg(Color::Black));

        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("  {}", self.message),
                Style::default().fg(Color::White),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "  y",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(": Yes  "),
                Span::styled(
                    "n",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::raw(": No"),
            ]),
        ];

        let paragraph = Paragraph::new(lines).block(block);
        Widget::render(paragraph, area, buf);
    }
}

/// Branch info popup (shown when multiple branches exist on selected node)
pub struct BranchInfoPopup<'a> {
    branches: &'a [&'a str],
    selected_branch: Option<&'a str>,
}

impl<'a> BranchInfoPopup<'a> {
    pub fn new(branches: &'a [&'a str], selected_branch: Option<&'a str>) -> Self {
        Self {
            branches,
            selected_branch,
        }
    }
}

impl<'a> Widget for BranchInfoPopup<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);

        let block = Block::default()
            .title(" Branches ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue))
            .style(Style::default().bg(Color::Black));

        let inner = block.inner(area);
        block.render(area, buf);

        // Render branch list
        for (i, branch) in self.branches.iter().enumerate() {
            if i as u16 >= inner.height {
                break;
            }

            let y = inner.y + i as u16;
            let is_selected = self.selected_branch == Some(*branch);
            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let prefix = if is_selected { "▶ " } else { "  " };
            let max_width = inner.width as usize;
            let display = format!(
                "{}{}",
                prefix,
                truncate_with_ellipsis(branch, max_width.saturating_sub(2))
            );

            buf.set_string(inner.x, y, &display, style);
        }
    }
}
