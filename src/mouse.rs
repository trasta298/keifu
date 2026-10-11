//! Mouse input routing (issue #12)
//!
//! Clicks and scroll events are routed to the pane under the cursor using the
//! pane regions recorded during the last render (`App::layout`).

use std::time::{Duration, Instant};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Margin, Position, Rect};

use crate::{
    action::Action,
    app::{App, AppMode, FocusedPane},
    selection::TextSelection,
};

/// Max delay between two clicks on the same cell to count as a double-click
const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(400);

pub fn handle_mouse(app: &mut App, event: MouseEvent) {
    match event.kind {
        MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => handle_scroll(app, event, 1),
        MouseEventKind::Down(MouseButton::Left) => {
            let position = Position::new(event.column, event.row);
            app.text_selection = None;
            if app.screen.area.contains(position) {
                let area = selection_area(app, position);
                app.text_selection = Some(TextSelection::new(&app.screen, area, position));
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(selection) = app.text_selection.as_mut().filter(|s| s.pressed) {
                selection.update(Position::new(event.column, event.row));
                if selection.dragged {
                    app.last_click = None;
                }
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            let Some(mut selection) = app.text_selection.take() else {
                return;
            };
            if !selection.pressed {
                app.text_selection = Some(selection);
                return;
            }
            selection.update(Position::new(event.column, event.row));
            selection.pressed = false;
            if selection.dragged {
                app.last_click = None;
                let text = selection.text();
                if !text.is_empty() {
                    match crate::tui::copy_to_clipboard(&text) {
                        Ok(()) => app.set_message("Copied selection"),
                        Err(error) => app.set_message(format!("Copy failed: {error}")),
                    }
                }
                app.text_selection = Some(selection);
            } else {
                handle_click(app, selection.start.x, selection.start.y);
            }
        }
        _ => {}
    }
}

pub fn handle_scroll(app: &mut App, event: MouseEvent, steps: usize) {
    app.text_selection = None;
    app.last_click = None;
    let steps = i32::try_from(steps).unwrap_or(i32::MAX);
    let delta = match event.kind {
        MouseEventKind::ScrollDown => steps,
        MouseEventKind::ScrollUp => -steps,
        _ => return,
    };
    handle_scroll_delta(app, delta, event.column, event.row);
}

fn selection_area(app: &App, position: Position) -> Rect {
    let panes = match app.mode {
        AppMode::FileDiff { .. } if !app.file_view_replace() => vec![Rect::new(
            app.screen.area.x,
            app.screen.area.y,
            app.screen.area.width,
            app.screen.area.height.saturating_sub(1),
        )],
        AppMode::Normal | AppMode::FileSelect { .. } | AppMode::FileDiff { .. } => {
            vec![app.layout.graph, app.layout.commit_detail, app.layout.files]
        }
        _ => Vec::new(),
    };
    panes
        .into_iter()
        .map(|area| area.intersection(app.screen.area))
        .find(|area| area.contains(position))
        .map(|area| {
            let inner = area.inner(Margin::new(1, 1));
            if inner.contains(position) {
                inner
            } else {
                area
            }
        })
        .unwrap_or(app.screen.area)
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

fn offset_index(current: usize, max: usize, delta: i32) -> usize {
    if delta >= 0 {
        current.saturating_add(delta as usize).min(max)
    } else {
        current.saturating_sub(delta.unsigned_abs() as usize)
    }
}

fn handle_scroll_delta(app: &mut App, delta: i32, x: u16, y: u16) {
    match &app.mode {
        // With the diff in the graph pane, the bottom panes keep their own wheel
        AppMode::FileDiff {
            file_index,
            file_list,
            ..
        } if app.file_view_replace() && !contains(app.layout.graph, x, y) => {
            let (current, last) = (*file_index, file_list.len().saturating_sub(1));
            let layout = app.layout;
            if contains(layout.commit_detail, x, y) {
                app.scroll_detail(delta);
            } else if contains(layout.files, x, y) {
                let target = offset_index(current, last, delta);
                if target != current {
                    app.open_file_select(target);
                    dispatch(app, Action::OpenFileDiff);
                }
            }
        }
        AppMode::FileDiff { .. } => {
            let viewport = app.diff_viewport_height as usize;
            let AppMode::FileDiff {
                total_lines,
                scroll_offset,
                ..
            } = &mut app.mode
            else {
                return;
            };
            let max_scroll = total_lines.saturating_sub(viewport);
            *scroll_offset = offset_index(*scroll_offset, max_scroll, delta.saturating_mul(3));
        }
        AppMode::Help => {
            if delta >= 0 {
                app.help_scroll = app
                    .help_scroll
                    .saturating_add(u16::try_from(delta).unwrap_or(u16::MAX));
            } else {
                app.help_scroll = app
                    .help_scroll
                    .saturating_sub(u16::try_from(delta.unsigned_abs()).unwrap_or(u16::MAX));
            }
        }
        AppMode::Normal | AppMode::FileSelect { .. } => {
            let layout = app.layout;
            if contains(layout.commit_detail, x, y) {
                app.scroll_detail(delta);
            } else if contains(layout.files, x, y) {
                if let AppMode::FileSelect {
                    selected_index,
                    file_list,
                } = &mut app.mode
                {
                    *selected_index =
                        offset_index(*selected_index, file_list.len().saturating_sub(1), delta);
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

    // Status bar hints are clickable in every mode
    if contains(app.layout.status_bar, x, y) {
        let action = app
            .status_hints
            .iter()
            .find(|(rect, _)| contains(*rect, x, y))
            .map(|(_, action)| action.clone());
        if let Some(action) = action {
            dispatch(app, action);
        }
        return;
    }

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
                click_file(app, y, is_double);
            }
        }
        // With the diff in the graph pane, the file list stays clickable
        AppMode::FileDiff { .. } if app.file_view_replace() && contains(app.layout.files, x, y) => {
            click_file(app, y, false);
        }
        // Fullscreen FileDiff / Input / Confirm: keyboard only for now
        _ => {}
    }
}

/// Click on a row of the file list: select it, and open its diff when `open`
/// or when the diff replaces the graph pane.
fn click_file(app: &mut App, y: u16, open: bool) {
    let Some(row) = inner_row(app.layout.files, y) else {
        return;
    };
    let line_idx = app.files_pane_scroll as usize + row as usize;
    // The first two lines of the pane are the summary header
    if line_idx < 2 {
        return;
    }
    app.open_file_select(line_idx - 2);
    if (open || app.file_view_replace()) && matches!(app.mode, AppMode::FileSelect { .. }) {
        dispatch(app, Action::OpenFileDiff);
    }
}
