//! グラフ描画アルゴリズムのテスト

use chrono::Local;
use git2::Oid;
use git_graph_tui::git::{build_graph, graph::CellType, BranchInfo, CommitInfo};

fn make_oid(id: &str) -> Oid {
    // idをハッシュに変換して40文字の16進数を生成
    let hash = format!("{:0>40x}", id.bytes().fold(0u128, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u128)));
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
        parent_oids: parents.into_iter().map(|p| make_oid(p)).collect(),
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
        })
        .collect()
}

fn get_short_id(node: &git_graph_tui::git::graph::GraphNode) -> String {
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
    // 全てのコミットがレーン0にあるべき
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

    // コミットノードのみを抽出（接続行を除外）
    let commit_nodes: Vec<_> = layout.nodes.iter().filter(|n| n.commit.is_some()).collect();

    // C4はレーン0にあり、C2への分岐がある
    assert_eq!(commit_nodes[0].lane, 0); // C4
    // C3はレーン0
    assert_eq!(commit_nodes[1].lane, 0); // C3
    // C2はレーン1（別ブランチ）
    assert_eq!(commit_nodes[2].lane, 1); // C2
    // C1はレーン0
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

    // 期待される出力:
    // C7 lane=0 -> '○─╭ '  (マージ: C6が0、C5が1)
    // C6 lane=0 -> '○ │ '  (mainの続き、featureが1で継続)
    // C5 lane=1 -> '│─○ '  (feature、mainのパイプ)
    // C4 lane=0 -> '○─╭ '  (マージ: C3が0、C2が1)
    // C3 lane=0 -> '○ │ '  (mainの続き)
    // C2 lane=1 -> '│─○ '  (develop)
    // C1 lane=0 -> '○   '  (root)
}

#[test]
fn test_cell_structure() {
    // シンプルなマージのセル構造を詳細に確認
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

    // m1のセル構造を確認
    let m1_cells = &layout.nodes[0].cells;
    println!("  m1 rendered: '{}'", render_cells(m1_cells));

    // m1はレーン0でコミット、レーン1への分岐線があるはず
    // CellType内の値はカラーインデックスなので、セルタイプのみを検証
    assert!(
        matches!(m1_cells.get(0), Some(CellType::Commit(_))),
        "m1 cell[0] should be Commit, got {:?}",
        m1_cells.get(0)
    );
}

#[test]
fn test_octopus_merge() {
    // オクトパスマージ（3つ以上の親）
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
    // 並行して進むブランチ
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
    // 複数のレーンが同時にアクティブ
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

    // max_laneは少なくとも3（4つのブランチがマージされる）
    assert!(layout.max_lane >= 3, "Expected max_lane >= 3, got {}", layout.max_lane);
}
