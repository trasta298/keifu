//! Event loop and key input handling

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent, MouseEventKind};

use crate::action::Action;

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

/// Drain all pending events from the queue.
/// After detecting a scroll event, briefly waits to batch same-notch follow-up
/// events that arrive slightly later (some terminals send multiple events per notch).
pub fn drain_events() -> Result<Vec<Event>> {
    let mut events = Vec::new();
    // First, do a blocking poll with timeout
    if event::poll(Duration::from_millis(100))? {
        events.push(event::read()?);
        // Then drain any remaining events (non-blocking)
        while event::poll(Duration::from_millis(0))? {
            events.push(event::read()?);
        }
        // If scroll events detected, briefly wait for same-notch follow-up events
        // that may arrive a few ms later (e.g. Ghostty/macOS sends ~2 events per notch)
        if events.iter().any(|e| is_scroll_event(e)) {
            if event::poll(Duration::from_millis(10))? {
                events.push(event::read()?);
                while event::poll(Duration::from_millis(0))? {
                    events.push(event::read()?);
                }
            }
        }
    }
    Ok(events)
}

fn is_scroll_event(event: &Event) -> bool {
    matches!(
        event,
        Event::Mouse(mouse) if matches!(mouse.kind, MouseEventKind::ScrollUp | MouseEventKind::ScrollDown)
    )
}

/// Extract scroll delta from mouse events, coalescing multiple events
/// Returns net scroll direction: negative = up, positive = down
pub fn coalesce_scroll_events(events: &[Event]) -> i32 {
    events.iter().fold(0, |acc, event| {
        if let Event::Mouse(mouse) = event {
            match mouse.kind {
                MouseEventKind::ScrollUp => acc - 1,
                MouseEventKind::ScrollDown => acc + 1,
                _ => acc,
            }
        } else {
            acc
        }
    })
}

/// Get scroll action from coalesced delta
pub fn scroll_delta_to_action(delta: i32) -> Option<Action> {
    match delta.cmp(&0) {
        std::cmp::Ordering::Less => Some(Action::MoveUp),
        std::cmp::Ordering::Greater => Some(Action::MoveDown),
        std::cmp::Ordering::Equal => None,
    }
}

/// Convert raw scroll delta to normalized movement steps.
///
/// This is used only when `[scroll].events_per_notch` is configured.
/// If `events_per_notch = 1`, the raw delta is returned as-is.
/// If `events_per_notch > 1`, deltas are grouped with frame-to-frame remainder carry.
pub fn scroll_delta_to_steps(delta: i32, events_per_notch: i32, remainder: &mut i32) -> i32 {
    let events_per_notch = events_per_notch.max(1);
    if events_per_notch == 1 {
        *remainder = 0;
        return delta;
    }

    let total = delta + *remainder;
    let steps = total / events_per_notch;
    *remainder = total % events_per_notch;
    steps
}

#[cfg(test)]
mod tests {
    use super::{scroll_delta_to_action, scroll_delta_to_steps};
    use crate::action::Action;

    #[test]
    fn delta_to_action_uses_sign_only() {
        assert_eq!(scroll_delta_to_action(-3), Some(Action::MoveUp));
        assert_eq!(scroll_delta_to_action(2), Some(Action::MoveDown));
        assert_eq!(scroll_delta_to_action(0), None);
    }

    #[test]
    fn steps_passthrough_when_events_per_notch_is_one() {
        let mut remainder = 0;
        let steps = scroll_delta_to_steps(5, 1, &mut remainder);
        assert_eq!(steps, 5);
        assert_eq!(remainder, 0);
    }

    #[test]
    fn steps_keep_remainder_across_batches() {
        let mut remainder = 0;
        let first = scroll_delta_to_steps(2, 6, &mut remainder);
        let second = scroll_delta_to_steps(4, 6, &mut remainder);

        assert_eq!(first, 0);
        assert_eq!(second, 1);
        assert_eq!(remainder, 0);
    }
}
