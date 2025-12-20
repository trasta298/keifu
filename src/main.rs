//! git-graph-tui: a TUI tool that shows Git graphs in the CLI

use anyhow::Result;

use git_graph_tui::{
    app::App,
    event::{get_key_event, poll_event},
    git::{build_graph, graph::CellType, GitRepository},
    keybindings::map_key_to_action,
    tui, ui,
};

fn main() -> Result<()> {
    // Text output mode (--text flag)
    if std::env::args().any(|a| a == "--text") {
        return text_output();
    }

    // Restore the terminal on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = tui::restore();
        original_hook(panic_info);
    }));

    // Initialize application
    let mut app = App::new()?;

    // Initialize terminal
    let mut terminal = tui::init()?;

    // Main loop
    loop {
        // Render
        terminal.draw(|frame| {
            ui::draw(frame, &mut app);
        })?;

        // Exit check
        if app.should_quit {
            break;
        }

        // Event handling
        if let Some(event) = poll_event()? {
            if let Some(key) = get_key_event(&event) {
                if let Some(action) = map_key_to_action(key, &app.mode) {
                    if let Err(e) = app.handle_action(action) {
                        // Show errors in the UI
                        app.show_error(format!("{}", e));
                    }
                }
            }
            // Resize events trigger redraw automatically
        }
    }

    // Restore terminal
    tui::restore()?;

    Ok(())
}

/// Text output mode
fn text_output() -> Result<()> {
    let repo = GitRepository::discover()?;
    let commits = repo.get_commits(50)?;
    let branches = repo.get_branches()?;
    let layout = build_graph(&commits, &branches);

    for node in &layout.nodes {
        let mut graph = String::from(" "); // Left margin

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

        // Padding
        let graph_width = (layout.max_lane + 1) * 2;
        while graph.chars().count() - 1 < graph_width {
            graph.push(' ');
        }

        // If there is no commit (connector-only row)
        let Some(commit) = &node.commit else {
            println!("{}", graph);
            continue;
        };

        // Branch names
        let branch_str = if !node.branch_names.is_empty() {
            format!(" [{}]", node.branch_names.join(", "))
        } else {
            String::new()
        };

        // Truncate to 40 characters with multibyte awareness
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
