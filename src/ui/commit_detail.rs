//! コミット詳細Widget

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

use crate::app::App;
use crate::git::{CommitDiffInfo, FileChangeKind};

pub struct CommitDetailWidget<'a> {
    commit_lines: Vec<Line<'a>>,
    file_lines: Vec<Line<'a>>,
}

impl<'a> CommitDetailWidget<'a> {
    pub fn new(app: &App) -> Self {
        let mut commit_lines = Vec::new();

        if let Some(selected) = app.graph_list_state.selected() {
            if let Some(node) = app.graph_layout.nodes.get(selected) {
                // 接続行の場合はスキップ
                let Some(commit) = &node.commit else {
                    commit_lines.push(Line::from(Span::styled(
                        "(connector line)",
                        Style::default().fg(Color::DarkGray),
                    )));
                    return Self {
                        commit_lines,
                        file_lines: Vec::new(),
                    };
                };

                // コミットハッシュ
                commit_lines.push(Line::from(vec![
                    Span::styled("Commit: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(commit.oid.to_string(), Style::default().fg(Color::Yellow)),
                ]));

                // 著者
                commit_lines.push(Line::from(vec![
                    Span::styled("Author: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(
                        format!("{} <{}>", commit.author_name, commit.author_email),
                        Style::default().fg(Color::Blue),
                    ),
                ]));

                // 日時
                commit_lines.push(Line::from(vec![
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
                    commit_lines.push(Line::from(vec![
                        Span::styled("Parent: ", Style::default().add_modifier(Modifier::BOLD)),
                        Span::styled(parents.join(", "), Style::default().fg(Color::DarkGray)),
                    ]));
                }

                commit_lines.push(Line::from(""));

                // メッセージ
                for line in commit.full_message.lines() {
                    commit_lines.push(Line::from(Span::raw(line.to_string())));
                }
            }
        } else {
            commit_lines.push(Line::from(Span::styled(
                "Select a commit",
                Style::default().fg(Color::DarkGray),
            )));
        }

        // ファイル一覧を構築（キャッシュから）
        let file_lines = if app.is_diff_loading() {
            vec![Line::from(Span::styled(
                "Loading...",
                Style::default().fg(Color::DarkGray),
            ))]
        } else {
            Self::build_file_list_lines_from(app.cached_diff())
        };

        Self {
            commit_lines,
            file_lines,
        }
    }

    fn build_file_list_lines_from(diff: Option<&CommitDiffInfo>) -> Vec<Line<'a>> {
        let mut lines = Vec::new();

        let Some(diff) = diff else {
            return lines;
        };

        // ヘッダー行
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

        // ファイル一覧
        for file in &diff.files {
            let (indicator, color) = match file.kind {
                FileChangeKind::Added => ("A", Color::Green),
                FileChangeKind::Modified => ("M", Color::Yellow),
                FileChangeKind::Deleted => ("D", Color::Red),
                FileChangeKind::Renamed => ("R", Color::Cyan),
                FileChangeKind::Copied => ("C", Color::Cyan),
            };

            let path_str = file.path.to_string_lossy().to_string();

            lines.push(Line::from(vec![
                Span::styled(format!(" {} ", indicator), Style::default().fg(color)),
                Span::raw(path_str),
                Span::raw(" "),
                Span::styled(
                    format!("+{}", file.insertions),
                    Style::default().fg(Color::Green),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("-{}", file.deletions),
                    Style::default().fg(Color::Red),
                ),
            ]));
        }

        // 切り捨てメッセージ
        if diff.truncated {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("  ...and {} more files", diff.total_files - diff.files.len()),
                Style::default().fg(Color::DarkGray),
            )));
        }

        lines
    }
}

impl<'a> Widget for CommitDetailWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // 水平分割: 左50% (コミット情報) / 右50% (ファイル一覧)
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        // 左側: コミット情報
        let left_block = Block::default()
            .title(" Commit Detail ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let left_paragraph = Paragraph::new(self.commit_lines)
            .block(left_block)
            .wrap(Wrap { trim: false });

        Widget::render(left_paragraph, chunks[0], buf);

        // 右側: ファイル一覧
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
