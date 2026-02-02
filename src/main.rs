//! keifu: a TUI tool that shows Git commit graphs

use anyhow::Result;
use clap::Parser;

use clap::ValueEnum;

use keifu::{
    app::App,
    event::{get_key_event, poll_event},
    git::graph::GraphOrientation,
    keybindings::map_key_to_action,
    tui, ui,
};

#[derive(Parser)]
#[command(name = "keifu")]
#[command(
    version,
    about = "A TUI tool to visualize Git commit graphs with branch genealogy"
)]
struct Cli {
    /// Set initial graph orientation (vertical or horizontal)
    #[arg(long, value_enum, default_value = "vertical")]
    orientation: CliGraphOrientation,
    /// Shortcut for --orientation horizontal
    #[arg(long)]
    horizontal: bool,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum CliGraphOrientation {
    Vertical,
    Horizontal,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Handle mutually exclusive flags
    let orientation = if cli.horizontal {
        // --horizontal flag takes precedence
        GraphOrientation::Horizontal
    } else {
        match cli.orientation {
            CliGraphOrientation::Vertical => GraphOrientation::Vertical,
            CliGraphOrientation::Horizontal => GraphOrientation::Horizontal,
        }
    };

    // Restore the terminal on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = tui::restore();
        original_hook(panic_info);
    }));

    // Initialize application
    let mut app = App::new_with_orientation(Some(orientation))?;

    // Initialize terminal
    let mut terminal = tui::init()?;

    // Main loop
    loop {
        // Render
        terminal.draw(|frame| {
            ui::draw(frame, &mut app);
        })?;

        // Check if async fetch has completed
        app.update_fetch_status();

        // Auto-refresh check
        app.check_auto_refresh();

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
