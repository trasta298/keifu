//! Git layer

pub mod branch;
pub mod commit;
pub mod diff;
pub mod graph;
pub mod layout;
pub mod operations;
pub mod repository;
pub mod tag;

pub use branch::BranchInfo;
pub use commit::CommitInfo;

pub use diff::{CommitDiffInfo, FileChangeKind, FileDiffInfo};
pub use graph::{build_graph, build_horizontal_graph, GraphOrientation};
pub use layout::{
    GraphLayoutStrategy, UnifiedSelection, NavigationDirection, RenderContext,
};
pub use repository::{GitRepository, WorkingTreeStatus};
pub use tag::TagInfo;
