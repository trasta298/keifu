//! Legend sidebar widget for horizontal layout

use std::collections::BTreeMap;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style, Modifier},
    widgets::{Block, Borders, Widget},
};

use crate::git::graph::{LaneInfo, LaneBranch};
use crate::graph::colors::get_color_by_index;

struct SidebarItem<'a> {
    lane: &'a LaneInfo,
    branch: Option<&'a LaneBranch>, // None for "no name" lanes
}

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

        // Flatten lanes into items
        let mut local_items: Vec<SidebarItem> = Vec::new();
        let mut remote_groups: BTreeMap<String, Vec<SidebarItem>> = BTreeMap::new();
        let mut deleted_items: Vec<SidebarItem> = Vec::new();

        for lane in self.lanes {
            if lane.branches.is_empty() {
                deleted_items.push(SidebarItem { lane, branch: None });
            } else {
                for branch in &lane.branches {
                    let item = SidebarItem { lane, branch: Some(branch) };
                    if !branch.is_remote {
                        local_items.push(item);
                    } else {
                         let remote_name = branch.name.split('/').next().unwrap_or("unknown");
                         remote_groups.entry(remote_name.to_string()).or_default().push(item);
                    }
                }
            }
        }

        let mut y = inner.y;

        // Render Local
        if !local_items.is_empty() {
            for item in local_items {
                if y >= inner.y + inner.height { break; }
                self.render_item(buf, inner.x, y, inner.width, &item);
                y += 1;
            }
            // Add separator if there are subsequent items
            if !remote_groups.is_empty() || !deleted_items.is_empty() {
                y += 1;
            }
        }

        // Render Remote Groups
        if !remote_groups.is_empty() {
            let count = remote_groups.len();
            for (i, (remote, items)) in remote_groups.into_iter().enumerate() {
                if y >= inner.y + inner.height { break; }
                
                // Header (no extra separator before first one, handled by previous block if needed)
                buf.set_string(
                    inner.x + 1, 
                    y, 
                    format!("-- {} --", remote), 
                    Style::default().fg(Color::DarkGray)
                );
                y += 1;

                for item in items {
                    if y >= inner.y + inner.height { break; }
                    self.render_item(buf, inner.x, y, inner.width, &item);
                    y += 1;
                }
                
                // Add separator between this group and the next
                if i < count - 1 {
                    y += 1;
                }
            }
            
            if !deleted_items.is_empty() {
                y += 1;
            }
        }

        // Render Deleted (no name)
        if !deleted_items.is_empty() {
             for item in deleted_items {
                 if y >= inner.y + inner.height { break; }
                 self.render_item(buf, inner.x, y, inner.width, &item);
                 y += 1;
            }
        }
    }
}

impl<'a> LegendSidebarWidget<'a> {
    fn render_item(&self, buf: &mut Buffer, x: u16, y: u16, width: u16, item: &SidebarItem) {
        let mut x_offset = 0;

        // Color indicator
        let color = get_color_by_index(item.lane.color_index);
        
        // Use branch-specific is_head if available, otherwise lane's aggregation
        let is_head = if let Some(branch) = item.branch {
            branch.is_head
        } else {
            item.lane.is_head
        };
        
        let indicator = if is_head { '◉' } else { '●' };

        buf[(x + x_offset, y)]
            .set_char(indicator)
            .set_style(Style::default().fg(color).add_modifier(Modifier::BOLD));
        x_offset += 2;

        // Branch name
        let name_text = if let Some(branch) = item.branch {
            branch.name.as_str()
        } else {
            "(no name)"
        };

        let style = Style::default()
            .fg(color)
            .add_modifier(Modifier::BOLD);

        // Truncate based on actual width
        for (i, ch) in name_text.chars().enumerate() {
            if x + x_offset as u16 + i as u16 >= x + width {
                break;
            }
            buf[(x + x_offset as u16 + i as u16, y)]
                .set_char(ch)
                .set_style(style);
        }
    }
}