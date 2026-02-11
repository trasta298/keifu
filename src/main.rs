//! keifu: a TUI tool that shows Git commit graphs

use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Parser;

use keifu::{
    action::Action,
    app::{App, AppMode},
    event::{coalesce_scroll_events, drain_events, get_key_event, scroll_delta_to_steps},
    keybindings::map_key_to_action,
    tui, ui,
};

/// ~60 fps cap
const MIN_FRAME_INTERVAL: Duration = Duration::from_millis(16);

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

    let mut last_draw = Instant::now() - MIN_FRAME_INTERVAL;
    let mut needs_redraw = true;
    let mut last_scroll_time: Option<Instant> = None;

    // Main loop
    loop {
        // 1. Receive completed async diff results (lightweight)
        app.poll_diff_results();

        // 2. Check if async fetch has completed
        app.update_fetch_status();

        // 3. Start diff computation if debounce elapsed
        app.request_diff_if_needed();

        // 4. Auto-refresh check
        app.check_auto_refresh();

        // 5. Frame-rate limited rendering
        if needs_redraw && last_draw.elapsed() >= MIN_FRAME_INTERVAL {
            terminal.draw(|frame| {
                ui::draw(frame, &mut app);
            })?;
            last_draw = Instant::now();
            needs_redraw = false;
        }

        // 6. Exit check
        if app.should_quit {
            break;
        }

        // 7. Calculate poll timeout
        let poll_timeout = if needs_redraw {
            MIN_FRAME_INTERVAL.saturating_sub(last_draw.elapsed())
        } else {
            Duration::from_millis(100)
        };

        // 8. Event collection (bounded drain)
        let events = drain_events(poll_timeout)?;

        // 9. Process keyboard events
        for event in &events {
            if let Some(key) = get_key_event(event) {
                if let Some(action) = map_key_to_action(key, &app.mode) {
                    if let Err(e) = app.handle_action(action) {
                        app.show_error(format!("{e}"));
                    }
                }
            }
        }

        // 10. Coalesce and process scroll events (Normal mode only)
        if matches!(app.mode, AppMode::Normal) {
            let scroll_delta = coalesce_scroll_events(&events);
            if scroll_delta != 0 {
                let now = Instant::now();
                let fast = last_scroll_time
                    .is_some_and(|t| now.duration_since(t) < Duration::from_millis(50));
                last_scroll_time = Some(now);

                let scroll_steps = scroll_delta_to_steps(
                    scroll_delta,
                    scroll_events_per_notch,
                    &mut scroll_remainder,
                    fast,
                );
                if scroll_steps != 0 {
                    if let Err(e) = app.handle_action(Action::ScrollMove(scroll_steps)) {
                        app.show_error(format!("{e}"));
                    }
                }
            }
        }

        // Every loop allows redraw (frame-rate limiter controls actual draw frequency).
        // ratatui double-buffers, so no-change frames produce near-zero I/O.
        needs_redraw = true;
    }

    // Restore terminal
    tui::restore()?;

    Ok(())
}
