//! Vertical layout strategy implementation
//!
//! This is the traditional git graph layout with commits flowing top-to-bottom
//! and branches as horizontal lanes.

use anyhow::Result;
use ratatui::widgets::ListState;
use git2::Oid;

use crate::action::Action;
use crate::app::{App, AppMode};
use crate::git::{
    BranchInfo, CommitInfo, CommitDiffInfo, build_graph,
};
use crate::git::layout::{
    GraphLayoutStrategy, UnifiedSelection, NavigationDirection, RenderContext,
};
use crate::git::graph::GraphLayout;
use crate::ui::{
    draw, GraphViewWidget, CommitDetailWidget, StatusBar,
    dialog::{BranchInfoPopup, ConfirmDialog, InputDialog},
    help_popup::HelpPopup,
    search_dropdown::{calculate_dropdown_height, SearchDropdown},
};

/// Vertical layout strategy implementation
pub struct VerticalLayoutStrategy {
    /// Reference to the graph layout
    pub graph_layout: GraphLayout,
    /// UI list state for scrolling
    pub list_state: ListState,
    /// Branch positions: (node_index, branch_name)
    pub branch_positions: Vec<(usize, String)>,
    /// Currently selected branch position
    pub selected_branch_position: Option<usize>,
}

impl VerticalLayoutStrategy {
    /// Create a new vertical layout strategy from an app
    pub fn from_app(app: &App) -> Result<Self> {
        let list_state = app.graph_list_state.clone();
        let graph_layout = app.graph_layout.clone();
        let branch_positions = Self::build_branch_positions(&graph_layout);
        let selected_branch_position = app.selected_branch_position;

        Ok(Self {
            graph_layout,
            list_state,
            branch_positions,
            selected_branch_position,
        })
    }

    /// Create a new vertical layout with default selection
    pub fn new(
        commits: &[CommitInfo],
        branches: &[BranchInfo],
        uncommitted_count: Option<usize>,
        head_commit_oid: Option<Oid>,
    ) -> Self {
        let graph_layout = build_graph(commits, branches, uncommitted_count, head_commit_oid);
        let branch_positions = Self::build_branch_positions(&graph_layout);

        // Check if uncommitted node exists
        let has_uncommitted = graph_layout
            .nodes
            .first()
            .is_some_and(|node| node.is_uncommitted);

        let mut list_state = ListState::default();
        list_state.select(Some(0));

        let selected_branch_position = if has_uncommitted || branch_positions.is_empty() {
            None
        } else {
            Some(0)
        };

        Self {
            graph_layout,
            list_state,
            branch_positions,
            selected_branch_position,
        }
    }

    /// Build a flat list of (node_index, branch_name) for all branches
    fn build_branch_positions(graph_layout: &GraphLayout) -> Vec<(usize, String)> {
        use crate::app::filter_remote_duplicates;

        graph_layout
            .nodes
            .iter()
            .enumerate()
            .flat_map(|(node_idx, node)| {
                filter_remote_duplicates(&node.branch_names)
                    .into_iter()
                    .map(move |name| (node_idx, name.to_string()))
            })
            .collect()
    }

    /// Move selection by delta
    fn move_selection(&mut self, delta: i32) {
        let max = self.graph_layout.nodes.len().saturating_sub(1);
        let current = self.list_state.selected().unwrap_or(0);
        let new = (current as i32 + delta).clamp(0, max as i32) as usize;
        self.list_state.select(Some(new));
        self.sync_branch_selection_to_node(new);
    }

    /// Select first commit
    fn select_first(&mut self) {
        self.list_state.select(Some(0));
        self.sync_branch_selection_to_node(0);
    }

    /// Select last commit
    fn select_last(&mut self) {
        let max = self.graph_layout.nodes.len().saturating_sub(1);
        self.list_state.select(Some(max));
        self.sync_branch_selection_to_node(max);
    }

    /// Sync branch selection to the first branch of the given node
    fn sync_branch_selection_to_node(&mut self, node_idx: usize) {
        self.selected_branch_position = self
            .branch_positions
            .iter()
            .position(|(idx, _)| *idx == node_idx);
    }

    /// Move to next branch
    fn move_to_next_branch(&mut self) {
        if self.branch_positions.is_empty() {
            return;
        }

        let next = match self.selected_branch_position {
            Some(pos) if pos + 1 < self.branch_positions.len() => pos + 1,
            None => 0,
            Some(_) => return,
        };

        self.selected_branch_position = Some(next);
        if let Some((node_idx, _)) = self.branch_positions.get(next) {
            self.list_state.select(Some(*node_idx));
        }
    }

    /// Move to previous branch
    fn move_to_prev_branch(&mut self) {
        if self.branch_positions.is_empty() {
            return;
        }

        let prev = match self.selected_branch_position {
            Some(pos) if pos > 0 => pos - 1,
            None => self.branch_positions.len() - 1,
            Some(_) => return,
        };

        self.selected_branch_position = Some(prev);
        if let Some((node_idx, _)) = self.branch_positions.get(prev) {
            self.list_state.select(Some(*node_idx));
        }
    }

    /// Move to adjacent branch within same commit
    fn move_branch_within_node(&mut self, delta: isize) {
        let Some(pos) = self.selected_branch_position else {
            return;
        };

        let new_pos = (pos as isize + delta) as usize;
        if new_pos >= self.branch_positions.len() {
            return;
        }

        let Some((current_node, _)) = self.branch_positions.get(pos) else {
            return;
        };
        let Some((target_node, _)) = self.branch_positions.get(new_pos) else {
            return;
        };

        if current_node == target_node {
            self.selected_branch_position = Some(new_pos);
        }
    }

    fn move_branch_left(&mut self) {
        self.move_branch_within_node(-1);
    }

    fn move_branch_right(&mut self) {
        self.move_branch_within_node(1);
    }

    /// Get selected node
    fn selected_node(&self) -> Option<&crate::git::graph::GraphNode> {
        self.list_state
            .selected()
            .and_then(|i| self.graph_layout.nodes.get(i))
    }

    /// Get selected branch info
    fn selected_branch(&self, branches: &[BranchInfo]) -> Option<&BranchInfo> {
        let (_, branch_name) = self
            .selected_branch_position
            .and_then(|pos| self.branch_positions.get(pos))?;
        branches.iter().find(|b| &b.name == branch_name)
    }
}

impl GraphLayoutStrategy for VerticalLayoutStrategy {
    fn get_selection(&self) -> UnifiedSelection {
        UnifiedSelection::Vertical(self.list_state.selected().unwrap_or(0))
    }

    fn set_selection(&mut self, selection: UnifiedSelection) -> Result<()> {
        match selection {
            UnifiedSelection::Vertical(idx) => {
                let max = self.graph_layout.nodes.len().saturating_sub(1);
                let clamped = idx.min(max);
                self.list_state.select(Some(clamped));
                self.sync_branch_selection_to_node(clamped);
                Ok(())
            }
            UnifiedSelection::Horizontal { .. } => {
                // Convert horizontal selection to vertical by finding nearest commit
                self.select_first();
                Ok(())
            }
        }
    }

    fn navigate(&mut self, direction: NavigationDirection) -> Result<()> {
        match direction {
            NavigationDirection::Previous => self.move_selection(-1),
            NavigationDirection::Next => self.move_selection(1),
            NavigationDirection::PreviousLane => self.move_branch_left(),
            NavigationDirection::NextLane => self.move_branch_right(),
            NavigationDirection::First => self.select_first(),
            NavigationDirection::Last => self.select_last(),
            NavigationDirection::PageUp => self.move_selection(-10),
            NavigationDirection::PageDown => self.move_selection(10),
        }
        Ok(())
    }

    fn selected_commit(&self) -> Option<&CommitInfo> {
        self.selected_node()?.commit.as_ref()
    }

    fn selected_branch_name(&self) -> Option<&str> {
        self.selected_branch_position
            .and_then(|pos| self.branch_positions.get(pos))
            .map(|(_, name)| name.as_str())
    }

    fn selected_branches(&self) -> Vec<&str> {
        let Some(node_idx) = self.list_state.selected() else {
            return vec![];
        };
        self.branch_positions
            .iter()
            .filter(|(idx, _)| *idx == node_idx)
            .map(|(_, name)| name.as_str())
            .collect()
    }

    fn cached_diff(&self) -> Option<&CommitDiffInfo> {
        // This would be provided by the app's diff cache
        None
    }

    fn is_diff_loading(&self) -> bool {
        false
    }

    fn is_uncommitted_selected(&self) -> bool {
        self.selected_node()
            .is_some_and(|node| node.is_uncommitted)
    }

    fn selection_description(&self) -> String {
        if let Some(node) = self.selected_node() {
            if node.is_uncommitted {
                return "Uncommitted changes".to_string();
            }
            if let Some(commit) = &node.commit {
                let short_id = format!("{:.7}", commit.oid);
                let msg_first_line = commit.message.lines().next().unwrap_or("");
                return format!("{} {}", short_id, msg_first_line);
            }
        }
        "No selection".to_string()
    }

    fn handle_action(&mut self, action: &Action, app: &mut App) -> Result<()> {
        // Delegate most actions to the app's handler
        app.handle_action(action.clone())
    }

    fn refresh(&mut self, app: &mut App) -> Result<()> {
        self.graph_layout = app.graph_layout.clone();
        self.branch_positions = Self::build_branch_positions(&self.graph_layout);
        self.list_state = app.graph_list_state.clone();
        self.selected_branch_position = app.selected_branch_position;
        Ok(())
    }

    fn toggle_orientation(&mut self, app: &mut App) -> Result<()> {
        app.toggle_orientation();
        Ok(())
    }

    fn orientation_name(&self) -> &str {
        "vertical"
    }

    fn render(&self, frame: &mut ratatui::Frame, app: &mut App, _ctx: &RenderContext) -> Result<()> {
        draw(frame, app);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_commits() -> Vec<CommitInfo> {
        vec![
            CommitInfo {
                oid: git2::Oid::zero().clone(),
                parent_oids: vec![],
                author: "Test Author".to_string(),
                committer: "Test Author".to_string(),
                message: "Test commit 1".to_string(),
                time: 0,
            },
            CommitInfo {
                oid: git2::Oid::zero().clone(),
                parent_oids: vec![git2::Oid::zero()],
                author: "Test Author".to_string(),
                committer: "Test Author".to_string(),
                message: "Test commit 2".to_string(),
                time: 1,
            },
        ]
    }

    #[test]
    fn test_vertical_layout_creation() {
        let commits = create_test_commits();
        let branches = vec![];
        let layout = VerticalLayoutStrategy::new(&commits, &branches, None, None);

        assert_eq!(layout.graph_layout.nodes.len(), commits.len());
        assert_eq!(layout.list_state.selected(), Some(0));
    }

    #[test]
    fn test_navigation() {
        let commits = create_test_commits();
        let layout = VerticalLayoutStrategy::new(&commits, &[], None, None);

        layout.navigate(NavigationDirection::Next).unwrap();
        assert_eq!(layout.list_state.selected(), Some(1));

        layout.navigate(NavigationDirection::Previous).unwrap();
        assert_eq!(layout.list_state.selected(), Some(0));

        layout.navigate(NavigationDirection::Last).unwrap();
        assert_eq!(layout.list_state.selected(), Some(1));

        layout.navigate(NavigationDirection::First).unwrap();
        assert_eq!(layout.list_state.selected(), Some(0));
    }

    #[test]
    fn test_selection_description() {
        let commits = create_test_commits();
        let layout = VerticalLayoutStrategy::new(&commits, &[], None, None);

        let desc = layout.selection_description();
        assert!(desc.contains("Test commit 1"));
    }

    #[test]
    fn test_unified_selection() {
        let commits = create_test_commits();
        let mut layout = VerticalLayoutStrategy::new(&commits, &[], None, None);

        let sel = layout.get_selection();
        assert!(matches!(sel, UnifiedSelection::Vertical(0)));

        layout.set_selection(UnifiedSelection::Vertical(1)).unwrap();
        assert_eq!(layout.list_state.selected(), Some(1));
    }
}
