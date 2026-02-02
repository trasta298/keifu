//! Layout strategies for git graph visualization
//!
//! This module defines the abstraction layer for different graph layout orientations.
//! It provides a trait-based interface that allows the application to work with
//! different layout strategies (vertical, horizontal) without coupling to specific
//! implementations.

use ratatui::layout::Rect;

use crate::app::{App, AppMode};
use crate::action::Action;
use crate::git::{CommitInfo, CommitDiffInfo};

/// Unified selection type that can represent both vertical and horizontal selections
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnifiedSelection {
    /// Vertical layout selection: row index in the graph
    Vertical(usize),
    /// Horizontal layout selection: chunk_index, column, lane
    Horizontal { chunk_index: usize, column: usize, lane: usize },
}

impl Default for UnifiedSelection {
    fn default() -> Self {
        Self::Vertical(0)
    }
}

/// Navigation direction for layout-agnostic movement
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationDirection {
    /// Move to previous commit (up in vertical, left in horizontal)
    Previous,
    /// Move to next commit (down in vertical, right in horizontal)
    Next,
    /// Move to previous lane/branch (left in vertical, up in horizontal)
    PreviousLane,
    /// Move to next lane/branch (right in vertical, down in horizontal)
    NextLane,
    /// Jump to first commit (top of vertical, leftmost of horizontal)
    First,
    /// Jump to last commit (bottom of vertical, rightmost of horizontal)
    Last,
    /// Page up (previous page or previous chunk)
    PageUp,
    /// Page down (next page or next chunk)
    PageDown,
}

/// Rendering context information passed to layout strategies
#[derive(Debug, Clone)]
pub struct RenderContext {
    /// Available drawing area
    pub area: Rect,
    /// Whether we're currently in a popup/dialog mode
    pub is_popup_mode: bool,
    /// Current app mode
    pub app_mode: AppMode,
}

impl RenderContext {
    /// Create a new render context
    pub fn new(area: Rect, is_popup_mode: bool, app_mode: AppMode) -> Self {
        Self {
            area,
            is_popup_mode,
            app_mode,
        }
    }

    /// Create from app state and terminal area
    pub fn from_app(app: &App, area: Rect) -> Self {
        Self {
            area,
            is_popup_mode: !matches!(app.mode, AppMode::Normal),
            app_mode: app.mode.clone(),
        }
    }
}

/// Trait defining the interface for graph layout strategies
///
/// This trait abstracts the differences between vertical and horizontal layouts,
/// allowing the application to handle navigation and rendering in a layout-agnostic way.
pub trait GraphLayoutStrategy: Send + Sync {
    /// Get the current selection in unified format
    fn get_selection(&self) -> UnifiedSelection;

    /// Set the selection from a unified format
    fn set_selection(&mut self, selection: UnifiedSelection) -> anyhow::Result<()>;

    /// Navigate in the specified direction
    fn navigate(&mut self, direction: NavigationDirection) -> anyhow::Result<()>;

    /// Get the currently selected commit (if any)
    fn selected_commit(&self) -> Option<&CommitInfo>;

    /// Get the currently selected branch name (if any)
    fn selected_branch_name(&self) -> Option<&str>;

    /// Get all branch names at the current selection
    fn selected_branches(&self) -> Vec<&str>;

    /// Get the cached diff for the current selection
    fn cached_diff(&self) -> Option<&CommitDiffInfo>;

    /// Check if diff is currently loading for the current selection
    fn is_diff_loading(&self) -> bool;

    /// Check if the current selection is uncommitted changes
    fn is_uncommitted_selected(&self) -> bool;

    /// Get a human-readable description of the current selection
    fn selection_description(&self) -> String;

    /// Handle an action specific to this layout
    fn handle_action(&mut self, action: &Action, app: &mut App) -> anyhow::Result<()>;

    /// Render the layout to the terminal
    fn render(&self, frame: &mut ratatui::Frame, app: &mut App, ctx: &RenderContext) -> anyhow::Result<()>;

    /// Refresh the layout data from the app
    fn refresh(&mut self, app: &mut App) -> anyhow::Result<()>;

    /// Toggle orientation (if supported)
    fn toggle_orientation(&mut self, app: &mut App) -> anyhow::Result<()>;

    /// Get the orientation name for display
    fn orientation_name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unified_selection_default() {
        let selection = UnifiedSelection::default();
        assert!(matches!(selection, UnifiedSelection::Vertical(0)));
    }

    #[test]
    fn test_unified_selection_clone() {
        let sel1 = UnifiedSelection::Vertical(5);
        let sel2 = sel1;
        assert_eq!(sel1, UnifiedSelection::Vertical(5));
        assert_eq!(sel2, UnifiedSelection::Vertical(5));
    }

    #[test]
    fn test_navigation_direction_equality() {
        assert_eq!(NavigationDirection::Previous, NavigationDirection::Previous);
        assert_ne!(NavigationDirection::Previous, NavigationDirection::Next);
    }

    #[test]
    fn test_render_context_creation() {
        let area = Rect::new(0, 0, 80, 24);
        let ctx = RenderContext::new(area, false, AppMode::Normal);
        assert_eq!(ctx.area, area);
        assert!(!ctx.is_popup_mode);
        assert!(matches!(ctx.app_mode, AppMode::Normal));
    }
}
