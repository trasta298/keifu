//! keifu: a TUI tool that shows Git commit graphs

use anyhow::Result;
use clap::Parser;

use keifu::{
    action::Action,
    app::{App, AppMode},
    event::{coalesce_scroll_events, drain_events, get_key_event, scroll_delta_to_steps},
    keybindings::map_key_to_action,
    tui, ui,
};

#[derive(Parser)]
#[command(name = "keifu")]
#[command(
    version,
    about = "A TUI tool to visualize Git commit graphs with branch genealogy"
)]
struct Cli {}

fn main() -> Result<()> {
    Cli::parse();
    // Restore the terminal on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = tui::restore();
        original_hook(panic_info);
    }));

    // Initialize application
    let mut app = App::new()?;
    let scroll_events_per_notch = app.scroll_events_per_notch();
    let mut scroll_remainder = 0;

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

        // Event handling - drain all pending events to prevent accumulation
        let events = drain_events()?;

        // Process keyboard events
        for event in &events {
            if let Some(key) = get_key_event(event) {
                if let Some(action) = map_key_to_action(key, &app.mode) {
                    if let Err(e) = app.handle_action(action) {
                        app.show_error(format!("{e}"));
                    }
                }
            }
        }

        // Coalesce and process scroll events (Normal mode only)
        if matches!(app.mode, AppMode::Normal) {
            let scroll_delta = coalesce_scroll_events(&events);
            let scroll_steps = scroll_delta_to_steps(
                scroll_delta,
                scroll_events_per_notch,
                &mut scroll_remainder,
            );
            if scroll_steps != 0 {
                if let Err(e) = app.handle_action(Action::ScrollMove(scroll_steps)) {
                    app.show_error(format!("{e}"));
                }
            }
        }
    }

    // Restore terminal
    tui::restore()?;

    Ok(())
}
