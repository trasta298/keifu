//! Selection of visible text within a pane.

use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Color, Modifier},
};
use unicode_width::UnicodeWidthStr;

pub struct TextSelection {
    pub start: Position,
    end: Position,
    area: Rect,
    screen: Buffer,
    pub pressed: bool,
    pub dragged: bool,
}

impl TextSelection {
    pub fn new(screen: &Buffer, area: Rect, start: Position) -> Self {
        Self {
            start,
            end: start,
            area,
            screen: screen.clone(),
            pressed: true,
            dragged: false,
        }
    }

    pub fn update(&mut self, position: Position) {
        self.dragged |= position != self.start;
        self.end = Position::new(
            position.x.clamp(self.area.x, self.area.right() - 1),
            position.y.clamp(self.area.y, self.area.bottom() - 1),
        );
    }

    fn rows(&self) -> impl Iterator<Item = (u16, u16, u16)> + '_ {
        let (start, end) = if (self.start.y, self.start.x) <= (self.end.y, self.end.x) {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        };
        (start.y..=end.y).map(move |y| {
            let left = if y == start.y { start.x } else { self.area.x };
            let right = if y == end.y {
                end.x + 1
            } else {
                self.area.right()
            };
            (y, left, right)
        })
    }

    pub fn text(&self) -> String {
        let mut lines = Vec::new();
        for (y, left, right) in self.rows() {
            let mut line = String::new();
            let mut x = self.area.x;
            while x < right {
                let symbol = self.screen[(x, y)].symbol();
                let width = symbol.width().max(1) as u16;
                if x + width > left {
                    line.push_str(symbol);
                }
                x += width;
            }
            lines.push(line.trim_end().to_string());
        }
        lines.join("\n")
    }

    pub fn matches_screen(&self, screen: &Buffer) -> bool {
        self.screen.area == screen.area
            && (self.area.y..self.area.bottom()).all(|y| {
                (self.area.x..self.area.right())
                    .all(|x| self.screen[(x, y)].symbol() == screen[(x, y)].symbol())
            })
    }

    pub fn highlight(&self, screen: &mut Buffer) {
        if !self.dragged {
            return;
        }
        for (y, left, right) in self.rows() {
            let mut x = self.area.x;
            while x < right {
                let width = self.screen[(x, y)].symbol().width().max(1) as u16;
                if x + width > left {
                    for column in x..(x + width).min(self.area.right()) {
                        let cell = &mut screen[(column, y)];
                        cell.set_fg(Color::Black).set_bg(Color::LightBlue);
                        cell.modifier.remove(Modifier::REVERSED);
                    }
                }
                x += width;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_forward_and_backward_without_neighboring_panes() {
        let screen = Buffer::with_lines(["|abc  |other", "|def  |pane "]);
        let area = Rect::new(1, 0, 5, 2);
        for (start, end) in [
            (Position::new(2, 0), Position::new(2, 1)),
            (Position::new(2, 1), Position::new(2, 0)),
        ] {
            let mut selection = TextSelection::new(&screen, area, start);
            selection.update(end);
            assert_eq!(selection.text(), "bc\nde");
            let mut rendered = screen.clone();
            selection.highlight(&mut rendered);
            assert_eq!(rendered[(2, 0)].bg, Color::LightBlue);
            assert_ne!(rendered[(7, 0)].bg, Color::LightBlue);
        }
    }

    #[test]
    fn includes_wide_characters_and_combining_marks_without_padding() {
        let screen = Buffer::with_lines(["a界e\u{301}🙂z"]);
        let mut selection = TextSelection::new(&screen, screen.area, Position::new(2, 0));
        selection.update(Position::new(5, 0));
        assert_eq!(selection.text(), "界e\u{301}🙂");
    }

    #[test]
    fn clamps_drag_and_invalidates_changed_content_or_size() {
        let screen = Buffer::with_lines(["first", "last "]);
        let mut selection = TextSelection::new(&screen, screen.area, Position::new(0, 0));
        selection.update(Position::new(u16::MAX, u16::MAX));
        assert_eq!(selection.text(), "first\nlast");
        assert!(selection.matches_screen(&screen));
        assert!(!selection.matches_screen(&Buffer::with_lines(["other", "last "])));
        assert!(!selection.matches_screen(&Buffer::with_lines(["first"])));
    }
}
