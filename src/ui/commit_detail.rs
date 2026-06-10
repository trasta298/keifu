//! Commit detail widget

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};

use std::collections::HashMap;
use std::path::PathBuf;

use crate::app::{App, AppMode, FocusedPane};
use crate::git::{CommitDiffInfo, FileChangeKind, StageState};

use super::{render_placeholder_block, MIN_WIDGET_HEIGHT, MIN_WIDGET_WIDTH};

/// Commit info pane (left/top half of the detail area)
pub struct CommitDetailWidget<'a> {
    commit_lines: Vec<Line<'a>>,
    scroll: u16,
    focused: bool,
}

impl<'a> CommitDetailWidget<'a> {
    pub fn new(app: &App) -> Self {
        Self {
            commit_lines: Self::build_commit_lines(app),
            scroll: app.detail_scroll,
            focused: matches!(app.mode, AppMode::Normal) && app.focused_pane == FocusedPane::Detail,
        }
    }

    pub fn with_scroll(mut self, scroll: u16) -> Self {
        self.scroll = scroll;
        self
    }

    /// Estimate the rendered height (in rows) for the given inner width,
    /// accounting for word wrap. Used to clamp the scroll offset.
    pub fn estimated_height(&self, inner_width: u16) -> u16 {
        let width = inner_width.max(1) as usize;
        self.commit_lines
            .iter()
            .map(|line| line.width().max(1).div_ceil(width))
            .sum::<usize>()
            .min(u16::MAX as usize) as u16
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
                    match node.uncommitted_count {
                        Some(count) => format!("{} files with changes", count),
                        None => "files with changes".to_string(),
                    },
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
}

impl<'a> Widget for CommitDetailWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < MIN_WIDGET_WIDTH || area.height < MIN_WIDGET_HEIGHT {
            render_placeholder_block(area, buf);
            return;
        }

        let block = super::pane_block("Commit Detail", self.focused);

        let max_scroll = self
            .estimated_height(area.width.saturating_sub(2))
            .saturating_sub(area.height.saturating_sub(2));

        let paragraph = Paragraph::new(self.commit_lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll.min(max_scroll), 0));

        Widget::render(paragraph, area, buf);
    }
}

/// Changed files pane (right/bottom half of the detail area)
pub struct FileListWidget<'a> {
    file_lines: Vec<Line<'a>>,
    file_scroll: u16,
    focused: bool,
}

impl<'a> FileListWidget<'a> {
    pub fn new(app: &App) -> Self {
        let file_scroll = match &app.mode {
            AppMode::FileSelect { selected_index, .. } => *selected_index as u16,
            _ => 0,
        };
        Self {
            file_lines: Self::build_file_lines(app),
            file_scroll,
            focused: matches!(app.mode, AppMode::FileSelect { .. }),
        }
    }

    fn build_file_lines(app: &App) -> Vec<Line<'a>> {
        let selected_file_index = match &app.mode {
            AppMode::FileSelect { selected_index, .. } => Some(*selected_index),
            _ => None,
        };

        let stage_states = app.is_uncommitted_selected().then_some(&app.stage_states);

        // Prefer cached data (even if stale) over a loading indicator so that
        // auto-refresh doesn't cause the file list to flicker.
        if let Some(diff) = app.cached_diff() {
            return Self::build_file_list_lines_from(Some(diff), selected_file_index, stage_states);
        }
        if app.is_diff_loading() {
            return vec![Line::from(Span::styled(
                "Loading...",
                Style::default().fg(Color::DarkGray),
            ))];
        }
        Self::build_file_list_lines_from(None, None, None)
    }

    fn build_file_list_lines_from(
        diff: Option<&CommitDiffInfo>,
        selected_file_index: Option<usize>,
        stage_states: Option<&HashMap<PathBuf, StageState>>,
    ) -> Vec<Line<'a>> {
        let mut lines = Vec::new();

        let Some(diff) = diff else {
            return lines;
        };

        // Header row
        let mut header = vec![
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
        ];
        if let Some(states) = stage_states {
            let staged = states
                .values()
                .filter(|s| matches!(s, StageState::Staged | StageState::Partial))
                .count();
            if staged > 0 {
                header.push(Span::raw("  "));
                header.push(Span::styled(
                    format!("● {} staged", staged),
                    Style::default().fg(Color::Cyan),
                ));
            }
        }
        lines.push(Line::from(header));
        lines.push(Line::from(""));

        // File list
        for (idx, file) in diff.files.iter().enumerate() {
            let is_selected = selected_file_index == Some(idx);

            let (indicator, color) = match file.kind {
                FileChangeKind::Added => ("A", Color::Green),
                FileChangeKind::Modified => ("M", Color::Yellow),
                FileChangeKind::Deleted => ("D", Color::Red),
                FileChangeKind::Renamed => ("R", Color::Cyan),
                FileChangeKind::Copied => ("C", Color::Cyan),
            };

            let path_str = file.path.to_string_lossy().to_string();

            let mut spans = Vec::new();
            if let Some(states) = stage_states {
                let (mark, mark_color) = match states.get(&file.path) {
                    Some(StageState::Staged) => ("●", Color::Green),
                    Some(StageState::Partial) => ("◐", Color::Yellow),
                    _ => ("○", Color::DarkGray),
                };
                spans.push(Span::styled(
                    format!(" {}", mark),
                    Style::default().fg(mark_color),
                ));
            }
            spans.push(Span::styled(
                format!(" {} ", indicator),
                Style::default().fg(color),
            ));
            spans.push(Span::raw(path_str));

            if file.is_binary {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    "(binary)",
                    Style::default().fg(Color::DarkGray),
                ));
            } else if file.insertions > 0 || file.deletions > 0 {
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

            let mut line = Line::from(spans);
            if is_selected {
                line = line.style(
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                );
            }
            lines.push(line);
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

    /// Scroll offset of the file list so the selected file stays visible.
    /// File lines: 2 header lines (summary + blank) + file entries.
    pub fn scroll_offset(&self, area: Rect) -> u16 {
        if !self.focused {
            return 0;
        }
        let visible_height = area.height.saturating_sub(2); // minus block borders
        let total_lines = self.file_lines.len() as u16;
        let selected_line = self.file_scroll + 2; // offset for header lines
        let max_scroll = total_lines.saturating_sub(visible_height);
        if visible_height > 0 && selected_line >= visible_height {
            (selected_line - visible_height / 2).min(max_scroll)
        } else {
            0
        }
    }
}

impl<'a> Widget for FileListWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < MIN_WIDGET_WIDTH || area.height < MIN_WIDGET_HEIGHT {
            render_placeholder_block(area, buf);
            return;
        }

        let block = super::pane_block("Changed Files", self.focused);

        let scroll_y = self.scroll_offset(area);

        let paragraph = Paragraph::new(self.file_lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((scroll_y, 0));

        Widget::render(paragraph, area, buf);
    }
}
