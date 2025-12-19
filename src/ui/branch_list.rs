//! ブランチ一覧Widget

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, StatefulWidget},
};

use crate::app::{App, PaneFocus};

pub struct BranchListWidget<'a> {
    branches: Vec<ListItem<'a>>,
    is_focused: bool,
}

impl<'a> BranchListWidget<'a> {
    pub fn new(app: &App) -> Self {
        let branches: Vec<ListItem> = app
            .branches
            .iter()
            .map(|branch| {
                let prefix = if branch.is_head { "● " } else { "○ " };
                let style = if branch.is_head {
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                } else if branch.is_remote {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default()
                };

                let name = if branch.is_remote {
                    branch.name.clone()
                } else {
                    branch.name.clone()
                };

                ListItem::new(Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled(name, style),
                ]))
            })
            .collect();

        Self {
            branches,
            is_focused: app.focus == PaneFocus::BranchList,
        }
    }
}

impl<'a> StatefulWidget for BranchListWidget<'a> {
    type State = ListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let border_style = if self.is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title(" Branches ")
            .borders(Borders::ALL)
            .border_style(border_style);

        let highlight_style = Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD);

        let list = List::new(self.branches)
            .block(block)
            .highlight_style(highlight_style)
            .highlight_symbol("→ ");

        StatefulWidget::render(list, area, buf, state);
    }
}
