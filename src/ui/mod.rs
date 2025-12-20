//! UI components

pub mod commit_detail;
pub mod dialog;
pub mod graph_view;
pub mod help_popup;
pub mod status_bar;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};

use crate::app::App;

use self::{
    commit_detail::CommitDetailWidget,
    dialog::{ConfirmDialog, InputDialog},
    graph_view::GraphViewWidget,
    help_popup::HelpPopup,
    status_bar::StatusBar,
};

/// Render the main UI
pub fn draw(frame: &mut Frame, app: &mut App) {
    // Update the diff cache once before rendering
    app.update_diff_cache();

    let area = frame.area();

    // Vertical split: main area + status bar (1 row)
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let main_area = vertical[0];
    let status_area = vertical[1];

    // Split main area vertically: graph (70%) + detail (30%)
    let content_vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(main_area);

    let graph_area = content_vertical[0];
    let detail_area = content_vertical[1];

    // Render widgets
    frame.render_stateful_widget(
        GraphViewWidget::new(app, graph_area.width),
        graph_area,
        &mut app.graph_list_state,
    );
    frame.render_widget(CommitDetailWidget::new(app), detail_area);
    frame.render_widget(StatusBar::new(app), status_area);

    // Popups
    match &app.mode {
        crate::app::AppMode::Help => {
            let popup_area = centered_rect(60, 70, area);
            frame.render_widget(HelpPopup, popup_area);
        }
        crate::app::AppMode::Input { title, input, .. } => {
            let popup_area = centered_rect(50, 20, area);
            frame.render_widget(InputDialog::new(title, input), popup_area);
        }
        crate::app::AppMode::Confirm { message, .. } => {
            let popup_area = centered_rect(50, 20, area);
            frame.render_widget(ConfirmDialog::new(message), popup_area);
        }
        _ => {}
    }
}

/// Calculate a centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
