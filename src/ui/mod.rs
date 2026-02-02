//! UI components

pub mod commit_detail;
pub mod dialog;
pub mod graph_view;
pub mod horizontal_graph_view;
pub mod help_popup;
pub mod legend_sidebar;
pub mod search_dropdown;
pub mod status_bar;

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Widget},
    Frame,
};

use crate::app::{App, AppMode, InputAction};

use self::{
    commit_detail::CommitDetailWidget,
    dialog::{BranchInfoPopup, ConfirmDialog, InputDialog},
    graph_view::GraphViewWidget,
    horizontal_graph_view::{HorizontalGraphViewWidget, HorizontalGraphState},
    help_popup::HelpPopup,
    legend_sidebar::LegendSidebarWidget,
    search_dropdown::{calculate_dropdown_height, SearchDropdown},
    status_bar::StatusBar,
};

/// Minimum terminal width required for rendering
const MIN_WIDTH: u16 = 20;
/// Minimum terminal height required for rendering
const MIN_HEIGHT: u16 = 6;

/// Minimum widget dimensions for safe rendering
pub const MIN_WIDGET_WIDTH: u16 = 12;
pub const MIN_WIDGET_HEIGHT: u16 = 3;

/// Render a placeholder block when widget area is too small
pub fn render_placeholder_block(area: Rect, buf: &mut Buffer) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    block.render(area, buf);
}

/// Render the main UI
pub fn draw(frame: &mut Frame, app: &mut App) {
    // Update the diff cache once before rendering
    app.update_diff_cache();

    let area = frame.area();

    // Check minimum terminal size to prevent buffer overflow panics
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        let msg = format!(
            "Terminal too small ({}x{}). Need at least {}x{}.",
            area.width, area.height, MIN_WIDTH, MIN_HEIGHT
        );
        let paragraph = Paragraph::new(msg).style(Style::default().fg(Color::Red));
        frame.render_widget(paragraph, area);
        return;
    }

    // Draw based on current orientation
    match app.current_orientation {
        crate::git::graph::GraphOrientation::Vertical => {
            draw_vertical_layout(frame, app);
        }
        crate::git::graph::GraphOrientation::Horizontal => {
            draw_horizontal_layout(frame, app);
        }
    }
}

fn draw_vertical_layout(frame: &mut Frame, app: &mut App) {
    // Vertical split: main area + status bar (1 row)
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(frame.area());

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

    // Branch info popup (when multiple branches exist on selected node)
    render_branch_info_popup(frame, app, graph_area);

    // Popups
    match &app.mode {
        AppMode::Help => {
            let popup_area = centered_rect(60, 70, main_area);
            frame.render_widget(HelpPopup, popup_area);
        }
        AppMode::Input {
            input,
            action: InputAction::Search,
            ..
        } => {
            // Search dropdown at bottom of screen
            let results = app.search_results();
            let height = calculate_dropdown_height(results.len());
            let popup_area = bottom_rect(60, height, main_area);
            frame.render_widget(
                SearchDropdown::new(
                    input,
                    results,
                    &app.branch_positions,
                    app.search_selection(),
                ),
                popup_area,
            );
        }
        AppMode::Input { title, input, .. } => {
            let popup_area = centered_rect(50, 20, main_area);
            frame.render_widget(InputDialog::new(title, input), popup_area);
        }
        AppMode::Confirm { message, .. } => {
            let popup_area = centered_rect(50, 20, main_area);
            frame.render_widget(ConfirmDialog::new(message), popup_area);
        }
        _ => {}
    }
}

/// Render branch info popup when multiple branches exist on selected node
fn render_branch_info_popup(frame: &mut Frame, app: &App, graph_area: Rect) {
    let selected_branches = app.selected_node_branches();

    // Only show popup in Normal mode with multiple branches
    if selected_branches.len() <= 1 || !matches!(app.mode, crate::app::AppMode::Normal) {
        return;
    }

    let popup_height = (selected_branches.len() + 2).min(10) as u16;
    let max_branch_len = selected_branches
        .iter()
        .map(|b| b.len())
        .max()
        .unwrap_or(10);
    let popup_width = (max_branch_len + 6).min(50) as u16;

    // Calculate selected row's screen position (add 1 for border)
    let selected_idx = app.graph_list_state.selected().unwrap_or(0);
    let offset = app.graph_list_state.offset();
    let selected_screen_y = graph_area.y + 1 + selected_idx.saturating_sub(offset) as u16;

    // Position popup at right side of graph area
    let popup_x = graph_area.x + graph_area.width.saturating_sub(popup_width + 2);
    let default_popup_y = graph_area.y + 1;

    // Shift down only if popup overlaps with selected row
    let overlaps_selected =
        selected_screen_y >= default_popup_y && selected_screen_y < default_popup_y + popup_height;
    let popup_y = if overlaps_selected {
        (selected_screen_y + 1).min(graph_area.y + graph_area.height - popup_height)
    } else {
        default_popup_y
    };

    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);
    frame.render_widget(
        BranchInfoPopup::new(&selected_branches, app.selected_branch_name()),
        popup_area,
    );
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

/// Calculate a bottom-aligned rectangle (for dropdowns)
fn bottom_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let clamped_height = height.min(area.height.saturating_sub(2));
    let y = area.y + area.height.saturating_sub(clamped_height + 1);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(area);

    Rect::new(horizontal[1].x, y, horizontal[1].width, clamped_height)
}
fn draw_horizontal_layout(frame: &mut Frame, app: &mut App) {
    let size = frame.area();

    // Split: Legend (left) | Graph+Detail (right) + status bar (bottom)
    // Main area split
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)]) // status bar
        .split(size);

    let content_area = main_chunks[0];
    let status_area = main_chunks[1];

    // Content split: Legend | Graph+Detail
    let horizontal_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(22),  // Legend sidebar
            Constraint::Min(40),     // Graph area
        ])
        .split(content_area);

    let legend_area = horizontal_chunks[0];
    let graph_detail_area = horizontal_chunks[1];

    // Graph+Detail split: Graph (top) | Detail (bottom)
    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(70),  // Graph
            Constraint::Percentage(30),  // Detail
        ])
        .split(graph_detail_area);

    let graph_area = vertical_chunks[0];
    let detail_area = vertical_chunks[1];

    // Update horizontal layout if terminal width changed
    app.update_horizontal_layout_width(graph_area.width as usize);

    // Render legend
    if let Some(ref layout) = app.horizontal_layout {
        let legend_widget = LegendSidebarWidget {
            lanes: &layout.lanes,
        };
        frame.render_widget(legend_widget, legend_area);
    }

    // Render horizontal graph
    if let Some(ref layout) = app.horizontal_layout {
        let widget = HorizontalGraphViewWidget::new(
            layout,
            graph_area.width as usize,
            graph_area.height as usize,
            app.show_tags,
        );

        let mut state = HorizontalGraphState {
            current_chunk: layout.selection.chunk_index,
            selection: layout.selection,
        };

        frame.render_stateful_widget(widget, graph_area, &mut state);
    }

    // Render commit detail
    frame.render_widget(CommitDetailWidget::new(app), detail_area);

    // Render status bar
    frame.render_widget(StatusBar::new(app), status_area);

    // Popups
    match &app.mode {
        AppMode::Help => {
            let popup_area = centered_rect(60, 70, size);
            frame.render_widget(HelpPopup, popup_area);
        }
        AppMode::Input { title, input: _, action: _ } => {
            let popup_area = centered_rect(50, 3, size);
            frame.render_widget(
                InputDialog::new(title, ""),
                popup_area,
            );
        }
        AppMode::Confirm { message, action: _ } => {
            let popup_area = centered_rect(60, 3, size);
            frame.render_widget(
                ConfirmDialog::new(message),
                popup_area,
            );
        }
        AppMode::Error { message } => {
            let popup_area = centered_rect(60, 3, size);
            let error_dialog = ConfirmDialog::new(message);
            frame.render_widget(error_dialog, popup_area);
        }
        AppMode::Normal => {
            // No popup in normal mode
        }
    }

    // Branch info popup (when multiple branches exist on selected node)
    // This is handled by the existing render_branch_info_popup function
    // which works with both vertical and horizontal layouts
    render_branch_info_popup(frame, app, graph_area);
}