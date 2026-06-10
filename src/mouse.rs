//! Mouse input routing (issue #12)
//!
//! Clicks and scroll events are routed to the pane under the cursor using the
//! pane regions recorded during the last render (`App::layout`).

use std::time::{Duration, Instant};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use crate::{
    action::Action,
    app::{App, AppMode, FocusedPane},
};

/// Max delay between two clicks on the same cell to count as a double-click
const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(400);

pub fn handle_mouse(app: &mut App, event: MouseEvent) {
    match event.kind {
        MouseEventKind::ScrollDown => handle_scroll(app, 1, event.column, event.row),
        MouseEventKind::ScrollUp => handle_scroll(app, -1, event.column, event.row),
        MouseEventKind::Down(MouseButton::Left) => handle_click(app, event.column, event.row),
        _ => {}
    }
}

fn dispatch(app: &mut App, action: Action) {
    if let Err(e) = app.handle_action(action) {
        app.show_error(format!("{}", e));
    }
}

fn contains(rect: Rect, x: u16, y: u16) -> bool {
    rect.contains(Position { x, y })
}

/// Inner row index (0-based, borders excluded) for the given y, if inside
fn inner_row(rect: Rect, y: u16) -> Option<u16> {
    let top = rect.y + 1;
    let bottom = (rect.y + rect.height).saturating_sub(1);
    (y >= top && y < bottom).then(|| y - top)
}

fn handle_scroll(app: &mut App, delta: i32, x: u16, y: u16) {
    match &app.mode {
        AppMode::FileDiff { .. } => {
            let action = if delta > 0 {
                Action::ScrollDown
            } else {
                Action::ScrollUp
            };
            // Diff view: 3x scroll speed
            for _ in 0..3 {
                dispatch(app, action.clone());
            }
        }
        AppMode::Normal | AppMode::FileSelect { .. } => {
            let layout = app.layout;
            if contains(layout.commit_detail, x, y) {
                app.scroll_detail(delta);
            } else if contains(layout.files, x, y) {
                if matches!(app.mode, AppMode::FileSelect { .. }) {
                    let action = if delta > 0 {
                        Action::FileSelectDown
                    } else {
                        Action::FileSelectUp
                    };
                    dispatch(app, action);
                }
            } else if contains(layout.graph, x, y) && matches!(app.mode, AppMode::Normal) {
                app.move_selection(delta);
            }
        }
        _ => {}
    }
}

fn handle_click(app: &mut App, x: u16, y: u16) {
    let now = Instant::now();
    let is_double = matches!(
        app.last_click,
        Some((t, px, py)) if now.duration_since(t) < DOUBLE_CLICK_WINDOW && px == x && py == y
    );
    app.last_click = Some((now, x, y));

    match &app.mode {
        AppMode::Help | AppMode::Error { .. } => {
            dispatch(app, Action::Cancel);
        }
        AppMode::Normal | AppMode::FileSelect { .. } => {
            let layout = app.layout;
            if contains(layout.graph, x, y) {
                let Some(row) = inner_row(layout.graph, y) else {
                    return;
                };
                let idx = app.graph_list_state.offset() + row as usize;
                if idx >= app.graph_layout.nodes.len() {
                    return;
                }
                if matches!(app.mode, AppMode::FileSelect { .. }) {
                    dispatch(app, Action::Cancel);
                }
                app.focused_pane = FocusedPane::Graph;
                app.select_node(idx);
                if is_double {
                    dispatch(app, Action::EnterFileSelect);
                }
            } else if contains(layout.commit_detail, x, y) {
                if matches!(app.mode, AppMode::Normal) {
                    app.focused_pane = FocusedPane::Detail;
                }
            } else if contains(layout.files, x, y) {
                let Some(row) = inner_row(layout.files, y) else {
                    return;
                };
                let line_idx = app.files_pane_scroll as usize + row as usize;
                // The first two lines of the pane are the summary header
                if line_idx < 2 {
                    return;
                }
                app.open_file_select(line_idx - 2);
                if is_double {
                    dispatch(app, Action::OpenFileDiff);
                }
            }
        }
        // FileDiff / Input / Confirm: keyboard only for now
        _ => {}
    }
}
