//! Event loop and key input handling

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent, MouseEvent, MouseEventKind};

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
    if let Event::Key(key) = event {
        Some(*key)
    } else {
        None
    }
}

/// Extract mouse scroll event (returns positive for scroll down, negative for scroll up)
pub fn get_mouse_scroll(event: &Event) -> Option<i32> {
    if let Event::Mouse(MouseEvent { kind, .. }) = event {
        match kind {
            MouseEventKind::ScrollDown => Some(1),
            MouseEventKind::ScrollUp => Some(-1),
            _ => None,
        }
    } else {
        None
    }
}
