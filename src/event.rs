//! Event loop and input handling

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event};

/// Poll for events (100ms timeout) and drain everything already queued.
///
/// Processing the whole batch before the next render keeps scrolling
/// responsive: rendering once per event made queued scroll events burst
/// out at unpredictable speed (issue #12).
pub fn poll_events() -> Result<Vec<Event>> {
    if !event::poll(Duration::from_millis(100))? {
        return Ok(Vec::new());
    }
    let mut events = vec![event::read()?];
    while event::poll(Duration::ZERO)? {
        events.push(event::read()?);
    }
    Ok(events)
}
