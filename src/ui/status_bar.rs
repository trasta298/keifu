//! Status bar widget
//!
//! Key hints are modeled as segments with an optional `Action`, so the same
//! data drives both rendering and mouse hit regions (`hint_regions`).

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};
use unicode_width::UnicodeWidthStr;

use crate::action::Action;
use crate::app::{App, AppMode, FocusedPane, InputAction};

struct Hint {
    key: &'static str,
    desc: &'static str,
    action: Option<Action>,
}

impl Hint {
    fn new(key: &'static str, desc: &'static str, action: Option<Action>) -> Self {
        Self { key, desc, action }
    }

    fn key_text(&self) -> String {
        format!(" {} ", self.key)
    }

    fn desc_text(&self) -> String {
        format!(" {} ", self.desc)
    }

    fn width(&self) -> u16 {
        (self.key_text().width() + self.desc_text().width()) as u16
    }
}

pub struct StatusBar {
    prefix: Vec<Span<'static>>,
    hints: Vec<Hint>,
    mode_label: Option<&'static str>,
}

impl StatusBar {
    pub fn new(app: &App) -> Self {
        let repo_style = Style::default()
            .fg(Color::Black)
            .bg(Color::Magenta)
            .add_modifier(Modifier::BOLD);

        let mut prefix: Vec<Span> = Vec::new();

        // Repository name (folder name) on the left
        let repo_name = std::path::Path::new(&app.repo_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&app.repo_path);
        prefix.push(Span::styled(format!(" {} ", repo_name), repo_style));
        prefix.push(Span::raw(" "));

        // HEAD branch
        if let Some(head) = app.head_name.as_deref() {
            prefix.push(Span::styled(
                format!(" {} ", head),
                Style::default().fg(Color::Black).bg(Color::Green),
            ));
            prefix.push(Span::raw(" "));
        }
        if !app.show_remote_branches() {
            prefix.push(Span::styled(
                " remotes hidden ",
                Style::default()
                    .fg(Color::White)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ));
            prefix.push(Span::raw(" "));
        }

        let mut hints: Vec<Hint> = Vec::new();
        let mut mode_label = None;

        match &app.mode {
            AppMode::Normal => {
                if let Some(msg) = app.get_message() {
                    // Yellow for in-progress, Cyan for success
                    let bg = if app.is_fetching() || app.is_pushing() {
                        Color::Yellow
                    } else {
                        Color::Cyan
                    };
                    prefix.push(Span::styled(
                        format!(" {} ", msg),
                        Style::default()
                            .fg(Color::Black)
                            .bg(bg)
                            .add_modifier(Modifier::BOLD),
                    ));
                    prefix.push(Span::raw("  "));
                } else if app.focused_pane == FocusedPane::Detail {
                    hints.push(Hint::new("j/k", "scroll", None));
                    hints.push(Hint::new("Tab", "graph", Some(Action::FocusNext)));
                    hints.push(Hint::new("Esc", "back", Some(Action::Quit)));
                    hints.push(Hint::new("?", "help", Some(Action::ToggleHelp)));
                } else {
                    hints.push(Hint::new("j/k", "move", None));
                    hints.push(Hint::new("Enter", "checkout", Some(Action::Checkout)));
                    hints.push(Hint::new("Space", "files", Some(Action::EnterFileSelect)));
                    hints.push(Hint::new("c", "commit", Some(Action::CommitDialog)));
                    hints.push(Hint::new("p", "push", Some(Action::Push)));
                    hints.push(Hint::new("?", "help", Some(Action::ToggleHelp)));
                    hints.push(Hint::new("q", "quit", Some(Action::Quit)));
                }
            }
            AppMode::Help => {
                mode_label = Some(" HELP ");
                hints.push(Hint::new("j/k", "scroll", None));
                hints.push(Hint::new("Esc/q", "close help", Some(Action::ToggleHelp)));
            }
            AppMode::Input { action, .. } => {
                mode_label = Some(" INPUT ");
                if *action == InputAction::Search {
                    let count = app.search_match_count();
                    let info = if count > 0 {
                        format!(" {} matches ", count)
                    } else {
                        " No matches ".to_string()
                    };
                    prefix.push(Span::styled(
                        info,
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ));
                    prefix.push(Span::raw("  "));
                }
                hints.push(Hint::new("Enter", "confirm", Some(Action::Confirm)));
                hints.push(Hint::new("Esc", "cancel", Some(Action::Cancel)));
            }
            AppMode::Confirm { .. } => {
                mode_label = Some(" CONFIRM ");
                hints.push(Hint::new("y", "yes", Some(Action::Confirm)));
                hints.push(Hint::new("n", "no", Some(Action::Cancel)));
            }
            AppMode::Error { message } => {
                mode_label = Some(" ERROR ");
                prefix.push(Span::styled(
                    format!(" {} ", message),
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                ));
                prefix.push(Span::raw("  "));
                hints.push(Hint::new("Esc/Enter", "close", Some(Action::Cancel)));
            }
            AppMode::FileSelect { .. } => {
                mode_label = Some(" FILES ");
                hints.push(Hint::new("j/k", "select", None));
                hints.push(Hint::new("Enter", "diff", Some(Action::OpenFileDiff)));
                if app.is_uncommitted_selected() {
                    hints.push(Hint::new("s", "stage", Some(Action::StageToggle)));
                    hints.push(Hint::new("a", "all", Some(Action::StageAll)));
                    hints.push(Hint::new("u", "none", Some(Action::UnstageAll)));
                    hints.push(Hint::new("c", "commit", Some(Action::CommitDialog)));
                }
                hints.push(Hint::new("Esc", "back", Some(Action::Cancel)));
            }
            AppMode::FileDiff { .. } => {
                mode_label = Some(" DIFF ");
                hints.push(Hint::new("n/N", "file", Some(Action::NextFile)));
                hints.push(Hint::new("]/[", "hunk", Some(Action::NextHunk)));
                hints.push(Hint::new("j/k", "scroll", None));
                hints.push(Hint::new("h/l", "pan", None));
                hints.push(Hint::new("Esc", "back", Some(Action::Cancel)));
            }
        }

        Self {
            prefix,
            hints,
            mode_label,
        }
    }

    fn prefix_width(&self) -> u16 {
        self.prefix
            .iter()
            .map(|span| span.content.width() as u16)
            .sum()
    }

    /// Clickable regions for the hints, mirroring the render layout
    pub fn hint_regions(&self, area: Rect) -> Vec<(Rect, Action)> {
        let mut regions = Vec::new();
        let mut x = area.x + self.prefix_width();
        for hint in &self.hints {
            let width = hint.width();
            if x + width > area.x + area.width {
                break;
            }
            if let Some(action) = &hint.action {
                regions.push((Rect::new(x, area.y, width, 1), action.clone()));
            }
            x += width;
        }
        regions
    }
}

impl Widget for StatusBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let key_style = Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD);
        let desc_style = Style::default().fg(Color::White);
        let mode_style = Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD);

        let mut spans = self.prefix.clone();
        for hint in &self.hints {
            spans.push(Span::styled(hint.key_text(), key_style));
            spans.push(Span::styled(hint.desc_text(), desc_style));
        }

        let line = Line::from(spans);
        buf.set_line(area.x, area.y, &line, area.width);

        // Show the mode on the right (only for non-Normal modes)
        if let Some(text) = self.mode_label {
            let mode_len = text.len() as u16;
            if area.width > mode_len {
                let x = area.x + area.width - mode_len;
                buf.set_string(x, area.y, text, mode_style);
            }
        }
    }
}
