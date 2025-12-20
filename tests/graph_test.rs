//! Tests for the graph rendering algorithm

use chrono::Local;
use git2::Oid;
use keifu::git::{build_graph, graph::CellType, BranchInfo, CommitInfo};

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

fn render_cells(cells: &[CellType]) -> String {
    cells
        .iter()
        .map(|c| match c {
            CellType::Empty => ' ',
            CellType::Pipe(_) => '│',
            CellType::Commit(_) => '○',
            CellType::BranchRight(_) => '╭',
            CellType::BranchLeft(_) => '╮',
            CellType::MergeRight(_) => '╰',
            CellType::MergeLeft(_) => '╯',
            CellType::Horizontal(_) => '─',
            CellType::HorizontalPipe(_, _) => '┼',
            CellType::TeeRight(_) => '├',
            CellType::TeeLeft(_) => '┤',
            CellType::TeeUp(_) => '┴',
        })
        .collect()
}

fn get_short_id(node: &keifu::git::graph::GraphNode) -> String {
    node.commit
        .as_ref()
        .map(|c| c.short_id.clone())
        .unwrap_or_else(|| "(connector)".to_string())
}

#[test]
fn test_linear_history() {
    // C3 -> C2 -> C1
    let commits = vec![
        make_commit("c3", vec!["c2"]),
        make_commit("c2", vec!["c1"]),
        make_commit("c1", vec![]),
    ];
    let branches = vec![make_branch("main", "c3", true)];

    let layout = build_graph(&commits, &branches);

    println!("Linear history:");
    for node in &layout.nodes {
        println!("  {} -> {}", get_short_id(node), render_cells(&node.cells));
    }

    assert_eq!(layout.max_lane, 0);
    // All commits should be on lane 0
    for node in &layout.nodes {
        assert_eq!(node.lane, 0);
    }
}

#[test]
fn test_simple_branch_merge() {
    // C4 (merge) -> C3, C2
    // C3 -> C1
    // C2 -> C1
    // C1 (root)
    let commits = vec![
        make_commit("c4", vec!["c3", "c2"]), // merge commit
        make_commit("c3", vec!["c1"]),       // main branch
        make_commit("c2", vec!["c1"]),       // feature branch
        make_commit("c1", vec![]),           // root
    ];
    let branches = vec![
        make_branch("main", "c4", true),
        make_branch("feature", "c2", false),
    ];

    let layout = build_graph(&commits, &branches);

    println!("\nSimple branch merge:");
    for node in &layout.nodes {
        println!(
            "  {} lane={} -> {}",
            get_short_id(node),
            node.lane,
            render_cells(&node.cells)
        );
    }

    // Extract commit nodes only (exclude connector rows)
    let commit_nodes: Vec<_> = layout.nodes.iter().filter(|n| n.commit.is_some()).collect();

    // C4 should be in lane 0 with a branch to C2
    assert_eq!(commit_nodes[0].lane, 0); // C4
                                         // C3 should be in lane 0
    assert_eq!(commit_nodes[1].lane, 0); // C3
                                         // C2 should be in lane 1 (separate branch)
    assert_eq!(commit_nodes[2].lane, 1); // C2
                                         // C1 should be in lane 0
    assert_eq!(commit_nodes[3].lane, 0); // C1
}

#[test]
fn test_multiple_merges() {
    // C7 (merge) -> C6, C5
    // C6 -> C4
    // C5 -> C4
    // C4 (merge) -> C3, C2
    // C3 -> C1
    // C2 -> C1
    // C1 (root)
    let commits = vec![
        make_commit("c7", vec!["c6", "c5"]),
        make_commit("c6", vec!["c4"]),
        make_commit("c5", vec!["c4"]),
        make_commit("c4", vec!["c3", "c2"]),
        make_commit("c3", vec!["c1"]),
        make_commit("c2", vec!["c1"]),
        make_commit("c1", vec![]),
    ];
    let branches = vec![
        make_branch("main", "c7", true),
        make_branch("feature", "c5", false),
        make_branch("develop", "c2", false),
    ];

    let layout = build_graph(&commits, &branches);

    println!("\nMultiple merges:");
    for node in &layout.nodes {
        println!(
            "  {} lane={} -> '{}'",
            get_short_id(node),
            node.lane,
            render_cells(&node.cells)
        );
    }

    // Expected output:
    // C7 lane=0 -> '○─╭ '  (merge: C6 is 0, C5 is 1)
    // C6 lane=0 -> '○ │ '  (main continues, feature stays on 1)
    // C5 lane=1 -> '│─○ '  (feature, main pipe)
    // C4 lane=0 -> '○─╭ '  (merge: C3 is 0, C2 is 1)
    // C3 lane=0 -> '○ │ '  (main continues)
    // C2 lane=1 -> '│─○ '  (develop)
    // C1 lane=0 -> '○   '  (root)
}

#[test]
fn test_cell_structure() {
    // Inspect the cell structure of a simple merge in detail
    let commits = vec![
        make_commit("m1", vec!["a1", "b1"]), // merge
        make_commit("a1", vec!["r1"]),       // main
        make_commit("b1", vec!["r1"]),       // branch
        make_commit("r1", vec![]),           // root
    ];
    let branches = vec![make_branch("main", "m1", true)];

    let layout = build_graph(&commits, &branches);

    println!("\nCell structure analysis:");
    for node in &layout.nodes {
        println!("  {} cells: {:?}", get_short_id(node), node.cells);
    }

    // Check the cell structure for m1
    let m1_cells = &layout.nodes[0].cells;
    println!("  m1 rendered: '{}'", render_cells(m1_cells));

    // m1 is a commit on lane 0 with a branch line to lane 1
    // CellType stores color indices, so only validate the cell type
    assert!(
        matches!(m1_cells.first(), Some(CellType::Commit(_))),
        "m1 cell[0] should be Commit, got {:?}",
        m1_cells.first()
    );
}

#[test]
fn test_octopus_merge() {
    // Octopus merge (3+ parents)
    // M -> A, B, C
    // A -> R
    // B -> R
    // C -> R
    // R (root)
    let commits = vec![
        make_commit("M", vec!["A", "B", "C"]),
        make_commit("A", vec!["R"]),
        make_commit("B", vec!["R"]),
        make_commit("C", vec!["R"]),
        make_commit("R", vec![]),
    ];
    let branches = vec![
        make_branch("main", "M", true),
        make_branch("branch-b", "B", false),
        make_branch("branch-c", "C", false),
    ];

    let layout = build_graph(&commits, &branches);

    println!("\nOctopus merge:");
    for node in &layout.nodes {
        println!(
            "  {} lane={} -> '{}'",
            get_short_id(node),
            node.lane,
            render_cells(&node.cells)
        );
    }
}

#[test]
fn test_parallel_branches() {
    // Parallel branches
    // M2 (merge) -> A2, B2
    // A2 -> A1
    // B2 -> B1
    // A1 -> M1
    // B1 -> M1
    // M1 (merge) -> R, X
    // X -> R
    // R (root)
    let commits = vec![
        make_commit("M2", vec!["A2", "B2"]),
        make_commit("A2", vec!["A1"]),
        make_commit("B2", vec!["B1"]),
        make_commit("A1", vec!["M1"]),
        make_commit("B1", vec!["M1"]),
        make_commit("M1", vec!["R", "X"]),
        make_commit("X", vec!["R"]),
        make_commit("R", vec![]),
    ];
    let branches = vec![make_branch("main", "M2", true)];

    let layout = build_graph(&commits, &branches);

    println!("\nParallel branches:");
    for node in &layout.nodes {
        println!(
            "  {} lane={} -> '{}'",
            get_short_id(node),
            node.lane,
            render_cells(&node.cells)
        );
    }
}

#[test]
fn test_many_active_lanes() {
    // Multiple lanes active at once
    // HEAD -> M
    // M (merge) -> A, B, C, D
    // A -> R
    // B -> R
    // C -> R
    // D -> R
    // R (root)
    let commits = vec![
        make_commit("HEAD", vec!["M"]),
        make_commit("M", vec!["A", "B", "C", "D"]),
        make_commit("A", vec!["R"]),
        make_commit("B", vec!["R"]),
        make_commit("C", vec!["R"]),
        make_commit("D", vec!["R"]),
        make_commit("R", vec![]),
    ];
    let branches = vec![
        make_branch("main", "HEAD", true),
        make_branch("b", "B", false),
        make_branch("c", "C", false),
        make_branch("d", "D", false),
    ];

    let layout = build_graph(&commits, &branches);

    println!("\nMany active lanes:");
    for node in &layout.nodes {
        println!(
            "  {} lane={} -> '{}'",
            get_short_id(node),
            node.lane,
            render_cells(&node.cells)
        );
    }

    // max_lane should be at least 3 (4 branches merge)
    assert!(
        layout.max_lane >= 3,
        "Expected max_lane >= 3, got {}",
        layout.max_lane
    );
}
