//! Detailed visual test - shows the widget's actual rendering transformation

use chrono::Local;
use git2::Oid;
use keifu::git::{build_horizontal_graph, BranchInfo, CommitInfo};
use keifu::git::graph::HorizontalCellType;
use keifu::git::graph::CompressionMode;

fn make_oid(id: &str) -> Oid {
    let hash = format!(
        "{:0>40x}",
        id.bytes()
            .fold(0u128, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u128))
    );
    Oid::from_str(&hash[..40]).unwrap()
}

fn make_commit(id: &str, parents: Vec<&str>) -> CommitInfo {
    CommitInfo {
        oid: make_oid(id),
        short_id: id.to_string(),
        author_name: "test".to_string(),
        author_email: "test@example.com".to_string(),
        timestamp: Local::now(),
        message: format!("Commit {}", id),
        full_message: format!("Commit {}", id),
        parent_oids: parents.into_iter().map(make_oid).collect(),
    }
}

fn make_branch(name: &str, tip: &str, is_head: bool) -> BranchInfo {
    BranchInfo {
        name: name.to_string(),
        tip_oid: make_oid(tip),
        is_head,
        is_remote: false,
        upstream: None,
    }
}

/// This mimics the widget's cell_to_char transformation
fn widget_cell_to_char(cell: &HorizontalCellType, is_selected: bool) -> (char, &'static str) {
    let (ch, color_idx) = match cell {
        HorizontalCellType::Empty => (' ', 0),
        HorizontalCellType::Commit(ci, _) => ('●', *ci),
        HorizontalCellType::Pipe(ci) => ('│', *ci),
        HorizontalCellType::HLine(ci) => ('─', *ci),
        HorizontalCellType::JumpUp(ci) => ('╰', *ci),
        HorizontalCellType::JumpDown(ci) => ('╭', *ci),
        HorizontalCellType::HookUp(ci) => ('╯', *ci),
        HorizontalCellType::HookDown(ci) => ('╮', *ci),
        HorizontalCellType::TeeDown(ci) => ('┬', *ci),
        HorizontalCellType::TeeUp(ci) => ('┴', *ci),
        HorizontalCellType::TeeLeft(ci) => ('┤', *ci),
        HorizontalCellType::TeeRight(ci) => ('├', *ci),
        HorizontalCellType::Cross(_, pci) => ('┼', *pci),
        HorizontalCellType::Compressed(count, pci) => (char::from_digit((*count as u32) % 10, 10).unwrap_or('.'), *pci),
        HorizontalCellType::CornerTopLeft(pci) => ('┌', *pci),
    };

    let color_name = match color_idx {
        0 => "Gray",
        1 => "Red",
        2 => "Green",
        3 => "Yellow",
        4 => "Blue",
        5 => "Magenta",
        6 => "Cyan",
        7 => "White",
        _ => "Unknown",
    };

    if is_selected {
        ('●', "SELECTED")
    } else {
        (ch, color_name)
    }
}

fn print_chunk_detailed(chunk_index: usize, layout: &keifu::git::graph::HorizontalGraphLayout, selection: &keifu::git::graph::HorizontalSelection) {
    if let Some(chunk) = layout.chunks.get(chunk_index) {
        println!("=== Chunk {} (columns {}-{}, {} lanes) ===",
            chunk.index,
            chunk.start_column,
            chunk.end_column - 1,
            chunk.lane_count
        );

        // Print header with column numbers
        print!("    ");
        for col in 0..(chunk.end_column - chunk.start_column).min(20) {
            print!("{:3}", col);
        }
        println!();

        // Print each lane
        for lane in 0..chunk.lane_count.min(10) {
            print!("{:2}: ", lane);

            for col in 0..(chunk.end_column - chunk.start_column).min(20) {
                if let Some(cell) = chunk.cells.get(lane).and_then(|row| row.get(col)) {
                    let is_selected = selection.chunk_index == chunk.index
                        && selection.lane == lane
                        && selection.column == col;

                    let (ch, color) = widget_cell_to_char(cell, is_selected);
                    print!("{}[{}]", ch, color.chars().next().unwrap_or('?'));
                } else {
                    print!("   ");
                }
            }
            println!();
        }

        // Show commits by lane
        println!("\nCommits:");
        for lane in 0..chunk.lane_count.min(10) {
            let commits: Vec<_> = chunk.commits.get(lane)
                .into_iter()
                .flatten()
                .enumerate()
                .filter_map(|(col, c)| c.as_ref().map(|c| (col, c.short_id.clone())))
                .collect();

            if !commits.is_empty() {
                print!("  Lane {}: ", lane);
                for (col, id) in commits {
                    print!("[{}]{} ", col, id);
                }
                println!();
            }
        }
        println!();
    }
}

#[test]
fn test_detailed_visualize_simple_linear() {
    println!("\n{}", "=".repeat(60));
    println!("TEST: Simple Linear History (C3 -> C2 -> C1)");
    println!("{}\n", "=".repeat(60));

    let commits = vec![
        make_commit("c3", vec!["c2"]),
        make_commit("c2", vec!["c1"]),
        make_commit("c1", vec![]),
    ];
    let branches = vec![make_branch("main", "c3", true)];

    let layout = build_horizontal_graph(&commits, &branches, &[], None, None, 80, CompressionMode::default());

    println!("Total chunks: {}", layout.chunks.len());
    println!("Total columns: {}", layout.total_columns);
    println!("Initial selection: chunk={}, column={}, lane={}\n",
        layout.selection.chunk_index,
        layout.selection.column,
        layout.selection.lane
    );

    print_chunk_detailed(0, &layout, &layout.selection);
}

#[test]
fn test_detailed_visualize_branch_and_merge() {
    println!("\n{}", "=".repeat(60));
    println!("TEST: Branch and Merge");
    println!("    D -> C");
    println!("    E -> C");
    println!("    C -> B -> A");
    println!("{}\n", "=".repeat(60));

    //    D    E
    //     \  /
    //      C
    //      |
    //      B
    //      |
    //      A
    let commits = vec![
        make_commit("d", vec!["c"]),
        make_commit("e", vec!["c"]),
        make_commit("c", vec!["b"]),
        make_commit("b", vec!["a"]),
        make_commit("a", vec![]),
    ];
    let branches = vec![
        make_branch("main", "d", true),
        make_branch("feature", "e", false),
    ];

    let layout = build_horizontal_graph(&commits, &branches, &[], None, None, 80, CompressionMode::default());

    println!("Total chunks: {}", layout.chunks.len());
    println!("Total columns: {}", layout.total_columns);
    println!("Lanes:");
    for (i, lane) in layout.lanes.iter().enumerate() {
        println!("  Lane {}: {:?} (head: {})",
            i, lane.branch_names, lane.is_head);
    }
    println!();

    for i in 0..layout.chunks.len() {
        print_chunk_detailed(i, &layout, &layout.selection);
    }
}

#[test]
fn test_detailed_visualize_merge_commit() {
    println!("\n{}", "=".repeat(60));
    println!("TEST: Merge Commit (D merges C and B)");
    println!("{}\n", "=".repeat(60));

    // A <- B <- D (merge)
    //   \     /
    //    C <-/
    let commits = vec![
        make_commit("d", vec!["b", "c"]),  // merge commit
        make_commit("b", vec!["a"]),
        make_commit("c", vec!["a"]),
        make_commit("a", vec![]),
    ];
    let branches = vec![
        make_branch("main", "d", true),
        make_branch("feature", "c", false),
    ];

    let layout = build_horizontal_graph(&commits, &branches, &[], None, None, 80, CompressionMode::default());

    println!("Total chunks: {}", layout.chunks.len());
    println!("Total columns: {}", layout.total_columns);
    println!();

    for i in 0..layout.chunks.len() {
        print_chunk_detailed(i, &layout, &layout.selection);
    }
}
