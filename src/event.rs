//! Event loop and key input handling

use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent, MouseEventKind};

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
///
/// The drain loop is bounded to 5ms to prevent free-spin scroll wheels from
/// monopolising the main thread.
pub fn drain_events(timeout: Duration) -> Result<Vec<Event>> {
    let mut events = Vec::new();
    if event::poll(timeout)? {
        events.push(event::read()?);
        // Non-blocking drain, bounded to 5ms
        let drain_deadline = Instant::now() + Duration::from_millis(5);
        while Instant::now() < drain_deadline && event::poll(Duration::ZERO)? {
            events.push(event::read()?);
        }
        // Scroll follow-up: independent 10ms window (existing behaviour)
        if events.iter().any(is_scroll_event) && event::poll(Duration::from_millis(10))? {
            events.push(event::read()?);
            let followup_deadline = Instant::now() + Duration::from_millis(5);
            while Instant::now() < followup_deadline && event::poll(Duration::ZERO)? {
                events.push(event::read()?);
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

/// Convert raw scroll delta to movement steps.
///
/// When `events_per_notch` is `None` (default):
/// - Slow scrolling (`fast == false`): produces one step per batch (sign-only)
///   so that multi-event-per-notch terminals don't double-step.
/// - Fast scrolling (`fast == true`): passes through the raw delta for
///   proportional movement (free-spin / rapid scrolling).
///
/// When `Some(1)`, the raw delta is returned as-is.
/// When `Some(n)` with n > 1, deltas are grouped with frame-to-frame remainder carry.
pub fn scroll_delta_to_steps(
    delta: i32,
    events_per_notch: Option<i32>,
    remainder: &mut i32,
    fast: bool,
) -> i32 {
    let Some(epn) = events_per_notch else {
        *remainder = 0;
        return if fast { delta } else { delta.signum() };
    };

    let epn = epn.max(1);
    if epn == 1 {
        *remainder = 0;
        return delta;
    }

    let total = delta + *remainder;
    let steps = total / epn;
    *remainder = total % epn;
    steps
}

#[cfg(test)]
mod tests {
    use super::scroll_delta_to_steps;

    #[test]
    fn slow_scroll_uses_sign_only_when_none() {
        let mut remainder = 0;
        assert_eq!(scroll_delta_to_steps(-3, None, &mut remainder, false), -1);
        assert_eq!(scroll_delta_to_steps(2, None, &mut remainder, false), 1);
        assert_eq!(scroll_delta_to_steps(0, None, &mut remainder, false), 0);
    }

    #[test]
    fn fast_scroll_uses_raw_delta_when_none() {
        let mut remainder = 0;
        assert_eq!(scroll_delta_to_steps(-3, None, &mut remainder, true), -3);
        assert_eq!(scroll_delta_to_steps(20, None, &mut remainder, true), 20);
    }

    #[test]
    fn steps_passthrough_when_events_per_notch_is_one() {
        let mut remainder = 0;
        assert_eq!(scroll_delta_to_steps(5, Some(1), &mut remainder, false), 5);
        assert_eq!(remainder, 0);
    }

    #[test]
    fn steps_keep_remainder_across_batches() {
        let mut remainder = 0;
        let first = scroll_delta_to_steps(2, Some(6), &mut remainder, false);
        let second = scroll_delta_to_steps(4, Some(6), &mut remainder, false);

        assert_eq!(first, 0);
        assert_eq!(second, 1);
        assert_eq!(remainder, 0);
    }
}
