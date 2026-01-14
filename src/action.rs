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
    NextMatch,
    PrevMatch,

    // Git operations
    Checkout,
    CreateBranch,
    DeleteBranch,
    Fetch,
    Merge,
    Rebase,

    // UI
    ToggleHelp,
    Search,
    Refresh,
    Quit,

    // Dialogs
    Confirm,
    Cancel,
    InputChar(char),
    InputBackspace,
}
