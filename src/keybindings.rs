//! キーバインド定義

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
        // 移動
        (KeyModifiers::NONE, KeyCode::Char('j')) | (KeyModifiers::NONE, KeyCode::Down) => {
            Some(Action::MoveDown)
        }
        (KeyModifiers::NONE, KeyCode::Char('k')) | (KeyModifiers::NONE, KeyCode::Up) => {
            Some(Action::MoveUp)
        }

        // ページスクロール
        (KeyModifiers::CONTROL, KeyCode::Char('d')) => Some(Action::PageDown),
        (KeyModifiers::CONTROL, KeyCode::Char('u')) => Some(Action::PageUp),

        // 先頭/末尾
        (KeyModifiers::NONE, KeyCode::Char('g')) | (KeyModifiers::NONE, KeyCode::Home) => {
            Some(Action::GoToTop)
        }
        (KeyModifiers::SHIFT, KeyCode::Char('G')) | (KeyModifiers::NONE, KeyCode::End) => {
            Some(Action::GoToBottom)
        }

        // ブランチ間ジャンプ
        (KeyModifiers::NONE, KeyCode::Char(']')) | (KeyModifiers::NONE, KeyCode::Tab) => {
            Some(Action::NextBranch)
        }
        (KeyModifiers::NONE, KeyCode::Char('[')) | (KeyModifiers::SHIFT, KeyCode::BackTab) => {
            Some(Action::PrevBranch)
        }

        // Git操作
        (KeyModifiers::NONE, KeyCode::Enter) => Some(Action::Checkout),
        (KeyModifiers::NONE, KeyCode::Char('b')) => Some(Action::CreateBranch),
        (KeyModifiers::NONE, KeyCode::Char('d')) => Some(Action::DeleteBranch),
        (KeyModifiers::NONE, KeyCode::Char('m')) => Some(Action::Merge),
        (KeyModifiers::NONE, KeyCode::Char('r')) => Some(Action::Rebase),

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
