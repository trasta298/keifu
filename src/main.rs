//! keifu: a TUI tool that shows Git commit graphs

use anyhow::Result;

use keifu::{
    app::App,
    event::{get_key_event, poll_event},
    keybindings::map_key_to_action,
    tui, ui,
};

fn main() -> Result<()> {
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
