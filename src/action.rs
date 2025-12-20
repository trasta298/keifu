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
    NextBranch,
    PrevBranch,
    BranchLeft,
    BranchRight,

    // Git operations
    Checkout,
    CreateBranch,
    DeleteBranch,
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
