//! Commit detail widget

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, AppMode, DiffSource, FocusedPane};
use crate::git::{FileChangeKind, StageState};

use super::{render_placeholder_block, MIN_WIDGET_HEIGHT, MIN_WIDGET_WIDTH};

/// Human-friendly relative time like "3 days ago"
pub(crate) fn relative_time(ts: chrono::DateTime<chrono::Local>) -> String {
    let secs = chrono::Local::now().signed_duration_since(ts).num_seconds();
    if secs < 0 {
        return "in the future".to_string();
    }
    let (value, unit) = match secs {
        0..=59 => return "just now".to_string(),
        60..=3599 => (secs / 60, "minute"),
        3600..=86_399 => (secs / 3600, "hour"),
        86_400..=2_591_999 => (secs / 86_400, "day"),
        2_592_000..=31_535_999 => (secs / 2_592_000, "month"),
        _ => (secs / 31_536_000, "year"),
    };
    let plural = if value == 1 { "" } else { "s" };
    format!("{} {}{} ago", value, unit, plural)
}

/// Commit info pane (left/top half of the detail area)
pub struct CommitDetailWidget {
    commit_lines: Vec<Line<'static>>,
    scroll: u16,
    focused: bool,
}

impl CommitDetailWidget {
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

    fn metadata_label(text: &str) -> Span<'static> {
        Span::styled(
            format!(" {:<8}", text),
            Style::default().fg(Color::DarkGray),
        )
    }

    fn build_commit_lines(app: &App) -> Vec<Line<'static>> {
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
                    " Uncommitted Changes",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    match node.uncommitted_count {
                        Some(count) => format!(" {} files with changes", count),
                        None => " files with changes".to_string(),
                    },
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    " Space: open files / stage / commit",
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

        // Hash line: short hash emphasized, remainder dimmed
        let full_hash = commit.oid.to_string();
        let (short, rest) = full_hash.split_at(7.min(full_hash.len()));
        let mut lines = vec![
            Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    short.to_string(),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(rest.to_string(), Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Self::metadata_label("Author"),
                Span::styled(
                    format!("{} <{}>", commit.author_name, commit.author_email),
                    Style::default().fg(Color::Blue),
                ),
            ]),
            Line::from(vec![
                Self::metadata_label("Date"),
                Span::raw(commit.timestamp.format("%Y-%m-%d %H:%M:%S").to_string()),
                Span::styled(
                    format!(" ({})", relative_time(commit.timestamp)),
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
                Self::metadata_label("Parent"),
                Span::styled(parents.join(", "), Style::default().fg(Color::DarkGray)),
            ]));
        }

        // Branches pointing at this commit
        if !node.branch_names.is_empty() {
            let mut spans = vec![Self::metadata_label("Branch")];
            for (i, name) in node.branch_names.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::raw(" "));
                }
                spans.push(Span::styled(
                    format!("[{}]", name),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            lines.push(Line::from(spans));
        }

        // Worktrees checked out from branches on this commit
        for name in &node.branch_names {
            if let Some(worktree) = app.worktree_for_branch(name) {
                lines.push(Line::from(vec![
                    // "Worktree" fills the 8-char label column; keep one space
                    Self::metadata_label("Worktree"),
                    Span::styled(
                        format!(" ⌂ {}", worktree.path.display()),
                        Style::default().fg(Color::Magenta),
                    ),
                ]));
            }
        }

        lines.push(Line::from(Span::styled(
            " ".to_string() + &"─".repeat(28),
            Style::default().fg(Color::DarkGray),
        )));

        // Message
        for line in commit.full_message.lines() {
            lines.push(Line::from(Span::raw(format!(" {}", line))));
        }

        lines
    }
}

impl Widget for CommitDetailWidget {
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

/// One file entry prepared for width-aware rendering
struct FileRow {
    path: String,
    kind_mark: &'static str,
    kind_color: Color,
    stage_mark: Option<(&'static str, Color)>,
    is_binary: bool,
    insertions: usize,
    deletions: usize,
    selected: bool,
}

struct FileListHeader {
    total_files: usize,
    insertions: usize,
    deletions: usize,
    staged: Option<usize>,
}

enum FileListContent {
    Empty,
    Loading,
    Diff {
        header: FileListHeader,
        rows: Vec<FileRow>,
        hidden_files: usize,
    },
}

/// Changed files pane (right/bottom half of the detail area)
pub struct FileListWidget {
    content: FileListContent,
    file_scroll: u16,
    focused: bool,
}

impl FileListWidget {
    pub fn new(app: &App) -> Self {
        let file_scroll = match &app.mode {
            AppMode::FileSelect { selected_index, .. } => *selected_index as u16,
            _ => 0,
        };
        Self {
            content: Self::build_content(app),
            file_scroll,
            focused: matches!(app.mode, AppMode::FileSelect { .. }),
        }
    }

    /// Number of files shown (for the pane title)
    pub fn file_count(&self) -> Option<usize> {
        match &self.content {
            FileListContent::Diff { header, .. } => Some(header.total_files),
            _ => None,
        }
    }

    fn build_content(app: &App) -> FileListContent {
        let selected_file_index = match &app.mode {
            AppMode::FileSelect { selected_index, .. } => Some(*selected_index),
            _ => None,
        };

        // Branch review shows the precomputed range diff instead of the
        // selection's diff
        let review_diff = match &app.mode {
            AppMode::FileSelect {
                source: DiffSource::Range { .. },
                ..
            } => app.review_diff.as_ref(),
            _ => None,
        };

        let stage_states =
            (review_diff.is_none() && app.is_uncommitted_selected()).then_some(&app.stage_states);

        // Prefer cached data (even if stale) over a loading indicator so that
        // auto-refresh doesn't cause the file list to flicker.
        let diff = if let Some(diff) = review_diff {
            diff
        } else if let Some(diff) = app.cached_diff() {
            diff
        } else {
            if app.is_diff_loading() {
                return FileListContent::Loading;
            }
            return FileListContent::Empty;
        };

        let staged = stage_states.map(|states| {
            states
                .values()
                .filter(|s| matches!(s, StageState::Staged | StageState::Partial))
                .count()
        });

        let rows = diff
            .files
            .iter()
            .enumerate()
            .map(|(idx, file)| {
                let (kind_mark, kind_color) = match file.kind {
                    FileChangeKind::Added => ("A", Color::Green),
                    FileChangeKind::Modified => ("M", Color::Yellow),
                    FileChangeKind::Deleted => ("D", Color::Red),
                    FileChangeKind::Renamed => ("R", Color::Cyan),
                    FileChangeKind::Copied => ("C", Color::Cyan),
                };
                let stage_mark = stage_states.map(|states| match states.get(&file.path) {
                    Some(StageState::Staged) => ("●", Color::Green),
                    Some(StageState::Partial) => ("◐", Color::Yellow),
                    _ => ("○", Color::DarkGray),
                });
                FileRow {
                    path: file.path.to_string_lossy().to_string(),
                    kind_mark,
                    kind_color,
                    stage_mark,
                    is_binary: file.is_binary,
                    insertions: file.insertions,
                    deletions: file.deletions,
                    selected: selected_file_index == Some(idx),
                }
            })
            .collect();

        FileListContent::Diff {
            header: FileListHeader {
                total_files: diff.total_files,
                insertions: diff.total_insertions,
                deletions: diff.total_deletions,
                staged,
            },
            rows,
            hidden_files: if diff.truncated {
                diff.total_files - diff.files.len()
            } else {
                0
            },
        }
    }

    /// Truncate a path to the given display width, keeping the tail
    fn truncate_path(path: &str, max_width: usize) -> String {
        if path.width() <= max_width {
            return path.to_string();
        }
        let mut result: String = String::new();
        let mut width = 1; // for the leading ellipsis
        for c in path.chars().rev() {
            let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            if width + w > max_width {
                break;
            }
            result.insert(0, c);
            width += w;
        }
        format!("…{}", result)
    }

    fn build_lines(&self, inner_width: usize) -> Vec<Line<'static>> {
        match &self.content {
            FileListContent::Loading => vec![Line::from(Span::styled(
                " Loading...",
                Style::default().fg(Color::DarkGray),
            ))],
            FileListContent::Empty => Vec::new(),
            FileListContent::Diff {
                header,
                rows,
                hidden_files,
            } => {
                let mut lines = Vec::with_capacity(rows.len() + 3);

                // Header row
                let mut spans = vec![
                    Span::styled(
                        format!(" {} files changed", header.total_files),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("  "),
                    Span::styled(
                        format!("+{}", header.insertions),
                        Style::default().fg(Color::Green),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        format!("-{}", header.deletions),
                        Style::default().fg(Color::Red),
                    ),
                ];
                if let Some(staged) = header.staged {
                    if staged > 0 {
                        spans.push(Span::raw("  "));
                        spans.push(Span::styled(
                            format!("● {} staged", staged),
                            Style::default().fg(Color::Cyan),
                        ));
                    }
                }
                lines.push(Line::from(spans));
                lines.push(Line::from(""));

                let show_stats = inner_width >= 28;

                for row in rows {
                    lines.push(self.build_file_line(row, inner_width, show_stats));
                }

                if *hidden_files > 0 {
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        format!("  ...and {} more files", hidden_files),
                        Style::default().fg(Color::DarkGray),
                    )));
                }

                lines
            }
        }
    }

    fn build_file_line(
        &self,
        row: &FileRow,
        inner_width: usize,
        show_stats: bool,
    ) -> Line<'static> {
        // Right-aligned block: "+NNNN -NNNN"
        let stats_text = if !show_stats {
            String::new()
        } else if row.is_binary {
            "(binary)".to_string()
        } else {
            format!("+{:<4} -{:<4}", row.insertions, row.deletions)
        };
        let right_width = stats_text.width();

        // Left part: marker + stage mark + kind + path
        let marker_width = 1 + row.stage_mark.map_or(0, |_| 2) + 3;
        let path_budget = inner_width
            .saturating_sub(marker_width)
            .saturating_sub(right_width + 1);
        let path = Self::truncate_path(&row.path, path_budget);

        let mut spans = Vec::new();
        if row.selected {
            spans.push(Span::styled("▌", Style::default().fg(Color::Cyan)));
        } else {
            spans.push(Span::raw(" "));
        }
        if let Some((mark, color)) = row.stage_mark {
            spans.push(Span::styled(
                format!("{} ", mark),
                Style::default().fg(color),
            ));
        }
        spans.push(Span::styled(
            format!("{}  ", row.kind_mark),
            Style::default().fg(row.kind_color),
        ));
        spans.push(Span::raw(path.clone()));

        // Padding so the stats block is right-aligned
        let used = marker_width - 1 + path.width() + 1;
        if right_width > 0 {
            let padding = inner_width.saturating_sub(used + right_width);
            spans.push(Span::raw(" ".repeat(padding.max(1))));
            if row.is_binary {
                spans.push(Span::styled(
                    stats_text,
                    Style::default().fg(Color::DarkGray),
                ));
            } else {
                let (ins, del) = stats_text.split_at(stats_text.find(" -").unwrap_or(0));
                spans.push(Span::styled(
                    ins.to_string(),
                    Style::default().fg(Color::Green),
                ));
                spans.push(Span::styled(
                    del.to_string(),
                    Style::default().fg(Color::Red),
                ));
            }
        }

        let mut line = Line::from(spans);
        if row.selected {
            line = line.style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );
        }
        line
    }

    /// Scroll offset of the file list so the selected file stays visible.
    /// File lines: 2 header lines (summary + blank) + file entries.
    pub fn scroll_offset(&self, area: Rect) -> u16 {
        if !self.focused {
            return 0;
        }
        let total_lines = match &self.content {
            FileListContent::Diff {
                rows, hidden_files, ..
            } => rows.len() + 2 + if *hidden_files > 0 { 2 } else { 0 },
            _ => 1,
        } as u16;
        let visible_height = area.height.saturating_sub(2); // minus block borders
        let selected_line = self.file_scroll + 2; // offset for header lines
        let max_scroll = total_lines.saturating_sub(visible_height);
        if visible_height > 0 && selected_line >= visible_height {
            (selected_line - visible_height / 2).min(max_scroll)
        } else {
            0
        }
    }
}

impl Widget for FileListWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < MIN_WIDGET_WIDTH || area.height < MIN_WIDGET_HEIGHT {
            render_placeholder_block(area, buf);
            return;
        }

        let title = match self.file_count() {
            Some(count) => format!("Changed Files ({})", count),
            None => "Changed Files".to_string(),
        };
        let block = super::pane_block(&title, self.focused);

        let scroll_y = self.scroll_offset(area);
        let lines = self.build_lines(area.width.saturating_sub(2) as usize);

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((scroll_y, 0));

        Widget::render(paragraph, area, buf);
    }
}
