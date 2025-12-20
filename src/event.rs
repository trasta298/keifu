//! Event loop and key input handling

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent};

/// Poll for events (100ms timeout)
pub fn poll_event() -> Result<Option<Event>> {
    if event::poll(Duration::from_millis(100))? {
        Ok(Some(event::read()?))
    } else {
        Ok(None)
    }
}

/// Extract key event
pub fn get_key_event(event: &Event) -> Option<KeyEvent> {
    match event {
        Event::Key(key) => Some(*key),
        _ => None,
    }
}

/// Check whether the event is a resize
pub fn is_resize_event(event: &Event) -> bool {
    matches!(event, Event::Resize(_, _))
}
