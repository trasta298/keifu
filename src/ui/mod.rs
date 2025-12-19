//! UIコンポーネント

pub mod branch_list;
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
    branch_list::BranchListWidget,
    commit_detail::CommitDetailWidget,
    dialog::{ConfirmDialog, InputDialog},
    graph_view::GraphViewWidget,
    help_popup::HelpPopup,
    status_bar::StatusBar,
};

/// メインUIを描画
pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    // 縦分割: メイン + ステータスバー(1行)
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let main_area = vertical[0];
    let status_area = vertical[1];

    // メインを横分割: ブランチリスト(20%) + 右側(80%)
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
        .split(main_area);

    let branch_area = horizontal[0];
    let right_area = horizontal[1];

    // 右側を縦分割: グラフ(70%) + 詳細(30%)
    let right_vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(right_area);

    let graph_area = right_vertical[0];
    let detail_area = right_vertical[1];

    // 各Widgetを描画
    frame.render_stateful_widget(
        BranchListWidget::new(app),
        branch_area,
        &mut app.branch_list_state,
    );
    frame.render_stateful_widget(
        GraphViewWidget::new(app),
        graph_area,
        &mut app.graph_list_state,
    );
    frame.render_widget(CommitDetailWidget::new(app), detail_area);
    frame.render_widget(StatusBar::new(app), status_area);

    // ポップアップ
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

/// 中央に配置された矩形を計算
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
