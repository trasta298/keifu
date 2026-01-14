//! Status bar widget

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

use crate::app::{App, AppMode, InputAction};

pub struct StatusBar<'a> {
    mode: &'a AppMode,
    repo_path: &'a str,
    head_name: Option<&'a str>,
    error_message: Option<&'a str>,
    message: Option<&'a str>,
    is_fetching: bool,
    search_info: Option<String>,
}

impl<'a> StatusBar<'a> {
    pub fn new(app: &'a App) -> Self {
        let error_message = match &app.mode {
            AppMode::Error { message } => Some(message.as_str()),
            _ => None,
        };

        // Generate search status message
        let search_info = match &app.mode {
            AppMode::Input { action: InputAction::Search, .. } => {
                let count = app.search_match_count();
                Some(if count > 0 { format!("{} matches", count) } else { "No matches".to_string() })
            }
            _ => None,
        };

        Self {
            mode: &app.mode,
            repo_path: &app.repo_path,
            head_name: app.head_name.as_deref(),
            error_message,
            message: app.get_message(),
            is_fetching: app.is_fetching(),
            search_info,
        }
    }
}

impl<'a> Widget for StatusBar<'a> {
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
        let repo_style = Style::default()
            .fg(Color::Black)
            .bg(Color::Magenta)
            .add_modifier(Modifier::BOLD);

        let mut spans: Vec<Span> = Vec::new();

        // Show the repository name (folder name) on the left
        let repo_name = std::path::Path::new(self.repo_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(self.repo_path);
        spans.push(Span::styled(format!(" {} ", repo_name), repo_style));
        spans.push(Span::raw(" "));

        // HEAD branch
        if let Some(head) = self.head_name {
            spans.push(Span::styled(
                format!(" {} ", head),
                Style::default().fg(Color::Black).bg(Color::Green),
            ));
            spans.push(Span::raw(" "));
        }

        // Key hints (vary by mode)
        match self.mode {
            AppMode::Normal => match self.message {
                Some(msg) => {
                    // Yellow for in-progress, Cyan for success
                    let bg = if self.is_fetching {
                        Color::Yellow
                    } else {
                        Color::Cyan
                    };
                    let msg_style = Style::default()
                        .fg(Color::Black)
                        .bg(bg)
                        .add_modifier(Modifier::BOLD);
                    spans.push(Span::styled(format!(" {} ", msg), msg_style));
                    spans.push(Span::raw("  "));
                }
                None => {
                    // Show search info if available
                    if let Some(info) = &self.search_info {
                        let search_style = Style::default()
                            .fg(Color::Black)
                            .bg(Color::Green)
                            .add_modifier(Modifier::BOLD);
                        spans.push(Span::styled(format!(" {} ", info), search_style));
                        spans.push(Span::raw("  "));
                    }

                    spans.push(Span::styled(" j/k ", key_style));
                    spans.push(Span::styled("move ", desc_style));
                    spans.push(Span::styled(" Enter ", key_style));
                    spans.push(Span::styled("checkout ", desc_style));
                    spans.push(Span::styled(" b ", key_style));
                    spans.push(Span::styled("branch ", desc_style));
                    spans.push(Span::styled(" f ", key_style));
                    spans.push(Span::styled("fetch ", desc_style));
                    spans.push(Span::styled(" ? ", key_style));
                    spans.push(Span::styled("help ", desc_style));
                    spans.push(Span::styled(" q ", key_style));
                    spans.push(Span::styled("quit", desc_style));
                }
            },
            AppMode::Help => {
                spans.push(Span::styled(" Esc/q ", key_style));
                spans.push(Span::styled("close help", desc_style));
            }
            AppMode::Input { .. } => {
                spans.push(Span::styled(" Enter ", key_style));
                spans.push(Span::styled("confirm ", desc_style));
                spans.push(Span::styled(" Esc ", key_style));
                spans.push(Span::styled("cancel", desc_style));
            }
            AppMode::Confirm { .. } => {
                spans.push(Span::styled(" y ", key_style));
                spans.push(Span::styled("yes ", desc_style));
                spans.push(Span::styled(" n ", key_style));
                spans.push(Span::styled("no", desc_style));
            }
            AppMode::Error { .. } => {
                // In error mode, show the message and hide key hints
                let error_style = Style::default()
                    .fg(Color::White)
                    .bg(Color::Red)
                    .add_modifier(Modifier::BOLD);
                if let Some(msg) = self.error_message {
                    spans.push(Span::styled(format!(" {} ", msg), error_style));
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(" Esc/Enter ", key_style));
                    spans.push(Span::styled("close", desc_style));
                }
            }
        }

        let line = Line::from(spans);
        buf.set_line(area.x, area.y, &line, area.width);

        // Show the mode on the right (only for non-Normal modes)
        let mode_text = match self.mode {
            AppMode::Normal => None,
            AppMode::Help => Some(" HELP "),
            AppMode::Input { .. } => Some(" INPUT "),
            AppMode::Confirm { .. } => Some(" CONFIRM "),
            AppMode::Error { .. } => Some(" ERROR "),
        };
        if let Some(text) = mode_text {
            let mode_len = text.len() as u16;
            if area.width > mode_len {
                let x = area.x + area.width - mode_len;
                buf.set_string(x, area.y, text, mode_style);
            }
        }
    }
}
