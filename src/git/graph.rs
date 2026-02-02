//! Commit graph construction

use std::collections::{HashMap, HashSet};

use git2::Oid;

use super::{BranchInfo, CommitInfo, TagInfo};
use crate::graph::colors::{ColorAssigner, UNCOMMITTED_COLOR_INDEX};

/// Graph node
#[derive(Debug, Clone)]
pub struct GraphNode {
    /// Commit info (None means connector row only or uncommitted changes row)
    pub commit: Option<CommitInfo>,
    /// Lane position for this commit
    pub lane: usize,
    /// Color index for this node
    pub color_index: usize,
    /// Branch names pointing to this commit
    pub branch_names: Vec<String>,
    /// Whether HEAD points to this commit
    pub is_head: bool,
    /// Whether this is an uncommitted changes node
    pub is_uncommitted: bool,
    /// Number of uncommitted files (valid only when is_uncommitted is true)
    pub uncommitted_count: usize,
    /// Render info for this row
    pub cells: Vec<CellType>,
}

/// Cell types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellType {
    /// Empty
    Empty,
    /// Vertical line (active lane)
    Pipe(usize),
    /// Commit node
    Commit(usize),
    /// Start branch to the right (├ for horizontal layout)
    BranchRight(usize),
    /// Start branch to the left (┤ for horizontal layout)
    BranchLeft(usize),
    /// Merge from the right (┐ for horizontal layout)
    MergeRight(usize),
    /// Merge from the left (┘ for horizontal layout)
    MergeLeft(usize),
    /// Horizontal line
    Horizontal(usize),
    /// Horizontal line (lane crossing)
    HorizontalPipe(usize, usize), // (horizontal_lane, pipe_lane)
    /// T junction to the right ├
    TeeRight(usize),
    /// T junction to the left ┤
    TeeLeft(usize),
    /// Upward T junction (fork point) ┴
    TeeUp(usize),
    /// Downward T junction (fork point) ┬
    TeeDown(usize),
    /// Diagonal slash /
    Slash(usize),
    /// Diagonal backslash \
    Backslash(usize),
}

/// Graph orientation setting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GraphOrientation {
    #[default]
    Vertical,   // Current: commits top-to-bottom
    Horizontal, // New: commits left-to-right in chunks
}

/// Compression mode for horizontal graph
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompressionMode {
    #[default]
    Off,
    On,
    Short,
}

impl CompressionMode {
    pub fn next(&self) -> Self {
        match self {
            Self::Off => Self::On,
            Self::On => Self::Short,
            Self::Short => Self::Off,
        }
    }
}

/// A horizontal position in the graph
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HorizontalPosition {
    pub column: usize,  // Horizontal position (time)
    pub lane: usize,    // Vertical position (branch lane)
}

/// Selection within horizontal graph
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HorizontalSelection {
    pub chunk_index: usize,
    pub column: usize,  // Commit column within chunk
    pub lane: usize,    // Lane position
}

/// Cell types for Horizontal Layout (Orthogonal)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HorizontalCellType {
    Empty,
    Commit(usize),          // ●
    Pipe(usize),            // │
    HLine(usize),           // ─
    Compressed(usize, usize), // (count, color_index)
    CornerTopLeft(usize),   // ┌
    JumpUp(usize),          // ╰ Left-to-Up
    JumpDown(usize),        // ╭ Left-to-Down
    HookUp(usize),          // ╯ Right-to-Up
    HookDown(usize),        // ╮ Right-to-Down
    TeeDown(usize),         // ┬
    TeeUp(usize),           // ┴
    TeeLeft(usize),         // ┤
    TeeRight(usize),        // ├
    Cross(usize, usize),    // ┼ (v_color, h_color)
}

/// Position of tag relative to the graph lanes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagPosition {
    /// Display tag above the graph (for branches above main or alternating main tags)
    Top,
    /// Display tag below the graph (for branches below main or alternating main tags)
    Bottom,
}

/// Tag display information for a specific column
#[derive(Debug, Clone)]
pub struct TagDisplay {
    /// Tag name
    pub name: String,
    /// Column index where the tag connects
    pub column: usize,
    /// Whether to display on top or bottom
    pub position: TagPosition,
    /// Which lane the tagged commit is on
    pub lane: usize,
    /// Color index for the tag line
    pub color_index: usize,
}

/// A horizontal chunk (slice of the graph in time)
#[derive(Debug, Clone)]
pub struct HorizontalChunk {
    /// Chunk index (0 = newest)
    pub index: usize,
    /// Starting column index in the full graph
    pub start_column: usize,
    /// Ending column index (exclusive)
    pub end_column: usize,
    /// Number of lanes (rows) in this chunk
    pub lane_count: usize,
    /// Grid of cells: [lane][column] = cell at that position
    pub cells: Vec<Vec<HorizontalCellType>>,
    /// Commit data for each cell: [lane][column] = optional commit
    pub commits: Vec<Vec<Option<CommitInfo>>>,
    /// Which lanes have commits in this chunk
    pub active_lanes: HashSet<usize>,
    /// Tags to display in this chunk
    pub tags: Vec<TagDisplay>,
}

/// Complete horizontal graph layout
#[derive(Debug, Clone)]
pub struct HorizontalGraphLayout {
    /// All chunks ordered newest to oldest
    pub chunks: Vec<HorizontalChunk>,
    /// Lane info for legend display
    pub lanes: Vec<LaneInfo>,
    /// Currently selected position
    pub selection: HorizontalSelection,
    /// Total number of columns across all chunks
    pub total_columns: usize,
    /// Which lane is the "main" branch (for tag positioning)
    pub main_lane: usize,
}

/// Info about a lane for legend display
#[derive(Debug, Clone)]
pub struct LaneInfo {
    pub lane: usize,
    pub branch_names: Vec<String>,
    pub color_index: usize,
    pub is_head: bool,
    /// The index of the last chunk where this lane has content (for label positioning)
    pub last_chunk_index: Option<usize>,
}

/// Graph layout
#[derive(Debug, Clone)]
pub struct GraphLayout {
    pub nodes: Vec<GraphNode>,
    pub max_lane: usize,
}

/// Build a horizontal graph layout from commit list
pub fn build_horizontal_graph(
    commits: &[CommitInfo],
    branches: &[BranchInfo],
    tags: &[TagInfo],
    _uncommitted_count: Option<usize>,
    head_commit_oid: Option<Oid>,
    terminal_width: usize,
    compression_mode: CompressionMode,
) -> HorizontalGraphLayout {
    // Phase 1: Swimlane Assignment
    let (commit_lane_map, lane_colors, num_lanes, lane_branches) = assign_lanes(commits, branches);

    // Determine main_lane (lane containing main/master branch, or lane 0)
    let main_lane = branches.iter()
        .find(|b| b.name == "main" || b.name == "master")
        .and_then(|b| commit_lane_map.get(&b.tip_oid).copied())
        .unwrap_or(0);

    // Phase 2: Sparse Grid Construction & Routing
    let mut grid: HashMap<(usize, usize), HorizontalCellType> = HashMap::new();
    let mut commit_grid: HashMap<(usize, usize), CommitInfo> = HashMap::new();
    let mut col_has_node: HashSet<usize> = HashSet::new();

    // 2.0 Identify compressible commits
    // Build parent->children map
    let mut parent_children: HashMap<Oid, Vec<Oid>> = HashMap::new();
    let oid_set: HashSet<Oid> = commits.iter().map(|c| c.oid).collect();
    for commit in commits {
        for parent_oid in &commit.parent_oids {
            if oid_set.contains(parent_oid) {
                parent_children.entry(*parent_oid).or_default().push(commit.oid);
            }
        }
    }

    // Identify interesting commits (cannot be compressed)
    let mut interesting_oids: HashSet<Oid> = HashSet::new();
    // - Fork/Merge points
    for commit in commits {
        if commit.parent_oids.len() > 1 {
            interesting_oids.insert(commit.oid);
            for p in &commit.parent_oids { interesting_oids.insert(*p); } // Parents of merge often interesting? Maybe not strictly necessary if linear.
        }
        if let Some(children) = parent_children.get(&commit.oid) {
            if children.len() > 1 {
                interesting_oids.insert(commit.oid);
            }
        }
    }
    // - Branch tips
    for b in branches {
        interesting_oids.insert(b.tip_oid);
    }
    // - Tags
    for t in tags {
        interesting_oids.insert(t.target_oid);
    }
    // - HEAD
    if let Some(h) = head_commit_oid {
        interesting_oids.insert(h);
    }
    // - First and Last visible commits (to avoid dangling ends)
    if let Some(first) = commits.first() { interesting_oids.insert(first.oid); }
    if let Some(last) = commits.last() { interesting_oids.insert(last.oid); }

    // Helper to check if compressible
    let is_compressible = |oid: Oid| -> bool {
        !interesting_oids.contains(&oid) && compression_mode != CompressionMode::Off
    };

    // 2.1 Place Commits (Nodes) with Compression
    let mut commit_x_map: HashMap<Oid, usize> = HashMap::new();
    
    // We iterate Oldest -> Newest (commits.rev)
    let mut current_x = 0;
    
    // Iterate rev() but we need to identify groups. 
    // Groups are contiguous sequences in the rev() iteration.
    
    // We'll collect (Oid, Lane, Color) tuples to iterate
    let nodes: Vec<_> = commits.iter().rev().map(|c| {
        let lane = *commit_lane_map.get(&c.oid).unwrap_or(&0);
        let color = *lane_colors.get(&lane).unwrap_or(&0);
        (c, lane, color)
    }).collect();

    let mut idx = 0;
    while idx < nodes.len() {
        let (commit, lane, color) = nodes[idx];
        
        if is_compressible(commit.oid) {
            // Start of a potential compressed block
            // Look ahead to find how many compressible commits are in this sequence
            let mut count = 1;
            let mut j = idx + 1;
            while j < nodes.len() {
                let (next_c, next_lane, _) = nodes[j];
                // Must be compressible AND on the same lane to be grouped
                if is_compressible(next_c.oid) && next_lane == lane {
                    count += 1;
                    j += 1;
                } else {
                    break;
                }
            }
            
            // Check if we should compress this group
            // For Short mode, maybe we strictly collapse > 2? 
            // The user req says "ON mode: ...19..." or "SHORT: ...". 
            // Let's assume On/Short both compress any sequence >= 1?
            // "OFF mode: ●───●" (1 commit)
            // If I have A -> B -> C. B is compressible.
            // A -> ... -> C.
            // If I just have A->B (end). B is compressible? No, B is last commit, so interesting.
            
            // If count is small (e.g. 1), maybe don't compress?
            // But user might want to hide even single trivial commits.
            // Let's compress if count >= 1.
            
            // In Short mode, user asked "ON mode (can we support three modes: ON/OFF/SHORT?): ●───...19...───● or ●─...─●"
            // Let's interpret:
            // ON: Show count "19"
            // SHORT: Show just dot or shorter symbol? Or maybe "ON" uses more width?
            // For now, struct is same, rendering differs. Or cell type varies.
            
            let x = current_x;
            // Record map for all compressed commits
            for k in 0..count {
                let (c, _, _) = nodes[idx + k];
                commit_x_map.insert(c.oid, x);
                // We do NOT add to commit_grid for compressed nodes (they are not selectable/visible individually)
                // OR we do, but they overlap? If we add to commit_grid, selection might pick them up.
                // Better to NOT add keys to commit_grid for hidden commits.
            }
            
            grid.insert((lane, x), HorizontalCellType::Compressed(count, color));
            col_has_node.insert(x);
            
            // Advance
            idx += count;
        } else {
            // Normal visible commit
            let x = current_x;
            commit_x_map.insert(commit.oid, x);
            grid.insert((lane, x), HorizontalCellType::Commit(color));
            commit_grid.insert((lane, x), commit.clone());
            col_has_node.insert(x);
            idx += 1;
        }
        
        // Always space for connector after node (visible or compressed)
        current_x += 2;
    }

    // 2.2 Route Connections
    for (_idx, commit) in commits.iter().rev().enumerate() {
        let child_oid = commit.oid;
        
        // Skip routing if child is not in map (shouldn't happen)
        let child_x = match commit_x_map.get(&child_oid) {
            Some(&x) => x,
            None => continue,
        };
        let child_lane = *commit_lane_map.get(&child_oid).unwrap_or(&0);
        
        for parent_oid in &commit.parent_oids {
            if let Some(&parent_x) = commit_x_map.get(parent_oid) {
                 let parent_lane = *commit_lane_map.get(parent_oid).unwrap_or(&0);
                 
                 // If both are in same compressed block (same X, same lane), do NOT route
                 if child_x == parent_x && child_lane == parent_lane {
                     continue;
                 }
                 
                 route_connection(
                     parent_x, parent_lane,
                     child_x, child_lane,
                     *lane_colors.get(&parent_lane).unwrap_or(&0),
                     &mut grid,
                     &col_has_node
                 );
            }
        }
    }

    // Phase 3: Chunking
    chunk_grid(
        grid, 
        commit_grid, 
        num_lanes, 
        lane_branches, 
        lane_colors.clone(), 
        head_commit_oid, 
        &commit_lane_map, 
        &commit_x_map,
        commits, 
        tags,
        main_lane,
        terminal_width
    )
}

fn assign_lanes(
    commits: &[CommitInfo], 
    branches: &[BranchInfo]
) -> (HashMap<Oid, usize>, HashMap<usize, usize>, usize, Vec<Vec<String>>) {
    // Build OID -> row index and parent -> children mappings
    let oid_to_row: HashMap<Oid, usize> = commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.oid, i))
        .collect();
    
    // Build parent -> children map (like vertical graph does)
    let mut parent_children: HashMap<Oid, Vec<Oid>> = HashMap::new();
    for commit in commits {
        for parent_oid in &commit.parent_oids {
            if oid_to_row.contains_key(parent_oid) {
                parent_children
                    .entry(*parent_oid)
                    .or_default()
                    .push(commit.oid);
            }
        }
    }
    
    // Fork points: commits with 2+ children
    let _fork_points: HashSet<Oid> = parent_children
        .iter()
        .filter(|(_, children)| children.len() >= 2)
        .map(|(parent, _)| *parent)
        .collect();
    
    // Merge points: commits with 2+ parents
    let _merge_points: HashSet<Oid> = commits
        .iter()
        .filter(|c| c.parent_oids.iter().filter(|p| oid_to_row.contains_key(p)).count() >= 2)
        .map(|c| c.oid)
        .collect();
    
    // Lane tracking: process commits from newest to oldest (like vertical graph)
    // This mirrors the vertical graph's top-to-bottom approach
    let mut commit_lane_map: HashMap<Oid, usize> = HashMap::new();
    let mut lane_tracking: Vec<Option<Oid>> = Vec::new(); // What OID each lane is tracking
    let mut lane_branches: Vec<Vec<String>> = Vec::new();
    
    // Map branch tips to their branch names
    let mut tip_to_branch: HashMap<Oid, Vec<String>> = HashMap::new();
    for branch in branches {
        tip_to_branch
            .entry(branch.tip_oid)
            .or_default()
            .push(branch.name.clone());
    }
    
    // Sort branches: main/master first, then by name
    let mut sorted_branches: Vec<&BranchInfo> = branches.iter().collect();
    sorted_branches.sort_by(|a, b| {
        let a_is_main = a.name == "master" || a.name == "main";
        let b_is_main = b.name == "master" || b.name == "main";
        if a_is_main && !b_is_main { std::cmp::Ordering::Less }
        else if !a_is_main && b_is_main { std::cmp::Ordering::Greater }
        else { a.name.cmp(&b.name) }
    });
    
    // Process commits from newest to oldest (same order as input, which is topological)
    for commit in commits {
        // Check if any lane is tracking this commit's OID
        let tracking_lane = lane_tracking
            .iter()
            .position(|l| l.map(|oid| oid == commit.oid).unwrap_or(false));
        
        let lane = if let Some(l) = tracking_lane {
            l
        } else {
            // No lane tracking this commit, find an empty lane or create one
            let empty_lane = lane_tracking.iter().position(|l| l.is_none());
            if let Some(l) = empty_lane {
                l
            } else {
                lane_tracking.push(None);
                lane_branches.push(Vec::new());
                lane_tracking.len() - 1
            }
        };
        
        // Add branch names to this lane if it's a branch tip
        if let Some(branch_names) = tip_to_branch.get(&commit.oid) {
            if lane < lane_branches.len() {
                for name in branch_names {
                    if !lane_branches[lane].contains(name) {
                        lane_branches[lane].push(name.clone());
                    }
                }
            }
        }
        
        // Handle fork points (multiple lanes converging)
        // Check if multiple lanes are tracking this commit
        let tracking_lanes: Vec<usize> = lane_tracking
            .iter()
            .enumerate()
            .filter(|(_, l)| l.map(|oid| oid == commit.oid).unwrap_or(false))
            .map(|(i, _)| i)
            .collect();
        
        // If this is a fork point (multiple branches diverging from here),
        // release extra lanes after recording them
        if tracking_lanes.len() >= 2 {
            let main_lane = *tracking_lanes.iter().min().unwrap();
            for &l in &tracking_lanes {
                if l != main_lane {
                    lane_tracking[l] = None;
                }
            }
        }
        
        // Clear this lane's tracking
        if lane < lane_tracking.len() {
            lane_tracking[lane] = None;
        }
        
        // Record this commit's lane
        commit_lane_map.insert(commit.oid, lane);
        
        // Set up tracking for parent commits
        let valid_parents: Vec<Oid> = commit
            .parent_oids
            .iter()
            .filter(|oid| oid_to_row.contains_key(oid))
            .copied()
            .collect();
        
        for (parent_idx, parent_oid) in valid_parents.iter().enumerate() {
            // First parent continues on this lane
            if parent_idx == 0 {
                if lane < lane_tracking.len() {
                    lane_tracking[lane] = Some(*parent_oid);
                }
            } else {
                // Subsequent parents need new lanes (these are merge parents)
                // Check if already tracked
                let already_tracked = lane_tracking
                    .iter()
                    .any(|l| l.map(|oid| oid == *parent_oid).unwrap_or(false));
                
                if !already_tracked {
                    // Find or create a new lane
                    let empty_lane = lane_tracking.iter().position(|l| l.is_none());
                    let new_lane = if let Some(l) = empty_lane {
                        l
                    } else {
                        lane_tracking.push(None);
                        lane_branches.push(Vec::new());
                        lane_tracking.len() - 1
                    };
                    lane_tracking[new_lane] = Some(*parent_oid);
                }
            }
        }
    }
    
    let num_lanes = lane_branches.len().max(1);
    
    // Colors
    let mut lane_colors = HashMap::new();
    let mut color_assigner = ColorAssigner::new();
    for i in 0..num_lanes {
        lane_colors.insert(i, color_assigner.assign_color(i));
    }
    
    (commit_lane_map, lane_colors, num_lanes, lane_branches)
}

fn route_connection(
    p_x: usize, p_lane: usize,
    c_x: usize, c_lane: usize,
    color: usize,
    grid: &mut HashMap<(usize, usize), HorizontalCellType>,
    _col_has_node: &HashSet<usize>
) {
    // 1. Same Lane: Horizontal
    if p_lane == c_lane {
        for x in (p_x + 1)..c_x {
            place_cell(grid, p_lane, x, HorizontalCellType::HLine(color));
        }
        return;
    }
    
    // 2. Different Lane: Horizontal -> Vertical -> Horizontal
    // Turn point: p_x + 1
    let turn_x = p_x + 1;
    
    // If vertical segment is blocked by a node (Column Reservation Rule),
    // strictly speaking we should have shifted everything.
    // For MVP/first-pass: we rely on x = idx*2 spacing (every odd column is empty).
    // turn_x is odd (since p_x is even). So it should be empty of nodes.
    
    // Segment A: Horizontal from p_x to turn_x (single step usually)
    // Actually if turn_x == p_x + 1, we just draw the corner at turn_x?
    // No, corners occupy a cell.
    
    // Corner 1 at (p_lane, turn_x)
    // We arrive from Left. We leave Vertical.
    if c_lane > p_lane {
        // Going Down. Need Left-Down (╮ - HookDown)
        place_cell(grid, p_lane, turn_x, HorizontalCellType::HookDown(color));
    } else {
        // Going Up. Need Left-Up (╯ - HookUp)
        place_cell(grid, p_lane, turn_x, HorizontalCellType::HookUp(color));
    }
    
    // Segment B: Vertical
    let min_y = p_lane.min(c_lane) + 1;
    let max_y = p_lane.max(c_lane);
    
    for y in min_y..max_y {
        place_cell(grid, y, turn_x, HorizontalCellType::Pipe(color));
    }
    
    // Corner 2 at (c_lane, turn_x)
    // We arrive from Vertical. We leave Right.
    if c_lane > p_lane {
        // Coming from Top (Down). Turning Right. Need Up-Right (╰ - JumpUp)
        place_cell(grid, c_lane, turn_x, HorizontalCellType::JumpUp(color));
    } else {
        // Coming from Bottom (Up). Turning Right. Need Down-Right (╭ - JumpDown)
        place_cell(grid, c_lane, turn_x, HorizontalCellType::JumpDown(color));
    }
    
    // Segment C: Horizontal from turn_x to c_x
    for x in (turn_x + 1)..c_x {
        place_cell(grid, c_lane, x, HorizontalCellType::HLine(color));
    }
}

fn place_cell(grid: &mut HashMap<(usize, usize), HorizontalCellType>, lane: usize, col: usize, new_cell: HorizontalCellType) {
    // Collision Resolution
    let existing = grid.get(&(lane, col)).copied().unwrap_or(HorizontalCellType::Empty);
    
    let merged = match (existing, new_cell) {
        (HorizontalCellType::Empty, _) => new_cell,
        (_, HorizontalCellType::Empty) => existing,
        
        // CRITICAL: Never overwrite a Commit! Commits always win.
        (HorizontalCellType::Commit(c), _) => HorizontalCellType::Commit(c),
        (_, HorizontalCellType::Commit(c)) => HorizontalCellType::Commit(c),
        
        // Vertical + Horizontal = Cross
        (HorizontalCellType::Pipe(v), HorizontalCellType::HLine(h)) => HorizontalCellType::Cross(v, h),
        (HorizontalCellType::HLine(h), HorizontalCellType::Pipe(v)) => HorizontalCellType::Cross(v, h),
        
        // HLine + CornerDown (╭) -> TeeDown (┬)
        (HorizontalCellType::HLine(h), HorizontalCellType::JumpDown(_)) => HorizontalCellType::TeeDown(h), 
        (HorizontalCellType::JumpDown(_), HorizontalCellType::HLine(h)) => HorizontalCellType::TeeDown(h),
        
        // HLine + CornerUp (╰) -> TeeUp (┴)
        (HorizontalCellType::HLine(h), HorizontalCellType::JumpUp(_)) => HorizontalCellType::TeeUp(h),
        (HorizontalCellType::JumpUp(_), HorizontalCellType::HLine(h)) => HorizontalCellType::TeeUp(h),
        
        // HLine + HookDown (╮) -> TeeDown (┬)
        (HorizontalCellType::HLine(h), HorizontalCellType::HookDown(_)) => HorizontalCellType::TeeDown(h),
        (HorizontalCellType::HookDown(_), HorizontalCellType::HLine(h)) => HorizontalCellType::TeeDown(h),
        
        // HLine + HookUp (╯) -> TeeUp (┴)
        (HorizontalCellType::HLine(h), HorizontalCellType::HookUp(_)) => HorizontalCellType::TeeUp(h),
        (HorizontalCellType::HookUp(_), HorizontalCellType::HLine(h)) => HorizontalCellType::TeeUp(h),
        
        // Tee + Vertical Pipe -> Cross
        (HorizontalCellType::TeeDown(h), HorizontalCellType::Pipe(v)) => HorizontalCellType::Cross(v, h),
        (HorizontalCellType::TeeUp(h), HorizontalCellType::Pipe(v)) => HorizontalCellType::Cross(v, h),
        
        // Pipe + JumpUp (╰) -> TeeRight (├) - vertical pipe with branch going right
        // This happens when a vertical line passes through and another branch forks right
        (HorizontalCellType::Pipe(v), HorizontalCellType::JumpUp(_)) => HorizontalCellType::TeeRight(v),
        (HorizontalCellType::JumpUp(_), HorizontalCellType::Pipe(v)) => HorizontalCellType::TeeRight(v),
        
        // Pipe + JumpDown (╭) -> TeeRight (├) - vertical pipe with branch going right
        (HorizontalCellType::Pipe(v), HorizontalCellType::JumpDown(_)) => HorizontalCellType::TeeRight(v),
        (HorizontalCellType::JumpDown(_), HorizontalCellType::Pipe(v)) => HorizontalCellType::TeeRight(v),
        
        // Pipe + HookUp (╯) -> TeeLeft (┤) - vertical pipe with branch coming from left
        (HorizontalCellType::Pipe(v), HorizontalCellType::HookUp(_)) => HorizontalCellType::TeeLeft(v),
        (HorizontalCellType::HookUp(_), HorizontalCellType::Pipe(v)) => HorizontalCellType::TeeLeft(v),
        
        // Pipe + HookDown (╮) -> TeeLeft (┤) - vertical pipe with branch coming from left
        (HorizontalCellType::Pipe(v), HorizontalCellType::HookDown(_)) => HorizontalCellType::TeeLeft(v),
        (HorizontalCellType::HookDown(_), HorizontalCellType::Pipe(v)) => HorizontalCellType::TeeLeft(v),
        
        // TeeRight + corner combinations
        (HorizontalCellType::TeeRight(v), HorizontalCellType::JumpUp(_)) => HorizontalCellType::TeeRight(v),
        (HorizontalCellType::TeeRight(v), HorizontalCellType::JumpDown(_)) => HorizontalCellType::TeeRight(v),
        
        // Corner + Corner at same position (two branches from same point)
        // HookDown + HookDown -> keep existing (both going down from same horizontal)
        (HorizontalCellType::HookDown(c), HorizontalCellType::HookDown(_)) => HorizontalCellType::HookDown(c),
        (HorizontalCellType::HookUp(c), HorizontalCellType::HookUp(_)) => HorizontalCellType::HookUp(c),
        
        // Default: keep existing (safer than overwriting)
        _ => existing,
    };
    
    grid.insert((lane, col), merged);
}

fn chunk_grid(
    grid: HashMap<(usize, usize), HorizontalCellType>,
    commit_grid: HashMap<(usize, usize), CommitInfo>,
    num_lanes: usize,
    lane_branches: Vec<Vec<String>>,
    lane_colors: HashMap<usize, usize>,
    head_oid: Option<Oid>, 
    commit_lane_map: &HashMap<Oid, usize>,
    commit_x_map: &HashMap<Oid, usize>,
    _commits: &[CommitInfo],
    tags: &[TagInfo],
    main_lane: usize,
    terminal_width: usize
) -> HorizontalGraphLayout {
    // Build commit OID -> (lane, column) map for tag positioning
    let commit_positions: HashMap<Oid, (usize, usize)> = commit_x_map
        .iter()
        .filter_map(|(oid, &col)| {
            commit_lane_map.get(oid).map(|&lane| (*oid, (lane, col)))
        })
        .collect();
    

    
    // Calculate tag positions with alternation for main lane
    let tag_displays = determine_tag_positions(tags, &commit_positions, &lane_colors, main_lane);
    
    // Convert sparse grid to chunks
    // Determine max column
    let max_col = grid.keys().map(|(_, x)| *x).max().unwrap_or(0);
    let total_columns = max_col + 1;
    
    // Chunk size
    // The terminal_width passed is already just the graph panel width (legend is separate)
    // Only subtract 2 for the panel borders
    let graph_width = terminal_width.saturating_sub(2).max(10);
    let chars_per_col = 2;  // Compact: each column is 2 chars (symbol + connector)
    let cols_per_chunk = graph_width / chars_per_col;
    
    // Right-align: pad the left side so that max_col ends at a chunk boundary
    // This ensures the newest commits (after reversal) fill Chunk 1 completely
    let remainder = total_columns % cols_per_chunk;
    let pad_left = if remainder == 0 { 0 } else { cols_per_chunk - remainder };
    let total_columns_padded = total_columns + pad_left;
    
    let mut chunks = Vec::new();
    let mut chunk_idx = 0;
    
    for start_col in (0..total_columns_padded).step_by(cols_per_chunk) {
        let end_col = (start_col + cols_per_chunk).min(total_columns_padded);
        let width = end_col - start_col;
        
        let mut cell_matrix = vec![vec![HorizontalCellType::Empty; width]; num_lanes];
        let mut commit_matrix = vec![vec![None; width]; num_lanes];
        let mut active_lanes = HashSet::new();
        
        for lane in 0..num_lanes {
            for col in 0..width {
                let abs_col = start_col + col;
                // Original grid coordinates are 0-based, padded coordinates are shifted
                // Grid lookup needs to subtract pad_left to get original coordinate
                let orig_col = abs_col.saturating_sub(pad_left);
                
                // Only look up if we're past the padding
                if abs_col >= pad_left {
                    if let Some(&cell) = grid.get(&(lane, orig_col)) {
                        cell_matrix[lane][col] = cell;
                        active_lanes.insert(lane);
                    }
                    if let Some(c) = commit_grid.get(&(lane, orig_col)) {
                        commit_matrix[lane][col] = Some(c.clone());
                    }
                }
            }
        }
        
        // Find tags that belong to this chunk
        // Tags reference original column indices, so we need to check against orig_col range
        let orig_start = start_col.saturating_sub(pad_left);
        let orig_end = end_col.saturating_sub(pad_left);
        
        let chunk_tags: Vec<TagDisplay> = tag_displays
            .iter()
            .filter(|t| t.column >= orig_start && t.column < orig_end)
            .map(|t| TagDisplay {
                name: t.name.clone(),
                // Map grid column to cell array index:
                // The cell array index = grid_column - orig_start + pad_left
                // But chunk.start_column = start_col, and we iterate cells from 0 to width
                // where cells[col] corresponds to grid column (start_col + col - pad_left)
                // So: col = grid_column - orig_start + pad_left
                //        = t.column - (start_col - pad_left) + pad_left  
                //        = t.column - start_col + 2*pad_left
                // Actually simpler: the cells array covers [start_col..end_col)
                // cells[i] = grid[(lane, start_col + i - pad_left)] if i >= pad_left
                // We want: cells[?] to be at grid column t.column
                // So: start_col + ? - pad_left = t.column
                //     ? = t.column - start_col + pad_left
                // We use + pad_left first to avoid underflow when start_col > t.column (which shouldn't happen for valid tags, 
                // but start_col can be > t.column if we just subtracted, so careful with order)
                // Actually, t.column is grid index. start_col is padded index. 
                // The cell index is relative to start_col.
                column: (t.column + pad_left).saturating_sub(start_col),
                position: t.position,
                lane: t.lane,
                color_index: t.color_index,
            })
            .collect();
        
        chunks.push(HorizontalChunk {
            index: chunk_idx,
            start_column: start_col,
            end_column: end_col,
            lane_count: num_lanes,
            cells: cell_matrix,
            commits: commit_matrix,
            active_lanes,
            tags: chunk_tags,
        });
        chunk_idx += 1;
    }
    
    // Reverse chunks so newest commits appear first (Chunk 1 = newest)
    chunks.reverse();
    // Re-index the chunks
    for (new_idx, chunk) in chunks.iter_mut().enumerate() {
        chunk.index = new_idx;
    }
    
    // Build Lanes Info
    let mut lanes_info = Vec::new();
    for (i, names) in lane_branches.iter().enumerate() {
        // Is Head?
        let is_head = if let Some(h) = head_oid {
            commit_lane_map.get(&h) == Some(&i)
        } else { false };
        
        lanes_info.push(LaneInfo {
            lane: i,
            branch_names: names.clone(),
            color_index: *lane_colors.get(&i).unwrap_or(&0),
            is_head,
            last_chunk_index: None, // Todo: calc
        });
    }
    
    // Determine selection - newest commits are now in chunk 0
    let newest_col = commit_grid.keys().map(|(_, x)| *x).max().unwrap_or(0);
    // Find commit at newest_col
    let newest_lane = commit_grid.iter()
        .find(|&(&(_, x), _)| x == newest_col)
        .map(|(&(l, _), _)| l)
        .unwrap_or(0);
    
    // After reversal, newest commits are in chunk 0
    // Account for pad_left when calculating position in padded coordinate space
    let padded_col = newest_col + pad_left;
    let original_chunk_idx = padded_col / cols_per_chunk;
    let original_chunk_count = chunks.len();
    // After reversal: new_idx = (original_chunk_count - 1) - original_idx
    let new_chunk_idx = original_chunk_count.saturating_sub(1).saturating_sub(original_chunk_idx);
    let final_col_rel = padded_col % cols_per_chunk;

    HorizontalGraphLayout {
        chunks,
        lanes: lanes_info,
        selection: HorizontalSelection {
            chunk_index: new_chunk_idx,
            column: final_col_rel,
            lane: newest_lane,
        },
        total_columns,
        main_lane,
    }
}

/// Determine tag positions based on lane relative to main lane
/// - Commits on lanes ABOVE main (lane < main_lane): tags on TOP
/// - Commits on lanes BELOW main (lane > main_lane): tags on BOTTOM
/// - Commits on MAIN lane: alternate between TOP and BOTTOM
fn determine_tag_positions(
    tags: &[TagInfo],
    commit_positions: &HashMap<Oid, (usize, usize)>,
    lane_colors: &HashMap<usize, usize>,
    main_lane: usize,
) -> Vec<TagDisplay> {
    let mut result = Vec::new();
    let mut main_lane_alternator = false; // false = top, true = bottom
    
    // Collect tags with their commit positions
    let mut tagged_commits: Vec<(&TagInfo, usize, usize)> = tags
        .iter()
        .filter_map(|tag| {
            commit_positions.get(&tag.target_oid)
                .map(|&(lane, col)| (tag, lane, col))
        })
        .collect();
    
    // Sort by column (left to right) for consistent alternation
    tagged_commits.sort_by_key(|(_, _, col)| *col);
    
    for (tag, lane, column) in tagged_commits {
        let position = if lane < main_lane {
            // Branch above main -> tag on top
            TagPosition::Top
        } else if lane > main_lane {
            // Branch below main -> tag on bottom
            TagPosition::Bottom
        } else {
            // Main lane -> alternate
            let pos = if main_lane_alternator {
                TagPosition::Bottom
            } else {
                TagPosition::Top
            };
            main_lane_alternator = !main_lane_alternator;
            pos
        };
        
        result.push(TagDisplay {
            name: tag.name.clone(),
            column,
            position,
            lane,
            color_index: *lane_colors.get(&lane).unwrap_or(&0),
        });
    }
    
    result
}

/// Build a graph from commit list
/// uncommitted_count: Number of uncommitted files (None if no uncommitted changes)
/// head_commit_oid: The OID of the commit that HEAD points to (for uncommitted changes)
pub fn build_graph(
    commits: &[CommitInfo],
    branches: &[BranchInfo],
    uncommitted_count: Option<usize>,
    head_commit_oid: Option<Oid>,
) -> GraphLayout {
    if commits.is_empty() {
        return GraphLayout {
            nodes: Vec::new(),
            max_lane: 0,
        };
    }

    // OID -> branch name mapping
    let mut oid_to_branches: HashMap<Oid, Vec<String>> = HashMap::new();
    let mut head_oid: Option<Oid> = None;
    for branch in branches {
        oid_to_branches
            .entry(branch.tip_oid)
            .or_default()
            .push(branch.name.clone());
        if branch.is_head {
            head_oid = Some(branch.tip_oid);
        }
    }

    // OID -> row index mapping
    let oid_to_row: HashMap<Oid, usize> = commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.oid, i))
        .collect();

    // Detect fork points (commits with multiple children)
    // parent_oid -> list of child commits
    // Check ALL parents, not just first parent, to detect fork points like
    // hotfix branches that are merged into multiple release branches
    let mut parent_children: HashMap<Oid, Vec<Oid>> = HashMap::new();
    for commit in commits {
        for parent_oid in &commit.parent_oids {
            if oid_to_row.contains_key(parent_oid) {
                parent_children
                    .entry(*parent_oid)
                    .or_default()
                    .push(commit.oid);
            }
        }
    }
    // Fork points: commits with 2+ children
    let fork_points: std::collections::HashSet<Oid> = parent_children
        .iter()
        .filter(|(_, children)| children.len() >= 2)
        .map(|(parent, _)| *parent)
        .collect();

    // Lane tracking: OID tracked by each lane
    let mut lanes: Vec<Option<Oid>> = Vec::new();
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut max_lane: usize = 0;

    // Color management
    let mut color_assigner = ColorAssigner::new();
    // OID -> color index mapping
    let mut oid_color_index: HashMap<Oid, usize> = HashMap::new();
    // Lane -> color index mapping (keep colors during forks)
    let mut lane_color_index: HashMap<usize, usize> = HashMap::new();

    for commit in commits {
        // Start processing a new row
        color_assigner.advance_row();

        // Find the lane tracking this commit OID
        let commit_lane_opt = lanes
            .iter()
            .position(|l| l.map(|oid| oid == commit.oid).unwrap_or(false));

        // Determine the lane
        let lane = if let Some(l) = commit_lane_opt {
            l
        } else {
            // Find an empty lane or create one
            let empty = lanes.iter().position(|l| l.is_none());
            if let Some(l) = empty {
                l
            } else {
                lanes.push(None);
                lanes.len() - 1
            }
        };

        // Fork point handling: multiple lanes track this commit
        // Build fork connector and release extra lanes
        let fork_lanes: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(_, l)| l.map(|oid| oid == commit.oid).unwrap_or(false))
            .map(|(i, _)| i)
            .collect();

        if fork_lanes.len() >= 2 {
            // Use the smallest lane as main
            let main_lane = *fork_lanes.iter().min().unwrap();
            let merging_lanes: Vec<(usize, usize)> = fork_lanes
                .iter()
                .filter(|&&l| l != main_lane)
                .map(|&l| {
                    // Use lane color, else OID color, else lane index
                    let color = lane_color_index
                        .get(&l)
                        .copied()
                        .or_else(|| oid_color_index.get(&commit.oid).copied())
                        .unwrap_or(l);
                    (l, color)
                })
                .collect();

            // Update max_lane for fork connector
            for &(l, _) in &merging_lanes {
                max_lane = max_lane.max(l);
            }
            max_lane = max_lane.max(main_lane);

            let main_color = lane_color_index
                .get(&main_lane)
                .copied()
                .or_else(|| oid_color_index.get(&commit.oid).copied())
                .unwrap_or(main_lane);
            let fork_connector_cells = build_fork_connector_cells(
                main_lane,
                main_color,
                &merging_lanes,
                &lanes,
                &oid_color_index,
                &lane_color_index,
                max_lane,
            );
            nodes.push(GraphNode {
                commit: None,
                lane: main_lane,
                color_index: main_color,
                branch_names: Vec::new(),
                is_head: false,
                is_uncommitted: false,
                uncommitted_count: 0,
                cells: fork_connector_cells,
            });

            // Release merging lanes
            for &(l, _) in &merging_lanes {
                if l < lanes.len() {
                    lanes[l] = None;
                    color_assigner.release_lane(l);
                    lane_color_index.remove(&l);
                }
            }
        }

        // Determine color index
        let commit_color_index = if commit_lane_opt.is_some() {
            // Continue existing branch
            color_assigner.continue_lane(lane)
        } else if nodes.is_empty() {
            // First commit (main branch) - reserve color so others cannot use it
            color_assigner.assign_main_color(lane)
        } else {
            // New branch start - assign a new color (exclude reserved)
            color_assigner.assign_color(lane)
        };
        oid_color_index.insert(commit.oid, commit_color_index);
        // Record lane color (to preserve colors during forks)
        lane_color_index.insert(lane, commit_color_index);

        // Clear this commit lane
        if lane < lanes.len() {
            lanes[lane] = None;
        }

        // Process parent commits
        // (OID, lane, already tracked?, color index, already shown?)
        let mut parent_lanes: Vec<(Oid, usize, bool, usize, bool)> = Vec::new();
        let valid_parents: Vec<Oid> = commit
            .parent_oids
            .iter()
            .filter(|oid| oid_to_row.contains_key(oid))
            .copied()
            .collect();

        // Whether this is a fork sibling (parent is a fork point tracked on another lane)
        let mut is_fork_sibling = false;
        // Color for fork siblings (overrides commit_color_index)
        let mut fork_sibling_color: Option<usize> = None;

        // Start fork handling for merge commits (multiple parents)
        if valid_parents.len() >= 2 {
            color_assigner.begin_fork();
        }

        for (parent_idx, parent_oid) in valid_parents.iter().enumerate() {
            // Check if the parent is already in a lane
            let existing_parent_lane = lanes
                .iter()
                .position(|l| l.map(|oid| oid == *parent_oid).unwrap_or(false));

            // Check if parent commit has already been shown
            let parent_already_shown = nodes
                .iter()
                .any(|n| n.commit.as_ref().map(|c| c.oid) == Some(*parent_oid));

            let (parent_lane, was_existing, parent_color) = if let Some(pl) = existing_parent_lane {
                // If parent is a fork point, treat as fork sibling
                if parent_idx == 0 && fork_points.contains(parent_oid) {
                    // Track the parent on this lane as well (same OID on multiple lanes)
                    lanes[lane] = Some(*parent_oid);
                    is_fork_sibling = true;
                    // Keep main lane color, otherwise use commit_color_index
                    let color = if color_assigner.is_main_lane(lane) {
                        color_assigner.get_main_color()
                    } else {
                        // Use current commit color (do not assign new)
                        commit_color_index
                    };
                    fork_sibling_color = Some(color);
                    lane_color_index.insert(lane, color);
                    (lane, false, color)
                } else {
                    // Existing lane - use the lane's color (from lane_color_index)
                    let color = lane_color_index
                        .get(&pl)
                        .copied()
                        .or_else(|| oid_color_index.get(parent_oid).copied())
                        .unwrap_or(pl);
                    (pl, true, color)
                }
            } else if parent_idx == 0 {
                // First parent uses the same lane - inherit color
                lanes[lane] = Some(*parent_oid);
                oid_color_index.insert(*parent_oid, commit_color_index);
                (lane, false, commit_color_index)
            } else {
                // Subsequent parents use new lanes - assign fork sibling colors
                let empty = lanes.iter().position(|l| l.is_none());
                let new_lane = if let Some(l) = empty {
                    l
                } else {
                    lanes.push(None);
                    lanes.len() - 1
                };
                lanes[new_lane] = Some(*parent_oid);
                let new_color = color_assigner.assign_fork_sibling_color(new_lane);
                oid_color_index.insert(*parent_oid, new_color);
                lane_color_index.insert(new_lane, new_color);
                (new_lane, false, new_color)
            };

            // Include parent_already_shown flag for proper symbol selection
            parent_lanes.push((
                *parent_oid,
                parent_lane,
                was_existing,
                parent_color,
                parent_already_shown,
            ));
        }

        // Skip lane_merge for fork siblings
        let _ = is_fork_sibling; // Reserved for future use

        // Use fork sibling color if set
        let final_color_index = fork_sibling_color.unwrap_or(commit_color_index);

        // Update max_lane
        max_lane = max_lane.max(lane);
        for &(_, pl, _, _, _) in &parent_lanes {
            max_lane = max_lane.max(pl);
        }

        // Check whether lane merge is needed
        // If commit lane differs from parent lane and parent is already tracked
        // -> higher lane ends and merges into lower lane
        let lane_merge: Option<(usize, usize)> = parent_lanes
            .iter()
            .find(|(_, pl, was_existing, _, _)| *was_existing && *pl != lane)
            .map(|(_, pl, _, color, _)| (*pl, *color));

        // Build cells for this row
        // Include ALL parents to draw connections directly on the commit row
        let cells = build_row_cells_with_colors(
            lane,
            final_color_index,
            &parent_lanes,
            &lanes,
            &oid_color_index,
            &lane_color_index,
            max_lane,
        );

        // Get branch names
        let branch_names = oid_to_branches
            .get(&commit.oid)
            .cloned()
            .unwrap_or_default();

        let is_head = head_oid.map(|h| h == commit.oid).unwrap_or(false);

        // Add commit row
        nodes.push(GraphNode {
            commit: Some(commit.clone()),
            lane,
            color_index: final_color_index,
            branch_names,
            is_head,
            is_uncommitted: false,
            uncommitted_count: 0,
            cells,
        });

        // Handle lane merging: when a parent is already tracked on a different lane
        if let Some((parent_lane, _)) = lane_merge {
            // Lower lane is main, higher lane is ending
            let (main_lane, ending_lane) = if parent_lane < lane {
                (parent_lane, lane)
            } else {
                (lane, parent_lane)
            };

            // Check if the ending lane is tracking a commit that hasn't been shown yet
            let ending_lane_oid = lanes.get(ending_lane).and_then(|o| *o);
            let ending_oid_already_shown = ending_lane_oid
                .map(|oid| {
                    nodes
                        .iter()
                        .any(|n| n.commit.as_ref().map(|c| c.oid) == Some(oid))
                })
                .unwrap_or(true);

            let continues_down = !ending_oid_already_shown;

            // Release the ending lane only if:
            // 1. The first parent is NOT on the ending lane
            // 2. The OID on ending lane has already been shown (not continuing downward)
            if ending_lane < lanes.len() {
                let first_parent_on_ending_lane = parent_lanes
                    .first()
                    .map(|(_, pl, _, _, _)| *pl == ending_lane)
                    .unwrap_or(false);

                if !first_parent_on_ending_lane && !continues_down {
                    // Move the ending lane OID into the main lane
                    if let Some(oid) = lanes[ending_lane] {
                        if lanes.get(main_lane).map(|l| l.is_none()).unwrap_or(false) {
                            lanes[main_lane] = Some(oid);
                        }
                    }
                    lanes[ending_lane] = None;
                    color_assigner.release_lane(ending_lane);
                    lane_color_index.remove(&ending_lane);
                }
            }
        }
    }

    // Insert uncommitted changes node at the beginning if there are uncommitted changes
    if let Some(count) = uncommitted_count {
        // Find the node index that HEAD points to
        let head_node_idx = head_commit_oid.and_then(|oid| {
            nodes
                .iter()
                .position(|n| n.commit.as_ref().map(|c| c.oid) == Some(oid))
        });

        if let Some(head_idx) = head_node_idx {
            let head_lane = nodes[head_idx].lane;

            // Find an available lane for the uncommitted line
            // Check if head_lane is available for all nodes before HEAD
            let head_lane_available = (0..head_idx).all(|i| {
                let cell_idx = head_lane * 2;
                nodes[i]
                    .cells
                    .get(cell_idx)
                    .map(|c| *c == CellType::Empty)
                    .unwrap_or(true)
            });

            let uncommitted_lane = if head_lane_available {
                head_lane
            } else {
                // Find an available lane closest to head_lane
                let mut best_lane = max_lane + 1;
                let mut best_distance = usize::MAX;

                for candidate_lane in 0..=max_lane + 1 {
                    let available = (0..head_idx).all(|i| {
                        let cell_idx = candidate_lane * 2;
                        nodes[i]
                            .cells
                            .get(cell_idx)
                            .map(|c| *c == CellType::Empty)
                            .unwrap_or(true)
                    });
                    if available {
                        let distance = candidate_lane.abs_diff(head_lane);
                        if distance < best_distance {
                            best_distance = distance;
                            best_lane = candidate_lane;
                        }
                    }
                }
                best_lane
            };

            // Update max_lane if needed
            if uncommitted_lane > max_lane {
                max_lane = uncommitted_lane;
            }

            // Ensure all nodes have enough cells
            let required_cells = (max_lane + 1) * 2;
            for node in nodes.iter_mut() {
                while node.cells.len() < required_cells {
                    node.cells.push(CellType::Empty);
                }
            }

            // Add Pipe to all nodes before HEAD commit
            let cell_idx = uncommitted_lane * 2;
            for node in nodes.iter_mut().take(head_idx) {
                if node.cells[cell_idx] == CellType::Empty {
                    node.cells[cell_idx] = CellType::Pipe(UNCOMMITTED_COLOR_INDEX);
                }
            }

            // If uncommitted_lane != head_lane, add a connector from HEAD to the uncommitted lane
            if uncommitted_lane != head_lane {
                let head_cell_idx = head_lane * 2;
                let uncommitted_cell_idx = uncommitted_lane * 2;

                if uncommitted_lane > head_lane {
                    // Uncommitted lane is to the right - draw horizontal line and curve up (╯)
                    for col in (head_cell_idx + 1)..uncommitted_cell_idx {
                        if nodes[head_idx].cells[col] == CellType::Empty {
                            nodes[head_idx].cells[col] =
                                CellType::Horizontal(UNCOMMITTED_COLOR_INDEX);
                        }
                    }
                    nodes[head_idx].cells[uncommitted_cell_idx] =
                        CellType::MergeLeft(UNCOMMITTED_COLOR_INDEX);
                } else {
                    // Uncommitted lane is to the left - draw horizontal line and curve up (╰)
                    for col in (uncommitted_cell_idx + 1)..head_cell_idx {
                        if nodes[head_idx].cells[col] == CellType::Empty {
                            nodes[head_idx].cells[col] =
                                CellType::Horizontal(UNCOMMITTED_COLOR_INDEX);
                        }
                    }
                    nodes[head_idx].cells[uncommitted_cell_idx] =
                        CellType::MergeRight(UNCOMMITTED_COLOR_INDEX);
                }
            }

            // Build cells for the uncommitted node
            let mut cells = vec![CellType::Empty; required_cells];
            cells[uncommitted_lane * 2] = CellType::Commit(UNCOMMITTED_COLOR_INDEX);

            // Insert uncommitted node at the beginning
            nodes.insert(
                0,
                GraphNode {
                    commit: None,
                    lane: uncommitted_lane,
                    color_index: UNCOMMITTED_COLOR_INDEX,
                    branch_names: Vec::new(),
                    is_head: false,
                    is_uncommitted: true,
                    uncommitted_count: count,
                    cells,
                },
            );
        }
    }

    GraphLayout { nodes, max_lane }
}

/// Build cells for one row - color index version
/// parent_lanes: (parent OID, lane, existing-tracked flag, color index, already-shown flag)
fn build_row_cells_with_colors(
    commit_lane: usize,
    commit_color: usize,
    parent_lanes: &[(Oid, usize, bool, usize, bool)],
    active_lanes: &[Option<Oid>],
    oid_color_index: &HashMap<Oid, usize>,
    lane_color_index: &HashMap<usize, usize>,
    max_lane: usize,
) -> Vec<CellType> {
    let mut cells = vec![CellType::Empty; (max_lane + 1) * 2];

    // Draw vertical lines for active lanes
    for (lane_idx, lane_oid) in active_lanes.iter().enumerate() {
        if let Some(oid) = lane_oid {
            if lane_idx != commit_lane {
                let cell_idx = lane_idx * 2;
                if cell_idx < cells.len() {
                    // Prefer lane color, else OID color, else lane index
                    let color = lane_color_index
                        .get(&lane_idx)
                        .copied()
                        .or_else(|| oid_color_index.get(oid).copied())
                        .unwrap_or(lane_idx);
                    cells[cell_idx] = CellType::Pipe(color);
                }
            }
        }
    }

    // Draw commit node
    let commit_cell_idx = commit_lane * 2;
    if commit_cell_idx < cells.len() {
        cells[commit_cell_idx] = CellType::Commit(commit_color);
    }

    // Draw connections to parents
    for &(_parent_oid, parent_lane, was_existing, parent_color, already_shown) in
        parent_lanes.iter()
    {
        if parent_lane == commit_lane {
            // Same lane - only a vertical line (drawn on next row)
            continue;
        }

        // Connection to a different lane
        if parent_lane > commit_lane {
            // Connection to a lane on the right
            // Horizontal line to the right from the commit position
            for col in (commit_lane * 2 + 1)..(parent_lane * 2) {
                if col < cells.len() {
                    let existing = cells[col];
                    if let CellType::Pipe(pl) = existing {
                        cells[col] = CellType::HorizontalPipe(parent_color, pl);
                    } else if existing == CellType::Empty {
                        cells[col] = CellType::Horizontal(parent_color);
                    }
                }
            }
            // End marker
            let end_idx = parent_lane * 2;
            if end_idx < cells.len() {
                if was_existing && already_shown {
                    // Parent already shown: lane ends and merges ╯ (connect upward)
                    cells[end_idx] = CellType::MergeLeft(parent_color);
                } else if was_existing {
                    // Parent not yet shown but lane exists: ┤ (T-junction, line continues down)
                    cells[end_idx] = CellType::TeeLeft(parent_color);
                } else {
                    // New lane for parent: ╮ (branch starts here, continues down)
                    cells[end_idx] = CellType::BranchLeft(parent_color);
                }
            }
        } else {
            // Branch end: connect to the left lane (main line)
            // Horizontal line to the left from the commit position
            // Use the parent's color for the connection line
            for col in (parent_lane * 2 + 1)..(commit_lane * 2) {
                if col < cells.len() {
                    let existing = cells[col];
                    if let CellType::Pipe(pl) = existing {
                        cells[col] = CellType::HorizontalPipe(parent_color, pl);
                    } else if existing == CellType::Empty {
                        cells[col] = CellType::Horizontal(parent_color);
                    }
                }
            }
            // Start marker
            let start_idx = parent_lane * 2;
            if start_idx < cells.len() {
                if was_existing && already_shown {
                    // Parent already shown: lane ends and merges ╰ (connect upward)
                    cells[start_idx] = CellType::MergeRight(parent_color);
                } else if was_existing {
                    // Parent not yet shown but lane exists: ├ (T-junction, line continues down)
                    cells[start_idx] = CellType::TeeRight(parent_color);
                } else {
                    // New lane for parent: ╭ (branch starts here, continues down)
                    cells[start_idx] = CellType::BranchRight(parent_color);
                }
            }
        }
    }

    cells
}

/// Build fork connector row cells (multiple branches from the same parent)
/// Example: ├─┴─╯ (main lane connecting to multiple branch lanes)
fn build_fork_connector_cells(
    main_lane: usize,
    main_color: usize,
    merging_lanes: &[(usize, usize)], // (lane, color_index)
    active_lanes: &[Option<Oid>],
    oid_color_index: &HashMap<Oid, usize>,
    lane_color_index: &HashMap<usize, usize>,
    max_lane: usize,
) -> Vec<CellType> {
    let mut cells = vec![CellType::Empty; (max_lane + 1) * 2];

    // Sorted list of merging lane numbers
    let mut merging_lane_nums: Vec<usize> = merging_lanes.iter().map(|(l, _)| *l).collect();
    merging_lane_nums.sort();

    // Draw a T junction on the main lane
    let main_cell_idx = main_lane * 2;
    if main_cell_idx < cells.len() {
        cells[main_cell_idx] = CellType::TeeRight(main_color);
    }

    // Draw vertical lines for active lanes (except main and merging lanes)
    for (lane_idx, lane_oid) in active_lanes.iter().enumerate() {
        if let Some(oid) = lane_oid {
            if lane_idx != main_lane && !merging_lane_nums.contains(&lane_idx) {
                let cell_idx = lane_idx * 2;
                if cell_idx < cells.len() {
                    let color = lane_color_index
                        .get(&lane_idx)
                        .copied()
                        .or_else(|| oid_color_index.get(oid).copied())
                        .unwrap_or(lane_idx);
                    cells[cell_idx] = CellType::Pipe(color);
                }
            }
        }
    }

    // Rightmost merging lane
    let rightmost_lane = *merging_lane_nums.last().unwrap_or(&main_lane);

    // Draw connectors to merging lanes
    for &(merge_lane, merge_color) in merging_lanes {
        // Horizontal line from main lane to merging lane
        for col in (main_lane * 2 + 1)..(merge_lane * 2) {
            if col < cells.len() {
                let existing = cells[col];
                if let CellType::Pipe(pl) = existing {
                    cells[col] = CellType::HorizontalPipe(merge_color, pl);
                } else if matches!(existing, CellType::Empty | CellType::Horizontal(_)) {
                    cells[col] = CellType::Horizontal(merge_color);
                }
            }
        }

        // End of merge lane
        let end_idx = merge_lane * 2;
        if end_idx < cells.len() {
            if merge_lane == rightmost_lane {
                // Rightmost lane uses ╯
                cells[end_idx] = CellType::MergeLeft(merge_color);
            } else {
                // Middle lanes use ┴
                cells[end_idx] = CellType::TeeUp(merge_color);
            }
        }
    }

    cells
}
/// Get the color index for a given lane from the layout
/// This is a helper function for the horizontal graph view
pub fn color_index_for_lane(
    layout: &HorizontalGraphLayout,
    lane: usize,
) -> usize {
    layout
        .lanes
        .get(lane)
        .map(|l| l.color_index)
        .unwrap_or(0)
}
#[cfg(test)]
mod tests {
    use super::*;
    use git2::Oid;
    use chrono::Local;
    use std::str::FromStr;

    fn create_mock_commit(oid_str: &str, parents: Vec<&str>, message: &str) -> CommitInfo {
        let oid = Oid::from_str(oid_str).unwrap();
        let parent_oids = parents.iter().map(|p| Oid::from_str(p).unwrap()).collect();
        CommitInfo {
            oid,
            short_id: oid_str[..7].to_string(),
            author_name: "Test".to_string(),
            author_email: "test@example.com".to_string(),
            timestamp: Local::now(),
            message: message.to_string(),
            full_message: message.to_string(),
            parent_oids,
        }
    }

    fn create_mock_branch(name: &str, tip: &str, is_head: bool) -> BranchInfo {
        BranchInfo {
            name: name.to_string(),
            is_head,
            is_remote: false,
            upstream: None,
            tip_oid: Oid::from_str(tip).unwrap(),
        }
    }
    
    fn layout_to_ascii(layout: &HorizontalGraphLayout) -> String {
        let mut lines = Vec::new();
        if layout.chunks.is_empty() { return String::new(); }

        for lane in 0..layout.lanes.len() {
            let mut line = String::new();
            for chunk in &layout.chunks {
                 if lane < chunk.cells.len() {
                     for cell in &chunk.cells[lane] {
                         let ch = match cell {
                             HorizontalCellType::Empty => ' ',
                             HorizontalCellType::Commit(_) => '●',
                             HorizontalCellType::Pipe(_) => '│',
                             HorizontalCellType::HLine(_) => '─',
                             HorizontalCellType::JumpUp(_) => '╰',
                             HorizontalCellType::JumpDown(_) => '╭',
                             HorizontalCellType::HookUp(_) => '╯',
                             HorizontalCellType::HookDown(_) => '╮',
                             HorizontalCellType::TeeDown(_) => '┬',
                             HorizontalCellType::TeeUp(_) => '┴',
                             HorizontalCellType::TeeLeft(_) => '┤',
                             HorizontalCellType::TeeRight(_) => '├',
                             HorizontalCellType::Cross(_, _) => '┼',
                             HorizontalCellType::CornerTopLeft(_) => '┌',
                             HorizontalCellType::Compressed(c, _) => if *c > 9 { '+' } else { char::from_digit((*c) as u32, 10).unwrap() },
                             _ => '?', 
                         };
                         line.push(ch);
                     }
                 }
            }
            lines.push(line);
        }
        lines.join("\n")
    }

    #[test]
    fn test_horizontal_graph_linear() {
        let c1 = create_mock_commit("1111111111111111111111111111111111111111", vec![], "Initial");
        let c2 = create_mock_commit("2222222222222222222222222222222222222222", vec!["1111111111111111111111111111111111111111"], "Second");
        let c3 = create_mock_commit("3333333333333333333333333333333333333333", vec!["2222222222222222222222222222222222222222"], "Third");
        
        let commits = vec![c3.clone(), c2.clone(), c1.clone()];
        let branches = vec![create_mock_branch("master", "3333333333333333333333333333333333333333", true)];
        
        let layout = build_horizontal_graph(&commits, &branches, &[], None, Some(c3.oid), 80, CompressionMode::default());
        
        let ascii = layout_to_ascii(&layout);
        assert_eq!(ascii.trim_start(), "●─●─●");
        
        assert_eq!(layout.chunks.len(), 1);
        assert_eq!(layout.lanes.len(), 1);
    }

    #[test]
    fn test_horizontal_graph_topology() {
        let oid_c1 = "1111111111111111111111111111111111111111"; // Oldest
        let oid_c2 = "2222222222222222222222222222222222222222";
        let oid_c3 = "3333333333333333333333333333333333333333";
        let oid_c4 = "4444444444444444444444444444444444444444"; // Newest
        
        let c1 = create_mock_commit(oid_c1, vec![], "C1");
        let c2 = create_mock_commit(oid_c2, vec![oid_c1], "C2");
        let c3 = create_mock_commit(oid_c3, vec![oid_c1], "C3");
        let c4 = create_mock_commit(oid_c4, vec![oid_c3, oid_c2], "C4 (Merge)");
        
        let commits = vec![c4.clone(), c3.clone(), c2.clone(), c1.clone()];
        let branches = vec![
            create_mock_branch("main", oid_c4, true),
            create_mock_branch("feature", oid_c2, false),
        ];

        let layout = build_horizontal_graph(&commits, &branches, &[], None, Some(c4.oid), 80, CompressionMode::default());
        
        let ascii = layout_to_ascii(&layout);
        let expected_lines = vec![
            "●┬─┬●─●",
            "╰●╯" 
        ];
        
        let actual_lines: Vec<&str> = ascii.lines().collect();
        assert_eq!(actual_lines.len(), 2);
        assert_eq!(actual_lines[0].trim(), expected_lines[0]);
        assert_eq!(actual_lines[1].trim(), expected_lines[1]);
    }

    #[test]
    fn test_horizontal_graph_compression() {
        let c1 = create_mock_commit("1111111111111111111111111111111111111111", vec![], "Initial");
        let c2 = create_mock_commit("2222222222222222222222222222222222222222", vec!["1111111111111111111111111111111111111111"], "Second");
        let c3 = create_mock_commit("3333333333333333333333333333333333333333", vec!["2222222222222222222222222222222222222222"], "Third");
        let c4 = create_mock_commit("4444444444444444444444444444444444444444", vec!["3333333333333333333333333333333333333333"], "Fourth");
        let c5 = create_mock_commit("5555555555555555555555555555555555555555", vec!["4444444444444444444444444444444444444444"], "Fifth");

        let commits = vec![c5.clone(), c4.clone(), c3.clone(), c2.clone(), c1.clone()];
        let branches = vec![create_mock_branch("main", c5.oid.to_string().as_str(), true)];

        // Compression ON: 3 linear commits (C2, C3, C4) should be compressed
        let layout_on = build_horizontal_graph(&commits, &branches, &[], None, Some(c5.oid), 80, CompressionMode::On);
        let ascii_on = layout_to_ascii(&layout_on);
        
        // Expected: C1(●) ─ 3 ─ C5(●)
        assert_eq!(ascii_on.trim_start(), "●─3─●");

        // Compression OFF: all visible
        let layout_off = build_horizontal_graph(&commits, &branches, &[], None, Some(c5.oid), 80, CompressionMode::Off);
        let ascii_off = layout_to_ascii(&layout_off);
        // ●─●─●─●─●
        assert_eq!(ascii_off.trim_start(), "●─●─●─●─●");
    }
}
