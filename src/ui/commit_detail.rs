//! Commit detail widget

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

use crate::app::App;
use crate::git::{CommitDiffInfo, FileChangeKind};

use super::{render_placeholder_block, MIN_WIDGET_HEIGHT, MIN_WIDGET_WIDTH};

/// Width threshold for switching to vertical layout
/// When panel width would be <= 28 chars, use vertical layout
const VERTICAL_LAYOUT_THRESHOLD: u16 = 56;

pub struct CommitDetailWidget<'a> {
    commit_lines: Vec<Line<'a>>,
    file_lines: Vec<Line<'a>>,
}

impl<'a> CommitDetailWidget<'a> {
    pub fn new(app: &App) -> Self {
        let commit_lines = Self::build_commit_lines(app);
        let file_lines = Self::build_file_lines(app);
        Self { commit_lines, file_lines }
    }

    fn build_file_lines(app: &App) -> Vec<Line<'a>> {
        if app.is_diff_loading() {
            return vec![Line::from(Span::styled(
                "Loading...",
                Style::default().fg(Color::DarkGray),
            ))];
        }
        Self::build_file_list_lines_from(app.cached_diff())
    }

    fn build_commit_lines(app: &App) -> Vec<Line<'a>> {
        let Some(selected) = app.graph_list_state.selected() else {
            return vec![Line::from(Span::styled(
                "Select a commit",
                Style::default().fg(Color::DarkGray),
            ))];
        };

        let Some(node) = app.graph_layout.nodes.get(selected) else {
            return Vec::new();
        };

        // Handle uncommitted changes node
        if node.is_uncommitted {
            return vec![
                Line::from(Span::styled(
                    "Uncommitted Changes",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    format!("{} files with changes", node.uncommitted_count),
                    Style::default().fg(Color::DarkGray),
                )),
            ];
        }

        // Handle connector rows (no commit)
        let Some(commit) = &node.commit else {
            return vec![Line::from(Span::styled(
                "(connector line)",
                Style::default().fg(Color::DarkGray),
            ))];
        };

        // Build commit detail lines
        let mut lines = vec![
            // Commit hash
            Line::from(vec![
                Span::styled("Commit: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(commit.oid.to_string(), Style::default().fg(Color::Yellow)),
            ]),
            // Author
            Line::from(vec![
                Span::styled("Author: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!("{} <{}>", commit.author_name, commit.author_email),
                    Style::default().fg(Color::Blue),
                ),
            ]),
            // Date
            Line::from(vec![
                Span::styled("Date:   ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(
                    commit.timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
        ];

        // Parent commits
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

        // Message
        for line in commit.full_message.lines() {
            lines.push(Line::from(Span::raw(line.to_string())));
        }

        lines
    }

    fn build_file_list_lines_from(diff: Option<&CommitDiffInfo>) -> Vec<Line<'a>> {
        let mut lines = Vec::new();

        let Some(diff) = diff else {
            return lines;
        };

        // Header row
        lines.push(Line::from(vec![
            Span::styled(
                format!("{} files changed", diff.total_files),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                format!("+{}", diff.total_insertions),
                Style::default().fg(Color::Green),
            ),
            Span::raw(" "),
            Span::styled(
                format!("-{}", diff.total_deletions),
                Style::default().fg(Color::Red),
            ),
        ]));
        lines.push(Line::from(""));

        // File list
        for file in &diff.files {
            let (indicator, color) = match file.kind {
                FileChangeKind::Added => ("A", Color::Green),
                FileChangeKind::Modified => ("M", Color::Yellow),
                FileChangeKind::Deleted => ("D", Color::Red),
                FileChangeKind::Renamed => ("R", Color::Cyan),
                FileChangeKind::Copied => ("C", Color::Cyan),
            };

            let path_str = file.path.to_string_lossy().to_string();

            let mut spans = vec![
                Span::styled(format!(" {} ", indicator), Style::default().fg(color)),
                Span::raw(path_str),
            ];

            // Only show diff stats if there are actual changes (skip for binary files)
            if file.insertions > 0 || file.deletions > 0 {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    format!("+{}", file.insertions),
                    Style::default().fg(Color::Green),
                ));
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    format!("-{}", file.deletions),
                    Style::default().fg(Color::Red),
                ));
            }

            lines.push(Line::from(spans));
        }

        // Truncation message
        if diff.truncated {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!(
                    "  ...and {} more files",
                    diff.total_files - diff.files.len()
                ),
                Style::default().fg(Color::DarkGray),
            )));
        }

        lines
    }
}

impl<'a> Widget for CommitDetailWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < MIN_WIDGET_WIDTH || area.height < MIN_WIDGET_HEIGHT {
            render_placeholder_block(area, buf);
            return;
        }

        // Use vertical layout when each panel would be <= 28 chars wide
        let direction = if area.width <= VERTICAL_LAYOUT_THRESHOLD {
            Direction::Vertical
        } else {
            Direction::Horizontal
        };

        let chunks = Layout::default()
            .direction(direction)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        // Left: commit info
        let left_block = Block::default()
            .title(" Commit Detail ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let left_paragraph = Paragraph::new(self.commit_lines)
            .block(left_block)
            .wrap(Wrap { trim: false });

        Widget::render(left_paragraph, chunks[0], buf);

        // Right: file list
        let right_block = Block::default()
            .title(" Changed Files ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let right_paragraph = Paragraph::new(self.file_lines)
            .block(right_block)
            .wrap(Wrap { trim: false });

        Widget::render(right_paragraph, chunks[1], buf);
    }
}
