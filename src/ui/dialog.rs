//! Input and confirmation dialog widgets

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Widget},
};

use super::graph_view::{display_width, truncate_with_ellipsis};
use crate::git::branch::BranchLabel;

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
            .border_type(BorderType::Rounded)
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
            .border_type(BorderType::Rounded)
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
    branches: &'a [BranchLabel],
    selected_branch: Option<&'a str>,
}

impl<'a> BranchInfoPopup<'a> {
    pub fn new(branches: &'a [BranchLabel], selected_branch: Option<&'a str>) -> Self {
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
            .border_type(BorderType::Rounded)
            .style(Style::default().bg(Color::Black));

        let inner = block.inner(area);
        block.render(area, buf);

        // Render branch list
        for (i, branch) in self.branches.iter().enumerate() {
            if i as u16 >= inner.height {
                break;
            }

            let y = inner.y + i as u16;
            let is_selected = self.selected_branch == Some(branch.name.as_str());
            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let prefix = if is_selected { "▶ " } else { "  " };
            let max_width = (inner.width as usize).saturating_sub(2);
            let suffix = truncate_with_ellipsis(
                &branch.remote_suffix,
                max_width.saturating_sub(display_width(&branch.name).min(4)),
            );
            let display = format!(
                "{}{}{}",
                prefix,
                truncate_with_ellipsis(
                    &branch.name,
                    max_width.saturating_sub(display_width(&suffix))
                ),
                suffix
            );

            buf.set_string(inner.x, y, &display, style);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn many_remotes_keep_branch_name_and_popup_border_visible() {
        for suffix in [
            " ↔ origin, origin_2, origin_3, origin_4, origin_5",
            " ↔ origin, リモートの長い名前, 別のリモートの長い名前",
        ] {
            let branches = [BranchLabel {
                name: "main".to_string(),
                remote_suffix: suffix.to_string(),
            }];
            for width in [20, 32, 50] {
                let area = Rect::new(0, 0, width, 3);
                let mut buffer = Buffer::empty(area);
                BranchInfoPopup::new(&branches, Some("main")).render(area, &mut buffer);
                let row: String = (0..width).map(|x| buffer[(x, 1)].symbol()).collect();
                assert!(row.starts_with("│▶ main ↔ "), "{row}");
                assert!(row.contains("..."), "{row}");
                assert_eq!(buffer[(width - 1, 1)].symbol(), "│", "{row}");
            }
        }
    }
}
