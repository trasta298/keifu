//! ユーザーアクションの定義

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // ナビゲーション
    MoveUp,
    MoveDown,
    PageUp,
    PageDown,
    GoToTop,
    GoToBottom,
    NextBranch,
    PrevBranch,

    // Git操作
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

    // ダイアログ
    Confirm,
    Cancel,
    InputChar(char),
    InputBackspace,
}
