//! Visual test for horizontal graph rendering
//! This test prints the actual graph visualization to see what it looks like

use chrono::Local;
use git2::Oid;
use keifu::git::{build_horizontal_graph, BranchInfo, CommitInfo};
use keifu::git::graph::HorizontalCellType;
use keifu::git::graph::CompressionMode;

fn make_oid(id: &str) -> Oid {
    // Convert id into a 40-char hex hash
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

fn cell_to_char(cell: &HorizontalCellType) -> char {
    match cell {
        HorizontalCellType::Empty => ' ',
        HorizontalCellType::Commit(_, _, _) => '●',
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
        HorizontalCellType::Compressed(count, _) => char::from_digit((*count as u32) % 10, 10).unwrap_or('.'),
        HorizontalCellType::CornerTopLeft(_) => '┌',
    }
}

fn print_chunk(chunk_index: usize, layout: &keifu::git::graph::HorizontalGraphLayout) {
    if let Some(chunk) = layout.chunks.get(chunk_index) {
        println!("=== Chunk {} (columns {}-{}, {} lanes) ===",
            chunk.index,
            chunk.start_column,
            chunk.end_column - 1,
            chunk.lane_count
        );

        // Print header with column numbers
        print!("   ");
        for col in 0..(chunk.end_column - chunk.start_column) {
            print!("{:2}", col);
        }
        println!();

        // Print each lane
        for lane in 0..chunk.lane_count {
            print!("{:2}: ", lane);
            for col in 0..(chunk.end_column - chunk.start_column) {
                if let Some(cell) = chunk.cells.get(lane).and_then(|row| row.get(col)) {
                    let ch = cell_to_char(cell);
                    print!(" {} ", ch);
                } else {
                    print!("   ");
                }
            }
            println!();
        }
        println!();
    }
}

#[test]
fn test_visualize_simple_linear() {
    println!("\n=== TEST: Simple Linear History (C3 -> C2 -> C1) ===\n");
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

    for i in 0..layout.chunks.len() {
        print_chunk(i, &layout);
    }
}

#[test]
fn test_visualize_branch_and_merge() {
    println!("\n=== TEST: Branch and Merge (D -> C, E -> C, C -> B -> A) ===\n");
    //    A <- B <- C <- D
    //              \
    //               E
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
    println!("Lanes: {:?}", layout.lanes.iter().map(|l| &l.branches).collect::<Vec<_>>());
    println!();

    for i in 0..layout.chunks.len() {
        print_chunk(i, &layout);
    }
}

#[test]
fn test_visualize_multiple_branches() {
    println!("\n=== TEST: Multiple Branches ===\n");
    // A <- B <- D
    //   \     /
    //    C <- E
    let commits = vec![
        make_commit("d", vec!["b", "c"]),
        make_commit("e", vec!["c"]),
        make_commit("b", vec!["a"]),
        make_commit("c", vec!["a"]),
        make_commit("a", vec![]),
    ];
    let branches = vec![
        make_branch("main", "d", true),
        make_branch("dev", "e", false),
    ];

    let layout = build_horizontal_graph(&commits, &branches, &[], None, None, 80, CompressionMode::default());

    println!("Total chunks: {}", layout.chunks.len());
    println!("Total columns: {}", layout.total_columns);
    println!();

    for i in 0..layout.chunks.len() {
        print_chunk(i, &layout);
    }
}

#[test]
fn test_visualize_narrow_terminal() {
    println!("\n=== TEST: Narrow Terminal (width=12, 4 commits per chunk) ===\n");
    let commits = vec![
        make_commit("c10", vec!["c9"]),
        make_commit("c9", vec!["c8"]),
        make_commit("c8", vec!["c7"]),
        make_commit("c7", vec!["c6"]),
        make_commit("c6", vec!["c5"]),
        make_commit("c5", vec!["c4"]),
        make_commit("c4", vec!["c3"]),
        make_commit("c3", vec!["c2"]),
        make_commit("c2", vec!["c1"]),
        make_commit("c1", vec![]),
    ];
    let branches = vec![make_branch("main", "c10", true)];

    // Narrow terminal should create multiple chunks
    let layout = build_horizontal_graph(&commits, &branches, &[], None, None, 12, CompressionMode::default());

    println!("Total chunks: {}", layout.chunks.len());
    println!("Total columns: {}", layout.total_columns);
    println!();

    for i in 0..layout.chunks.len() {
        print_chunk(i, &layout);
    }
}
