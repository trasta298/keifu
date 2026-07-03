//! Git layer

pub mod branch;
pub mod commit;
pub mod diff;
pub mod extensions;
pub mod graph;
pub mod operations;
pub mod repository;
pub mod worktree;

pub use branch::{collect_triage, default_base_branch, BranchInfo, BranchTriageRow};
pub use commit::CommitInfo;
pub use diff::{
    CommitDiffInfo, DiffHunkContent, DiffLineContent, DiffLineOrigin, FileChangeKind,
    FileDiffContent, FileDiffInfo,
};
pub use extensions::configure_git_extensions;
pub use graph::build_graph;
pub use repository::{GitRepository, StageState, WorkingTreeStatus};
pub use worktree::WorktreeInfo;
