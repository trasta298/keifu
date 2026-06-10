//! keifu: a TUI tool that shows Git commit graphs

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use crossterm::event::Event;

use keifu::{
    app::App, debug_server, event::poll_events, git::configure_git_extensions,
    keybindings::map_key_to_action, logging, mouse, tui, ui,
};

#[derive(Parser)]
#[command(name = "keifu")]
#[command(
    version,
    about = "A TUI tool to visualize Git commit graphs with branch genealogy"
)]
struct Cli {
    /// Append debug logs to this file (level via KEIFU_LOG, default "debug")
    #[arg(long, value_name = "PATH")]
    log_file: Option<PathBuf>,

    /// Listen for debug commands (NDJSON over TCP, e.g. 127.0.0.1:7167)
    #[arg(long, value_name = "ADDR")]
    debug_listen: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Some(path) = &cli.log_file {
        logging::init(path)?;
    }
    let debug_rx = match &cli.debug_listen {
        Some(addr) => Some(debug_server::spawn(addr)?),
        None => None,
    };

    // Restore the terminal on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = tui::restore();
        original_hook(panic_info);
    }));

    configure_git_extensions()?;

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

        // Check if async fetch/push has completed
        app.update_fetch_status();
        app.update_push_status();

        // Auto-refresh check
        app.check_auto_refresh();

        // Exit check
        if app.should_quit {
            break;
        }

        // Process all queued events before the next render
        for event in poll_events()? {
            match event {
                Event::Key(key) => {
                    if let Some(action) = map_key_to_action(key, &app.mode) {
                        if let Err(e) = app.handle_action(action) {
                            // Show errors in the UI
                            app.show_error(format!("{}", e));
                        }
                    }
                }
                Event::Mouse(mouse_event) => {
                    mouse::handle_mouse(&mut app, mouse_event);
                }
                // Resize events trigger redraw automatically
                _ => {}
            }
            if app.should_quit {
                break;
            }
        }

        // Process pending debug commands
        if let Some(rx) = &debug_rx {
            while let Ok(command) = rx.try_recv() {
                let size = terminal.size()?;
                let response =
                    debug_server::handle_request(&mut app, size.width, size.height, command.request);
                let _ = command.reply.send(response);
            }
        }
    }

    // Restore terminal
    tui::restore()?;

    Ok(())
}
