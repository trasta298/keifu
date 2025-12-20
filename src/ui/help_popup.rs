//! Help popup widget

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

pub struct HelpPopup;

impl Widget for HelpPopup {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Clear the background
        Clear.render(area, buf);

        let key_style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);
        let desc_style = Style::default().fg(Color::White);
        let header_style = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD);

        let lines = vec![
            Line::from(Span::styled("Navigation", header_style)),
            Line::from(vec![
                Span::styled("  j / ↓      ", key_style),
                Span::styled("Move down", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  k / ↑      ", key_style),
                Span::styled("Move up", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  ] / Tab    ", key_style),
                Span::styled("Select next branch", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  [ / S-Tab  ", key_style),
                Span::styled("Select previous branch", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  h / ←      ", key_style),
                Span::styled("Select left branch (same commit)", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  l / →      ", key_style),
                Span::styled("Select right branch (same commit)", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+d     ", key_style),
                Span::styled("Page down", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+u     ", key_style),
                Span::styled("Page up", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  g / Home   ", key_style),
                Span::styled("Go to top", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  G / End    ", key_style),
                Span::styled("Go to bottom", desc_style),
            ]),
            Line::from(""),
            Line::from(Span::styled("Git Operations", header_style)),
            Line::from(vec![
                Span::styled("  Enter      ", key_style),
                Span::styled("Checkout selected branch/commit", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  b          ", key_style),
                Span::styled("Create new branch", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  d          ", key_style),
                Span::styled("Delete branch", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  m          ", key_style),
                Span::styled("Merge branch", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  r          ", key_style),
                Span::styled("Rebase onto branch", desc_style),
            ]),
            Line::from(""),
            Line::from(Span::styled("Other", header_style)),
            Line::from(vec![
                Span::styled("  R          ", key_style),
                Span::styled("Refresh", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  ?          ", key_style),
                Span::styled("Toggle this help", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  q / Esc    ", key_style),
                Span::styled("Quit", desc_style),
            ]),
        ];

        let block = Block::default()
            .title(" Help ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .style(Style::default().bg(Color::Black));

        let paragraph = Paragraph::new(lines).block(block);

        Widget::render(paragraph, area, buf);
    }
}
