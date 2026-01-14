//! Keybindings

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::action::Action;
use crate::app::AppMode;

pub fn map_key_to_action(key: KeyEvent, mode: &AppMode) -> Option<Action> {
    match mode {
        AppMode::Normal => map_normal_mode(key),
        AppMode::Help => map_help_mode(key),
        AppMode::Input { .. } => map_input_mode(key),
        AppMode::Confirm { .. } => map_confirm_mode(key),
        AppMode::Error { .. } => map_error_mode(key),
    }
}

fn map_normal_mode(key: KeyEvent) -> Option<Action> {
    match (key.modifiers, key.code) {
        // Movement
        (KeyModifiers::NONE, KeyCode::Char('j')) | (KeyModifiers::NONE, KeyCode::Down) => {
            Some(Action::MoveDown)
        }
        (KeyModifiers::NONE, KeyCode::Char('k')) | (KeyModifiers::NONE, KeyCode::Up) => {
            Some(Action::MoveUp)
        }

        // Page scroll
        (KeyModifiers::CONTROL, KeyCode::Char('d')) => Some(Action::PageDown),
        (KeyModifiers::CONTROL, KeyCode::Char('u')) => Some(Action::PageUp),

        // Top/bottom
        (KeyModifiers::NONE, KeyCode::Char('g')) | (KeyModifiers::NONE, KeyCode::Home) => {
            Some(Action::GoToTop)
        }
        (KeyModifiers::SHIFT, KeyCode::Char('G')) | (KeyModifiers::NONE, KeyCode::End) => {
            Some(Action::GoToBottom)
        }

        // Jump to HEAD (@ works with or without Shift depending on keyboard layout)
        (_, KeyCode::Char('@')) => Some(Action::JumpToHead),

        // Branch jump
        (KeyModifiers::NONE, KeyCode::Char(']')) | (KeyModifiers::NONE, KeyCode::Tab) => {
            Some(Action::NextBranch)
        }
        (KeyModifiers::NONE, KeyCode::Char('[')) | (KeyModifiers::SHIFT, KeyCode::BackTab) => {
            Some(Action::PrevBranch)
        }

        // Branch selection within same commit
        (KeyModifiers::NONE, KeyCode::Char('h')) | (KeyModifiers::NONE, KeyCode::Left) => {
            Some(Action::BranchLeft)
        }
        (KeyModifiers::NONE, KeyCode::Char('l')) | (KeyModifiers::NONE, KeyCode::Right) => {
            Some(Action::BranchRight)
        }

        // Git operations
        (KeyModifiers::NONE, KeyCode::Enter) => Some(Action::Checkout),
        (KeyModifiers::NONE, KeyCode::Char('b')) => Some(Action::CreateBranch),
        (KeyModifiers::NONE, KeyCode::Char('d')) => Some(Action::DeleteBranch),
        (KeyModifiers::NONE, KeyCode::Char('f')) => Some(Action::Fetch),
        // TODO: merge and rebase will be implemented in the future
        // (KeyModifiers::NONE, KeyCode::Char('m')) => Some(Action::Merge),
        // (KeyModifiers::NONE, KeyCode::Char('r')) => Some(Action::Rebase),

        // Search navigation
        (KeyModifiers::NONE, KeyCode::Char('n')) => Some(Action::NextMatch),
        (KeyModifiers::SHIFT, KeyCode::Char('N')) => Some(Action::PrevMatch),

        // UI
        (KeyModifiers::NONE, KeyCode::Char('/')) => Some(Action::Search),
        (KeyModifiers::SHIFT, KeyCode::Char('R')) => Some(Action::Refresh),
        (KeyModifiers::NONE, KeyCode::Char('?')) => Some(Action::ToggleHelp),
        (KeyModifiers::NONE, KeyCode::Char('q')) | (KeyModifiers::NONE, KeyCode::Esc) => {
            Some(Action::Quit)
        }

        _ => None,
    }
}

fn map_help_mode(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => Some(Action::ToggleHelp),
        _ => None,
    }
}

fn map_input_mode(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Enter => Some(Action::Confirm),
        KeyCode::Esc => Some(Action::Cancel),
        KeyCode::Backspace => Some(Action::InputBackspace),
        KeyCode::Char(c) => Some(Action::InputChar(c)),
        _ => None,
    }
}

fn map_confirm_mode(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Enter => Some(Action::Confirm),
        KeyCode::Char('n') | KeyCode::Esc => Some(Action::Cancel),
        _ => None,
    }
}

fn map_error_mode(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => Some(Action::Cancel),
        _ => None,
    }
}
