//! イベントループとキー入力処理

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent};

/// イベントをポーリング（100msタイムアウト）
pub fn poll_event() -> Result<Option<Event>> {
    if event::poll(Duration::from_millis(100))? {
        Ok(Some(event::read()?))
    } else {
        Ok(None)
    }
}

/// キーイベントを取得
pub fn get_key_event(event: &Event) -> Option<KeyEvent> {
    match event {
        Event::Key(key) => Some(*key),
        _ => None,
    }
}

/// リサイズイベントかどうかを確認
pub fn is_resize_event(event: &Event) -> bool {
    matches!(event, Event::Resize(_, _))
}
