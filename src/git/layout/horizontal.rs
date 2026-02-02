//! Horizontal layout strategy implementation
//!
//! This layout displays commits left-to-right in chunks, with branches as
//! vertical lanes. Designed for wide terminal displays.

use anyhow::Result;
use git2::Oid;

use crate::action::Action;
use crate::app::{App, AppMode};
use crate::git::{
    BranchInfo, CommitInfo, CommitDiffInfo, TagInfo, build_horizontal_graph,
};
use crate::git::layout::{
    GraphLayoutStrategy, UnifiedSelection, NavigationDirection, RenderContext,
};
use crate::git::graph::{
    HorizontalGraphLayout, HorizontalSelection, HorizontalChunk,
    GraphOrientation,
};
use crate::ui::{
    draw, HorizontalGraphViewWidget, HorizontalGraphState, CommitDetailWidget,
    StatusBar, LegendSidebarWidget,
};

/// Horizontal layout strategy implementation
pub struct HorizontalLayoutStrategy {
    /// The horizontal graph layout data
    pub layout: HorizontalGraphLayout,
    /// Current chunk index being viewed
    pub current_chunk: usize,
}

impl HorizontalLayoutStrategy {
    /// Create a new horizontal layout strategy from an app
    pub fn from_app(app: &App) -> Result<Self> {
        let layout = app.horizontal_layout
            .as_ref()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No horizontal layout available"))?;

        Ok(Self {
            layout,
            current_chunk: 0,
        })
    }

    /// Create a new horizontal layout with default selection
    pub fn new(
        commits: &[CommitInfo],
        branches: &[BranchInfo],
        tags: &[TagInfo],
        uncommitted_count: Option<usize>,
        head_commit_oid: Option<Oid>,
        terminal_width: usize,
    ) -> Self {
        let layout = build_horizontal_graph(
            commits,
            branches,
            tags,
            uncommitted_count,
            head_commit_oid,
            terminal_width,
        );

        Self {
            layout,
            current_chunk: 0,
        }
    }

    /// Get the currently selected chunk
    fn current_chunk(&self) -> Option<&HorizontalChunk> {
        self.layout.chunks.get(self.current_chunk)
    }

    /// Move to previous commit (left)
    fn move_selection_left(&mut self) {
        let mut curr_chunk_idx = self.layout.selection.chunk_index;
        let mut curr_col = self.layout.selection.column;
        let lane = self.layout.selection.lane;

        // Search backwards for the next commit cell
        loop {
            if curr_col > 0 {
                curr_col -= 1;
            } else if curr_chunk_idx > 0 {
                curr_chunk_idx -= 1;
                if let Some(chunk) = self.layout.chunks.get(curr_chunk_idx) {
                    curr_col = chunk.end_column - chunk.start_column - 1;
                } else {
                    break; // Should not happen
                }
            } else {
                break; // Start of history
            }

            // Check if found commit
            if let Some(chunk) = self.layout.chunks.get(curr_chunk_idx) {
                if let Some(cell) = chunk.cells.get(lane).and_then(|row| row.get(curr_col)) {
                    if matches!(cell, crate::git::graph::CellType::Commit(_)) {
                        self.layout.selection.chunk_index = curr_chunk_idx;
                        self.layout.selection.column = curr_col;
                        break;
                    }
                }
            }
        }
    }

    /// Move to next commit (right)
    fn move_selection_right(&mut self) {
        let mut curr_chunk_idx = self.layout.selection.chunk_index;
        let mut curr_col = self.layout.selection.column;
        let lane = self.layout.selection.lane;
        let max_chunk_idx = self.layout.chunks.len().saturating_sub(1);

        // Search forwards for the next commit cell
        loop {
            let chunk = match self.layout.chunks.get(curr_chunk_idx) {
                Some(c) => c,
                None => break,
            };
            let chunk_width = chunk.end_column - chunk.start_column;
            
            if curr_col < chunk_width - 1 {
                curr_col += 1;
            } else if curr_chunk_idx < max_chunk_idx {
                curr_chunk_idx += 1;
                curr_col = 0;
            } else {
                break; // End of history
            }
            
            // Check if found commit
            if let Some(next_chunk) = self.layout.chunks.get(curr_chunk_idx) {
                 if let Some(cell) = next_chunk.cells.get(lane).and_then(|row| row.get(curr_col)) {
                    if matches!(cell, crate::git::graph::CellType::Commit(_)) {
                        self.layout.selection.chunk_index = curr_chunk_idx;
                        self.layout.selection.column = curr_col;
                        break;
                    }
                 }
            }
        }
    }

    /// Move to previous lane (up)
    fn move_selection_up(&mut self) {
        if self.layout.selection.lane > 0 {
            self.layout.selection.lane -= 1;
            self.snap_to_nearest_commit();
        }
    }

    /// Move to next lane (down)
    fn move_selection_down(&mut self) {
        if let Some(chunk) = self.layout.chunks.get(self.layout.selection.chunk_index) {
            if self.layout.selection.lane < chunk.lane_count - 1 {
                self.layout.selection.lane += 1;
                self.snap_to_nearest_commit();
            }
        }
    }

    /// Helper: Snap selection to the nearest commit on the current lane
    /// Searches outwards from current column
    fn snap_to_nearest_commit(&mut self) {
        let lane = self.layout.selection.lane;
        let chunk_idx = self.layout.selection.chunk_index;
        let col = self.layout.selection.column;
        
        // check current
        if self.is_commit_at(chunk_idx, lane, col) { return; }
        
        // search radius
        let radius = 20; 
        for i in 1..=radius {
            // Check Left
            if col >= i {
                if self.is_commit_at(chunk_idx, lane, col - i) {
                    self.layout.selection.column = col - i;
                    return;
                }
            }
            // Check Right
            if self.is_commit_at(chunk_idx, lane, col + i) {
                 self.layout.selection.column = col + i;
                 return;
            }
        }
    }
    
    fn is_commit_at(&self, chunk_idx: usize, lane: usize, col: usize) -> bool {
        if let Some(chunk) = self.layout.chunks.get(chunk_idx) {
            if let Some(cell) = chunk.cells.get(lane).and_then(|row| row.get(col)) {
                return matches!(cell, crate::git::graph::CellType::Commit(_));
            }
        }
        false
    }
    
    /// Page up - previous chunk

    /// Page up - previous chunk
    fn prev_chunk(&mut self) {
        if self.layout.selection.chunk_index > 0 {
            self.layout.selection.chunk_index -= 1;
            self.layout.selection.column = 0;
        }
    }

    /// Page down - next chunk
    fn next_chunk(&mut self) {
        if self.layout.selection.chunk_index < self.layout.chunks.len() - 1 {
            self.layout.selection.chunk_index += 1;
            self.layout.selection.column = 0;
        }
    }

    /// Jump to first commit
    fn select_first(&mut self) {
        self.layout.selection.chunk_index = 0;
        self.layout.selection.column = 0;
        self.layout.selection.lane = 0;
    }

    /// Jump to last commit
    fn select_last(&mut self) {
        if let Some(last_chunk) = self.layout.chunks.last() {
            self.layout.selection.chunk_index = self.layout.chunks.len() - 1;
            self.layout.selection.column = last_chunk.end_column - last_chunk.start_column - 1;
            self.layout.selection.lane = 0;
        }
    }

    /// Get selected commit from current position
    fn selected_commit_from_layout(&self) -> Option<&CommitInfo> {
        let chunk = self.layout.chunks.get(self.layout.selection.chunk_index)?;
        let commit_col = &chunk.commits.get(self.layout.selection.lane)?;
        commit_col.get(self.layout.selection.column)?.as_ref()
    }
}

impl GraphLayoutStrategy for HorizontalLayoutStrategy {
    fn get_selection(&self) -> UnifiedSelection {
        UnifiedSelection::Horizontal {
            chunk_index: self.layout.selection.chunk_index,
            column: self.layout.selection.column,
            lane: self.layout.selection.lane,
        }
    }

    fn set_selection(&mut self, selection: UnifiedSelection) -> Result<()> {
        match selection {
            UnifiedSelection::Horizontal { chunk_index, column, lane } => {
                // Clamp values to valid ranges
                let max_chunk = self.layout.chunks.len().saturating_sub(1);
                let chunk_index = chunk_index.min(max_chunk);

                if let Some(chunk) = self.layout.chunks.get(chunk_index) {
                    let max_col = chunk.end_column - chunk.start_column;
                    let max_lane = chunk.lane_count;
                    self.layout.selection = HorizontalSelection {
                        chunk_index,
                        column: column.min(max_col.saturating_sub(1)),
                        lane: lane.min(max_lane.saturating_sub(1)),
                    };
                }
                Ok(())
            }
            UnifiedSelection::Vertical(_) => {
                // Convert vertical to horizontal by selecting first commit
                self.select_first();
                Ok(())
            }
        }
    }

    fn navigate(&mut self, direction: NavigationDirection) -> Result<()> {
        match direction {
            NavigationDirection::Previous => self.move_selection_left(),
            NavigationDirection::Next => self.move_selection_right(),
            NavigationDirection::PreviousLane => self.move_selection_up(),
            NavigationDirection::NextLane => self.move_selection_down(),
            NavigationDirection::First => self.select_first(),
            NavigationDirection::Last => self.select_last(),
            NavigationDirection::PageUp => self.prev_chunk(),
            NavigationDirection::PageDown => self.next_chunk(),
        }
        Ok(())
    }

    fn selected_commit(&self) -> Option<&CommitInfo> {
        self.selected_commit_from_layout()
    }

    fn selected_branch_name(&self) -> Option<&str> {
        let lane = &self.layout.lanes.get(self.layout.selection.lane)?;
        lane.branch_names.first().map(|s| s.as_str())
    }

    fn selected_branches(&self) -> Vec<&str> {
        if let Some(lane) = self.layout.lanes.get(self.layout.selection.lane) {
            lane.branch_names.iter().map(|s| s.as_str()).collect()
        } else {
            vec![]
        }
    }

    fn cached_diff(&self) -> Option<&CommitDiffInfo> {
        None
    }

    fn is_diff_loading(&self) -> bool {
        false
    }

    fn is_uncommitted_selected(&self) -> bool {
        // Check if current selection is on uncommitted changes
        self.selected_commit()
            .is_none()
    }

    fn selection_description(&self) -> String {
        if let Some(commit) = self.selected_commit() {
            let short_id = format!("{:.7}", commit.oid);
            let msg_first_line = commit.message.lines().next().unwrap_or("");
            return format!("{} {}", short_id, msg_first_line);
        }

        let chunk = self.current_chunk()?;
        format!(
            "Chunk {}/{} - Lane {}",
            chunk.index + 1,
            self.layout.chunks.len(),
            self.layout.selection.lane
        )
        .into()
    }

    fn handle_action(&mut self, action: &Action, app: &mut App) -> Result<()> {
        // Delegate most actions to the app's handler
        app.handle_action(action.clone())
    }

    fn refresh(&mut self, app: &mut App) -> Result<()> {
        if let Some(ref layout) = app.horizontal_layout {
            self.layout = layout.clone();
        }
        Ok(())
    }

    fn toggle_orientation(&mut self, app: &mut App) -> Result<()> {
        app.toggle_orientation();
        Ok(())
    }

    fn orientation_name(&self) -> &str {
        "horizontal"
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
    fn test_horizontal_layout_creation() {
        let commits = create_test_commits();
        let layout = HorizontalLayoutStrategy::new(&commits, &[], &[], None, None, 80);

        assert!(!layout.layout.chunks.is_empty());
        assert_eq!(layout.layout.selection.chunk_index, 0);
    }

    #[test]
    fn test_navigation() {
        let commits = create_test_commits();
        let mut layout = HorizontalLayoutStrategy::new(&commits, &[], &[], None, None, 80);

        // Test lane navigation
        layout.navigate(NavigationDirection::NextLane).unwrap();
        assert_eq!(layout.layout.selection.lane, 1);

        layout.navigate(NavigationDirection::PreviousLane).unwrap();
        assert_eq!(layout.layout.selection.lane, 0);

        // Test commit navigation
        let original_column = layout.layout.selection.column;
        layout.navigate(NavigationDirection::Next).unwrap();
        assert_eq!(layout.layout.selection.column, original_column + 1);

        layout.navigate(NavigationDirection::Previous).unwrap();
        assert_eq!(layout.layout.selection.column, original_column);
    }

    #[test]
    fn test_unified_selection() {
        let commits = create_test_commits();
        let mut layout = HorizontalLayoutStrategy::new(&commits, &[], &[], None, None, 80);

        let sel = layout.get_selection();
        assert!(matches!(sel, UnifiedSelection::Horizontal { .. }));

        layout.set_selection(UnifiedSelection::Horizontal {
            chunk_index: 0,
            column: 1,
            lane: 0,
        }).unwrap();
        assert_eq!(layout.layout.selection.column, 1);
    }

    #[test]
    fn test_selection_description() {
        let commits = create_test_commits();
        let layout = HorizontalLayoutStrategy::new(&commits, &[], &[], None, None, 80);

        let desc = layout.selection_description();
        // Either has commit info or chunk info
        assert!(!desc.is_empty());
    }

    #[test]
    fn test_first_last_navigation() {
        let commits = create_test_commits();
        let mut layout = HorizontalLayoutStrategy::new(&commits, &[], &[], None, None, 80);

        layout.navigate(NavigationDirection::First).unwrap();
        assert_eq!(layout.layout.selection.chunk_index, 0);
        assert_eq!(layout.layout.selection.column, 0);

        layout.navigate(NavigationDirection::Last).unwrap();
        // Should be at or near the end
        assert!(layout.layout.selection.chunk_index >= 0);
    }


    #[test]
    fn test_navigation_skips_empty() {
        use crate::git::graph::{HorizontalCellType, HorizontalChunk, HorizontalSelection};
        
        // Setup layout with gap: [C] [ ] [C]
        let mut strategy = HorizontalLayoutStrategy {
             layout: HorizontalGraphLayout {
                 chunks: vec![HorizontalChunk {
                     index: 0,
                     start_column: 0,
                     end_column: 3,
                     lane_count: 1,
                     cells: vec![vec![
                         HorizontalCellType::Commit(0),
                         HorizontalCellType::Empty,
                         HorizontalCellType::Commit(0)
                     ]],
                     commits: vec![vec![None, None, None]], // Content doesn't matter for nav logic
                     active_lanes: std::collections::HashSet::new(),
                     tags: vec![],
                 }],
                 lanes: vec![],
                 selection: HorizontalSelection { chunk_index: 0, column: 0, lane: 0 },
                 total_columns: 3,
                 main_lane: 0,
             },
             current_chunk: 0,
        };
        
        // Move Right
        strategy.navigate(NavigationDirection::Next).unwrap();
        // Should skip Col 1 and land on Col 2
        assert_eq!(strategy.layout.selection.column, 2, "Should skip empty column 1 and land on 2");
        
        // Move Left
        strategy.navigate(NavigationDirection::Previous).unwrap();
        // Should skip Col 1 and land on Col 0
        assert_eq!(strategy.layout.selection.column, 0, "Should skip empty column 1 and land on 0");
    }
}
