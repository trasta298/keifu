//! Legend sidebar widget for horizontal layout

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style, Modifier},
    widgets::{Block, Borders, Widget},
};

use crate::git::graph::LaneInfo;
use crate::graph::colors::get_color_by_index;

pub struct LegendSidebarWidget<'a> {
    pub lanes: &'a [LaneInfo],
}

impl<'a> Widget for LegendSidebarWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Branches ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let inner = block.inner(area);
        block.render(area, buf);

        for (i, lane) in self.lanes.iter().enumerate() {
            let y = inner.y + i as u16;
            if y >= inner.y + inner.height {
                break;
            }

            self.render_lane(buf, inner.x, y, lane);
        }
    }
}

impl<'a> LegendSidebarWidget<'a> {
    fn render_lane(&self, buf: &mut Buffer, x: u16, y: u16, lane: &LaneInfo) {
        let mut x_offset = 0;

        // Color indicator
        let color = get_color_by_index(lane.color_index);
        let indicator = if lane.is_head { '◉' } else { '●' };

        buf[(x + x_offset, y)]
            .set_char(indicator)
            .set_style(Style::default().fg(color).add_modifier(Modifier::BOLD));
        x_offset += 2;

        // Branch names
        let name_text = if lane.branch_names.is_empty() {
            "(no name)".to_string()
        } else {
            lane.branch_names.join(", ")
        };

        let style = Style::default()
            .fg(color)
            .add_modifier(Modifier::BOLD);

        for (i, ch) in name_text.chars().enumerate() {
            if x + x_offset as u16 + i as u16 >= x + 20 { // Max width
                break;
            }
            buf[(x + x_offset as u16 + i as u16, y)]
                .set_char(ch)
                .set_style(style);
        }
    }
}