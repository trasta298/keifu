//! Commit graph construction

use std::collections::HashMap;

use git2::Oid;

use super::{BranchInfo, CommitInfo};
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
    /// Start branch to the right ╭ (branch goes up-right)
    BranchRight(usize),
    /// Start branch to the left ╮ (branch goes up-left)
    BranchLeft(usize),
    /// Merge from the right ╰ (branch joins from down-right)
    MergeRight(usize),
    /// Merge from the left ╯ (branch joins from down-left)
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
}

/// Graph layout
#[derive(Debug, Clone)]
pub struct GraphLayout {
    pub nodes: Vec<GraphNode>,
    pub max_lane: usize,
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
    let mut parent_children: HashMap<Oid, Vec<Oid>> = HashMap::new();
    for commit in commits {
        if let Some(first_parent) = commit.parent_oids.first() {
            if oid_to_row.contains_key(first_parent) {
                parent_children
                    .entry(*first_parent)
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
        // (OID, lane, already tracked?, color index)
        let mut parent_lanes: Vec<(Oid, usize, bool, usize)> = Vec::new();
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
                    // Existing lane - use existing color
                    let color = oid_color_index.get(parent_oid).copied().unwrap_or(pl);
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

            parent_lanes.push((*parent_oid, parent_lane, was_existing, parent_color));
        }

        // Skip lane_merge for fork siblings
        let _ = is_fork_sibling; // Reserved for future use

        // Use fork sibling color if set
        let final_color_index = fork_sibling_color.unwrap_or(commit_color_index);

        // Update max_lane
        max_lane = max_lane.max(lane);
        for &(_, pl, _, _) in &parent_lanes {
            max_lane = max_lane.max(pl);
        }

        // Check whether lane merge is needed
        // If commit lane differs from parent lane and parent is already tracked
        // -> higher lane ends and merges into lower lane
        let lane_merge: Option<(usize, usize)> = parent_lanes
            .iter()
            .find(|(_, pl, was_existing, _)| *was_existing && *pl != lane)
            .map(|(_, pl, _, color)| (*pl, *color));

        // Build cells for this row (exclude lines to was_existing parents; rendered in connector row)
        let non_merging_parents: Vec<(Oid, usize, bool, usize)> = parent_lanes
            .iter()
            .filter(|(_, pl, was_existing, _)| !(*was_existing && *pl != lane))
            .copied()
            .collect();
        let cells = build_row_cells_with_colors(
            lane,
            final_color_index,
            &non_merging_parents,
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

        // Add a connector row after the commit row (when lanes merge)
        // Connector row comes after the last commit of the ending lane
        if let Some((parent_lane, _)) = lane_merge {
            // Lower lane is main (├), higher lane ends with merge (╯)
            let (main_lane, ending_lane) = if parent_lane < lane {
                (parent_lane, lane)
            } else {
                (lane, parent_lane)
            };

            let main_color = lanes
                .get(main_lane)
                .and_then(|o| *o)
                .and_then(|oid| oid_color_index.get(&oid).copied())
                .unwrap_or(main_lane);
            let ending_color = oid_color_index
                .get(&commit.oid)
                .copied()
                .unwrap_or(ending_lane);

            let connector_cells = build_connector_cells_with_colors(
                main_lane,
                main_color,
                &[(ending_lane, ending_color)],
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
                cells: connector_cells,
            });

            // Release the ending lane
            if ending_lane < lanes.len() {
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
                        let distance = if candidate_lane > head_lane {
                            candidate_lane - head_lane
                        } else {
                            head_lane - candidate_lane
                        };
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
            for i in 0..head_idx {
                let cell_idx = uncommitted_lane * 2;
                if nodes[i].cells[cell_idx] == CellType::Empty {
                    nodes[i].cells[cell_idx] = CellType::Pipe(UNCOMMITTED_COLOR_INDEX);
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

/// Build connector row cells (merge row) - color index version
fn build_connector_cells_with_colors(
    main_lane: usize,
    main_color: usize,
    merging_lanes: &[(usize, usize)], // (lane, color_index)
    active_lanes: &[Option<Oid>],
    oid_color_index: &HashMap<Oid, usize>,
    lane_color_index: &HashMap<usize, usize>,
    max_lane: usize,
) -> Vec<CellType> {
    let mut cells = vec![CellType::Empty; (max_lane + 1) * 2];

    // Draw a T junction on the main lane
    let main_cell_idx = main_lane * 2;
    if main_cell_idx < cells.len() {
        cells[main_cell_idx] = CellType::TeeRight(main_color);
    }

    // List of merging lane numbers
    let merging_lane_nums: Vec<usize> = merging_lanes.iter().map(|(l, _)| *l).collect();

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

    // Draw connectors to merging lanes
    for &(merge_lane, merge_color) in merging_lanes {
        // Horizontal line from main lane to merging lane
        for col in (main_lane * 2 + 1)..(merge_lane * 2) {
            if col < cells.len() {
                let existing = cells[col];
                if let CellType::Pipe(pl) = existing {
                    cells[col] = CellType::HorizontalPipe(merge_color, pl);
                } else if existing == CellType::Empty {
                    cells[col] = CellType::Horizontal(merge_color);
                }
            }
        }
        // End of merge lane
        let end_idx = merge_lane * 2;
        if end_idx < cells.len() {
            cells[end_idx] = CellType::MergeLeft(merge_color);
        }
    }

    cells
}

/// Build cells for one row - color index version
/// parent_lanes: (parent OID, lane, existing-tracked flag, color index)
fn build_row_cells_with_colors(
    commit_lane: usize,
    commit_color: usize,
    parent_lanes: &[(Oid, usize, bool, usize)],
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
    for &(_parent_oid, parent_lane, was_existing, parent_color) in parent_lanes.iter() {
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
                if was_existing {
                    // Already tracked: lane ends and merges ╯ (connect upward)
                    cells[end_idx] = CellType::MergeLeft(parent_color);
                } else {
                    // New assignment: lane continues ╮ (continue downward)
                    cells[end_idx] = CellType::BranchLeft(parent_color);
                }
            }
        } else {
            // Branch end: connect to the left lane (main line)
            // Horizontal line to the left from the commit position
            for col in (parent_lane * 2 + 1)..(commit_lane * 2) {
                if col < cells.len() {
                    let existing = cells[col];
                    if let CellType::Pipe(pl) = existing {
                        cells[col] = CellType::HorizontalPipe(commit_color, pl);
                    } else if existing == CellType::Empty {
                        cells[col] = CellType::Horizontal(commit_color);
                    }
                }
            }
            // Start marker
            let start_idx = parent_lane * 2;
            if start_idx < cells.len() {
                let existing = cells[start_idx];
                if let CellType::Pipe(pl) = existing {
                    // If a pipe exists, change to T junction (├)
                    cells[start_idx] = CellType::TeeRight(pl);
                } else if existing == CellType::Empty {
                    // If empty, use merge mark (╰)
                    cells[start_idx] = CellType::MergeRight(commit_color);
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
