//! git-graph-tui: CLIでGitグラフを表示するTUIツール

use anyhow::Result;

use git_graph_tui::{
    app::App,
    event::{get_key_event, poll_event},
    git::{build_graph, graph::CellType, GitRepository},
    keybindings::map_key_to_action,
    tui, ui,
};

fn main() -> Result<()> {
    // テキスト出力モード（--text フラグ）
    if std::env::args().any(|a| a == "--text") {
        return text_output();
    }

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
                        // エラーをUIに表示
                        app.show_error(format!("{}", e));
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

/// テキスト出力モード
fn text_output() -> Result<()> {
    let repo = GitRepository::discover()?;
    let commits = repo.get_commits(50)?;
    let branches = repo.get_branches()?;
    let layout = build_graph(&commits, &branches);

    for node in &layout.nodes {
        let mut graph = String::from(" "); // 左マージン

        for cell in &node.cells {
            let ch = match cell {
                CellType::Empty => ' ',
                CellType::Pipe(_) => '│',
                CellType::Commit(_) => if node.is_head { '◉' } else { '○' },
                CellType::BranchRight(_) => '╭',
                CellType::BranchLeft(_) => '╮',
                CellType::MergeRight(_) => '╰',
                CellType::MergeLeft(_) => '╯',
                CellType::Horizontal(_) => '─',
                CellType::HorizontalPipe(_, _) => '┼',
                CellType::TeeRight(_) => '├',
                CellType::TeeLeft(_) => '┤',
                CellType::TeeUp(_) => '┴',
            };
            graph.push(ch);
        }

        // パディング
        let graph_width = (layout.max_lane + 1) * 2;
        while graph.chars().count() - 1 < graph_width {
            graph.push(' ');
        }

        // コミットがない場合（接続行のみ）
        let Some(commit) = &node.commit else {
            println!("{}", graph);
            continue;
        };

        // ブランチ名
        let branch_str = if !node.branch_names.is_empty() {
            format!(" [{}]", node.branch_names.join(", "))
        } else {
            String::new()
        };

        // マルチバイト文字を考慮して40文字で切り詰め
        let message: String = commit.message.chars().take(40).collect();
        println!(
            "{}  {} {}{}",
            graph,
            commit.short_id,
            message,
            branch_str
        );
    }

    Ok(())
}
