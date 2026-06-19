//! User action definitions

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // Navigation
    MoveUp,
    MoveDown,
    PageUp,
    PageDown,
    GoToTop,
    GoToBottom,
    JumpToHead,
    NextBranch,
    PrevBranch,
    BranchLeft,
    BranchRight,

    // Git operations
    Checkout,
    CreateBranch,
    DeleteBranch,
    Fetch,
    Merge,
    Rebase,

    // Staging / commit / push
    StageToggle,
    StageAll,
    UnstageAll,
    CommitDialog,
    Push,

    // Clipboard
    CopyHash,
    CopyBranch,

    // UI
    FocusNext,
    ToggleHelp,
    Search,
    Refresh,
    ToggleRemoteBranches,
    Quit,

    // Dialogs
    Confirm,
    Cancel,
    InputChar(char),
    InputBackspace,

    // Search dropdown
    SearchSelectUp,
    SearchSelectDown,
    SearchSelectUpQuiet,   // Tab navigation (no graph jump)
    SearchSelectDownQuiet, // Tab navigation (no graph jump)

    // File diff
    EnterFileSelect,
    FileSelectUp,
    FileSelectDown,
    OpenFileDiff,
    ScrollUp,
    ScrollDown,
    ScrollPageUp,
    ScrollPageDown,
    ScrollToTop,
    ScrollToBottom,
    ScrollLeft,
    ScrollRight,
    ScrollToLineStart,
    NextFile,
    PrevFile,
    NextHunk,
    PrevHunk,
}
