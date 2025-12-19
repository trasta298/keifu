//! ユーザーアクションの定義

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // ナビゲーション
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    PageUp,
    PageDown,
    GoToTop,
    GoToBottom,

    // フォーカス
    CycleFocus,

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
