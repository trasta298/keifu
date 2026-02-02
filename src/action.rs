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
    MoveLeft,
    MoveRight,

    // Horizontal navigation (context-sensitive based on orientation)
    MoveHorizontalLeft,  // Previous commit (horizontal) / Previous lane (vertical)
    MoveHorizontalRight, // Next commit (horizontal) / Next lane (vertical)
    MoveHorizontalUp,    // Previous lane (horizontal) / Previous commit (vertical)
    MoveHorizontalDown,  // Next lane (horizontal) / Next commit (vertical)
    HorizontalPrevChunk, // Page Up - newer chunk
    HorizontalNextChunk, // Page Down - older chunk

    // Git operations
    Checkout,
    CreateBranch,
    DeleteBranch,
    Fetch,
    Merge,
    Rebase,

    // UI
    ToggleHelp,
    ToggleOrientation,
    ToggleTags,
    Search,
    Refresh,
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
}
