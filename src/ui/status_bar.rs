//! ステータスバーWidget

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

use crate::app::{App, AppMode};

pub struct StatusBar<'a> {
    mode: &'a AppMode,
    repo_path: &'a str,
    head_name: Option<&'a str>,
}

impl<'a> StatusBar<'a> {
    pub fn new(app: &'a App) -> Self {
        Self {
            mode: &app.mode,
            repo_path: &app.repo_path,
            head_name: app.head_name.as_deref(),
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

        let mut spans: Vec<Span> = Vec::new();

        // モード表示
        let mode_text = match self.mode {
            AppMode::Normal => " NORMAL ",
            AppMode::Help => " HELP ",
            AppMode::Input { .. } => " INPUT ",
            AppMode::Confirm { .. } => " CONFIRM ",
        };
        spans.push(Span::styled(mode_text, mode_style));
        spans.push(Span::raw(" "));

        // HEADブランチ
        if let Some(head) = self.head_name {
            spans.push(Span::styled(
                format!(" {} ", head),
                Style::default().fg(Color::Black).bg(Color::Green),
            ));
            spans.push(Span::raw(" "));
        }

        // キーヒント（モードに応じて変更）
        match self.mode {
            AppMode::Normal => {
                spans.push(Span::styled(" j/k ", key_style));
                spans.push(Span::styled("move ", desc_style));
                spans.push(Span::styled(" Enter ", key_style));
                spans.push(Span::styled("checkout ", desc_style));
                spans.push(Span::styled(" b ", key_style));
                spans.push(Span::styled("branch ", desc_style));
                spans.push(Span::styled(" m ", key_style));
                spans.push(Span::styled("merge ", desc_style));
                spans.push(Span::styled(" r ", key_style));
                spans.push(Span::styled("rebase ", desc_style));
                spans.push(Span::styled(" ? ", key_style));
                spans.push(Span::styled("help ", desc_style));
                spans.push(Span::styled(" q ", key_style));
                spans.push(Span::styled("quit", desc_style));
            }
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
        }

        let line = Line::from(spans);
        buf.set_line(area.x, area.y, &line, area.width);

        // 右端にリポジトリパス
        let path_text = format!(" {} ", self.repo_path);
        let path_len = path_text.len() as u16;
        if area.width > path_len {
            let x = area.x + area.width - path_len;
            buf.set_string(
                x,
                area.y,
                &path_text,
                Style::default().fg(Color::DarkGray),
            );
        }
    }
}
