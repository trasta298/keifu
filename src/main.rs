//! git-graph-tui: CLIでGitグラフを表示するTUIツール

mod action;
mod app;
mod event;
mod git;
mod graph;
mod keybindings;
mod tui;
mod ui;

use anyhow::Result;

use crate::{
    app::App,
    event::{get_key_event, poll_event},
    keybindings::map_key_to_action,
};

fn main() -> Result<()> {
    // パニック時にターミナルを復元
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = tui::restore();
        original_hook(panic_info);
    }));

    // アプリケーション初期化
    let mut app = App::new()?;

    // ターミナル初期化
    let mut terminal = tui::init()?;

    // メインループ
    loop {
        // 描画
        terminal.draw(|frame| {
            ui::draw(frame, &mut app);
        })?;

        // 終了チェック
        if app.should_quit {
            break;
        }

        // イベント処理
        if let Some(event) = poll_event()? {
            if let Some(key) = get_key_event(&event) {
                if let Some(action) = map_key_to_action(key, &app.mode) {
                    if let Err(e) = app.handle_action(action) {
                        // エラーをメッセージとして表示（TODO: より良いエラー表示）
                        eprintln!("Error: {}", e);
                    }
                }
            }
            // リサイズイベントは自動的に再描画される
        }
    }

    // ターミナル復元
    tui::restore()?;

    Ok(())
}
