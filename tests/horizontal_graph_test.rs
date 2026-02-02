//! Tests for the horizontal graph rendering

use chrono::Local;
use git2::Oid;
use keifu::git::{build_horizontal_graph, BranchInfo, CommitInfo, GraphOrientation};
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

#[test]
fn test_build_horizontal_graph_creates_layout() {
    // Simple linear history: C2 -> C1
    let commits = vec![
        make_commit("c2", vec!["c1"]),
        make_commit("c1", vec![]),
    ];
    let branches = vec![make_branch("main", "c2", true)];

    let layout = build_horizontal_graph(&commits, &branches, &[], None, None, 80, CompressionMode::default());

    // Should have chunks
    assert!(!layout.chunks.is_empty());

    // Should have lanes
    assert!(!layout.lanes.is_empty());

    // Should have a selection pointing to the newest commit
    // With right-alignment, the exact column depends on padding
    assert_eq!(layout.selection.chunk_index, 0); // Newest commits in chunk 0 (reversed)
    assert_eq!(layout.selection.lane, 0);
    // Column should be valid within the chunk
    assert!(layout.selection.column < layout.chunks[0].cells[0].len());

    // Should have total columns
    assert!(layout.total_columns >= 2);
}

#[test]
fn test_horizontal_orientation_flag_in_app() {
    // This test verifies that the app correctly handles orientation
    // We can't easily test the TUI part, but we can test the orientation logic
    use keifu::app::App;

    // Test creating app with horizontal orientation
    let result = App::new_with_orientation(Some(GraphOrientation::Horizontal));

    // Should succeed
    assert!(result.is_ok());

    let app = result.unwrap();

    // Should be horizontal
    assert_eq!(app.current_orientation, GraphOrientation::Horizontal);

    // Should have horizontal layout
    assert!(app.horizontal_layout.is_some());
}

#[test]
fn test_vertical_orientation_flag_in_app() {
    use keifu::app::App;

    // Test creating app with vertical orientation
    let result = App::new_with_orientation(Some(GraphOrientation::Vertical));

    // Should succeed
    assert!(result.is_ok());

    let app = result.unwrap();

    // Should be vertical
    assert_eq!(app.current_orientation, GraphOrientation::Vertical);

    // Should not have horizontal layout
    assert!(app.horizontal_layout.is_none());
}

#[test]
fn test_app_toggle_orientation() {
    use keifu::app::App;

    // Start with vertical
    let mut app = App::new_with_orientation(Some(GraphOrientation::Vertical)).unwrap();

    // Should be vertical
    assert_eq!(app.current_orientation, GraphOrientation::Vertical);
    assert!(app.horizontal_layout.is_none());

    // Toggle to horizontal
    app.toggle_orientation();

    // Should now be horizontal
    assert_eq!(app.current_orientation, GraphOrientation::Horizontal);
    assert!(app.horizontal_layout.is_some());

    // Toggle back to vertical
    app.toggle_orientation();

    // Should now be vertical again
    assert_eq!(app.current_orientation, GraphOrientation::Vertical);
    assert!(app.horizontal_layout.is_none());
}