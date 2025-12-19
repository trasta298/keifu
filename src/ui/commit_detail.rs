//! コミット詳細Widget

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

use crate::app::App;

pub struct CommitDetailWidget<'a> {
    lines: Vec<Line<'a>>,
}

impl<'a> CommitDetailWidget<'a> {
    pub fn new(app: &App) -> Self {
        let mut lines = Vec::new();

        if let Some(selected) = app.graph_list_state.selected() {
            if let Some(node) = app.graph_layout.nodes.get(selected) {
                let commit = &node.commit;

                // コミットハッシュ
                lines.push(Line::from(vec![
                    Span::styled("Commit: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(commit.oid.to_string(), Style::default().fg(Color::Yellow)),
                ]));

                // 著者
                lines.push(Line::from(vec![
                    Span::styled("Author: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(
                        format!("{} <{}>", commit.author_name, commit.author_email),
                        Style::default().fg(Color::Blue),
                    ),
                ]));

                // 日時
                lines.push(Line::from(vec![
                    Span::styled("Date:   ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(
                        commit.timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));

                // 親コミット
                if !commit.parent_oids.is_empty() {
                    let parents: Vec<String> = commit
                        .parent_oids
                        .iter()
                        .map(|oid| oid.to_string()[..7].to_string())
                        .collect();
                    lines.push(Line::from(vec![
                        Span::styled("Parent: ", Style::default().add_modifier(Modifier::BOLD)),
                        Span::styled(parents.join(", "), Style::default().fg(Color::DarkGray)),
                    ]));
                }

                lines.push(Line::from(""));

                // メッセージ
                for line in commit.full_message.lines() {
                    lines.push(Line::from(Span::raw(line.to_string())));
                }
            }
        } else {
            lines.push(Line::from(Span::styled(
                "コミットを選択してください",
                Style::default().fg(Color::DarkGray),
            )));
        }

        Self { lines }
    }
}

impl<'a> Widget for CommitDetailWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Commit Detail ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let paragraph = Paragraph::new(self.lines)
            .block(block)
            .wrap(Wrap { trim: false });

        Widget::render(paragraph, area, buf);
    }
}
